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
use gtk4 as gtk;
use gtk4::glib;
use libadwaita as adw;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// 三档断点（以 Niri preset 宽度 640/960/1280 为基准，取中间值切档）。
/// STANDARD_MAX 原为 1080，但 niri 平铺下半屏窗口常见宽度 954-960，
/// 若断点高于此，双栏永远不会出现、内容全部竖排单卡（用户批判"空洞"）。
const COMPACT_MAX: i32 = 760;
const STANDARD_MAX: i32 = 940;

/// 控制台固定宽度：不随窗口拉宽，多余宽度全部交给场景舞台。
const CONSOLE_WIDTH: i32 = 420;

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
    /// 窄屏（<760）顶部紧凑状态条：圆点 + 状态字 + 副行 + 网卡胶囊。
    compact_status: gtk::Box,
    compact_dot: gtk::Box,
    compact_state: gtk::Label,
    compact_sub: gtk::Label,
    /// 右侧操作面板（玻璃卡，唯一容器）。
    console: gtk::Box,
    nic: gtk::DropDown,
    nic_model: gtk::StringList,
    nics: Arc<Vec<String>>,
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
    /// 完整日志浮层。
    log_popover: gtk::Popover,
    log_view: gtk::TextView,
    /// 诊断工具行：测试连通 / 重启 / 打开目录 / 实时日志 / 帮助
    diag: Vec<adw::ActionRow>,
    /// 最近日志预览（走 ActionRow subtitle，左对齐不飘）
    log_row: adw::ActionRow,
    /// 实时日志行：点击打开 journalctl 终端（复用 system::open_live_log）。
    live_log_row: adw::ActionRow,
    /// 迁移提示横幅：旧版客户端 / 不安全的开机认证服务配置时显示。
    banner: adw::Banner,
    busy: Arc<AtomicBool>,
    /// 刷新状态期间为 true：防止 set_active 触发的 notify 回调
    /// 误把"同步开关"当成用户操作去启停 systemd 服务。
    refreshing: Arc<AtomicBool>,
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

    /// 同步大状态字（舞台 + 窄屏状态条）与场景/链路模式。
    fn set_stage(&self, text: &str, ok: bool) {
        let cls = if ok { "stat-ok" } else { "stat-warn" };
        for l in [&self.stage_big, &self.compact_state] {
            l.set_label(text);
            l.remove_css_class("stat-ok");
            l.remove_css_class("stat-warn");
            l.add_css_class(cls);
        }
        self.compact_dot.remove_css_class("dot-ok");
        self.compact_dot.remove_css_class("dot-warn");
        self.compact_dot
            .add_css_class(if ok { "dot-ok" } else { "dot-warn" });
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
            self.scene
                .set_shift(if width < STANDARD_MAX { 0.5 } else { 0.58 });
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
const ICON_LOG: &[u8] = icon_png!("file-text");
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
    dot.add_css_class("dot-ok");
    let value = gtk::Label::builder().label(label).build();
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
    let ui = build_window(app);
    refresh_status(&ui);
    ui.window.present();
}

