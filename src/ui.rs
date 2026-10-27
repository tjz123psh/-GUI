//! 樱花学园皮肤层：场景舞台 + 连接控制台。
//!
//! 契约（grilling 已锁）：
//!   1. 皮肤层——后端骨架不动，二次元表达由视觉层承载；
//!   2. 轻小说基调：樱花插画、玻璃拟态、粉紫渐变；
//!   3. 舞台 + 控制台：左侧场景舞台承载状态可视化（自绘网络链路图 +
//!      大状态字 + 状态胶囊），右侧单张玻璃操作面板（连接表单 + 动作）；
//!   4. 不堆卡片：删除嵌套玻璃区块，控制台是唯一容器；
//!   5. 签名时刻：认证成功时链路点亮 + 光点流动 + 节点光晕 + 霞光漫开；
//!   6. 三档断点：<760 单栏 + 顶部紧凑状态条；760-939 紧凑舞台 + 控制台；
//!      >=940 完整舞台 + 控制台；
//!   7. 动效：场景层 Scene 逐帧特效 + 链路层 Link 光点动画。

use crate::{config, scene, system};
use adw::prelude::*;
use futures_channel::mpsc;
use futures_util::StreamExt;
use gtk4 as gtk;
use gtk4::glib;
use libadwaita as adw;
use rjsupplicant_gui::netcheck;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 三档断点（以 Niri preset 宽度 640/960/1280 为基准，取中间值切档）。
/// STANDARD_MAX 原为 1080，但 niri 平铺下半屏窗口常见宽度 954-960，
/// 若断点高于此，双栏永远不会出现、内容全部竖排单卡（用户批判"空洞"）。
const COMPACT_MAX: i32 = 760;
const STANDARD_MAX: i32 = 940;

/// 控制台固定宽度：不随窗口拉宽，多余宽度全部交给场景舞台。
const CONSOLE_WIDTH: i32 = 420;

/// 最近一次认证结果（会话内记录，供「连接详情」区块显示）。
#[derive(Clone)]
struct LastAuth {
    ok: bool,
    summary: String,
    /// 用单调钟：SystemTime 会被用户改系统时间或 NTP 步进影响，回拨时
    /// "X 分钟前"会退回"刚刚"甚至错误。
    at: std::time::Instant,
}

/// 大状态字/状态条的语义色。连接状态机只关心语义，CSS 负责具体色值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StageTone {
    Ok,
    Success,
    Busy,
    Warn,
    Off,
    Failed,
}

impl StageTone {
    fn stage_class(self) -> &'static str {
        match self {
            StageTone::Ok => "stat-ok",
            StageTone::Success => "stat-success",
            StageTone::Busy => "stat-busy",
            StageTone::Warn => "stat-warn",
            StageTone::Off => "stat-off",
            StageTone::Failed => "stat-failed",
        }
    }

    fn dot_class(self) -> &'static str {
        match self {
            StageTone::Ok | StageTone::Success => "dot-ok",
            StageTone::Busy => "dot-busy",
            StageTone::Warn => "dot-warn",
            StageTone::Off => "dot-off",
            StageTone::Failed => "dot-failed",
        }
    }

    /// 连接详情里状态值的文字颜色（HTML markup 不支持 CSS 变量，必须给实色）。
    fn detail_color(self) -> &'static str {
        match self {
            StageTone::Ok | StageTone::Success => "#C14D7C",
            StageTone::Busy => "#8C4A6B",
            StageTone::Warn => "#B7791F",
            StageTone::Off => "#6E5568",
            StageTone::Failed => "#C63F38",
        }
    }
}

/// 连接展示状态（四态）：探测/载波/客户端进程是三个独立证据源，
/// 不能再用一个 bool 把"客户端没在跑"直接等同于"未连接"。
#[derive(Clone, Debug, PartialEq, Eq)]
enum LinkState {
    Connected { evidence: String },
    RunningUnverified,
    Disconnected { reason: String },
    Unknown { reason: String },
}

impl LinkState {
    fn headline(&self) -> &'static str {
        match self {
            LinkState::Connected { .. } => "已连接",
            LinkState::RunningUnverified => "客户端运行中",
            LinkState::Disconnected { .. } => "未连接",
            LinkState::Unknown { .. } => "状态未知",
        }
    }

    fn tone(&self) -> StageTone {
        match self {
            LinkState::Connected { .. } => StageTone::Ok,
            LinkState::RunningUnverified => StageTone::Warn,
            // 载波断开/网卡不存在是"明确未连接"，用中性 off 而不是红色故障。
            LinkState::Disconnected { .. } | LinkState::Unknown { .. } => StageTone::Off,
        }
    }

    fn detail_text(&self) -> String {
        match self {
            LinkState::Connected { evidence } => format!("已连接（{evidence}）"),
            LinkState::RunningUnverified => "客户端运行中（等待路径确认）".to_string(),
            LinkState::Disconnected { reason } => format!("未连接（{reason}）"),
            LinkState::Unknown { reason } => format!("状态未知（{reason}）"),
        }
    }
}

/// 纯函数：把载波/探测/进程/网卡地址合成展示状态。
///
/// 优先级是产品语义：载波断开是硬事实；探测可达优先于进程（客户端可能已
/// SIGSEGV 但会话仍保持）；客户端运行时探测未确认只能标黄，不能报未连接；
/// 探测未知且没有客户端时，只有地址+默认路由都齐全才允许按已连接展示。
fn classify_link(
    carrier: &netcheck::CarrierState,
    probe: &netcheck::ProbeOutcome,
    client_running: bool,
    has_ipv4: bool,
    has_default_route: bool,
) -> LinkState {
    match carrier {
        netcheck::CarrierState::Down => {
            return LinkState::Disconnected {
                reason: "网线未连接".to_string(),
            };
        }
        netcheck::CarrierState::Missing => {
            return LinkState::Disconnected {
                reason: "网卡不存在".to_string(),
            };
        }
        netcheck::CarrierState::Up | netcheck::CarrierState::Unknown => {}
    }

    if matches!(probe, netcheck::ProbeOutcome::Reachable) {
        return LinkState::Connected {
            evidence: if client_running {
                "客户端运行且路径可达".to_string()
            } else {
                "会话保持 · 路径可达".to_string()
            },
        };
    }
    if client_running {
        return LinkState::RunningUnverified;
    }
    if matches!(probe, netcheck::ProbeOutcome::Unknown) {
        if has_ipv4 && has_default_route {
            return LinkState::Connected {
                evidence: "网卡已获取地址与默认路由".to_string(),
            };
        }
        return LinkState::Unknown {
            reason: "探测结果不可用".to_string(),
        };
    }
    LinkState::Disconnected {
        reason: "有线路径不可达".to_string(),
    }
}

/// 认证动效结束后，大状态字保持"认证成功/失败/未确认"的时长。
/// 10 秒轮询到来时自然回落到链路状态，避免成功文案一闪而过。
const STAGE_HOLD: Duration = Duration::from_secs(8);

/// 迁移/失败横幅的按钮动作：点击时按当前状态分派，避免把不同状态
/// 都接到安装对话框上。
#[derive(Clone, Copy, PartialEq, Eq)]
enum BannerAction {
    None,
    InstallClient,
    RestartService,
}

/// 窗口内需要被多个回调共享的最小状态集。
#[derive(Clone)]
struct Ui {
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    scene: scene::Scene,
    /// 舞台链路可视化（设备 ↔ 校园网关）。
    link: scene::Link,
    /// 宽屏舞台容器（>=760 显示）：大状态字 + 链路图 + 状态胶囊。
    stage: gtk::Box,
    stage_big: gtk::Label,
    stage_sub: gtk::Label,
    /// 状态胶囊：客户端 / 进程 / 服务 / 网卡，(圆点, 值)。
    pills: Vec<(gtk::Box, gtk::Label)>,
    /// 胶囊 FlowBox：按断点限制列数，窄舞台固定 2×2，宽舞台最多 4 列。
    pills_flow: gtk::FlowBox,
    /// 窄屏（<760）顶部紧凑状态条：圆点 + 状态字 + 副行 + 网卡胶囊。
    compact_status: gtk::Box,
    compact_dot: gtk::Box,
    compact_state: gtk::Label,
    compact_sub: gtk::Label,
    /// 右侧操作面板（玻璃卡，唯一容器）。
    console: gtk::Box,
    /// 连接详情区三行：账号 / 状态 / 最近认证结果。
    detail_account: gtk::Label,
    detail_state: gtk::Label,
    detail_last: gtk::Label,
    /// 会话内最近一次认证结果（连接成功/失败时更新）。
    last_auth: Arc<std::sync::Mutex<Option<LastAuth>>>,
    /// 横幅按钮当前应执行的动作（安装迁移 / 重新尝试服务）。
    banner_action: Arc<std::sync::Mutex<BannerAction>>,
    nic: gtk::DropDown,
    nic_model: gtk::StringList,
    username: gtk::Entry,
    password: gtk::Entry,
    connect: gtk::Button,
    disconnect: gtk::Button,
    install: gtk::Button,
    autostart: gtk::Switch,
    refresh: gtk::Button,
    /// 渐进披露浮层：header 上的"更多工具"按钮 + 弹出面板。
    more: gtk::Button,
    more_popover: gtk::Popover,
    /// 连接设置开关（浮层内控制 DHCP 与是否保存密码）。
    dhcp: gtk::Switch,
    save_password: gtk::Switch,
    /// 诊断工具行：测试连通 / 重启 / 打开目录 / 实时日志 / 帮助
    diag: Vec<adw::ActionRow>,
    /// 最近日志多行预览（日志区块内，实时随状态刷新）。
    log_preview: gtk::Label,
    /// 当前网卡 IP/网关信息（网卡行下方小字，随状态刷新）。
    net_info: gtk::Label,
    /// 迁移提示横幅：旧版客户端 / 不安全的开机认证服务配置时显示。
    banner: adw::Banner,
    busy: Arc<AtomicBool>,
    /// 刷新状态期间为 true：防止 set_active 触发的 notify 回调
    /// 误把"同步开关"当成用户操作去启停 systemd 服务。
    refreshing: Arc<AtomicBool>,
    /// 每次 refresh 递增；异步结果只有 generation 仍匹配才能落地，
    /// 防止慢的旧轮询覆盖刚完成的新状态。
    refresh_generation: Arc<AtomicU64>,
    /// 是否已经应用过一帧状态（含首次 loading 回退）。只有从未有过界面
    /// 数据时才回退"检测中…"，后续失败保留上一帧，不能把已有界面打回 loading。
    has_status: Arc<AtomicBool>,
    /// 后端明确报告客户端未安装：连接/开机认证禁用，安装按钮升为主操作。
    client_missing: Arc<AtomicBool>,
    /// 程序化 set_active 的重入保护，避免开关回滚反向触发 disable-service。
    autostart_syncing: Arc<AtomicBool>,
    /// 认证动作结束后短时间压住大状态字（认证成功/连接失败/结果未确认），
    /// 不让紧随其后的 refresh 立刻覆盖成普通链路状态。
    stage_hold: Arc<std::sync::Mutex<Option<(String, StageTone, Instant)>>>,
}

