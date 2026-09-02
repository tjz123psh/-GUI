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
use std::cell::{Cell, RefCell};
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

/// 各模式动画推进窗口（秒）：`progress = 已进行时间 / 窗口`，1.0 表示动画跑完。
/// 与 `State::progress` 共用，避免"多长算动画结束"在两处各写一份字面量。
const CONNECTING_ANIM: f64 = 2.4;
const SUCCESS_ANIM: f64 = 3.2;
const FAILED_ANIM: f64 = 0.9;

/// 成功霞光的驻留时长（秒）。满强度必须有终点：`draw_success_pink` 在 p=1 时
/// 仍以 alpha 0.05/0.14/0.22 铺满画布，不回落就会永久盖住花瓣场景，掉线之后
/// 画面还一直停在"认证成功"。10 秒够看见签名时刻，又不会常驻。
const SUCCESS_UNTIL_IDLE: f64 = 10.0;

/// Connecting 光带的兜底上限（秒）。常规终点是外层动作（成功→Success、
/// 失败→Failed），异常路径可能把 Connecting 永远留在屏上。取 240 秒是因为
/// 一次认证的最坏合法耗时已在后端封顶（授权等待 `ELEVATION_WAIT_TIMEOUT`
/// 120 秒 + 结果等待 `AUTH_RESULT_TIMEOUT` 60 秒 + `DHCP_RESTORE_DELAY` 8 秒），
/// 所以这条只兜"外层彻底卡住"，不会掐掉正常长度的认证。
const CONNECTING_UNTIL_IDLE: f64 = 240.0;

/// 场景帧预算（秒）：Idle 只有慢速花瓣与薄雾在动，30fps 看不出阶跃；特效层用
/// 满帧率保证光带/霞光连续。这个窗口是常驻的，不该一直吃满刷新率。
const IDLE_FRAME_INTERVAL: f64 = 1.0 / 30.0;
const EFFECT_FRAME_INTERVAL: f64 = 1.0 / 60.0;
/// 链路光点是主视觉反馈，保持满帧率。
const LINK_FRAME_INTERVAL: f64 = 1.0 / 60.0;

/// 未钳制进度：1.0 = 动画跑完，大于 1.0 = 在满强度上又停留了多少比例。
fn progress_of(mode: Mode, elapsed: f64) -> f64 {
    match mode {
        Mode::Idle => 0.0,
        Mode::Connecting => elapsed / CONNECTING_ANIM,
        Mode::Success => elapsed / SUCCESS_ANIM,
        Mode::Failed => elapsed / FAILED_ANIM,
    }
}

/// 回落 Idle 的进度阈值。`Idle` 永不回落；`Connecting` 的阈值只是异常兜底，
/// 常规终点由外层动作驱动（成功→Success、失败→Failed）。
fn fallback_progress(mode: Mode) -> f64 {
    match mode {
        Mode::Idle => f64::INFINITY,
        // 冷青闪击在 p=1.0 时 alpha 已衰减为 0，无需额外停留。
        Mode::Failed => 1.0,
        Mode::Connecting => CONNECTING_UNTIL_IDLE / CONNECTING_ANIM,
        Mode::Success => SUCCESS_UNTIL_IDLE / SUCCESS_ANIM,
    }
}

/// 纯函数：按当前进度结算模式归宿，供帧回调与测试共用。
fn mode_after_progress(mode: Mode, progress: f64) -> Mode {
    if progress.is_finite() && progress >= fallback_progress(mode) {
        Mode::Idle
    } else {
        mode
    }
}

fn frame_interval(mode: Mode) -> f64 {
    match mode {
        Mode::Idle => IDLE_FRAME_INTERVAL,
        Mode::Connecting | Mode::Success | Mode::Failed => EFFECT_FRAME_INTERVAL,
    }
}

/// 本帧是否到点该重绘。`now < last` 说明帧时钟换过（控件重建 root 等），
/// 立即放行一次，免得预算永远等不满。
fn frame_due(now: f64, last: Option<f64>, interval: f64) -> bool {
    match last {
        Some(last) => now < last || now - last >= interval,
        None => true,
    }
}

/// 链路在 Idle 下是静态图（pulse 固定、光点速度为 0），不需要逐帧唤醒帧时钟。
fn link_needs_frames(mode: Mode) -> bool {
    !matches!(mode, Mode::Idle)
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
    /// 上一次真正推进动画的帧时刻；`None` = 还没画过。
    last_frame: Option<f64>,
    /// 控件当前未映射（窗口隐藏/最小化）。重新映射时要把时间轴平移回来。
    hidden: bool,
}

impl State {
    fn new() -> Self {
        State {
            t: 0.0,
            mode: Mode::Idle,
            start: 0.0,
            shift: 0.5,
            last_frame: None,
            hidden: false,
        }
    }

    /// 当前模式已经进行的进度（0.0 ~ 1.0，到达 1 后钳制）。
    fn progress(&self) -> f64 {
        progress_of(self.mode, self.t - self.start).clamp(0.0, 1.0)
    }

