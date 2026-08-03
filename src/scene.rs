//! 樱花学园场景层：以用户壁纸素材为背景的窗口氛围层。
//!
//! 契约（grilling 已锁）：皮肤层 / 学园轻小说 / 暮色 / 混合背景 / 声明式 + 自绘光效层。
//! 素材（data/scene/scene-sakura.png）：用户本地壁纸"樱花"（1920x1080 明亮粉紫
//! 二次元插画：右侧婚纱少女、樱花垂枝与飘瓣、左侧虚化空区），由 install 编译进
//! 二进制，无运行时文件依赖。
//!
//! 自绘叠加：飘落樱花花瓣、底部粉白薄雾；模式动效：
//!   - Connecting：底部升起的粉色光带
//!   - Success：全画布粉色霞光漫开（樱花氛围的高光时刻）
//!   - Failed：冷青闪击后自然回落（惊险感）
//!
//! 该层只负责"氛围"，不承载任何 UI 控件；窄列时玻璃卡片铺满窗口，
//! 画面焦点移到素材左端虚化区，实现"窄列装饰零负担"。

use gtk4 as gtk;
use gtk4::cairo::{Context, ImageSurface, LinearGradient, RadialGradient};
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// 场景当前模式，由外层 UI 驱动。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// 默认：樱花飘落、薄雾浮动，长时间不动。
    Idle,
    /// 认证中：粉色光带自底部升起。
    Connecting,
    /// 认证成功：整幅画面向粉色霞光漫开。
    Success,
    /// 认证失败：冷青闪击后回落。
    Failed,
}

/// 链路节点图标：Tabler 线性图标（24px 源，深樱紫 #b8507c 描边，
/// 与 data/icons 其余图标同风格），64px PNG 编译期内嵌。
const ICON_DEVICE: &[u8] = include_bytes!("../data/icons/device-laptop.png");
const ICON_GATEWAY: &[u8] = include_bytes!("../data/icons/server.png");

/// 场景状态。全程仅主线程访问。
struct State {
    /// 全局时钟（秒），由 tick callback 维护。
    t: f64,
    mode: Mode,
    /// 当前模式开始时刻。
    start: f64,
    /// 画面水平焦点 0.0~1.0：0=显示素材左端（避开主体，窄列用），
    /// 0.5=居中（默认），1.0=显示素材右端。
    shift: f64,
}

impl State {
    /// 当前模式已经进行的进度（0.0 ~ 1.0，到达 1 后钳制）。
    fn progress(&self) -> f64 {
        let elapsed = self.t - self.start;
        match self.mode {
            Mode::Idle => 0.0,
            Mode::Connecting => (elapsed / 2.4).clamp(0.0, 1.0),
            Mode::Success => (elapsed / 3.2).clamp(0.0, 1.0),
            Mode::Failed => (elapsed / 0.9).clamp(0.0, 1.0),
        }
    }
}

/// 幕色场景控件。
#[derive(Clone)]
pub struct Scene {
    area: gtk::DrawingArea,
    state: Rc<RefCell<State>>,
}

impl Scene {
    pub fn new() -> Self {
        let state = Rc::new(RefCell::new(State {
            t: 0.0,
            mode: Mode::Idle,
            start: 0.0,
            shift: 0.5,
        }));

        let sakura = load_png(include_bytes!("../data/scene/scene-sakura.png"));

        let area = gtk::DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);

        let draw_state = state.clone();
        let draw_sakura = sakura.clone();
        area.set_draw_func(move |_, cr, w, h| {
            let s = draw_state.borrow();
            draw_scene(cr, w as f64, h as f64, &s, &draw_sakura);
        });

        let state2 = state.clone();
        let area2 = area.clone();
        area.add_tick_callback(move |_, clock| {
            let t = clock.frame_time() as f64 / 1_000_000.0;
            let mut s = state2.borrow_mut();
            s.t = t;
            if s.mode == Mode::Failed && s.progress() >= 1.0 {
                s.mode = Mode::Idle;
            }
            drop(s);
            area2.queue_draw();
            glib::ControlFlow::Continue
        });

        Scene { area, state }
    }

    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    pub fn set_mode(&self, mode: Mode) {
        let mut s = self.state.borrow_mut();
        s.mode = mode;
        s.start = s.t;
        drop(s);
        self.area.queue_draw();
    }

    /// 设置画面水平焦点。0.5=居中（默认）；越接近 0 越显示素材左端
    /// （窄列用于避开右侧主体）；负值进一步左移，可完全移出主体。
    pub fn set_shift(&self, shift: f64) {
        self.state.borrow_mut().shift = shift.clamp(-1.0, 1.0);
        self.area.queue_draw();
    }
}