impl Ui {
    fn selected_nic(&self) -> String {
        let idx = self.nic.selected();
        self.nic_model
            .string(idx)
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    fn settings(&self) -> config::Settings {
        config::Settings {
            username: self.username.text().to_string(),
            nic: self.selected_nic(),
            dhcp: self.dhcp.is_active(),
            save_password: self.save_password.is_active(),
        }
    }

    /// 用最新探测到的网卡列表重建下拉框，并尽量保留用户当前选择。
    /// 旧实现只在构建窗口时探测一次并存成快照：后接的网卡（USB 网卡、驱动
    /// 重载、拔插）既选不到也看不到状态，只能重启应用。
    fn sync_interfaces(&self, names: &[String]) {
        let selected = self.selected_nic();
        let known: Vec<String> = (0..self.nic_model.n_items())
            .filter_map(|index| self.nic_model.string(index).map(|s| s.to_string()))
            .collect();
        if known.as_slice() == names {
            return;
        }
        // 一次 splice 完成替换：StringList 没有 remove_all，且分步增删会让
        // 下拉框在中间态显示为空。
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        self.nic_model.splice(0, self.nic_model.n_items(), &refs);
        // 网卡还在列表里就保留用户的选择，否则回到第一项。
        let keep = names.iter().position(|name| *name == selected);
        self.nic.set_selected(keep.unwrap_or(0) as u32);
    }

    /// 同步大状态字（舞台 + 窄屏状态条）的文案与语义色。
    fn set_stage(&self, text: &str, tone: StageTone) {
        for l in [&self.stage_big, &self.compact_state] {
            l.set_label(text);
            for cls in [
                "stat-ok",
                "stat-success",
                "stat-busy",
                "stat-warn",
                "stat-off",
                "stat-failed",
            ] {
                l.remove_css_class(cls);
            }
            l.add_css_class(tone.stage_class());
        }
        for cls in ["dot-ok", "dot-busy", "dot-warn", "dot-off", "dot-failed"] {
            self.compact_dot.remove_css_class(cls);
        }
        self.compact_dot.add_css_class(tone.dot_class());
    }

    /// 压住大状态字一段时间，让认证结果文案不被紧随其后的刷新覆盖。
    fn hold_stage(&self, text: &str, tone: StageTone) {
        if let Ok(mut guard) = self.stage_hold.lock() {
            *guard = Some((text.to_string(), tone, Instant::now()));
        }
        self.set_stage(text, tone);
    }

    fn stage_hold_active(&self) -> bool {
        let Ok(mut guard) = self.stage_hold.lock() else {
            return false;
        };
        match guard.as_ref() {
            Some((_, _, at)) if at.elapsed() < STAGE_HOLD => true,
            Some(_) => {
                *guard = None;
                false
            }
            None => false,
        }
    }

    /// 按窗口宽度应用断点：窄屏单栏 + 顶部状态条；宽屏舞台 + 控制台。
    fn apply_breakpoint(&self, width: i32) {
        let compact = width < COMPACT_MAX;
        self.stage.set_visible(!compact);
        self.compact_status.set_visible(compact);
        if compact {
            // 窄列：控制台铺满窗口（实底变体保证文字可读），场景 focus 在素材左端
            self.console.set_halign(gtk::Align::Fill);
            self.console.set_margin_start(0);
            self.console.set_margin_end(0);
            self.console.set_size_request(-1, -1);
            self.console.add_css_class("solid");
            self.scene.set_shift(0.0);
        } else {
            // 中/宽屏：控制台固定宽靠右，左侧整块留给舞台
            self.console.set_halign(gtk::Align::End);
            self.console.set_margin_start(0);
            self.console.set_margin_end(0);
            self.console.set_size_request(CONSOLE_WIDTH, -1);
            self.console.remove_css_class("solid");
            if width < STANDARD_MAX {
                // 760-939 紧凑舞台只有约 268-448px：状态字降一档，长文案
                //（客户端运行中/认证结果未确认）也不会顶到舞台边缘。
                self.stage.add_css_class("narrow-stage");
                self.scene.set_shift(0.5);
            } else {
                self.stage.remove_css_class("narrow-stage");
                self.scene.set_shift(0.58);
            }
            // 按舞台实际可用宽度决定胶囊列数：<520px（760/940/960 档）排成
            // 稳定的 2×2，宽舞台（1280 档约 788px）才排 4 列；避免 940 档出现
            // 3+1 的孤行。这里只影响胶囊内部排布，不新增结构断点。
            let stage_width = width - CONSOLE_WIDTH - 72;
            self.pills_flow
                .set_max_children_per_line(if stage_width < 520 { 2 } else { 4 });
        }
    }
}

/// 内置线性图标资源（Tabler Icons 深樱紫描边，SVG 渲染成 64px PNG 内嵌）。
macro_rules! icon_png {
    ($name:literal) => {
        include_bytes!(concat!("../data/icons/", $name, ".png"))
    };
}
const ICON_REFRESH: &[u8] = icon_png!("refresh");
const ICON_CONNECT: &[u8] = icon_png!("plug-connected");
const ICON_DISCONNECT: &[u8] = icon_png!("plug-off");
const ICON_INSTALL: &[u8] = icon_png!("download");
const ICON_NETWORK: &[u8] = icon_png!("network");
const ICON_USER: &[u8] = icon_png!("user");
const ICON_LOCK: &[u8] = icon_png!("lock");
const ICON_ROUTER: &[u8] = icon_png!("router");
const ICON_SETTINGS: &[u8] = icon_png!("settings");
const ICON_TERMINAL: &[u8] = icon_png!("terminal-2");
const ICON_FOLDER: &[u8] = icon_png!("folder-open");
const ICON_HELP: &[u8] = icon_png!("help-circle");
const ICON_BULB: &[u8] = icon_png!("bulb");

/// 从内嵌 PNG 构造图标 Image。解码失败时退化为空图（尺寸仍保留，布局不塌）。
fn icon_image(png: &[u8], size: i32) -> gtk::Image {
    let image = gtk::Image::new();
    match gtk4::gdk::Texture::from_bytes(&glib::Bytes::from(&png)) {
        Ok(texture) => {
            image.set_paintable(Some(&texture));
            image.set_pixel_size(size);
        }
        Err(_) => {
            image.set_pixel_size(size);
        }
    }
    image
}

/// 带内置图标的按钮（图标 + 文字并排，区别于 icon-only）。
fn button_with_icon(label: &str, png: &[u8]) -> gtk::Button {
    let btn = gtk::Button::new();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();
    let img = icon_image(png, 16);
    content.append(&img);
    content.append(&gtk::Label::new(Some(label)));
    btn.set_child(Some(&content));
    btn
}

/// 给交互控件挂 pointer 光标（GTK4 无 CSS cursor，必须走 widget API）。
fn set_pointer_cursor(widget: &impl IsA<gtk::Widget>) {
    if let Some(cursor) = gtk::gdk::Cursor::from_name("pointer", None) {
        widget.set_cursor(Some(&cursor));
    }
}

/// 构造诊断工具行集合（图标 + 标题 + 副标题 + go-next 后缀）。
fn make_diag_rows(actions: &[(&str, &[u8], &str)]) -> Vec<adw::ActionRow> {
    let mut rows = Vec::new();
    for (label, icon, sub) in actions {
        let row = adw::ActionRow::builder()
            .title(*label)
            .subtitle(*sub)
            .activatable(true)
            .build();
        set_pointer_cursor(&row);
        row.add_prefix(&icon_image(icon, 18));
        row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        rows.push(row);
    }
    rows
}

/// 状态胶囊：圆点 + 值文字（舞台底部一行 / 窄屏状态条复用）。
fn make_pill(label: &str) -> (gtk::Box, gtk::Box, gtk::Label) {
    let dot = gtk::Box::builder().build();
    dot.set_size_request(8, 8);
    dot.set_valign(gtk::Align::Center);
    dot.add_css_class("pill-dot");
    // 初值是"加载中"，不能先用表示健康的绿点（窄屏状态条同样用 dot-warn）。
    dot.add_css_class("dot-warn");
    let value = gtk::Label::builder()
        .label(label)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(18)
        .xalign(0.0)
        .build();
    let pill = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .css_classes(["pill"])
        .build();
    pill.append(&dot);
    pill.append(&value);
    (pill, dot, value)
}

/// 表单行：图标 + 固定宽 label + 输入框（两列对齐，GNOME 式紧凑表单）。
fn form_line(png: &[u8], label: &str, child: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .css_classes(["form-line"])
        .build();
    row.append(&icon_image(png, 18));
    let name = gtk::Label::builder()
        .label(label)
        .css_classes(["form-label"])
        .xalign(0.0)
        .width_request(64)
        .build();
    row.append(&name);
    let widget = child.clone().upcast::<gtk::Widget>();
    widget.set_hexpand(true);
    row.append(&widget);
    row
}

pub fn activate(app: &adw::Application) {
    // 应用已在运行时再次启动（点击图标、从终端拉起）会让 GApplication 重发
    // activate；若无条件 build_window，同一进程会叠出第二个窗口和第二条轮询
    // 定时器。这里只呈现既有窗口，保持单窗口语义。
    if let Some(existing) = app
        .windows()
        .into_iter()
        .find_map(|window| window.downcast::<adw::ApplicationWindow>().ok())
    {
        existing.present();
        return;
    }
    let ui = build_window(app);
    refresh_status(&ui);
    ui.window.present();

    // 连接状态/时长/IP 轻量实时刷新：连接成功或断开后 run_* 会立即刷新，
    // 这里每 10 秒轮询一次保证「已连接 X 分钟」、IP、服务状态持续更新。
    // 认证动作进行中（busy）时跳过，避免干扰。
    let timer_ui = ui.clone();
    glib::timeout_add_seconds_local(10, move || {
        if !timer_ui.busy.load(Ordering::Relaxed) {
            refresh_status_polled(&timer_ui);
        }
        glib::ControlFlow::Continue
    });
}

fn build_window(app: &adw::Application) -> Ui {
    load_theme_css();
    // 一次读盘供四个初值共用：以前每处各调一次 config::load()，既重复 IO
    // 也可能在两次读取之间拿到不一致的值。
    let settings = config::load();

    // 强制深色主题，避免原生控件亮色皮肤破坏樱花氛围
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let toasts = adw::ToastOverlay::new();
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("锐捷有线认证")
        .default_width(1280)
        .default_height(900)
        .content(&toasts)
        .build();

    // 标题栏：原生 HeaderBar 保持简洁，右侧放刷新与更多工具按钮
    let header = adw::HeaderBar::builder()
        .title_widget(&gtk::Label::builder().label("锐捷有线认证").build())
        .build();
    let refresh = gtk::Button::new();
    refresh.set_tooltip_text(Some("刷新状态"));
    refresh.add_css_class("flat");
    refresh.set_child(Some(&icon_image(ICON_REFRESH, 18)));
    set_pointer_cursor(&refresh);
    header.pack_start(&refresh);

    let more = gtk::Button::new();
    more.set_tooltip_text(Some("更多工具"));
    more.add_css_class("flat");
    more.set_child(Some(&icon_image(ICON_SETTINGS, 18)));
    set_pointer_cursor(&more);
    header.pack_start(&more);

    let more_content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .width_request(340)
        .build();

    // 连接设置：DHCP / 保存密码，直接读写 config::load
    let settings_group = adw::PreferencesGroup::builder()
        .title("连接设置")
        .description("变更即时保存，下次连接生效")
        .build();
    let dhcp = gtk::Switch::builder()
        .active(settings.dhcp)
        .valign(gtk::Align::Center)
        .build();
    let dhcp_row = adw::ActionRow::builder()
        .title("DHCP 自动获取 IP")
        .subtitle("由学校网络自动分配地址")
        .build();
    dhcp_row.add_suffix(&dhcp);
    settings_group.add(&dhcp_row);
    let save_password = gtk::Switch::builder()
        .active(settings.save_password)
        .valign(gtk::Align::Center)
        .build();
    let save_row = adw::ActionRow::builder()
        .title("保存密码")
        .subtitle("让官方客户端记住密码")
        .build();
    save_row.add_suffix(&save_password);
    settings_group.add(&save_row);
    more_content.append(&settings_group);

    // 诊断与日志工具：接线已有后端能力（浮层唯一入口，控制台底部另有常驻入口）
    let diag_actions = [
        ("测试网络连通", ICON_BULB, "ping 阿里公共 DNS"),
        ("重启开机认证", ICON_ROUTER, "systemd 服务"),
        ("打开客户端目录", ICON_FOLDER, "查看已安装文件"),
        ("打开实时日志", ICON_TERMINAL, "终端 journalctl -f"),
        ("在线帮助", ICON_HELP, "校园网官方帮助页"),
    ];
    let diag_group = adw::PreferencesGroup::builder().title("诊断与工具").build();
    let diag_rows = make_diag_rows(&diag_actions);
    for row in &diag_rows {
        diag_group.add(row);
    }
    more_content.append(&diag_group);

    let more_popover = gtk::Popover::builder().child(&more_content).build();
    more_popover.set_parent(&more);

    // ---- 场景层：全窗口樱花背景 ----
    let scene = scene::Scene::new();
    let scene_widget = scene.widget().clone();
    scene_widget.set_hexpand(true);
    scene_widget.set_vexpand(true);

    // ---- 舞台：大状态字 + 链路图 + 状态胶囊（宽屏可见，窄屏折叠为状态条）----
    let stage = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .css_classes(["stage"])
        .hexpand(true)
        .vexpand(true)
        .build();

    let stage_big = gtk::Label::builder()
        .label("检测中…")
        .css_classes(["stage-big"])
        .halign(gtk::Align::Center)
        .hexpand(true)
        .margin_top(64)
        .build();
    stage.append(&stage_big);

    let stage_sub = gtk::Label::builder()
        .label("正在检测网络状态…")
        .css_classes(["stage-sub"])
        .halign(gtk::Align::Center)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(52)
        .build();
    stage.append(&stage_sub);

    // 链路可视化：设备 ↔ 校园网关，随认证模式点亮（签名时刻）
    let link = scene::Link::new();
    let link_widget = link.widget().clone();
    link_widget.set_vexpand(true);
    link_widget.set_margin_top(16);
    link_widget.set_margin_bottom(16);
    stage.append(&link_widget);

    // 状态胶囊：FlowBox 最多 4 列；760px 档舞台只有约 268px，四枚完整胶囊
    // 放不下时自动换行成 2×2，而不是像横向 Box 那样被窗口边缘裁切。
    let pills_row = gtk::FlowBox::builder()
        .max_children_per_line(4)
        .min_children_per_line(1)
        .selection_mode(gtk::SelectionMode::None)
        .column_spacing(10)
        .row_spacing(8)
        .halign(gtk::Align::Center)
        .margin_bottom(44)
        .build();
    let (pill_client, dot_client, val_client) = make_pill("客户端 加载中…");
    let (pill_proc, dot_proc, val_proc) = make_pill("进程 加载中…");
    let (pill_service, dot_service, val_service) = make_pill("服务 加载中…");
    let (pill_nic, dot_nic, val_nic) = make_pill("网卡 加载中…");
    for pill in [&pill_client, &pill_proc, &pill_service, &pill_nic] {
        pills_row.append(pill);
    }
    stage.append(&pills_row);

    // ---- 窄屏顶部状态条：圆点 + 状态字 + 副行 + 网卡胶囊 ----
    let compact_status = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .css_classes(["compact-status"])
        .build();
    let compact_dot = gtk::Box::builder().build();
    compact_dot.set_size_request(10, 10);
    compact_dot.set_valign(gtk::Align::Center);
    compact_dot.add_css_class("pill-dot");
    compact_dot.add_css_class("dot-warn");
    compact_status.append(&compact_dot);
    let compact_state = gtk::Label::builder()
        .label("检测中…")
        .css_classes(["compact-state"])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(14)
        .build();
    compact_status.append(&compact_state);
    let compact_sub = gtk::Label::builder()
        .label("正在检测网络状态…")
        .css_classes(["compact-sub"])
        .hexpand(true)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(36)
        .build();
    compact_status.append(&compact_sub);

    // ---- 控制台：连接表单 + 动作（唯一玻璃容器）----
    let console = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .css_classes(["glass-card", "console"])
        .valign(gtk::Align::Fill)
        .build();
    console.set_size_request(CONSOLE_WIDTH, -1);

    let title = gtk::Label::builder()
        .label("校园有线网认证")
        .css_classes(["card-title"])
        .xalign(0.0)
        .build();
    console.append(&title);

    let subtitle = gtk::Label::builder()
        .label("樱花纷飞，校园网正等你连上")
        .css_classes(["card-subtitle"])
        .xalign(0.0)
        .build();
    console.append(&subtitle);

    // ---- 连接设置区：紧凑表单（账号 / 密码 / 网卡 / 开机认证）----
    // 表单直接平铺在玻璃卡上（不再嵌套小卡），区块用文字标题分隔。
    let form_title = gtk::Label::builder()
        .label("连接设置")
        .css_classes(["console-heading"])
        .xalign(0.0)
        .build();
    console.append(&form_title);

    let names = system::wired_interfaces();
    let list = names.iter().map(String::as_str).collect::<Vec<_>>();
    let nic_model = gtk::StringList::new(&list);
    let nic = gtk::DropDown::builder().model(&nic_model).build();
    set_pointer_cursor(&nic);
    let default_nic = settings.nic.clone();
    let index = names
        .iter()
        .position(|name| name == &default_nic)
        .unwrap_or(0);
    nic.set_selected(index as u32);

    let username_entry = gtk::Entry::builder()
        .placeholder_text("校园网账号")
        .text(&settings.username)
        .build();
    let password_entry = gtk::Entry::builder()
        .placeholder_text("密码（仅本次连接）")
        .visibility(false)
        .build();

    let username_row = form_line(ICON_USER, "账号", &username_entry);
    console.append(&username_row);
    let password_row = form_line(ICON_LOCK, "密码", &password_entry);
    console.append(&password_row);
    let nic_row = form_line(ICON_NETWORK, "网卡", &nic);
    console.append(&nic_row);
    let net_info = gtk::Label::builder()
        .label("检测中…")
        .css_classes(["net-info"])
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(48)
        .build();
    console.append(&net_info);

    let autostart = gtk::Switch::builder().active(false).build();
    // GTK 默认 valign=Fill：放在 ActionRow 的 suffix 里会被行高纵向拉伸
    // 成高瘦胶囊，必须显式居中让它保持原生横置比例
    autostart.set_valign(gtk::Align::Center);
    set_pointer_cursor(&autostart);
    let autostart_row = adw::ActionRow::builder()
        .title("开机自动认证")
        .subtitle("通过 systemd 服务在开机后自动连接")
        .build();
    autostart_row.add_prefix(&icon_image(ICON_ROUTER, 18));
    autostart_row.add_suffix(&autostart);
    // ActionRow 只能放进 GtkListBox；直接塞进 Box 会触发 libadwaita 的
    // "placed into GtkBox" 警告，并且行样式/键盘导航不完整。
    let autostart_list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["autostart-list"])
        .build();
    autostart_list.append(&autostart_row);
    console.append(&autostart_list);