    /// 推进一帧：更新全局时钟并结算模式回落，返回本帧是否需要重绘。
    /// 未映射时既不重绘也不推进时钟；重新可见后把 `start` 平移同样的时间差，
    /// 让动画从停下那一帧继续，而不是按挂钟瞬间跑完（隐藏期间等于暂停）。
    fn advance(&mut self, t: f64, mapped: bool) -> bool {
        if !mapped {
            self.hidden = true;
            return false;
        }
        if self.hidden {
            self.hidden = false;
            self.start += t - self.t;
        } else if !frame_due(t, self.last_frame, frame_interval(self.mode)) {
            return false;
        }
        self.last_frame = Some(t);
        self.t = t;
        let next = mode_after_progress(self.mode, progress_of(self.mode, t - self.start));
        if next != self.mode {
            self.mode = next;
            self.start = t;
        }
        true
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
        let state = Rc::new(RefCell::new(State::new()));

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
        // 只用 GTK 传进来的控件引用重绘：闭包被控件拥有，再把控件克隆进闭包
        // 就构成自引用环，控件永不释放（每开一个窗口泄漏一份）。
        area.add_tick_callback(move |area, clock| {
            // mapped 才是"真的会被画出来"：is_visible() 只看 visible 标记，窗口
            // 最小化/隐藏时子控件仍然 visible，用它判断会继续空转重绘。
            let t = clock.frame_time() as f64 / 1_000_000.0;
            // borrow 在 queue_draw 之前结束，避免与 draw 回调形成双借用 panic。
            let redraw = state2.borrow_mut().advance(t, area.is_mapped());
            if redraw {
                area.queue_draw();
            }
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
    /// `Mode` 是 Copy，用 `Cell` 就够，省掉与 draw 回调之间的借用交叉。
    mode: Rc<Cell<Mode>>,
    /// 进入当前模式的时刻（与帧时钟同一单调时基），用于动画终点回落。
    start: Rc<Cell<f64>>,
    /// 回到 Idle 时回调自己 `Break` 退出帧时钟，之后由 `set_mode` 重新挂载；
    /// 这个标记防止同一次动画里重复挂载回调。
    ticking: Rc<Cell<bool>>,
}

impl Link {
    pub fn new() -> Self {
        let mode = Rc::new(Cell::new(Mode::Idle));

        let area = gtk::DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);

        let draw_mode = mode.clone();
        area.set_draw_func(move |_, cr, w, h| {
            draw_link(cr, w as f64, h as f64, draw_mode.get());
        });

        // 初始 Idle 是静态图，不挂载帧回调：常驻小工具不该逐帧唤醒帧时钟。
        Link {
            area,
            mode,
            start: Rc::new(Cell::new(0.0)),
            ticking: Rc::new(Cell::new(false)),
        }
    }

    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    pub fn set_mode(&self, mode: Mode) {
        self.mode.set(mode);
        self.start.set(glib::monotonic_time() as f64 / 1_000_000.0);
        self.area.queue_draw();
        if link_needs_frames(mode) && !self.ticking.get() {
            self.arm_tick();
        }
    }

    /// 挂载动画期帧回调。控件只用回调自带的那个引用，避免自引用环。
    fn arm_tick(&self) {
        self.ticking.set(true);
        let mode = self.mode.clone();
        let start = self.start.clone();
        let ticking = self.ticking.clone();
        let last_frame = Cell::new(None);
        self.area.add_tick_callback(move |area, clock| {
            let t = clock.frame_time() as f64 / 1_000_000.0;
            // 回落判定必须在这里做：外层只在动作结束时改一次模式，链路自己
            // 不结算的话，成功态光点会一直 60fps 动画并常亮，与已回落的场景
            // 画面互相矛盾（掉线后仍显示"已连通"）。
            let current = mode.get();
            let next = mode_after_progress(current, progress_of(current, t - start.get()));
            if next != current {
                mode.set(next);
                start.set(t);
                area.queue_draw();
            }
            if !link_needs_frames(mode.get()) {
                ticking.set(false);
                return glib::ControlFlow::Break;
            }
            // 未映射时保留回调（GTK 会自己挂起帧时钟）但不排队重绘。
            // 光点位置与 t 同一时基，合帧不会让它跳变。
            if area.is_mapped() && frame_due(t, last_frame.get(), LINK_FRAME_INTERVAL) {
                last_frame.set(Some(t));
                area.queue_draw();
            }
            glib::ControlFlow::Continue
        });
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
    // 必须 save/restore：旧写法用 identity_matrix() 收尾，会把 GTK 为控件准备好
    // 的变换（含 HiDPI/分数缩放的 device scale）一起清掉；而薄雾画在模式特效层
    // 之前，于是之后的 Connecting/Success/Failed 光效全按设备像素绘制而错位。
    let _ = cr.save();
    cr.translate(off, 0.0);
    let _ = cr.paint();
    let _ = cr.restore();
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

#[cfg(test)]
mod tests {
    use super::{
        CONNECTING_UNTIL_IDLE, FAILED_ANIM, IDLE_FRAME_INTERVAL, Mode, SUCCESS_UNTIL_IDLE, State,
        fallback_progress, frame_due, link_needs_frames, mode_after_progress, progress_of,
    };

    /// 该模式停留多少秒后自动回落 Idle；`None` = 永不自治回落。
    fn idle_after(mode: Mode) -> Option<f64> {
        let window = match mode {
            Mode::Connecting => super::CONNECTING_ANIM,
            Mode::Success => super::SUCCESS_ANIM,
            Mode::Failed => FAILED_ANIM,
            Mode::Idle => return None,
        };
        let threshold = fallback_progress(mode);
        threshold.is_finite().then_some(threshold * window)
    }

    #[test]
    fn success_glow_lingers_then_falls_back() {
        // 关键回归：满强度的全画布粉层必须有终点，否则掉线后画面一直停在
        // "认证成功"，还会永久盖住花瓣场景。
        assert_eq!(idle_after(Mode::Success), Some(SUCCESS_UNTIL_IDLE));
        assert_eq!(
            mode_after_progress(Mode::Success, progress_of(Mode::Success, 1.0)),
            Mode::Success,
            "霞光要驻留一段时间，不能动画一跑完就消失"
        );
        assert_eq!(
            mode_after_progress(
                Mode::Success,
                progress_of(Mode::Success, SUCCESS_UNTIL_IDLE + 0.1)
            ),
            Mode::Idle
        );
    }

    #[test]
    fn failed_flash_falls_back_without_outer_input() {
        assert_eq!(idle_after(Mode::Failed), Some(FAILED_ANIM));
        assert_eq!(
            mode_after_progress(Mode::Failed, progress_of(Mode::Failed, FAILED_ANIM / 2.0)),
            Mode::Failed
        );
        assert_eq!(
            mode_after_progress(Mode::Failed, progress_of(Mode::Failed, FAILED_ANIM)),
            Mode::Idle
        );
    }

    #[test]
    fn idle_never_self_terminates() {
        assert_eq!(idle_after(Mode::Idle), None);
        assert_eq!(mode_after_progress(Mode::Idle, f64::MAX), Mode::Idle);
    }

    #[test]
    fn connecting_only_falls_back_as_an_outer_wedge_backstop() {
        // 常规终点由外层动作驱动；240 秒兜的是"外层彻底没回状态"的异常。
        // 上界必须大于后端最坏合法耗时：授权 120s + 结果等待 60s + DHCP 注入 8s。
        assert_eq!(idle_after(Mode::Connecting), Some(CONNECTING_UNTIL_IDLE));
        assert_eq!(
            mode_after_progress(
                Mode::Connecting,
                progress_of(Mode::Connecting, CONNECTING_UNTIL_IDLE - 0.1)
            ),
            Mode::Connecting
        );
    }

    #[test]
    fn frame_budget_recovers_when_the_clock_restarts() {
        assert!(frame_due(1.0, None, IDLE_FRAME_INTERVAL), "首帧必须绘制");
        assert!(!frame_due(1.0, Some(1.0), IDLE_FRAME_INTERVAL));
        assert!(
            !frame_due(
                1.0 + IDLE_FRAME_INTERVAL / 2.0,
                Some(1.0),
                IDLE_FRAME_INTERVAL
            ),
            "未达帧预算时应合帧"
        );
        assert!(frame_due(
            1.0 + IDLE_FRAME_INTERVAL,
            Some(1.0),
            IDLE_FRAME_INTERVAL
        ));
        assert!(
            frame_due(0.5, Some(9.0), IDLE_FRAME_INTERVAL),
            "帧时钟回退（控件重建 root）后必须恢复绘制，否则动画永久停住"
        );
    }

    #[test]
    fn advance_respects_budget_and_settles_the_mode() {
        let mut state = State::new();
        assert!(state.advance(100.0, true), "首帧必须绘制");
        assert!(
            !state.advance(100.0 + IDLE_FRAME_INTERVAL / 2.0, true),
            "未达预算不该排队重绘"
        );
        state.mode = Mode::Failed;
        state.start = state.t;
        assert!(state.advance(state.t + FAILED_ANIM + 0.1, true));
        assert_eq!(state.mode, Mode::Idle, "帧回调要把动画结束的判定接上");
    }

    #[test]
    fn unmapped_pauses_instead_of_running_ahead() {
        let mut state = State::new();
        assert!(state.advance(10.0, true));
        state.mode = Mode::Success;
        state.start = 8.0; // 已经进行 2 秒
        assert!(!state.advance(12.0, false), "未映射时不重绘");
        assert!(state.advance(18.0, true));
        assert!(
            (state.t - state.start - 2.0).abs() < 1e-9,
            "隐藏的 6 秒应等价于暂停，实际进行了 {}",
            state.t - state.start
        );
        assert_eq!(state.mode, Mode::Success, "暂停期间不应被判定为已驻留完成");
    }

    #[test]
    fn static_link_stops_requesting_frames() {
        assert!(
            !link_needs_frames(Mode::Idle),
            "链路 Idle 是静态图，不该挂帧时钟"
        );
        for mode in [Mode::Connecting, Mode::Success, Mode::Failed] {
            assert!(link_needs_frames(mode));
        }
    }
}