// ---------------------------------------------------------------------------
// 网络链路层（舞台可视化）
// ---------------------------------------------------------------------------

/// 舞台上的网络链路可视化：设备节点 ↔ 链路 ↔ 校园网关节点。
/// 独立透明 DrawingArea，由 UI 定位在舞台区，不遮挡场景背景。
/// 模式复用 `Mode`：Idle 淡粉静态 / Connecting 流动光点 /
/// Success 亮金粉 + 快速光点 + 节点光晕 / Failed 冷红虚线闪烁。
#[derive(Clone)]
pub struct Link {
    area: gtk::DrawingArea,
    mode: Rc<RefCell<Mode>>,
}

impl Link {
    pub fn new() -> Self {
        let mode = Rc::new(RefCell::new(Mode::Idle));

        let area = gtk::DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);

        let draw_mode = mode.clone();
        area.set_draw_func(move |_, cr, w, h| {
            let mode = *draw_mode.borrow();
            draw_link(cr, w as f64, h as f64, mode);
        });

        let area2 = area.clone();
        let _mode2 = mode.clone();
        area.add_tick_callback(move |_, _| {
            area2.queue_draw();
            glib::ControlFlow::Continue
        });

        Link { area, mode }
    }

    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    pub fn set_mode(&self, mode: Mode) {
        *self.mode.borrow_mut() = mode;
        self.area.queue_draw();
    }
}

/// 绘制网络链路可视化。
/// 左：设备节点（笔记本），右：校园网关节点（服务器），中间链路随模式变化：
///   Idle      淡粉静态线
///   Connecting 粉色流动光点缓慢推进
///   Success    亮金粉链路 + 快速光点 + 节点光晕（签名时刻）
///   Failed     冷红虚线闪烁
fn draw_link(cr: &Context, w: f64, h: f64, mode: Mode) {
    if w < 120.0 || h < 80.0 {
        return;
    }
    let t = glib::monotonic_time() as f64 / 1_000_000.0;

    // 节点半径随舞台宽度自适应：中屏（~300px 舞台）缩小，宽屏保持 44px 上限
    let node_r = (h * 0.10).min(w * 0.10).clamp(20.0, 44.0);
    let left_x = w * 0.30;
    let right_x = w * 0.70;
    let cy = h * 0.52;

    // ---- 链路 ----
    let pulse = match mode {
        Mode::Connecting => (t * 1.2).sin() * 0.5 + 0.5,
        Mode::Success => 1.0,
        Mode::Failed => ((t * 6.0) as i64 % 2 == 0) as u8 as f64,
        Mode::Idle => 0.35,
    };
    let link_color = match mode {
        Mode::Failed => (0.78, 0.25, 0.22, 0.65 * pulse + 0.2),
        Mode::Success => (1.0, 0.56, 0.72, 0.9),
        _ => (0.76, 0.30, 0.49, 0.30 + 0.25 * pulse),
    };
    cr.set_line_width(3.0);
    cr.set_source_rgba(link_color.0, link_color.1, link_color.2, link_color.3);
    cr.move_to(left_x + node_r, cy);
    cr.line_to(right_x - node_r, cy);
    let _ = cr.stroke();

    // 流动光点：沿链路往返
    let speed = match mode {
        Mode::Success => 1.6,
        Mode::Connecting => 0.8,
        _ => 0.0,
    };
    if speed > 0.0 {
        let span = right_x - node_r - (left_x + node_r);
        let pos = (t * speed).fract();
        let x = left_x + node_r + span * pos;
        let glow_r = if mode == Mode::Success { 1.0 } else { 0.9 };
        let glow_g = if mode == Mode::Success { 0.62 } else { 0.42 };
        let glow_b = if mode == Mode::Success { 0.75 } else { 0.55 };
        let rg = RadialGradient::new(x, cy, 0.0, x, cy, 14.0);
        rg.add_color_stop_rgba(0.0, glow_r, glow_g, glow_b, 0.95);
        rg.add_color_stop_rgba(1.0, glow_r, glow_g, glow_b, 0.0);
        let _ = cr.set_source(&rg);
        let _ = cr.paint();
    }

    // 链路上方漂移的樱花色调点缀（保持主题）
    // ---- 左节点：设备（圆形玻璃底 + 笔记本图标）----
    draw_node_base(cr, left_x, cy, node_r, mode);
    draw_node_icon(cr, left_x, cy, node_r, ICON_DEVICE);

    // ---- 右节点：校园网关（圆形玻璃底 + 服务器图标）----
    draw_node_base(cr, right_x, cy, node_r, mode);
    draw_node_icon(cr, right_x, cy, node_r, ICON_GATEWAY);
}