    // ---- 主要操作按钮行：连接（主）/ 断开（次）----
    let connect = button_with_icon("连接校园网", ICON_CONNECT);
    connect.add_css_class("suggested-action");
    connect.add_css_class("btn-glow");
    let disconnect = button_with_icon("断开", ICON_DISCONNECT);
    disconnect.add_css_class("btn-ghost");
    set_pointer_cursor(&connect);
    set_pointer_cursor(&disconnect);
    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .css_classes(["actions-row"])
        .build();
    actions.append(&connect);
    actions.append(&disconnect);
    console.append(&actions);

    // 安装官方客户端（缺省 / 需迁移时使用）
    let install = button_with_icon("安装官方客户端", ICON_INSTALL);
    install.add_css_class("btn-ghost");
    set_pointer_cursor(&install);
    let install_wrap = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .halign(gtk::Align::Center)
        .build();
    install_wrap.append(&install);
    console.append(&install_wrap);

    // ---- 日志区块：标题 + 最近日志预览 + 完整日志行（实时日志收进「更多工具」浮层）----
    let log_title = gtk::Label::builder()
        .label("日志")
        .css_classes(["console-heading"])
        .xalign(0.0)
        .margin_top(2)
        .build();
    console.append(&log_title);
    let log_preview = gtk::Label::builder()
        .label("暂无日志")
        .css_classes(["log-preview"])
        .xalign(0.0)
        .wrap(true)
        .lines(LOG_PREVIEW_LINES as i32)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(72)
        .build();
    console.append(&log_preview);

    // ---- 连接详情区：认证账号 / 连接状态 / 最近认证结果（动态真实信息）----
    let conn_title = gtk::Label::builder()
        .label("连接详情")
        .css_classes(["console-heading"])
        .xalign(0.0)
        .margin_top(10)
        .build();
    console.append(&conn_title);
    let detail_account = gtk::Label::builder()
        .label("认证账号 -")
        .css_classes(["conn-row"])
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(44)
        .build();
    console.append(&detail_account);
    let detail_state = gtk::Label::builder()
        .label("连接状态 检测中…")
        .css_classes(["conn-row"])
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(44)
        .build();
    console.append(&detail_state);
    let detail_last = gtk::Label::builder()
        .label("最近认证 暂无记录")
        .css_classes(["conn-row"])
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(44)
        .build();
    console.append(&detail_last);

    // ---- 组合：舞台 + 控制台（控制台固定宽靠右，舞台吃满剩余）----
    let row_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(24)
        .hexpand(true)
        .vexpand(true)
        .build();
    row_box.append(&stage);
    row_box.append(&console);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .halign(gtk::Align::Fill)
        .valign(gtk::Align::Fill)
        .build();
    content.set_margin_top(48);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_bottom(28);
    // 迁移提示横幅：默认隐藏，refresh_status 发现旧版客户端或
    // 不安全的开机认证服务配置时显示（旧 UI 的"顶部提示"在重设计后缺失，这里补回）
    let banner = adw::Banner::builder().revealed(false).build();
    content.append(&banner);
    content.append(&compact_status);
    content.append(&row_box);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&scene_widget));
    // 内容直接铺在场景上（不包 ScrolledWindow）：控制台内容已压缩到固定高度，
    // 窗口高度不足时由舞台吸收，避免出现滚动条。
    overlay.add_overlay(&content);
    // 标题栏浮在背景之上：headerbar 全透明，只留按钮与标题胶囊
    header.set_halign(gtk::Align::Fill);
    header.set_valign(gtk::Align::Start);
    overlay.add_overlay(&header);

    toasts.set_child(Some(&overlay));

    let ui = Ui {
        window: window.clone(),
        toasts: toasts.clone(),
        scene,
        link,
        stage,
        stage_big,
        stage_sub,
        pills: vec![
            (dot_client, val_client),
            (dot_proc, val_proc),
            (dot_service, val_service),
            (dot_nic, val_nic),
        ],
        pills_flow: pills_row.clone(),
        compact_status,
        compact_dot,
        compact_state,
        compact_sub,
        console,
        nic,
        nic_model,
        username: username_entry,
        password: password_entry,
        connect,
        disconnect,
        install,
        autostart,
        refresh,
        more,
        more_popover,
        dhcp,
        save_password,
        diag: diag_rows,
        log_preview,
        net_info,
        detail_account,
        detail_state,
        detail_last,
        last_auth: Arc::new(std::sync::Mutex::new(None)),
        banner_action: Arc::new(std::sync::Mutex::new(BannerAction::None)),
        banner,
        busy: Arc::new(AtomicBool::new(false)),
        refreshing: Arc::new(AtomicBool::new(false)),
        refresh_generation: Arc::new(AtomicU64::new(0)),
        has_status: Arc::new(AtomicBool::new(false)),
        client_missing: Arc::new(AtomicBool::new(false)),
        autostart_syncing: Arc::new(AtomicBool::new(false)),
        stage_hold: Arc::new(std::sync::Mutex::new(None)),
    };

    wire_events(&ui);

    // 断点：监听场景层实际分配宽度（覆盖 Niri 预设列宽变化）
    let breakpoint_ui = ui.clone();
    scene_widget.connect_resize(move |_, width, _| {
        breakpoint_ui.apply_breakpoint(width);
    });
    ui.apply_breakpoint(window.width());

    ui
}