fn build_window(app: &adw::Application) -> Ui {
    load_theme_css();

    // 强制深色主题，避免原生控件亮色皮肤破坏樱花氛围
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let toasts = adw::ToastOverlay::new();
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("锐捷有线认证")
        .default_width(1280)
        .default_height(860)
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
        .active(config::load().dhcp)
        .valign(gtk::Align::Center)
        .build();
    let dhcp_row = adw::ActionRow::builder()
        .title("DHCP 自动获取 IP")
        .subtitle("由学校网络自动分配地址")
        .build();
    dhcp_row.add_suffix(&dhcp);
    settings_group.add(&dhcp_row);
    let save_password = gtk::Switch::builder()
        .active(config::load().save_password)
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
        .label("未连接")
        .css_classes(["stage-big"])
        .halign(gtk::Align::Center)
        .margin_top(64)
        .build();
    stage.append(&stage_big);

    let stage_sub = gtk::Label::builder()
        .label("输入账号密码，点击连接")
        .css_classes(["stage-sub"])
        .halign(gtk::Align::Center)
        .build();
    stage.append(&stage_sub);

    // 链路可视化：设备 ↔ 校园网关，随认证模式点亮（签名时刻）
    let link = scene::Link::new();
    let link_widget = link.widget().clone();
    link_widget.set_vexpand(true);
    link_widget.set_margin_top(16);
    link_widget.set_margin_bottom(16);
    stage.append(&link_widget);

    // 状态胶囊行：客户端 / 进程 / 服务 / 网卡
    let pills_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .halign(gtk::Align::Center)
        .margin_bottom(44)
        .build();
    let (pill_client, dot_client, val_client) = make_pill("客户端 加载中…");
    let (pill_proc, dot_proc, val_proc) = make_pill("进程 加载中…");
    let (pill_service, dot_service, val_service) = make_pill("服务 加载中…");
    let (pill_nic, dot_nic, val_nic) = make_pill("网卡 加载中…");
    pills_row.append(&pill_client);
    pills_row.append(&pill_proc);
    pills_row.append(&pill_service);
    pills_row.append(&pill_nic);
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
        .label("未连接")
        .css_classes(["compact-state"])
        .build();
    compact_status.append(&compact_state);
    let compact_sub = gtk::Label::builder()
        .label("")
        .css_classes(["compact-sub"])
        .build();
    compact_status.append(&compact_sub);

    // ---- 控制台：连接表单 + 动作（唯一玻璃容器）----
    let console = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
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
    let default_nic = config::load().nic;
    let index = names
        .iter()
        .position(|name| name == &default_nic)
        .unwrap_or(0);
    nic.set_selected(index as u32);

    let username_entry = gtk::Entry::builder()
        .placeholder_text("校园网账号")
        .text(&config::load().username)
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
    console.append(&autostart_row);

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

    // ---- 日志区块：标题 + 实时日志行 + 完整日志行 ----
    let log_title = gtk::Label::builder()
        .label("日志")
        .css_classes(["console-heading"])
        .xalign(0.0)
        .margin_top(2)
        .build();
    console.append(&log_title);
    let live_log_row = adw::ActionRow::builder()
        .title("打开实时日志")
        .subtitle("终端 journalctl -f")
        .activatable(true)
        .build();
    set_pointer_cursor(&live_log_row);
    let live_log_badge = gtk::Box::builder().css_classes(["row-badge"]).build();
    live_log_badge.append(&icon_image(ICON_TERMINAL, 16));
    live_log_row.add_prefix(&live_log_badge);
    console.append(&live_log_row);
    let log_row = adw::ActionRow::builder()
        .title("查看完整日志")
        .subtitle("暂无日志")
        .activatable(true)
        .build();
    set_pointer_cursor(&log_row);
    let log_badge = gtk::Box::builder().css_classes(["row-badge"]).build();
    log_badge.append(&icon_image(ICON_LOG, 16));
    log_row.add_prefix(&log_badge);
    console.append(&log_row);

    // ---- 诊断与工具：控制台底部常驻 5 行（压缩行高）----
    let diag_title = gtk::Label::builder()
        .label("诊断与工具")
        .css_classes(["console-heading"])
        .xalign(0.0)
        .margin_top(2)
        .build();
    console.append(&diag_title);
    let side_diag = make_diag_rows(&diag_actions);
    for row in &side_diag {
        console.append(row);
    }

    // 完整日志浮层：等宽文本，可滚动查看全部日志
    let log_view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .css_classes(["log-text"])
        .build();
    let log_scroll = gtk::ScrolledWindow::builder()
        .child(&log_view)
        .min_content_height(220)
        .max_content_height(320)
        .hexpand(true)
        .build();
    let log_popover = gtk::Popover::builder()
        .child(&log_scroll)
        .width_request(380)
        .build();
    log_popover.set_parent(&log_row);

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
        compact_status,
        compact_dot,
        compact_state,
        compact_sub,
        console,
        nic,
        nic_model,
        nics: Arc::new(names),
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
        log_popover,
        log_view,
        diag: diag_rows,
        log_row,
        live_log_row,
        banner,
        busy: Arc::new(AtomicBool::new(false)),
        refreshing: Arc::new(AtomicBool::new(false)),
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

    /* ---- 标题栏：全透明浮层，背景图贯穿到窗口顶部；标题做成胶囊徽章 ---- */
    headerbar {
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
        background-color: alpha(#ffffff, 0.45);
        color: #4A3048;
        border-radius: 999px;
        border: 1px solid alpha(#ffffff, 0.85);
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
            0 20px 60px alpha(#502846, 0.22),
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
        background-color: alpha(#4A3048, 0.55);
        border-radius: 999px;
        padding: 1px 14px;
        margin-top: 0;
    }

    .card-title {
        font-size: 19pt;
        font-weight: 700;
        color: #3A2438;
    }

    .card-subtitle {
        font-size: 10pt;
        color: #FFFFFF;
        background-color: alpha(#6E4A5E, 0.40);
        border-radius: 999px;
        padding: 2px 12px;
        margin-bottom: 2px;
    }

    /* ---- 舞台：透明容器，场景直接透出（无卡片背景）---- */
    .stage {
        background-color: transparent;
    }

    /* 大状态字：舞台主视觉（宽屏 26pt / 窄屏状态条 13pt 各有一份）。
       深紫玻璃胶囊托底 + 白字：在亮樱花背景上保持高对比，不依赖投影。 */
    .stage-big {
        font-size: 26pt;
        font-weight: 800;
        color: #FFFFFF;
        letter-spacing: 3px;
        background-color: alpha(#4A3048, 0.62);
        border-radius: 999px;
        padding: 8px 30px;
        box-shadow: 0 2px 12px alpha(#2d1420, 0.25);
    }
    .stage-big.stat-ok {
        color: #FFD3E2;
    }
    .stage-big.stat-warn {
        color: #FFC9BE;
    }
    .stage-sub {
        font-size: 11pt;
        color: #FFFFFF;
        background-color: alpha(#6E4A5E, 0.40);
        border-radius: 999px;
        padding: 2px 14px;
    }

    /* 状态胶囊：圆角胶囊 + 状态圆点（客户端/进程/服务/网卡） */
    .pill {
        background-color: alpha(#ffffff, 0.55);
        border: 1px solid alpha(#ffffff, 0.75);
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
    .pill-dot.dot-ok {
        background-color: #FF6F91;
        box-shadow: 0 0 6px alpha(#FF6F91, 0.7);
    }
    .pill-dot.dot-warn {
        background-color: #C63F38;
        box-shadow: 0 0 6px alpha(#C63F38, 0.5);
    }

    /* ---- 窄屏顶部状态条：玻璃胶囊，状态一目了然 ---- */
    .compact-status {
        background-color: alpha(#ffffff, 0.55);
        border: 1px solid alpha(#ffffff, 0.75);
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
    .compact-state.stat-ok {
        color: #C14D7C;
    }
    .compact-state.stat-warn {
        color: #C63F38;
    }
    .compact-sub {
        font-size: 9.5pt;
        color: #6E5568;
    }

    /* ---- 表单行：图标 + 固定宽 label + 输入框，两列对齐 ---- */
    .form-line {
        padding: 1px 2px;
    }
    .form-label {
        font-size: 10pt;
        font-weight: 600;
        color: #FFFFFF;
        background-color: alpha(#4A3048, 0.45);
        border-radius: 999px;
        padding: 2px 12px;
    }

    /* ---- 控制台内 ActionRow 文字：玻璃上必须用深紫 ---- */
    .console row .title {
        color: #2E1C26;
        font-weight: 600;
    }
    .console row .subtitle {
        color: #7A5A70;
    }

    /* 控制台行压缩：全部行在 820px 高度内完整放下（固定布局，无滚动） */
    .console row {
        min-height: 22px;
        padding: 1px 8px;
    }
    .console row .title {
        font-size: 9.5pt;
    }
    .console row .subtitle {
        font-size: 8pt;
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
    dropdown:focus {
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
        background-color: #FF6F91;
        border-color: alpha(#FF6F91, 0.7);
    }
    switch {
        background-color: alpha(#9a6a92, 0.35);
        border: 1px solid alpha(#b98fb0, 0.45);
    }

    /* 行前缀小圆底（日志等）：与状态胶囊同一视觉语言 */
    .row-badge {
        background-image: linear-gradient(160deg, alpha(#ffd3e2, 0.9), alpha(#ffb3cd, 0.7));
        border-radius: 999px;
        padding: 6px;
        box-shadow:
            inset 0 1px 0 alpha(#ffffff, 0.8),
            0 2px 8px alpha(#c14d7c, 0.15);
    }

    /* ---- 滚动容器透明化，保持樱花场景透出 ---- */
    scrolledwindow {
        background-color: transparent;
    }
    scrolledwindow viewport {
        background-color: transparent;
    }

    /* 完整日志：等宽深紫小字，可读性优先 */
    .log-text {
        font-family: "monospace";
        font-size: 9pt;
        color: #4A3048;
        background-color: alpha(#ffffff, 0.60);
        padding: 8px 10px;
        border-radius: 12px;
    }
    .log-text text {
        color: #4A3048;
    }
    "#;
    let provider = gtk::CssProvider::new();
    provider.load_from_string(css);
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn set_busy(ui: &Ui, busy: bool) {
    ui.busy.store(busy, Ordering::Relaxed);
    // 忙碌期间禁用全部交互控件：避免控件仍是可点外观（手状光标）却
    // 被 busy 保护吞掉点击，让用户误以为点击无效。
    ui.connect.set_sensitive(!busy);
    ui.disconnect.set_sensitive(!busy);
    ui.install.set_sensitive(!busy);
    ui.refresh.set_sensitive(!busy);
    ui.autostart.set_sensitive(!busy);
    ui.more.set_sensitive(!busy);
    ui.nic.set_sensitive(!busy);
    ui.username.set_sensitive(!busy);
    ui.password.set_sensitive(!busy);
    ui.live_log_row.set_sensitive(!busy);
    ui.log_row.set_sensitive(!busy);
    for row in &ui.diag {
        row.set_sensitive(!busy);
    }
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
    ui.scene.set_mode(scene::Mode::Connecting);
    ui.link.set_mode(scene::Mode::Connecting);

    let ui_done = ui.clone();
    glib::spawn_future_local(async move {
        let result = glib::spawn_future(async move { f() }).await;
        let message = match result {
            Ok(Ok(())) => None,
            Ok(Err(err)) if !err.to_string().is_empty() => Some(err.to_string()),
            Err(err) => Some(err.to_string()),
            _ => None,
        };
        match message {
            Some(err) => {
                ui_done.scene.set_mode(scene::Mode::Failed);
                ui_done.link.set_mode(scene::Mode::Failed);
                ui_done.set_stage("连接失败", false);
                toast(&ui_done, &err);
            }
            None => {
                if let Some(text) = ok_message {
                    ui_done.scene.set_mode(scene::Mode::Success);
                    ui_done.link.set_mode(scene::Mode::Success);
                    ui_done.set_stage(text, true);
                } else {
                    ui_done.scene.set_mode(scene::Mode::Idle);
                    ui_done.link.set_mode(scene::Mode::Idle);
                }
                toast(&ui_done, "完成");
            }
        }
        set_busy(&ui_done, false);
        // 非连接类动作成功后同步真实状态（含自启开关与服务胶囊）。
        // 必须先解除 busy，refresh_status 才会把开关同步到服务实际状态。
        if ok_message.is_none() {
            refresh_status(&ui_done);
        }
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
        let result = glib::spawn_future(async move { f() }).await;
        let message = match result {
            Ok(Ok(())) => None,
            Ok(Err(err)) if !err.to_string().is_empty() => Some(err.to_string()),
            Err(err) => Some(err.to_string()),
            _ => None,
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
        let result = glib::spawn_future(async move {
            if on {
                system::enable_service(&settings)
            } else {
                system::disable_service()
            }
        })
        .await;
        let message = match result {
            Ok(Ok(())) => None,
            Ok(Err(err)) if !err.to_string().is_empty() => Some(err.to_string()),
            Err(err) => Some(err.to_string()),
            _ => None,
        };
        match message {
            Some(err) => {
                // 失败：回滚开关（busy 为 true，回滚触发的 notify 会被 busy 保护跳过）
                ui_done.autostart.set_active(!on);
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
        let result = glib::spawn_future(async move { f() }).await;
        let message = match result {
            Ok(Ok(())) => None,
            Ok(Err(err)) if !err.to_string().is_empty() => Some(err.to_string()),
            Err(err) => Some(err.to_string()),
            _ => None,
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
                ui_done.set_stage("未连接", false);
                toast(&ui_done, "已断开");
            }
        }
        set_busy(&ui_done, false);
    });
}

/// 打开官方客户端安装包选择器并执行安装（安装按钮与迁移横幅共用）。
fn open_install_dialog(ui: &Ui) {
    let dialog = gtk::FileDialog::builder()
        .title("选择官方客户端安装包 (.zip)")
        .build();
    let future = dialog.open_future(Some(&ui.window));
    let ui2 = ui.clone();
    glib::spawn_future_local(async move {
        if let Ok(file) = future.await
            && let Some(path) = file.path()
        {
            run_backend(&ui2, "正在安装客户端…", None, move || {
                system::install_official_client(&path)
            });
        }
    });
}

fn wire_events(ui: &Ui) {
    let connect = ui.clone();
    ui.connect.connect_clicked(move |_| {
        let settings_ui = connect.clone();
        let password = settings_ui.password.text().to_string();
        let settings = settings_ui.settings();
        if let Err(err) = config::validate(&settings) {
            toast(&settings_ui, &err.to_string());
            return;
        }
        if password.is_empty() {
            toast(&settings_ui, "请输入密码");
            return;
        }
        let _ = config::save(&settings);
        run_backend(
            &settings_ui,
            "正在连接…",
            Some("已连接"),
            move || system::authenticate(&settings, &password),
        );
    });

    let disconnect = ui.clone();
    ui.disconnect.connect_clicked(move |_| {
        run_backend_quiet(&disconnect, "正在断开…", system::disconnect);
    });

    let install_ui = ui.clone();
    ui.install
        .connect_clicked(move |_| open_install_dialog(&install_ui));

    // 迁移横幅按钮：与安装按钮走同一选择器流程
    let banner_ui = ui.clone();
    ui.banner
        .connect_button_clicked(move |_| open_install_dialog(&banner_ui));

    let autostart_ui = ui.clone();
    ui.autostart.connect_active_notify(move |switch| {
        let ui = autostart_ui.clone();
        let on = switch.is_active();
        if ui.busy.load(Ordering::Relaxed) || ui.refreshing.load(Ordering::Relaxed) {
            return;
        }
        let settings = ui.settings();
        if on {
            match config::validate(&settings) {
                Ok(()) => run_service_toggle(&ui, true, settings),
                Err(err) => {
                    switch.set_active(false);
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

    // 实时日志行：点击在终端打开 journalctl -f（与诊断区「打开实时日志」同一动作）
    let live_log_ui = ui.clone();
    ui.live_log_row.connect_activated(move |_| {
        run_diag(
            &live_log_ui,
            "正在打开实时日志…",
            "已打开日志终端",
            system::open_live_log,
        );
    });

    // 日志行：点击弹出完整日志浮层
    let log_ui = ui.clone();
    ui.log_row.connect_activated(move |_| {
        let ui = log_ui.clone();
        glib::spawn_future_local(async move {
            let log = glib::spawn_future(async move { system::load_status().last_log })
                .await
                .unwrap_or_default();
            let log = if log.trim().is_empty() {
                "暂无日志。".to_string()
            } else {
                log
            };
            let buffer = ui.log_view.buffer();
            buffer.set_text(&log);
            ui.log_popover.popup();
        });
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
                3 => run_diag(&ui, "正在打开帮助页…", "已打开", system::open_help),
                _ => {}
            }
            ui.more_popover.popdown();
        });
    }
}

/// 刷新状态：后台读 system 状态，完成后回主线程更新舞台大状态字、
/// 状态胶囊、日志预览与自启开关。
fn refresh_status(ui: &Ui) {
    let ui_done = ui.clone();
    let nics = ui.nics.clone();
    ui.refreshing.store(true, Ordering::Relaxed);
    glib::spawn_future_local(async move {
        let (
            pills,
            log_text,
            autostart,
            conn,
            detail,
            active_nic,
            banner_show,
            banner_title,
            banner_action,
        ) = glib::spawn_future(async move {
            let status = system::load_status();

            let client = if status.client_installed {
                ("客户端 已安装".to_string(), "dot-ok")
            } else {
                ("客户端 未安装".to_string(), "dot-warn")
            };
            let proc = if status.client_running {
                ("进程 运行中".to_string(), "dot-ok")
            } else {
                ("进程 未运行".to_string(), "dot-warn")
            };
            // 服务 pill 区分"已启用且正常运行 / 已启用但异常 / 未启用"：
            // 官方客户端 root 下崩溃时服务会 failed，只看 enabled 会误报健康。
            let service = if status.service_enabled == "enabled" {
                if status.service_active == "active" {
                    ("服务 已启用".to_string(), "dot-ok")
                } else {
                    ("服务 异常".to_string(), "dot-warn")
                }
            } else {
                ("服务 未启用".to_string(), "dot-warn")
            };
            let nic = nics
                .iter()
                .map(|name| {
                    if system::interface_has_carrier(name) {
                        format!("{name} 已插线")
                    } else {
                        format!("{name} 无网线")
                    }
                })
                .collect::<Vec<_>>()
                .join(" / ");
            let nic_pill = if nic.is_empty() {
                ("网卡 无网卡".to_string(), "dot-warn")
            } else if nic.contains("已插线") {
                ("网卡 已插线".to_string(), "dot-ok")
            } else {
                ("网卡 无网线".to_string(), "dot-warn")
            };

            let log_text = if status.last_log.is_empty() {
                "暂无日志".to_string()
            } else {
                let trimmed = status.last_log.trim();
                if trimmed.chars().count() > 60 {
                    let mut s: String = trimmed.chars().take(60).collect();
                    s.push('…');
                    s
                } else {
                    trimmed.to_string()
                }
            };

            // 连接状态 / 副行（网卡 · 时长）
            let conn = status.client_running;
            let active_nic = nics
                .iter()
                .find(|name| system::interface_has_carrier(name))
                .cloned()
                .unwrap_or_else(|| nics.first().cloned().unwrap_or_default());
            let detail = if let Some(secs) = status.client_uptime_seconds {
                if secs < 60 {
                    format!("已连接 {secs}s")
                } else if secs < 3600 {
                    format!("已连接 {} 分钟", secs / 60)
                } else {
                    format!("已连接 {} 小时 {} 分", secs / 3600, (secs % 3600) / 60)
                }
            } else if conn {
                "已连接".to_string()
            } else {
                "未连接".to_string()
            };

            // 迁移提示：旧版客户端或旧版服务模板（不安全）时需要用户处理。
            // 客户端迁移可点横幅按钮直接重装；服务迁移只需操作自启开关，不给按钮。
            let (banner_show, banner_title, banner_action) = if status.client_requires_migration {
                (
                    true,
                    "检测到旧版客户端，请重新安装官方客户端完成安全迁移".to_string(),
                    "现在处理".to_string(),
                )
            } else if status.service_requires_migration {
                (
                    true,
                    "开机认证服务配置不安全，请关闭后重新开启开机认证完成迁移".to_string(),
                    String::new(),
                )
            } else {
                (false, String::new(), String::new())
            };

            (
                vec![client, proc, service, nic_pill],
                log_text,
                status.service_enabled == "enabled",
                conn,
                detail,
                active_nic,
                banner_show,
                banner_title,
                banner_action,
            )
        })
        .await
        .unwrap_or((
            vec![
                ("客户端 加载中…".to_string(), "dot-warn"),
                ("进程 加载中…".to_string(), "dot-warn"),
                ("服务 加载中…".to_string(), "dot-warn"),
                ("网卡 加载中…".to_string(), "dot-warn"),
            ],
            "暂无日志".to_string(),
            false,
            false,
            "未连接".to_string(),
            String::new(),
            false,
            String::new(),
            String::new(),
        ));

        // 状态胶囊：圆点 + 值
        let dot_classes = ["dot-ok", "dot-warn"];
        for ((dot, value), (text, cls)) in ui_done.pills.iter().zip(pills.iter()) {
            value.set_label(text);
            for c in dot_classes {
                dot.remove_css_class(c);
            }
            dot.add_css_class(cls);
        }
        // 大状态字（舞台 + 窄屏状态条）
        ui_done.set_stage(if conn { "已连接" } else { "未连接" }, conn);
        // 副行：网卡 · 详情
        let sub = if active_nic.is_empty() {
            detail.clone()
        } else {
            format!("{active_nic} · {detail}")
        };
        ui_done.stage_sub.set_label(&sub);
        ui_done.compact_sub.set_label(&sub);
        ui_done.log_row.set_subtitle(&log_text);
        // 迁移横幅：仅旧版客户端 / 不安全服务模板时显示
        ui_done.banner.set_title(&banner_title);
        ui_done
            .banner
            .set_button_label(if banner_action.is_empty() {
                None
            } else {
                Some(banner_action.as_str())
            });
        ui_done.banner.set_revealed(banner_show);
        if !ui_done.busy.load(Ordering::Relaxed) {
            ui_done.autostart.set_active(autostart);
        }
        ui_done.refreshing.store(false, Ordering::Relaxed);
    });
}