/// 节点圆形玻璃底，成功时带暖粉光晕。
fn draw_node_base(cr: &Context, x: f64, y: f64, r: f64, mode: Mode) {
    let halo = mode == Mode::Success;
    if halo {
        let rg = RadialGradient::new(x, y, r * 0.4, x, y, r * 2.1);
        rg.add_color_stop_rgba(0.0, 1.0, 0.62, 0.78, 0.55);
        rg.add_color_stop_rgba(1.0, 1.0, 0.62, 0.78, 0.0);
        let _ = cr.set_source(&rg);
        let _ = cr.paint();
    }
    let bg = LinearGradient::new(x, y - r, x, y + r);
    bg.add_color_stop_rgba(0.0, 1.0, 0.95, 0.98, 0.92);
    bg.add_color_stop_rgba(1.0, 1.0, 0.82, 0.91, 0.92);
    let _ = cr.save();
    cr.arc(x, y, r, 0.0, std::f64::consts::TAU);
    let _ = cr.set_source(&bg);
    let _ = cr.fill();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
    cr.set_line_width(1.5);
    let _ = cr.stroke();
    let _ = cr.restore();
}

/// 在节点圆形玻璃底上绘制 Tabler 图标：内接于圆、随节点半径缩放，
/// PNG 自带 alpha，与底部圆渐变自然合成。解码失败时静默跳过（只留圆底）。
fn draw_node_icon(cr: &Context, x: f64, y: f64, r: f64, png: &[u8]) {
    let mut reader = png;
    let Ok(icon) = ImageSurface::create_from_png(&mut reader) else {
        return;
    };
    // 内接于圆的正方形边长（约 r*1.25 是圆内最大内接方，留描边呼吸边用 1.20）
    let s = r * 1.20;
    let _ = cr.save();
    cr.translate(x - s / 2.0, y - s / 2.0);
    cr.scale(s / 64.0, s / 64.0);
    let _ = cr.set_source_surface(&icon, 0.0, 0.0);
    let _ = cr.paint();
    let _ = cr.restore();
}