/// 加载主题样式：樱花基调 + 舞台透明 + 控制台玻璃 + 实色文字。
fn load_theme_css() {
    let css = r#"
    /* ---- 状态色令牌：ok=健康 / warn=待确认（黄）/ off=中性未知 ---- */
    @define-color ok #FF6F91;
    @define-color warn #B7791F;
    @define-color off #8C7285;
    @define-color bad #C63F38;

    /* ---- 文字实色系：深紫主 / 紫灰次 / 浅灰禁，不用低透明度叠加 ---- */
    window {
        background-color: #cfa4b6;
        box-shadow: none;
        border: none;
    }
    /* GTK CSD 阴影 margin：libadwaita 默认在 window.csd 上有 box-shadow，
       它会向窗口 buffer 外扩 ~15px 的阴影区域，在浅色壁纸上显示成一圈
       浅色带（用户报告的"白色越界"）。用高优先级覆盖为 none。
       注意还要覆盖：
       - outline（window.csd 默认 1px 白色描边）
       - .tiled 变体（niri 平铺窗口触发，默认 1px color-mix 边框） */
    window.csd,
    window.csd:backdrop,
    window.csd.tiled,
    window.csd.tiled-top,
    window.csd.tiled-left,
    window.csd.tiled-right,
    window.csd.tiled-bottom,
    window.csd.maximized,
    window.csd.fullscreen {
        box-shadow: none;
        border: none;
        outline: none;
    }

    /* ---- 标题栏：全透明浮层，背景图贯穿到窗口顶部；标题做成胶囊徽章 ----
       注意不能只写 headerbar：libadwaita 的 AdwHeaderBar 真正刷底色的是
       windowhandle 子节点，漏掉它会被用户主题的 headerbar > windowhandle 补成实色。 */
    headerbar,
    headerbar > windowhandle {
        background-color: transparent;
        background-image: none;
        box-shadow: none;
        border: none;
        color: #4A3048;
        min-height: 42px;
        padding: 0 8px;
    }
    headerbar label {
        color: #FFFFFF;
        font-weight: 700;
        background-color: alpha(#4A3048, 0.72);
        border-radius: 999px;
        padding: 3px 16px;
        margin-top: 4px;
        box-shadow: 0 1px 6px alpha(#2d1420, 0.25);
    }
    headerbar button {
        background-color: alpha(#ffffff, 0.60);
        color: #4A3048;
        border-radius: 999px;
        border: 1px solid alpha(#ffffff, 0.85);
        min-width: 32px;
        min-height: 32px;
        padding: 0;
        transition: all 180ms ease-out;
    }
    headerbar button:hover {
        background-color: alpha(#ffffff, 0.8);
    }
    headerbar button:active {
        background-color: alpha(#ffffff, 0.65);
    }
    headerbar button image {
        color: #4A3048;
    }
    headerbar button:backdrop,
    headerbar:backdrop {
        background-color: transparent;
        background-image: none;
        color: #4A3048;
    }

    /* ---- 控制台：唯一玻璃容器，让场景舞台大面积透出 ---- */
    .glass-card {
        background-image: linear-gradient(
            165deg,
            alpha(#ffffff, 0.48),
            alpha(#fff1f6, 0.56) 55%,
            alpha(#fde7f0, 0.64)
        );
        border-radius: 28px;
        border: 1px solid alpha(#ffffff, 0.55);
        box-shadow:
            0 12px 32px alpha(#502846, 0.16),
            inset 0 1px 0 alpha(#ffffff, 0.95);
        padding: 16px 28px;
    }

    /* 窄列实底变体：控制台铺满窗口时提高不透明度，保证文字可读 */
    .glass-card.solid {
        background-image: linear-gradient(
            165deg,
            alpha(#ffffff, 0.72),
            alpha(#fff3f8, 0.82) 55%,
            alpha(#fde9f2, 0.88)
        );
        border-color: alpha(#ffffff, 0.9);
    }

    /* 控制台内部区块文字标题：深紫玻璃胶囊托底 + 白字（与舞台大字同一招式） */
    .console-heading {
        font-size: 9.5pt;
        font-weight: 700;
        color: #FFFFFF;
        background-color: alpha(#4A3048, 0.70);
        border-radius: 999px;
        padding: 2px 14px;
        margin-top: 2px;
    }

    .card-title {
        font-size: 20pt;
        font-weight: 700;
        color: #3A2438;
    }

    .card-subtitle {
        font-size: 10pt;
        color: #FFFFFF;
        background-color: alpha(#6E4A5E, 0.66);
        border-radius: 999px;
        padding: 2px 12px;
        margin-bottom: 3px;
    }

    /* ---- 舞台：透明容器，场景直接透出（无卡片背景）---- */
    .stage {
        background-color: transparent;
    }
    .stage.narrow-stage .stage-big {
        font-size: 20pt;
        padding: 6px 20px;
    }
    .stage.narrow-stage .stage-sub {
        font-size: 9.5pt;
    }

    /* 大状态字：舞台主视觉（宽屏 26pt / 窄屏状态条 13pt 各有一份）。
       深紫玻璃胶囊托底 + 白字：在亮樱花背景上保持高对比，不依赖投影。 */
    .stage-big {
        font-size: 26pt;
        font-weight: 800;
        color: #FFFFFF;
        background-color: alpha(#4A3048, 0.70);
        border-radius: 999px;
        padding: 8px 30px;
        box-shadow: 0 2px 12px alpha(#2d1420, 0.25);
    }
    .stage-big.stat-ok,
    .stage-big.stat-success {
        color: #FFE0EC;
    }
    .stage-big.stat-busy {
        color: #FFE7F0;
    }
    .stage-big.stat-warn {
        color: #FFE2A8;
    }
    .stage-big.stat-off {
        color: #F1DDE8;
    }
    .stage-big.stat-failed {
        color: #FFC9BE;
    }
    .stage-sub {
        font-size: 11pt;
        color: #FFFFFF;
        background-color: alpha(#6E4A5E, 0.65);
        border-radius: 999px;
        padding: 2px 14px;
    }

    /* 状态胶囊：圆角胶囊 + 状态圆点（客户端/进程/服务/网卡） */
    .pill {
        background-color: alpha(#ffffff, 0.68);
        border: 1px solid alpha(#ffffff, 0.82);
        border-radius: 999px;
        padding: 5px 14px;
        box-shadow:
            inset 0 1px 0 alpha(#ffffff, 0.9),
            0 2px 10px alpha(#502846, 0.12);
    }
    .pill label {
        font-size: 9.5pt;
        font-weight: 600;
        color: #4A3048;
    }
    .pill-dot {
        border-radius: 999px;
    }
    flowbox,
    flowboxchild {
        background-color: transparent;
    }
    flowboxchild {
        padding: 0;
        outline: none;
    }
    .pill-dot.dot-ok {
        background-color: @ok;
        box-shadow: 0 0 6px alpha(@ok, 0.7);
    }
    .pill-dot.dot-busy {
        background-color: #D98AA6;
        box-shadow: 0 0 8px alpha(#D98AA6, 0.65);
    }
    .pill-dot.dot-warn {
        background-color: @warn;
        box-shadow: 0 0 6px alpha(@warn, 0.55);
    }
    .pill-dot.dot-off {
        background-color: @off;
        box-shadow: 0 0 4px alpha(@off, 0.35);
    }
    .pill-dot.dot-failed {
        background-color: @bad;
        box-shadow: 0 0 6px alpha(@bad, 0.5);
    }

    /* ---- 窄屏顶部状态条：玻璃胶囊，状态一目了然 ---- */
    .compact-status {
        background-color: alpha(#ffffff, 0.68);
        border: 1px solid alpha(#ffffff, 0.82);
        border-radius: 18px;
        padding: 8px 16px;
        box-shadow:
            inset 0 1px 0 alpha(#ffffff, 0.9),
            0 4px 16px alpha(#502846, 0.12);
    }
    .compact-state {
        font-size: 13pt;
        font-weight: 700;
        color: #4A3048;
    }
    .compact-state.stat-ok,
    .compact-state.stat-success {
        color: #C14D7C;
    }
    .compact-state.stat-busy {
        color: #8C4A6B;
    }
    .compact-state.stat-warn {
        color: @warn;
    }
    .compact-state.stat-off {
        color: @off;
    }
    .compact-state.stat-failed {
        color: @bad;
    }
    .compact-sub {
        font-size: 9.5pt;
        color: #6E5568;
    }

    /* ---- 表单行：图标 + 固定宽 label + 输入框，两列对齐 ---- */
    .form-line {
        padding: 1px 2px;
    }
    /* 网络详情小字：网卡行下方的 IP/网关信息 */
    .net-info {
        font-size: 8.5pt;
        color: #6E5568;
        /* 与表单输入框左边缘对齐：图标 18 + 间距 10 + label 64 + 间距 10 + 行内边距 2 */
        padding: 0 2px 3px 104px;
    }
    /* 连接详情行：账号 / 状态 / 最近认证结果（标签与值由 markup 分色） */
    .conn-row {
        font-size: 10pt;
        padding: 2px 2px;
    }
    .form-label {
        font-size: 10pt;
        font-weight: 600;
        color: #FFFFFF;
        background-color: alpha(#4A3048, 0.68);
        border-radius: 999px;
        padding: 2px 12px;
    }

    /* ---- 控制台内 ActionRow 文字：玻璃上必须用深紫 ---- */

    /* 控制台行压缩：全部行在 820px 高度内完整放下（固定布局，无滚动） */
    .console .autostart-list {
        background: transparent;
        border: none;
        box-shadow: none;
        padding: 0;
    }
    .console row {
        min-height: 26px;
        padding: 1px 8px;
    }
    .console row .title {
        color: #2E1C26;
        font-weight: 600;
        font-size: 9.5pt;
    }
    .console row .subtitle {
        color: #7A5A70;
        font-size: 8pt;
    }
    /* 最近日志预览：等宽小字，玻璃托底，随状态实时刷新 */
    .log-preview {
        font-size: 8.5pt;
        font-family: monospace;
        color: #3E2B3A;
        background-color: alpha(#ffffff, 0.55);
        border-radius: 10px;
        padding: 8px 12px;
    }

    /* ---- 主按钮：暖粉渐变 + 浮起/按压反馈 ---- */
    button.btn-glow {
        background-image: linear-gradient(135deg, #FF8FB8, #FF6F91);
        color: #4A1F30;
        font-weight: 700;
        border-radius: 18px;
        padding: 8px 20px;
        border: none;
        box-shadow:
            0 6px 22px alpha(#FF6F91, 0.40),
            inset 0 1px 0 alpha(#ffffff, 0.55);
        transition: all 180ms ease-out;
    }
    button.btn-glow:hover {
        box-shadow:
            0 10px 32px alpha(#FF8FB8, 0.55),
            inset 0 1px 0 alpha(#ffffff, 0.65);
    }
    button.btn-glow:active {
        box-shadow:
            0 2px 8px alpha(#f2688f, 0.4),
            inset 0 3px 8px alpha(#7a2c4a, 0.30);
        background-image: linear-gradient(135deg, #f47fa8, #ef6487);
    }

    /* ---- 次级按钮：白玻璃幽灵 + 同样浮起/按压 ---- */
    button.btn-ghost {
        background-color: alpha(#ffffff, 0.38);
        color: #4A3048;
        border-radius: 18px;
        padding: 6px 16px;
        border: 1px solid alpha(#ffffff, 0.6);
        box-shadow: inset 0 1px 0 alpha(#ffffff, 0.7);
        transition: all 180ms ease-out;
    }
    button.btn-ghost:hover {
        background-color: alpha(#ffffff, 0.60);
        box-shadow: 0 8px 24px alpha(#502846, 0.18);
    }
    button.btn-ghost:active {
        background-color: alpha(#ffe3ee, 0.70);
        box-shadow:
            0 2px 6px alpha(#502846, 0.12),
            inset 0 2px 6px alpha(#8a4a6b, 0.15);
    }
    button:focus-visible {
        outline: 2px solid alpha(#FFFFFF, 0.92);
        outline-offset: 2px;
    }
    button:disabled {
        opacity: 0.48;
    }
    button.btn-glow:disabled,
    button.btn-ghost:disabled {
        background-image: none;
        background-color: alpha(#ffffff, 0.24);
        color: alpha(#4A3048, 0.58);
        border-color: alpha(#ffffff, 0.35);
        box-shadow: none;
    }

    /* ---- 浮层（popover）：深紫玻璃 + 浅色字，与文字胶囊同一套语言 ----
       白玻璃浅底让行文字在亮背景上发飘（用户反馈"字体和阴影很奇怪"）。
       改为与舞台大字/区块标题一致的深紫半透明底 + 白字，阴影收敛
       为紧贴面板的下投影而不是四周光晕。
       注意 libadwaita 默认给 popover > contents 也加了 1px 黑边框和
       双层 box-shadow（RGB(0 0 0/14%) + 黑影），必须在这里一并清掉。 */
    popover {
        background-color: alpha(#4A3048, 0.92);
        border: 1px solid alpha(#ffd3e2, 0.30);
        border-radius: 16px;
        box-shadow: 0 6px 20px alpha(#2d1420, 0.38);
    }
    popover > contents {
        padding: 6px;
        background-color: alpha(#4A3048, 0.92);
        border: none;
        box-shadow: none;
        border-radius: 16px;
    }
    popover arrow {
        background-color: alpha(#4A3048, 0.92);
    }
    popover list {
        background-color: transparent;
    }
    popover row {
        border-radius: 12px;
        padding: 4px 8px;
        color: #FFFFFF;
    }
    popover row:hover {
        background-color: alpha(#ffffff, 0.10);
    }
    popover row .title {
        color: #FFFFFF;
    }
    popover row .subtitle {
        color: #E8C9DC;
    }
    popover preferencesgroup label.title {
        color: #FFD3E2;
    }
    popover preferencesgroup label.description {
        color: #D9B4CA;
    }

    /* ---- 下拉菜单：浅色面板 + 深紫文字（修复 ForceDark 下选项发白）---- */
    dropdown {
        background-color: alpha(#ffffff, 0.78);
        color: #4A3048;
        border: 1px solid alpha(#9a6a92, 0.55);
        border-radius: 10px;
        padding: 4px 10px;
        box-shadow: inset 0 1px 2px alpha(#8a4a6b, 0.10);
    }
    dropdown:focus,
    dropdown:focus-within {
        border-color: alpha(#FF6F91, 0.65);
        box-shadow:
            0 0 0 2px alpha(#FF6F91, 0.20),
            inset 0 1px 2px alpha(#8a4a6b, 0.08);
    }
    dropdown button {
        color: #4A3048;
        background-color: transparent;
    }
    dropdown button label {
        color: #4A3048;
    }
    dropdown button image {
        color: #8C7285;
    }
    /* 下拉弹出列表：白底深字，hover 粉底 */
    popover listview {
        background-color: alpha(#ffffff, 0.98);
        border-radius: 12px;
    }
    popover listview row {
        color: #4A3048;
        background-color: transparent;
        border-radius: 8px;
        padding: 6px 10px;
    }
    popover listview row:hover,
    popover listview row:selected {
        color: #4A3048;
        background-color: alpha(#ffd3e2, 0.7);
    }
    popover listview row label {
        color: #4A3048;
    }

    entry {
        background-color: alpha(#ffffff, 0.78);
        color: #4A3048;
        border: 1px solid alpha(#9a6a92, 0.55);
        border-radius: 10px;
        padding: 4px 10px;
        box-shadow: inset 0 1px 2px alpha(#8a4a6b, 0.10);
    }
    entry:focus {
        border-color: alpha(#FF6F91, 0.65);
        box-shadow:
            0 0 0 2px alpha(#FF6F91, 0.20),
            inset 0 1px 2px alpha(#8a4a6b, 0.08);
    }
    entry placeholder {
        color: #B8A6B2;
    }

    /* 控制台内的占位符：在较透的玻璃层上，用暖紫灰实色保证可读（用户批判过淡） */
    .glass-card entry placeholder {
        color: #7A6373;
    }

    switch:checked {
        background-color: @ok;
        border-color: alpha(@ok, 0.7);
    }
    switch {
        background-color: alpha(#9a6a92, 0.35);
        border: 1px solid alpha(#b98fb0, 0.45);
        transition: background-color 160ms ease-out, border-color 160ms ease-out;
    }
    switch:hover {
        background-color: alpha(#b98fb0, 0.55);
        border-color: alpha(#d6aecb, 0.65);
    }
    switch:checked:hover {
        background-color: #FF85A3;
        border-color: alpha(#FF85A3, 0.8);
    }


    /* ---- 滚动容器透明化，保持樱花场景透出 ---- */
    scrolledwindow {
        background-color: transparent;
    }
    scrolledwindow viewport {
        background-color: transparent;
    }

    "#;
    let Some(display) = gtk::gdk::Display::default() else {
        eprintln!("ui: 没有可用的 GDK Display，跳过主题加载");
        return;
    };
    let provider = gtk::CssProvider::new();
    // gtk4-rs 0.11 没有暴露会返回解析错误的 CSS 构造入口，解析失败只会以 GLib
    // 告警写到 stderr。这里不静默处理：启动冒烟要求 stderr 为空，任何 CSS 语法
    // 错误都会以 `Gtk-WARNING ... parser error` 立刻暴露出来。
    provider.load_from_string(css);
    // 本窗口是全自绘皮肤，不跟随系统 GTK 主题：~/.config/gtk-4.0/gtk.css
    // （Noctalia 主题桥）以 USER(800) 优先级注入，会覆盖 APPLICATION(600)
    // 的透明 headerbar 与玻璃输入框，把标题栏/entry 涂成 @headerbar_bg_color
    // （实测 #1a1111），整窗变成"黑标题栏 + 浅色卡片"的割裂样式。
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
    );
}

fn set_busy(ui: &Ui, busy: bool) {
    ui.busy.store(busy, Ordering::Relaxed);
    apply_control_state(ui);
}

/// 统一计算控件可用性：busy / 刷新中 / 客户端缺失三个约束一起生效。
/// 刷新中只禁用自启开关（它会在结果落地时被程序化 set_active），其余操作
/// 仍可继续；不能用一个静默 return 假装开关可用。
fn apply_control_state(ui: &Ui) {
    let busy = ui.busy.load(Ordering::Relaxed);
    let refreshing = ui.refreshing.load(Ordering::Relaxed);
    let missing = ui.client_missing.load(Ordering::Relaxed);
    ui.connect.set_sensitive(!busy && !missing);
    ui.disconnect.set_sensitive(!busy && !missing);
    ui.install.set_sensitive(!busy);
    ui.refresh.set_sensitive(!busy);
    ui.autostart.set_sensitive(!busy && !refreshing && !missing);
    ui.more.set_sensitive(!busy);
    ui.banner.set_sensitive(!busy);
    ui.nic.set_sensitive(!busy);
    ui.username.set_sensitive(!busy);
    ui.password.set_sensitive(!busy);
    for row in &ui.diag {
        row.set_sensitive(!busy);
    }
}

/// 客户端缺失时把安装按钮升为主操作；已安装时恢复普通幽灵按钮。
fn apply_install_affordance(ui: &Ui, missing: bool) {
    ui.install.remove_css_class("suggested-action");
    ui.install.remove_css_class("btn-glow");
    ui.install.remove_css_class("btn-ghost");
    if missing {
        ui.install.add_css_class("suggested-action");
        ui.install.add_css_class("btn-glow");
        ui.install
            .set_tooltip_text(Some("安装官方客户端后才能连接校园网"));
    } else {
        ui.install.add_css_class("btn-ghost");
        ui.install.set_tooltip_text(None);
    }
}

/// 程序化同步开关位置。`autostart_syncing` 是同步重入保护：set_active 会
/// 立刻发出 notify，若没有这层标记，刷新/回滚把开关拨回 false 会被当成
/// 用户点击去 disable-service，反过来破坏服务状态。
fn set_autostart_silent(ui: &Ui, active: bool) {
    ui.autostart_syncing.store(true, Ordering::SeqCst);
    ui.autostart.set_active(active);
    ui.autostart_syncing.store(false, Ordering::SeqCst);
}

fn toast(ui: &Ui, text: &str) {
    ui.toasts.add_toast(adw::Toast::new(text));
}

/// 在后台线程执行后端调用 f，结束后切回主线程更新界面。
/// 成功/失败分别触发场景与链路动效。
/// `ok_message`：连接类动作传 `Some`（成功后大状态字显示该文案并触发成功霞光）；
/// 非连接类动作（安装客户端等）传 `None`（成功后回到平静场景并刷新真实状态，
/// 避免把"安装成功/自启开启"误报成"已连接"）。
fn run_backend<F>(ui: &Ui, busy_message: &str, ok_message: Option<&'static str>, f: F)
where
    F: FnOnce() -> anyhow::Result<()> + Send + 'static,
{
    if ui.busy.load(Ordering::Relaxed) {
        return;
    }
    set_busy(ui, true);
    toast(ui, busy_message);
    let connection_action = ok_message.is_some();
    if connection_action {
        // 连接一开始就进入明确的忙碌文案，不能继续显示上一次的"未连接"。
        ui.set_stage("认证中…", StageTone::Busy);
        ui.scene.set_mode(scene::Mode::Connecting);
        ui.link.set_mode(scene::Mode::Connecting);
    }

    // 阻塞调用（pkexec 等待、systemctl、官方客户端运行）放在独立线程执行：
    // glib::spawn_future 在主上下文线程上运行，阻塞体会冻结整个界面（实测：
    // 点击连接后 UI 卡死）。结果经 futures-channel 异步送回主循环，不阻塞 UI。
    let ui_done = ui.clone();
    glib::spawn_future_local(async move {
        let (tx, mut rx) = mpsc::unbounded();
        std::thread::spawn(move || {
            let _ = tx.unbounded_send(f());
        });
        let result = rx
            .next()
            .await
            .unwrap_or_else(|| Err(anyhow::anyhow!("后台任务异常退出")));
        match result {
            Err(err) if connection_action && system::is_auth_unconfirmed(&err) => {
                // 官方客户端没有协议级成功回调，超时/崩溃后无法断定失败：
                // 保持中性提示并立即刷新真实链路，绝不写"最近认证成功"。
                ui_done.scene.set_mode(scene::Mode::Idle);
                ui_done.link.set_mode(scene::Mode::Idle);
                ui_done.hold_stage("认证结果未确认", StageTone::Warn);
                toast(&ui_done, "认证结果未确认，正在刷新真实连接状态…");
            }
            Err(err) => {
                ui_done.scene.set_mode(scene::Mode::Failed);
                ui_done.link.set_mode(scene::Mode::Failed);
                if connection_action {
                    ui_done.hold_stage("连接失败", StageTone::Failed);
                    if let Ok(mut guard) = ui_done.last_auth.lock() {
                        *guard = Some(LastAuth {
                            ok: false,
                            summary: err.to_string(),
                            at: Instant::now(),
                        });
                    }
                }
                toast(&ui_done, &err.to_string());
            }
            Ok(()) => {
                if let Some(text) = ok_message {
                    ui_done.scene.set_mode(scene::Mode::Success);
                    ui_done.link.set_mode(scene::Mode::Success);
                    ui_done.hold_stage(text, StageTone::Success);
                    // 只有确认成功才写最近认证结果；未知结果走上面的分支。
                    if let Ok(mut guard) = ui_done.last_auth.lock() {
                        *guard = Some(LastAuth {
                            ok: true,
                            summary: String::new(),
                            at: Instant::now(),
                        });
                    }
                } else {
                    ui_done.scene.set_mode(scene::Mode::Idle);
                    ui_done.link.set_mode(scene::Mode::Idle);
                }
                toast(&ui_done, "完成");
            }
        }
        set_busy(&ui_done, false);
        // 无论成功/失败、无论连接类还是安装类动作，完成后都同步真实状态
        // （连接详情区、自启开关、服务胶囊实时反映）。必须先解除 busy，
        // refresh_status 才会把开关同步到服务实际状态。
        refresh_status(&ui_done);
    });
}

/// 诊断动作：后台执行后端调用，成功/失败 toast，不触发连接霞光。
fn run_diag<F>(ui: &Ui, busy_message: &str, ok_message: &str, f: F)
where
    F: FnOnce() -> anyhow::Result<()> + Send + 'static,
{
    if ui.busy.load(Ordering::Relaxed) {
        return;
    }
    set_busy(ui, true);
    toast(ui, busy_message);

    let ui_done = ui.clone();
    let ok_message = ok_message.to_string();
    glib::spawn_future_local(async move {
        let (tx, mut rx) = mpsc::unbounded();
        std::thread::spawn(move || {
            let _ = tx.unbounded_send(f());
        });
        let result = rx
            .next()
            .await
            .unwrap_or_else(|| Err(anyhow::anyhow!("后台任务异常退出")));
        let message = match result {
            Ok(()) => None,
            Err(err) if !err.to_string().is_empty() => Some(err.to_string()),
            Err(err) => Some(err.to_string()),
        };
        match message {
            Some(err) => {
                ui_done.scene.set_mode(scene::Mode::Failed);
                ui_done.link.set_mode(scene::Mode::Failed);
                toast(&ui_done, &err);
            }
            None => {
                ui_done.scene.set_mode(scene::Mode::Idle);
                ui_done.link.set_mode(scene::Mode::Idle);
                toast(&ui_done, &ok_message);
            }
        }
        set_busy(&ui_done, false);
        // 断开/诊断完成后同步真实状态（连接详情区实时反映）
        refresh_status(&ui_done);
    });
}

/// 开机自启开关动作：后台启用/禁用 systemd 服务。
/// 失败时回滚开关位置（服务实际状态未改变，避免开关状态与真实状态脱节）；
/// 成功后刷新真实状态（服务胶囊/开关同步）。
fn run_service_toggle(ui: &Ui, on: bool, settings: config::Settings) {
    if ui.busy.load(Ordering::Relaxed) || ui.refreshing.load(Ordering::Relaxed) {
        return;
    }
    set_busy(ui, true);
    toast(
        ui,
        if on {
            "正在启用开机认证…"
        } else {
            "正在关闭开机认证…"
        },
    );

    let ui_done = ui.clone();
    glib::spawn_future_local(async move {
        let (tx, mut rx) = mpsc::unbounded();
        std::thread::spawn(move || {
            let result = if on {
                system::enable_service(&settings)
            } else {
                system::disable_service()
            };
            let _ = tx.unbounded_send(result);
        });
        let result = rx
            .next()
            .await
            .unwrap_or_else(|| Err(anyhow::anyhow!("后台任务异常退出")));
        let message = match result {
            Ok(()) => None,
            Err(err) if !err.to_string().is_empty() => Some(err.to_string()),
            Err(err) => Some(err.to_string()),
        };
        match message {
            Some(err) => {
                // 失败：回滚开关。busy 与 autostart_syncing 双保险，回滚触发的
                // notify 绝不会被当成用户操作再次启用/禁用服务。
                set_autostart_silent(&ui_done, !on);
                ui_done.scene.set_mode(scene::Mode::Failed);
                ui_done.link.set_mode(scene::Mode::Failed);
                toast(&ui_done, &err);
            }
            None => {
                ui_done.scene.set_mode(scene::Mode::Idle);
                ui_done.link.set_mode(scene::Mode::Idle);
                toast(
                    &ui_done,
                    if on {
                        "开机认证已启用"
                    } else {
                        "开机认证已关闭"
                    },
                );
            }
        }
        set_busy(&ui_done, false);
        // 成功后同步真实状态（开关位置、服务胶囊）；失败后也刷新，反映服务未变的真相
        refresh_status(&ui_done);
    });
}

/// 断开：成功后回到平静场景，不触发霞光。
fn run_backend_quiet<F>(ui: &Ui, busy_message: &str, f: F)
where
    F: FnOnce() -> anyhow::Result<()> + Send + 'static,
{
    if ui.busy.load(Ordering::Relaxed) {
        return;
    }
    set_busy(ui, true);
    toast(ui, busy_message);

    let ui_done = ui.clone();
    glib::spawn_future_local(async move {
        let (tx, mut rx) = mpsc::unbounded();
        std::thread::spawn(move || {
            let _ = tx.unbounded_send(f());
        });
        let result = rx
            .next()
            .await
            .unwrap_or_else(|| Err(anyhow::anyhow!("后台任务异常退出")));
        let message = match result {
            Ok(()) => None,
            Err(err) if !err.to_string().is_empty() => Some(err.to_string()),
            Err(err) => Some(err.to_string()),
        };
        match message {
            Some(err) => {
                ui_done.scene.set_mode(scene::Mode::Failed);
                ui_done.link.set_mode(scene::Mode::Failed);
                toast(&ui_done, &err);
            }
            None => {
                ui_done.scene.set_mode(scene::Mode::Idle);
                ui_done.link.set_mode(scene::Mode::Idle);
                ui_done.set_stage("未连接", StageTone::Off);
                toast(&ui_done, "已断开");
            }
        }
        set_busy(&ui_done, false);
        // 断开完成后同步真实状态（连接详情区实时反映）
        refresh_status(&ui_done);
    });
}

/// 把会话内最近认证记录格式化为（文本, 语义色）：成功绿 / 失败红 / 无记录中性色。
fn last_auth_text(last: Option<&LastAuth>) -> (String, &'static str) {
    use std::time::Duration;
    let Some(record) = last else {
        return ("暂无记录".to_string(), "#6E5568");
    };
    let elapsed = record.at.elapsed();
    let ago = if elapsed < Duration::from_secs(60) {
        "刚刚".to_string()
    } else if elapsed < Duration::from_secs(3600) {
        format!("{} 分钟前", elapsed.as_secs() / 60)
    } else {
        format!("{} 小时前", elapsed.as_secs() / 3600)
    };
    if record.ok {
        (format!("成功（{ago}）"), "#2E8B57")
    } else {
        let summary: String = record.summary.chars().take(40).collect();
        (format!("失败：{summary}（{ago}）"), "#C63F38")
    }
}

/// 会话内没有记录（应用刚重启）时，用官方日志里的最近一次判定。
/// 日志格式：`2026-10-01 09:03:50 认证成功` / `... 认证失败您的账户已欠费`。
fn log_auth_text(record: Option<&(String, bool, String)>) -> (String, &'static str) {
    let Some((time, ok, detail)) = record else {
        return ("暂无记录".to_string(), "#6E5568");
    };
    let prefix = if time.is_empty() {
        String::new()
    } else {
        format!("{time} ")
    };
    if *ok {
        (format!("{prefix}认证成功"), "#2E8B57")
    } else {
        let detail: String = if detail.is_empty() {
            "认证失败".to_string()
        } else {
            detail.chars().take(30).collect()
        };
        (format!("{prefix}{detail}"), "#C63F38")
    }
}

/// 「日志」预览最多显示的行数。
const LOG_PREVIEW_LINES: usize = 4;
/// `system.rs` 拼接日志用的分段标题（拼接格式是契约，这里只解析，不改动）。
const LOG_CLIENT_HEADER: &str = "官方客户端日志";
const LOG_JOURNAL_HEADER: &str = "systemd 日志";

fn non_empty_trimmed(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

/// 按 `system.rs` 的拼接格式切出（客户端段, journal 段）。
/// 两段都有时带标题；只有单一来源时不带标题，此时整段当作客户端段返回，
/// 逐行清洗对两种来源都安全。
fn split_log_sections(text: &str) -> (Option<&str>, Option<&str>) {
    if let Some(pos) = text.find(LOG_CLIENT_HEADER) {
        let rest = &text[pos + LOG_CLIENT_HEADER.len()..];
        match rest.find(LOG_JOURNAL_HEADER) {
            Some(jpos) => (
                non_empty_trimmed(&rest[..jpos]),
                non_empty_trimmed(&rest[jpos + LOG_JOURNAL_HEADER.len()..]),
            ),
            None => (non_empty_trimmed(rest), None),
        }
    } else if let Some(pos) = text.find(LOG_JOURNAL_HEADER) {
        // 防御性分支：当前 system.rs 不会只拼 journal 标题。
        (
            None,
            non_empty_trimmed(&text[pos + LOG_JOURNAL_HEADER.len()..]),
        )
    } else {
        (non_empty_trimmed(text), None)
    }
}

/// 清洗一行日志：剥掉 journalctl short 格式的行首样板
/// （`10月 01 16:49:46 主机 systemd[1]: `），只留消息正文。
/// 前缀要求 `[` 与 `]:` 之间是纯数字 PID，避免误伤客户端日志里的 `[ERR]:`
/// 之类标记；空白行丢弃。
fn clean_log_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let body = match trimmed.find("]:") {
        Some(idx) => match trimmed[..idx].rfind('[') {
            Some(open)
                if !trimmed[open + 1..idx].is_empty()
                    && trimmed[open + 1..idx].bytes().all(|b| b.is_ascii_digit()) =>
            {
                trimmed[idx + 2..].trim_start_matches([' ', ':'])
            }
            // 不是 journal 前缀：客户端日志自带的时间戳不算样板，原样保留。
            _ => trimmed,
        },
        None => trimmed,
    };
    // 兜底：消息正文里若还嵌着 `systemd[1]:`，预览里也绝不能出现。
    let body = body.replace("systemd[1]:", "");
    let body = body.trim();
    if body.is_empty() {
        None
    } else {
        Some(body.to_string())
    }
}

/// 清洗后取最近 `max` 行；一行都不剩返回 `None`。
fn cleaned_log_tail(text: &str, max: usize) -> Option<String> {
    let lines = text.lines().filter_map(clean_log_line).collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    Some(lines[lines.len().saturating_sub(max)..].join("\n"))
}

/// 「日志」区块预览：优先官方客户端日志最近几行（信息密度高）；客户端日志
/// 为空时退回清洗掉 journal 样板的摘要；两段都空显示「暂无日志」。
/// 输出保证不超过 `LOG_PREVIEW_LINES` 行、不含 `systemd[1]:` 字样。
fn log_preview(text: &str) -> String {
    // system.rs 在两段都空时给的是「暂无日志。」，与本函数自己的占位统一。
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "暂无日志" || trimmed == "暂无日志。" {
        return "暂无日志".to_string();
    }
    let (client, journal) = split_log_sections(trimmed);
    client
        .and_then(|section| cleaned_log_tail(section, LOG_PREVIEW_LINES))
        .or_else(|| journal.and_then(|section| cleaned_log_tail(section, LOG_PREVIEW_LINES)))
        .unwrap_or_else(|| "暂无日志".to_string())
}

/// 打开官方客户端安装包选择器并执行安装（安装按钮与迁移横幅共用）。
fn open_install_dialog(ui: &Ui) {
    let dialog = gtk::FileDialog::builder()
        .title("选择官方客户端安装包 (.zip)")
        .build();
    let future = dialog.open_future(Some(&ui.window));
    let ui2 = ui.clone();
    glib::spawn_future_local(async move {
        let Ok(file) = future.await else {
            return; // 用户取消选择
        };
        let Some(path) = file.path() else {
            toast(&ui2, "安装包不在本地磁盘上，请先把它下载到本机再选择");
            return;
        };
        run_backend(&ui2, "正在安装客户端…", None, move || {
            system::install_official_client(&path)
        });
    });
}

/// 发起一次认证：按钮与账号/密码输入框的回车共用同一入口。
fn do_connect(ui: &Ui) {
    let settings_ui = ui.clone();
    let password = settings_ui.password.text().to_string();
    let settings = settings_ui.settings();
    if let Err(err) = config::validate(&settings) {
        toast(&settings_ui, &err.to_string());
        return;
    }
    if password.is_empty() {
        // 允许空密码：后端不传 -p，由官方客户端复用已保存密码（save_password=1 时）。
        toast(&settings_ui, "密码留空，将尝试复用已保存的密码");
    }
    let _ = config::save(&settings);
    run_backend(
        &settings_ui,
        "正在连接…",
        Some("认证成功"),
        move || system::authenticate(&settings, &password),
    );
}

fn wire_events(ui: &Ui) {
    let connect = ui.clone();
    ui.connect.connect_clicked(move |_| do_connect(&connect));

    // 键盘优先：在账号或密码框里按回车等同于点「连接校园网」。
    let password_enter = ui.clone();
    ui.password
        .connect_activate(move |_| do_connect(&password_enter));
    let username_enter = ui.clone();
    ui.username
        .connect_activate(move |_| do_connect(&username_enter));

    let disconnect = ui.clone();
    ui.disconnect.connect_clicked(move |_| {
        run_backend_quiet(&disconnect, "正在断开…", system::disconnect);
    });

    let install_ui = ui.clone();
    ui.install
        .connect_clicked(move |_| open_install_dialog(&install_ui));

    // 迁移/失败横幅按钮：按当前横幅状态分派（客户端迁移 → 安装选择器；
    // 服务停止重试 → 重启服务；纯提示 → 无按钮）。
    let banner_ui = ui.clone();
    ui.banner.connect_button_clicked(move |_| {
        let action = *banner_ui
            .banner_action
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match action {
            BannerAction::InstallClient => open_install_dialog(&banner_ui),
            BannerAction::RestartService => run_diag(
                &banner_ui,
                "正在重新尝试开机认证…",
                "已重新尝试开机认证",
                system::restart_service,
            ),
            BannerAction::None => {}
        }
    });

    let autostart_ui = ui.clone();
    ui.autostart.connect_active_notify(move |switch| {
        let ui = autostart_ui.clone();
        let on = switch.is_active();
        if ui.autostart_syncing.load(Ordering::Relaxed)
            || ui.busy.load(Ordering::Relaxed)
            || ui.refreshing.load(Ordering::Relaxed)
        {
            return;
        }
        let settings = ui.settings();
        if on {
            if !settings.save_password {
                set_autostart_silent(&ui, false);
                toast(
                    &ui,
                    "开机自动认证需要保存密码：unit 里没有口令，请先打开「保存密码」",
                );
                return;
            }
            match config::validate(&settings) {
                Ok(()) => run_service_toggle(&ui, true, settings),
                Err(err) => {
                    set_autostart_silent(&ui, false);
                    toast(&ui, &err.to_string());
                }
            }
        } else {
            run_service_toggle(&ui, false, settings);
        }
    });

    let refresh = ui.clone();
    ui.refresh
        .connect_clicked(move |_| refresh_status(&refresh));

    // ---- 渐进披露浮层交互 ----
    let more_ui = ui.clone();
    ui.more.connect_clicked(move |_| {
        more_ui.more_popover.popup();
    });

    // 设置开关：变更即时写入配置
    let dhcp_ui = ui.clone();
    ui.dhcp.connect_active_notify(move |_| {
        let ui = dhcp_ui.clone();
        match config::save(&ui.settings()) {
            Ok(()) => {}
            Err(err) => toast(&ui, &format!("设置未保存：{err}")),
        }
    });
    let save_ui = ui.clone();
    ui.save_password.connect_active_notify(move |_| {
        let ui = save_ui.clone();
        match config::save(&ui.settings()) {
            Ok(()) => {}
            Err(err) => toast(&ui, &format!("设置未保存：{err}")),
        }
    });

    // 诊断工具行：按顺序接线到已有后端能力
    let diag_ui = ui.clone();
    for (i, row) in ui.diag.iter().enumerate() {
        let ui = diag_ui.clone();
        let row = row.clone();
        row.connect_activated(move |_| {
            match i {
                0 => run_diag(
                    &ui,
                    "正在测试连通性…",
                    "网络连通正常",
                    system::test_connectivity,
                ),
                1 => run_diag(
                    &ui,
                    "正在重启开机认证…",
                    "已重启开机认证",
                    system::restart_service,
                ),
                2 => run_diag(
                    &ui,
                    "正在打开客户端目录…",
                    "已打开",
                    system::open_client_folder,
                ),
                3 => run_diag(
                    &ui,
                    "正在打开实时日志…",
                    "已打开日志终端",
                    system::open_live_log,
                ),
                4 => run_diag(&ui, "正在打开帮助页…", "已打开", system::open_help),
                _ => {}
            }
            ui.more_popover.popdown();
        });
    }
}

/// 一轮状态刷新在工作线程里算出的全部展示数据。
/// 用具名字段而不是 14 元组：刷新链路长，位置参数一旦错位很难发现。
struct StatusView {
    pills: Vec<(String, &'static str)>,
    preview: String,
    net_text: String,
    autostart_enabled: bool,
    /// 认证客户端进程是否在运行（Running 才算）。
    client_running: bool,
    /// 官方客户端是否已安装。
    client_installed: bool,
    /// 物理载波四态：Up/Down/Missing/Unknown。
    carrier: netcheck::CarrierState,
    /// 有线路径探测三态：Reachable/Unreachable/Unknown。
    probe: netcheck::ProbeOutcome,
    /// 探测 Unknown 时用于"网卡地址证据"的两个硬条件。
    has_ipv4: bool,
    has_default_route: bool,
    /// 副行用的详情文本（时长 / 会话保持 / 未连接原因 / 未知原因）。
    detail: String,
    active_nic: String,
    banner_show: bool,
    banner_title: String,
    banner_action: BannerAction,
    wifi_radio_enabled: Option<bool>,
    interfaces: Option<Vec<String>>,
    /// run.log 里的最近一次判定（会话内记录为空时回退展示）。
    last_auth_log: Option<(String, bool, String)>,
    /// 首轮加载尚未拿到真实状态。
    detecting: bool,
}

impl StatusView {
    fn loading() -> Self {
        Self {
            pills: vec![
                ("客户端 检测中…".to_string(), "dot-busy"),
                ("进程 检测中…".to_string(), "dot-busy"),
                ("服务 检测中…".to_string(), "dot-busy"),
                ("网卡 检测中…".to_string(), "dot-busy"),
            ],
            preview: "暂无日志".to_string(),
            net_text: "检测中…".to_string(),
            autostart_enabled: false,
            client_running: false,
            client_installed: false,
            carrier: netcheck::CarrierState::Unknown,
            probe: netcheck::ProbeOutcome::Unknown,
            has_ipv4: false,
            has_default_route: false,
            detail: "检测中…".to_string(),
            active_nic: String::new(),
            banner_show: false,
            banner_title: String::new(),
            banner_action: BannerAction::None,
            wifi_radio_enabled: None,
            interfaces: None,
            last_auth_log: None,
            detecting: true,
        }
    }

    fn link_state(&self) -> LinkState {
        classify_link(
            &self.carrier,
            &self.probe,
            self.client_running,
            self.has_ipv4,
            self.has_default_route,
        )
    }

    fn headline_and_tone(&self) -> (&'static str, StageTone) {
        if self.detecting {
            ("检测中…", StageTone::Busy)
        } else {
            let link = self.link_state();
            (link.headline(), link.tone())
        }
    }

    fn state_detail(&self) -> String {
        if self.detecting {
            "检测中…".to_string()
        } else {
            self.link_state().detail_text()
        }
    }
}

fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{} 分钟", secs / 60)
    } else {
        format!("{} 小时 {} 分", secs / 3600, (secs % 3600) / 60)
    }
}

/// 选择状态探测的网卡：优先用户当前选中且仍存在的网卡；其次第一块载波
/// 已连接的网卡；最后退回列表第一项。选中网卡被拔出/改名时不能继续拿它
/// 的旧名字去探测，否则会显示一个不存在的接口。
fn pick_active_nic(selected: &str, nics: &[String]) -> String {
    if !selected.is_empty() && nics.iter().any(|name| name == selected) {
        return selected.to_string();
    }
    nics.iter()
        .find(|name| matches!(netcheck::carrier_state(name), netcheck::CarrierState::Up))
        .cloned()
        .or_else(|| nics.first().cloned())
        .unwrap_or_default()
}

/// 定时轮询专用入口：上一轮还没回来就跳过本次，避免 `journalctl`/`systemctl`
/// 偶发卡住时 10 秒一次无限累积工作线程与子进程。
/// 手动刷新和动作完成后的刷新仍直接走 `refresh_status`——那是用户明确意图，
/// 不能被在途标记吞掉；万一某轮真的卡死，点刷新按钮仍能恢复。
fn refresh_status_polled(ui: &Ui) {
    if ui.refreshing.load(Ordering::Relaxed) {
        return;
    }
    refresh_status(ui);
}

/// 刷新状态：后台读 system 状态，完成后回主线程更新舞台大状态字、
/// 状态胶囊、日志预览与自启开关。
fn refresh_status(ui: &Ui) {
    let generation = ui.refresh_generation.fetch_add(1, Ordering::SeqCst) + 1;
    // 用户选中的网卡就是认证目标，状态探测必须优先用它；多网卡时不能
    // 因为另一块网卡先被枚举到就报错对象。控件只在主线程读，读到的值
    // 随闭包进入工作线程。
    let selected_nic = ui.selected_nic();
    let ui_done = ui.clone();
    ui.refreshing.store(true, Ordering::SeqCst);
    // 刷新期间自启开关明确置灰（而不是保持可点、回调里静默 return）。
    apply_control_state(ui);
    // 状态读取（journalctl/systemctl/ps/IP 命令）也是阻塞 I/O：放到工作线程，
    // 避免 10 秒轮询在主循环上造成界面卡顿；结果经 futures-channel 异步送回。
    glib::spawn_future_local(async move {
        let (tx, mut rx) = mpsc::unbounded();
        std::thread::spawn(move || {
            let status = system::load_status();
            // 每轮重新探测：网卡可能后接/消失（USB 网卡、驱动重载），旧实现把
            // 构建期的快照存进 Ui 后永不更新，新网卡既选不到也看不到状态。
            let nics = system::wired_interfaces();
            let active_nic = pick_active_nic(&selected_nic, &nics);
            let carrier = netcheck::carrier_state(&active_nic);
            // 客户端在运行时也要探测，才能区分"运行且可达"与"运行但未确认"；
            // probe_wired_path 对载波 Down/Missing 会直接短路，不会额外发包。
            let probe = netcheck::probe_wired_path(&active_nic);
            let has_ipv4 = !active_nic.is_empty() && system::interface_has_ipv4(&active_nic);
            let has_default_route =
                !active_nic.is_empty() && system::interface_has_default_route(&active_nic);

            let client_process = status.client_process;
            let client_running = matches!(&client_process, system::ClientProcessState::Running);
            let link = classify_link(
                &carrier,
                &probe,
                client_running,
                has_ipv4,
                has_default_route,
            );

            let client = if status.client_installed {
                ("客户端 已安装".to_string(), "dot-ok")
            } else {
                ("客户端 未安装".to_string(), "dot-failed")
            };
            let proc = match &client_process {
                system::ClientProcessState::Running => ("进程 运行中".to_string(), "dot-ok"),
                system::ClientProcessState::NotRunning => ("进程 未运行".to_string(), "dot-off"),
                system::ClientProcessState::Unknown => ("进程 未知".to_string(), "dot-off"),
            };
            // 服务 pill：按 systemd 实际状态分四态。inactive 是"已启用但当前没在
            // 运行"（ExecCondition 跳过、或已跑完退出），与 failed（停止重试）不同；
            // 旧实现把两者都写成"服务 异常"，在"网络其实可用"时严重误导。
            let service = if status.service_enabled == "enabled" {
                match status.service_active.as_str() {
                    "active" => ("服务 运行中".to_string(), "dot-ok"),
                    // ExecStartPost 里的 NM 恢复要睡 8 秒，这期间单元是 activating。
                    "activating" if status.service_sub == "auto-restart" => {
                        ("服务 等待重试".to_string(), "dot-warn")
                    }
                    "activating" => ("服务 启动中".to_string(), "dot-busy"),
                    "deactivating" => ("服务 停止中".to_string(), "dot-busy"),
                    "failed" => ("服务 已停止重试".to_string(), "dot-failed"),
                    "inactive" => ("服务 待命".to_string(), "dot-off"),
                    "unknown" => ("服务 未知".to_string(), "dot-off"),
                    other => (format!("服务 {other}"), "dot-off"),
                }
            } else if status.service_enabled == "unknown" {
                ("服务 未知".to_string(), "dot-off")
            } else if status.service_active == "active" {
                ("服务 运行中·未设开机".to_string(), "dot-ok")
            } else {
                ("服务 未启用".to_string(), "dot-off")
            };

            let mut any_carrier_up = false;
            let nic_text = nics
                .iter()
                .map(|name| match netcheck::carrier_state(name) {
                    netcheck::CarrierState::Up => {
                        any_carrier_up = true;
                        format!("{name} 已插线")
                    }
                    netcheck::CarrierState::Down => format!("{name} 无网线"),
                    netcheck::CarrierState::Missing => format!("{name} 不存在"),
                    netcheck::CarrierState::Unknown => format!("{name} 未知"),
                })
                .collect::<Vec<_>>()
                .join(" / ");
            let nic_pill = if nic_text.is_empty() {
                ("网卡 无网卡".to_string(), "dot-off")
            } else if any_carrier_up {
                ("网卡 已插线".to_string(), "dot-ok")
            } else if matches!(&carrier, netcheck::CarrierState::Down) {
                ("网卡 无网线".to_string(), "dot-off")
            } else {
                ("网卡 状态未知".to_string(), "dot-off")
            };

            // 预览区：优先客户端日志、回退清洗后的 journal 摘要；最多 4 行
            let preview = log_preview(&status.last_log);

            // 网络详情：当前网卡的 IP 与网关（认证成功后"拿到 IP 没"最直观）
            let net_text = if active_nic.is_empty() {
                "未获取到 IP".to_string()
            } else {
                match (
                    system::interface_ipv4(&active_nic),
                    system::interface_gateway(&active_nic),
                ) {
                    (Some(ip), Some(gw)) => format!("IP {ip} · 网关 {gw}"),
                    (Some(ip), None) => format!("IP {ip}"),
                    (None, _) => "未获取到 IP".to_string(),
                }
            };
            let detail = match (&link, status.client_uptime_seconds) {
                (LinkState::Connected { evidence }, Some(secs)) => {
                    format!("{} · {evidence}", format_uptime(secs))
                }
                (LinkState::Connected { evidence }, None) => evidence.clone(),
                (LinkState::RunningUnverified, Some(secs)) => {
                    format!("已运行 {} · 路径待确认", format_uptime(secs))
                }
                (LinkState::RunningUnverified, None) => "客户端运行中 · 路径待确认".to_string(),
                (LinkState::Disconnected { reason }, _) => reason.clone(),
                (LinkState::Unknown { reason }, _) => reason.clone(),
            };

            // 横幅优先级：客户端未安装 > 客户端迁移 > 服务不安全 > 服务模板过旧
            // > 服务停止重试。未安装时安装是唯一出路，banner 与主按钮都指向安装。
            let (banner_show, banner_title, banner_action) = if !status.client_installed {
                (
                    true,
                    "未安装官方客户端：请点击「安装客户端」选择官方 ZIP 安装包，安装完成后再连接"
                        .to_string(),
                    BannerAction::InstallClient,
                )
            } else if status.client_requires_migration {
                (
                    true,
                    "检测到旧版客户端，请重新安装官方客户端完成安全迁移".to_string(),
                    BannerAction::InstallClient,
                )
            } else if status.service_requires_migration {
                (
                    true,
                    "开机认证服务配置不安全，请关闭后重新开启开机认证完成迁移".to_string(),
                    BannerAction::None,
                )
            } else if status.service_template_outdated {
                (
                    true,
                    "开机认证服务模板已更新（新增「已联网时自动跳过」），请关闭后重新开启完成迁移"
                        .to_string(),
                    BannerAction::None,
                )
            } else if status.service_enabled == "enabled" && status.service_active == "failed" {
                (
                    true,
                    "开机认证服务已停止重试；可点击按钮重新尝试，或检查网线后手动连接".to_string(),
                    BannerAction::RestartService,
                )
            } else {
                (false, String::new(), BannerAction::None)
            };

            let view = StatusView {
                pills: vec![client, proc, service, nic_pill],
                preview,
                net_text,
                autostart_enabled: status.service_enabled == "enabled",
                client_running,
                client_installed: status.client_installed,
                carrier,
                probe,
                has_ipv4,
                has_default_route,
                detail,
                active_nic,
                banner_show,
                banner_title,
                banner_action,
                wifi_radio_enabled: status.wifi_radio_enabled,
                // Option：工作线程异常时必须是 None，不能把"没探到网卡"
                // 当成真实结果去清空用户的下拉框。
                interfaces: Some(nics),
                last_auth_log: system::last_auth_from_log(),
                detecting: false,
            };
            let _ = tx.unbounded_send(view);
        });
        let maybe_view = rx.next().await;

        // 慢的旧轮询结果直接丢弃：只有 generation 仍匹配才允许写界面，
        // 决不能让旧结果覆盖刚完成的新状态或清掉新一轮的 refreshing。
        if ui_done.refresh_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        let view = match maybe_view {
            Some(view) => view,
            None if !ui_done.has_status.load(Ordering::SeqCst) => StatusView::loading(),
            None => {
                // 已经展示过真实状态：保留上一帧，不把界面打回"检测中"。
                ui_done.refreshing.store(false, Ordering::SeqCst);
                apply_control_state(&ui_done);
                return;
            }
        };
        apply_status_view(&ui_done, &view);
        ui_done.has_status.store(true, Ordering::SeqCst);
        ui_done.refreshing.store(false, Ordering::SeqCst);
        apply_control_state(&ui_done);
    });
}

/// 把一轮状态结果落到控件上。所有文本更新集中在这里，方便保证舞台大字、
/// 窄屏状态条、胶囊与连接详情始终使用同一份 LinkState。
fn apply_status_view(ui: &Ui, view: &StatusView) {
    for ((dot, value), (text, cls)) in ui.pills.iter().zip(view.pills.iter()) {
        value.set_label(text);
        for c in ["dot-ok", "dot-busy", "dot-warn", "dot-off", "dot-failed"] {
            dot.remove_css_class(c);
        }
        dot.add_css_class(cls);
    }

    let (headline, tone) = view.headline_and_tone();
    if !ui.stage_hold_active() {
        ui.set_stage(headline, tone);
    }
    let mut sub = if view.active_nic.is_empty() {
        view.detail.clone()
    } else {
        format!("{} · {}", view.active_nic, view.detail)
    };
    if view.wifi_radio_enabled == Some(false) {
        sub.push_str(" · Wi-Fi 已禁用");
    }
    if view.detecting {
        ui.stage_sub.set_label("正在检测网络状态…");
        ui.compact_sub.set_label("正在检测网络状态…");
    } else {
        ui.stage_sub.set_label(&sub);
        ui.compact_sub.set_label(&sub);
    }
    ui.log_preview.set_label(&view.preview);
    ui.net_info.set_label(&view.net_text);

    // 连接详情区：账号 / 状态 / 最近认证结果（值按 LinkState 语义着色）
    let account = ui.username.text();
    ui.detail_account.set_markup(&format!(
        "<span color='#6E5568'>认证账号</span> <span color='#3E2B3A'>{}</span>",
        glib::markup_escape_text(&account)
    ));
    let state_text = view.state_detail();
    let state_color = if view.detecting {
        StageTone::Busy.detail_color()
    } else {
        view.link_state().tone().detail_color()
    };
    ui.detail_state.set_markup(&format!(
        "<span color='#6E5568'>连接状态</span> <span color='{state_color}'><b>{}</b></span>",
        glib::markup_escape_text(&state_text)
    ));

    let session = ui
        .last_auth
        .lock()
        .map(|guard| (*guard).clone())
        .unwrap_or_else(|poison| poison.into_inner().clone());
    let (last_text, last_color) = match session.as_ref() {
        Some(record) => last_auth_text(Some(record)),
        // 会话内没有记录（应用重启过）时回退到官方日志里的最近一次判定。
        None => log_auth_text(view.last_auth_log.as_ref()),
    };
    ui.detail_last.set_markup(&format!(
        "<span color='#6E5568'>最近认证</span> <span color='{last_color}'><b>{}</b></span>",
        glib::markup_escape_text(&last_text)
    ));

    // 横幅：安装引导 / 迁移提示 / 服务失败恢复；按钮动作按当前状态分派
    *ui.banner_action
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = view.banner_action;
    ui.banner.set_title(&view.banner_title);
    ui.banner.set_button_label(match view.banner_action {
        BannerAction::InstallClient => Some("安装客户端"),
        BannerAction::RestartService => Some("重新尝试"),
        BannerAction::None => None,
    });
    ui.banner.set_revealed(view.banner_show);

    let missing = !view.detecting && !view.client_installed;
    ui.client_missing.store(missing, Ordering::SeqCst);
    apply_install_affordance(ui, missing);
    if !ui.busy.load(Ordering::Relaxed) {
        set_autostart_silent(ui, view.autostart_enabled);
    }
    if let Some(names) = &view.interfaces {
        ui.sync_interfaces(names);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 官方客户端日志：`YYYY-MM-DD HH:MM:SS 正文`，最多取最近 4 行。
    const CLIENT_LOG: &str = "2026-10-01 14:00:00 开始认证\n\
        2026-10-01 14:00:01 发现接入设备\n\
        2026-10-01 14:00:02 认证成功\n\
        2026-10-01 14:00:03 获取到 IP 10.20.30.40\n\
        2026-10-01 14:00:10 心跳正常";
    /// journalctl -u 的真实 short 格式（无 TTY 时不带颜色）。
    const JOURNAL_LOG: &str = "10月 01 16:49:46 archlinux systemd[1]: Starting RJSchoolClient...\n\
        10月 01 16:49:46 archlinux systemd[1]: Started RJSchoolClient.\n\
        10月 01 16:49:47 archlinux rjsupplicant-helper[151664]: eno1 已可访问外网（认证会话保持），跳过开机认证";

    #[test]
    fn loading_never_claims_disconnected() {
        let view = StatusView::loading();
        assert_eq!(view.headline_and_tone().0, "检测中…");
        assert_eq!(view.state_detail(), "检测中…");
        assert!(!view.client_installed);
    }

    #[test]
    fn reachable_probe_is_connected_even_without_client() {
        let state = classify_link(
            &netcheck::CarrierState::Up,
            &netcheck::ProbeOutcome::Reachable,
            false,
            false,
            false,
        );
        assert!(matches!(state, LinkState::Connected { .. }));
        assert_eq!(state.headline(), "已连接");
    }

    #[test]
    fn carrier_down_is_disconnected_and_beats_probe() {
        let state = classify_link(
            &netcheck::CarrierState::Down,
            &netcheck::ProbeOutcome::Reachable,
            true,
            true,
            true,
        );
        assert!(matches!(state, LinkState::Disconnected { .. }));
        assert_eq!(state.headline(), "未连接");
    }

    #[test]
    fn running_client_with_unconfirmed_probe_is_yellow_not_disconnected() {
        let state = classify_link(
            &netcheck::CarrierState::Up,
            &netcheck::ProbeOutcome::Unknown,
            true,
            false,
            false,
        );
        assert_eq!(state, LinkState::RunningUnverified);
        assert_eq!(state.tone(), StageTone::Warn);
    }

    #[test]
    fn unknown_probe_without_client_is_neutral_unknown() {
        let state = classify_link(
            &netcheck::CarrierState::Unknown,
            &netcheck::ProbeOutcome::Unknown,
            false,
            false,
            false,
        );
        assert!(matches!(state, LinkState::Unknown { .. }));
        assert_eq!(state.headline(), "状态未知");
        assert_eq!(state.tone(), StageTone::Off);
    }

    #[test]
    fn unknown_probe_with_ipv4_and_default_route_is_connected() {
        let state = classify_link(
            &netcheck::CarrierState::Unknown,
            &netcheck::ProbeOutcome::Unknown,
            false,
            true,
            true,
        );
        assert!(matches!(state, LinkState::Connected { .. }));
    }

    #[test]
    fn unreachable_probe_without_client_is_disconnected() {
        let state = classify_link(
            &netcheck::CarrierState::Up,
            &netcheck::ProbeOutcome::Unreachable,
            false,
            true,
            true,
        );
        assert!(matches!(state, LinkState::Disconnected { .. }));
    }

    #[test]
    fn prefers_the_selected_nic_when_it_still_exists() {
        let nics = vec!["enp0s0".to_string(), "eno1".to_string()];
        assert_eq!(pick_active_nic("eno1", &nics), "eno1");
        assert_eq!(pick_active_nic("", &nics), pick_active_nic("", &nics));
        assert_eq!(pick_active_nic("", &[]), "");
    }

    #[test]
    fn preview_prefers_client_section_over_journal() {
        let text = format!("官方客户端日志\n{CLIENT_LOG}\n\nsystemd 日志\n{JOURNAL_LOG}");

        let preview = log_preview(&text);

        assert_eq!(
            preview,
            "2026-10-01 14:00:01 发现接入设备\n\
             2026-10-01 14:00:02 认证成功\n\
             2026-10-01 14:00:03 获取到 IP 10.20.30.40\n\
             2026-10-01 14:00:10 心跳正常"
        );
        assert!(!preview.contains("systemd[1]:"));
        assert!(preview.lines().count() <= 4);
    }

    #[test]
    fn preview_cleans_journal_when_client_log_missing() {
        // 客户端日志为空时 system.rs 直接返回 journal 原文（无标题）。
        let preview = log_preview(JOURNAL_LOG);

        assert_eq!(
            preview,
            "Starting RJSchoolClient...\n\
             Started RJSchoolClient.\n\
             eno1 已可访问外网（认证会话保持），跳过开机认证"
        );
        assert!(!preview.contains("systemd[1]:"));
        assert!(preview.lines().count() <= 4);
    }

    #[test]
    fn preview_falls_back_to_journal_when_client_section_is_blank() {
        let text = format!("官方客户端日志\n \n\nsystemd 日志\n{JOURNAL_LOG}");

        let preview = log_preview(&text);

        assert!(!preview.contains("systemd[1]:"));
        assert!(preview.contains("eno1 已可访问外网"));
    }

    #[test]
    fn preview_does_not_pad_short_client_section_with_journal() {
        let client = "2026-10-01 14:00:02 认证成功\n2026-10-01 14:00:10 心跳正常";
        let text = format!("官方客户端日志\n{client}\n\nsystemd 日志\n{JOURNAL_LOG}");

        assert_eq!(log_preview(&text), client);
    }

    #[test]
    fn preview_handles_journal_section_with_header() {
        let text = format!("systemd 日志\n{JOURNAL_LOG}");

        let preview = log_preview(&text);

        assert!(!preview.contains("systemd[1]:"));
        assert!(preview.contains("eno1 已可访问外网"));
    }

    #[test]
    fn preview_keeps_recent_client_lines_when_only_client_log_exists() {
        let preview = log_preview(CLIENT_LOG);

        assert_eq!(
            preview,
            "2026-10-01 14:00:01 发现接入设备\n\
             2026-10-01 14:00:02 认证成功\n\
             2026-10-01 14:00:03 获取到 IP 10.20.30.40\n\
             2026-10-01 14:00:10 心跳正常"
        );
        assert!(preview.lines().count() <= 4);
    }

    #[test]
    fn preview_keeps_client_level_tags_intact() {
        // `[ERR]` 不是 PID，不能被当成 journal 前缀剥掉。
        let text = "2026-10-01 14:00:00 [ERR]: 认证失败";

        assert_eq!(log_preview(text), text);
    }

    #[test]
    fn preview_falls_back_to_placeholder_when_logs_empty() {
        for text in [
            "",
            "   \n",
            "暂无日志。",
            "官方客户端日志\n\n\nsystemd 日志\n",
        ] {
            assert_eq!(log_preview(text), "暂无日志", "输入: {text:?}");
        }
    }

    #[test]
    fn preview_keeps_at_most_four_cleaned_lines() {
        let journal = (1..=6)
            .map(|i| format!("10月 01 16:49:4{i} archlinux rjsupplicant-helper[15{i}]: 第 {i} 条"))
            .collect::<Vec<_>>()
            .join("\n");

        let preview = log_preview(&journal);

        assert_eq!(preview, "第 3 条\n第 4 条\n第 5 条\n第 6 条");
    }

    #[test]
    fn preview_never_exposes_embedded_unit_boilerplate() {
        let text = "10月 01 16:49:46 archlinux systemd[1]: 见 systemd[1]: Failed to start 客户端";

        let preview = log_preview(text);

        assert!(!preview.contains("systemd[1]:"));
        assert!(preview.contains("Failed to start 客户端"));
    }
}