/// 从编译期嵌入的 PNG 字节解码场景素材。
/// 解码失败时回退到 1x1 空面（draw 层会检测尺寸并跳过），保证不 panic。
fn load_png(bytes: &[u8]) -> Rc<ImageSurface> {
    let mut reader = bytes;
    match ImageSurface::create_from_png(&mut reader) {
        Ok(s) => Rc::new(s),
        Err(_) => {
            eprintln!("scene: 素材 PNG 解码失败，跳过该素材层");
            Rc::new(
                ImageSurface::create(gtk4::cairo::Format::ARgb32, 1, 1).unwrap_or_else(|_| {
                    ImageSurface::create(gtk4::cairo::Format::ARgb32, 1, 1).expect("1x1 surface")
                }),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// 绘制
// ---------------------------------------------------------------------------

fn draw_scene(cr: &Context, w: f64, h: f64, s: &State, sakura: &ImageSurface) {
    let p = s.progress();

    // 默认背景：樱花壁纸素材（cover 铺满，按水平焦点裁剪）
    draw_image_cover(cr, sakura, w, h, 1.0, s.shift);

    // 飘落樱花花瓣（替代旧星点：亮背景上深色星点不可见）
    draw_petals(cr, w, h, s.t);

    // 底部粉白薄雾
    draw_mist(cr, w, h, s.t);

    // 模式特效覆盖层
    match s.mode {
        Mode::Connecting => draw_connecting_glow(cr, w, h, p),
        Mode::Success => draw_success_pink(cr, w, h, p),
        Mode::Failed => draw_failed_flash(cr, w, h, p),
        Mode::Idle => {}
    }
}

/// 图片 cover 铺满（按水平焦点裁剪），带透明度。
/// shift 0=靠左显示素材左端，0.5=居中，1=显示素材右端。
fn draw_image_cover(cr: &Context, img: &ImageSurface, w: f64, h: f64, alpha: f64, shift: f64) {
    let iw = img.width() as f64;
    let ih = img.height() as f64;
    if iw < 2.0 || ih < 2.0 {
        return;
    }
    let scale = (w / iw).max(h / ih);
    let sw = iw * scale;
    let sh = ih * scale;
    // 水平裁剪位置由 shift 决定（sw 可能大于窗口宽）
    let ox = (w - sw) * shift;
    let oy = (h - sh) / 2.0;
    let _ = cr.save();
    cr.rectangle(0.0, 0.0, w, h);
    cr.clip();
    cr.translate(ox, oy);
    cr.scale(scale, scale);
    let _ = cr.set_source_surface(img, 0.0, 0.0);
    let _ = cr.paint_with_alpha(alpha);
    let _ = cr.restore();
}

/// 飘落樱花花瓣：确定性伪随机分布，垂直下落循环 + 横向正弦漂移 + 自身旋转。
fn draw_petals(cr: &Context, w: f64, h: f64, t: f64) {
    for i in 0..70u32 {
        let seed = i.wrapping_mul(2654435761).rotate_left(5);
        // 下落速度（每秒经过屏幕高度的比例），0.05~0.13，慢速轻盈
        let speed = 0.05 + 0.08 * hash01(seed, 0x1111_1111);
        // y 归一化坐标，随 t 循环下落（0 顶部 ~ 1 底部），出生点略高于屏幕
        let y = (hash01(seed, 0x2222_2222) * 1.1 + t * speed) % 1.2 - 0.05;
        if !(0.0..=1.05).contains(&y) {
            continue;
        }
        let x_base = hash01(seed, 0x3333_3333) * w;
        let drift_phase = 0.5 + 0.4 * hash01(seed, 0x4444_4444);
        let drift = (t * drift_phase + seed as f64).sin() * w * 0.02;
        let x = x_base + drift;
        let size = 3.0 + 4.0 * hash01(seed, 0x5555_5555);
        let alpha = 0.30 + 0.45 * hash01(seed, 0x6666_6666);
        let rot = (t * 0.8 + seed as f64 * 0.01) % std::f64::consts::TAU;
        // 樱花色系：白粉到粉紫之间随机
        let r = 0.92 + 0.08 * hash01(seed, 0x7777_7777);
        let g = 0.68 + 0.14 * hash01(seed, 0x8888_8888);
        let b = 0.78 + 0.14 * hash01(seed, 0x9999_9999);
        let _ = cr.save();
        cr.translate(x, y * h);
        cr.rotate(rot);
        cr.scale(1.0, 0.55);
        cr.set_source_rgba(r, g, b, alpha);
        cr.arc(0.0, 0.0, size, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
        let _ = cr.restore();
    }
}

/// 确定性 0..1 哈希。
fn hash01(seed: u32, salt: u32) -> f64 {
    let mut x = seed.wrapping_add(salt);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^= x >> 16;
    (x % 10000) as f64 / 10000.0
}

/// 底部粉白薄雾（亮背景上比深雾更贴合樱花氛围）。
fn draw_mist(cr: &Context, _w: f64, h: f64, t: f64) {
    let y = h * 0.82;
    let g = LinearGradient::new(0.0, y - h * 0.05, 0.0, h);
    g.add_color_stop_rgba(0.0, 0.97, 0.88, 0.92, 0.0);
    g.add_color_stop_rgba(0.5, 0.99, 0.92, 0.95, 0.20);
    g.add_color_stop_rgba(1.0, 0.97, 0.88, 0.92, 0.0);
    let _ = cr.set_source(&g);
    let off = (t * 0.05).sin() * h * 0.012;
    cr.translate(off, 0.0);
    let _ = cr.paint();
    cr.identity_matrix();
}

/// 认证中：底部升起的粉色光带。
fn draw_connecting_glow(cr: &Context, w: f64, h: f64, p: f64) {
    let cx = w * 0.5;
    let cy = h * 1.05 - p * h * 0.5;
    let rg = RadialGradient::new(cx, cy, 0.0, cx, cy, h * 0.55);
    rg.add_color_stop_rgba(0.0, 1.0, 0.55, 0.70, 0.30 * p);
    rg.add_color_stop_rgba(1.0, 1.0, 0.55, 0.70, 0.0);
    let _ = cr.set_source(&rg);
    let _ = cr.paint();
}

/// 认证成功：全画布粉色霞光漫开。
fn draw_success_pink(cr: &Context, _w: f64, h: f64, p: f64) {
    let g = LinearGradient::new(0.0, 0.0, 0.0, h);
    g.add_color_stop_rgba(0.0, 1.0, 0.72, 0.82, 0.05 * p);
    g.add_color_stop_rgba(0.45, 1.0, 0.58, 0.72, 0.14 * p);
    g.add_color_stop_rgba(1.0, 1.0, 0.78, 0.88, 0.22 * p);
    let _ = cr.set_source(&g);
    let _ = cr.paint();
}

fn draw_failed_flash(cr: &Context, _w: f64, h: f64, p: f64) {
    let a = if p < 0.4 { p / 0.4 } else { (1.0 - p) / 0.6 }.clamp(0.0, 1.0);
    let g = LinearGradient::new(0.0, 0.0, 0.0, h);
    g.add_color_stop_rgba(0.0, 0.35, 0.85, 0.90, 0.0);
    g.add_color_stop_rgba(0.55, 0.30, 0.80, 0.85, 0.25 * a);
    g.add_color_stop_rgba(1.0, 0.25, 0.70, 0.80, 0.0);
    let _ = cr.set_source(&g);
    let _ = cr.paint();
}
