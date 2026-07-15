mod components;
mod layout;
mod navigation;
mod runtime;
mod settings;

use self::components::*;
use self::layout::install_breakpoints;
use self::navigation::{connect_sidebar, sidebar_navigation};
use self::runtime::{connect_actions, preferred_nic_index, refresh_status, update_controls};
use crate::{config, system};
use adw::prelude::*;
use gtk::gio;
use gtk4 as gtk;
use libadwaita as adw;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone)]
struct StageUi {
    dot: gtk::Box,
    value: gtk::Label,
}

#[derive(Clone)]
struct AppUi {
    window: adw::ApplicationWindow,
    stack: adw::ViewStack,
    username: adw::EntryRow,
    password: adw::PasswordEntryRow,
    nic: adw::ComboRow,
    nic_model: gtk::StringList,
    dhcp: adw::SwitchRow,
    save_password: adw::SwitchRow,
    autostart: adw::SwitchRow,
    link_panel: gtk::Box,
    status_icon: gtk::Image,
    status_title: gtk::Label,
    status_detail: gtk::Label,
    status_badge: gtk::Label,
    status_spinner: adw::Spinner,
    cable_stage: StageUi,
    client_stage: StageUi,
    process_stage: StageUi,
    uptime_stage: StageUi,
    client_row: adw::ActionRow,
    interface_row: adw::ActionRow,
    service_row: adw::ActionRow,
    action_btn: gtk::Button,
    disconnect_btn: gtk::Button,
    save_btn: gtk::Button,
    header_refresh_btn: gtk::Button,
    log_refresh_btn: gtk::Button,
    live_log_btn: gtk::Button,
    diagnostics_btn: gtk::Button,
    connectivity_btn: gtk::Button,
    restart_btn: gtk::Button,
    client_folder_btn: gtk::Button,
    help_btn: gtk::Button,
    sidebar_status: gtk::Label,
    client_banner: adw::Banner,
    log_buffer: gtk::TextBuffer,
    log_preview: gtk::TextView,
    log_full: gtk::TextView,
    toasts: adw::ToastOverlay,
    nics: Rc<RefCell<Vec<String>>>,
    last_status: Rc<RefCell<Option<system::ClientStatus>>>,
    busy: Rc<Cell<bool>>,
    refreshing: Rc<Cell<bool>>,
    autostart_guard: Rc<Cell<bool>>,
}

pub fn build(app: &adw::Application) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }

    let settings = config::load();
    let interface_names = system::wired_interfaces();
    let selected = preferred_nic_index(&interface_names, &settings.nic);
    let nics = Rc::new(RefCell::new(interface_names));
    let nic_model =
        gtk::StringList::new(&nics.borrow().iter().map(String::as_str).collect::<Vec<_>>());

    let toasts = adw::ToastOverlay::new();
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("锐捷有线认证")
        .default_width(960)
        .default_height(760)
        .width_request(420)
        .height_request(520)
        .content(&toasts)
        .build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_css_class("app-shell");
    toasts.set_child(Some(&toolbar));

    let stack = adw::ViewStack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    let header_switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .visible(false)
        .build();
    let bottom_switcher = adw::ViewSwitcherBar::builder()
        .stack(&stack)
        .reveal(false)
        .css_classes(["navigation-bar"])
        .build();

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    let header_refresh_btn = icon_button("view-refresh-symbolic", "刷新状态");
    header.pack_end(&header_refresh_btn);
    toolbar.add_top_bar(&header);
    toolbar.add_bottom_bar(&bottom_switcher);

    let client_banner = adw::Banner::builder()
        .title("未安装官方锐捷客户端，连接功能不可用")
        .button_label("选择安装包")
        .revealed(false)
        .build();
    toolbar.add_top_bar(&client_banner);

    let sidebar = sidebar_navigation();
    let app_body = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .hexpand(true)
        .vexpand(true)
        .build();
    app_body.append(&sidebar.root);
    app_body.append(&sidebar.compact_root);
    app_body.append(&stack);
    toolbar.set_content(Some(&app_body));

    let connection_page = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let connection_content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(16)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();
    let connection_clamp = adw::Clamp::builder()
        .maximum_size(1500)
        .tightening_threshold(1250)
        .child(&connection_content)
        .build();
    connection_page.set_child(Some(&connection_clamp));
    stack.add_titled_with_icon(
        &connection_page,
        Some("connection"),
        "连接",
        "network-wired-symbolic",
    );

    let link_panel = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(20)
        .css_classes(["link-panel", "state-idle"])
        .build();
    connection_content.append(&link_panel);

    let link_top = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(24)
        .build();
    link_panel.append(&link_top);

    let status_icon = gtk::Image::from_icon_name("network-offline-symbolic");
    status_icon.set_pixel_size(28);
    let icon_wrap = gtk::Box::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .css_classes(["link-icon"])
        .build();
    icon_wrap.append(&status_icon);
    link_top.append(&icon_wrap);

    let status_copy = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(5)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    link_top.append(&status_copy);
    status_copy.append(
        &gtk::Label::builder()
            .label("eno1 · 有线认证")
            .halign(gtk::Align::Start)
            .css_classes(["technical-label"])
            .build(),
    );

    let status_title_line = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .build();
    status_copy.append(&status_title_line);
    let status_title = gtk::Label::builder()
        .label("正在读取链路状态")
        .halign(gtk::Align::Start)
        .hexpand(true)
        .wrap(true)
        .xalign(0.0)
        .css_classes(["status-title"])
        .build();
    status_title_line.append(&status_title);
    let status_spinner = adw::Spinner::builder()
        .width_request(18)
        .height_request(18)
        .visible(true)
        .build();
    status_title_line.append(&status_spinner);
    let status_badge = gtk::Label::builder()
        .label("检查中")
        .valign(gtk::Align::Center)
        .css_classes(["state-badge", "badge-idle"])
        .build();
    status_title_line.append(&status_badge);
    let status_detail = gtk::Label::builder()
        .label("正在检查网线、官方客户端和认证进程")
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .css_classes(["status-detail"])
        .build();
    status_copy.append(&status_detail);

    let action_btn = text_action_button(
        "network-transmit-receive-symbolic",
        "连接网络",
        &["primary-action", "suggested-action"],
    );
    let disconnect_btn = text_action_button(
        "network-offline-symbolic",
        "断开连接",
        &["secondary-action"],
    );
    let action_wrap = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .homogeneous(true)
        .width_request(460)
        .halign(gtk::Align::End)
        .valign(gtk::Align::Center)
        .build();
    action_wrap.append(&action_btn);
    action_wrap.append(&disconnect_btn);
    link_top.append(&action_wrap);

    let stage_rail = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .homogeneous(true)
        .css_classes(["stage-rail"])
        .build();
    link_panel.append(&stage_rail);
    let (cable_widget, cable_stage) = stage_widget("network-wired-symbolic", "网线");
    let (client_widget, client_stage) = stage_widget("application-x-executable-symbolic", "客户端");
    let (process_widget, process_stage) =
        stage_widget("network-transmit-receive-symbolic", "认证进程");
    let (uptime_widget, uptime_stage) =
        stage_widget("preferences-system-time-symbolic", "运行时长");
    stage_rail.append(&cable_widget);
    stage_rail.append(&client_widget);
    stage_rail.append(&process_widget);
    stage_rail.append(&uptime_widget);

    let columns = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(28)
        .hexpand(true)
        .build();
    connection_content.append(&columns);

    let form_column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .hexpand(true)
        .css_classes(["dashboard-card"])
        .build();
    columns.append(&form_column);

    let credentials_group = adw::PreferencesGroup::builder()
        .title("认证信息")
        .description("连接时自动保存账号和连接选项；密码不会保存在本应用中")
        .build();
    form_column.append(&credentials_group);
    let username = adw::EntryRow::builder()
        .title("校园网账号")
        .text(&settings.username)
        .input_purpose(gtk::InputPurpose::FreeForm)
        .build();
    credentials_group.add(&username);
    let password = adw::PasswordEntryRow::builder()
        .title("本次使用的新密码（可选）")
        .build();
    credentials_group.add(&password);
    let nic = adw::ComboRow::builder()
        .title("有线网卡")
        .subtitle("仅显示物理以太网接口")
        .model(&nic_model)
        .selected(selected as u32)
        .build();
    credentials_group.add(&nic);

    let connection_group = adw::PreferencesGroup::builder().title("连接方式").build();
    form_column.append(&connection_group);
    let dhcp = adw::SwitchRow::builder()
        .title("自动获取网络地址")
        .subtitle("校园有线网络通常需要 DHCP")
        .build();
    dhcp.set_active(settings.dhcp);
    connection_group.add(&dhcp);
    let save_password = adw::SwitchRow::builder()
        .title("交给官方客户端保存密码")
        .subtitle("关闭后，本次密码仅用于当前认证")
        .build();
    save_password.set_active(settings.save_password);
    connection_group.add(&save_password);
    let autostart = adw::SwitchRow::builder()
        .title("开机自动认证")
        .subtitle("按当前账号和网卡写入系统服务")
        .build();

    let save_btn = gtk::Button::builder()
        .label("保存设置")
        .tooltip_text("保存账号、网卡和连接选项，不包含密码")
        .halign(gtk::Align::End)
        .css_classes(["flat"])
        .build();
    form_column.append(&save_btn);

    let side_preview = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .width_request(360)
        .hexpand(true)
        .css_classes(["dashboard-card"])
        .build();
    columns.append(&side_preview);
    side_preview.append(&section_heading("开机认证", "系统启动时自动认证"));
    let service_switch_group = adw::PreferencesGroup::new();
    service_switch_group.add(&autostart);
    side_preview.append(&service_switch_group);
    side_preview.append(&section_heading("运行说明", "官方客户端的权限与状态边界"));

    let status_group = adw::PreferencesGroup::new();
    side_preview.append(&status_group);
    let client_row = status_row("application-x-executable-symbolic", "官方客户端");
    let interface_row = status_row("network-wired-symbolic", "有线链路");
    let service_row = status_row("system-run-symbolic", "开机认证");
    status_group.add(&client_row);
    status_group.add(&interface_row);
    status_group.add(&service_row);

    let quick_column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .width_request(360)
        .hexpand(true)
        .css_classes(["dashboard-card", "quick-card"])
        .build();
    columns.append(&quick_column);
    quick_column.append(&section_heading("快捷操作", "常用诊断和客户端工具"));
    let connectivity_btn = quick_action_button(
        "network-transmit-receive-symbolic",
        "测试网络连通性",
        "检测当前网络是否可以访问公网",
    );
    let restart_btn = quick_action_button(
        "view-refresh-symbolic",
        "重启认证服务",
        "重新启动 rjsupplicant.service",
    );
    let client_folder_btn = quick_action_button(
        "folder-open-symbolic",
        "打开客户端目录",
        "查看官方客户端和日志文件",
    );
    let help_btn = quick_action_button(
        "help-browser-symbolic",
        "查看帮助文档",
        "打开学校有线认证说明",
    );
    for button in [
        &connectivity_btn,
        &restart_btn,
        &client_folder_btn,
        &help_btn,
    ] {
        quick_column.append(button);
    }

    let log_buffer = gtk::TextBuffer::new(None);
    let log_preview = log_view(&log_buffer);
    let log_panel = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .css_classes(["dashboard-card", "recent-log-card"])
        .build();
    connection_content.append(&log_panel);
    let preview_header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .build();
    log_panel.append(&preview_header);
    let preview_copy = section_heading("最近日志", "用于判断认证成功、密码错误或网卡异常");
    preview_copy.set_hexpand(true);
    preview_header.append(&preview_copy);
    let diagnostics_btn = gtk::Button::builder()
        .label("实时日志")
        .valign(gtk::Align::Center)
        .css_classes(["outline-accent"])
        .build();
    preview_header.append(&diagnostics_btn);
    let preview_scroller = gtk::ScrolledWindow::builder()
        .min_content_height(110)
        .max_content_height(150)
        .vexpand(false)
        .child(&log_preview)
        .css_classes(["log-frame"])
        .build();
    log_panel.append(&preview_scroller);

    let diagnostics_page = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    let diagnostics_clamp = adw::Clamp::builder()
        .maximum_size(1400)
        .tightening_threshold(1100)
        .child(&diagnostics_page)
        .build();
    stack.add_titled_with_icon(
        &diagnostics_clamp,
        Some("diagnostics"),
        "诊断",
        "utilities-terminal-symbolic",
    );

    let diagnostics_header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .build();
    diagnostics_page.append(&diagnostics_header);
    let diagnostics_copy = section_heading("诊断日志", "用于判断密码错误、网卡异常和服务启动问题");
    diagnostics_copy.set_hexpand(true);
    diagnostics_header.append(&diagnostics_copy);
    let log_refresh_btn = gtk::Button::builder()
        .label("刷新")
        .icon_name("view-refresh-symbolic")
        .valign(gtk::Align::Center)
        .build();
    let live_log_btn = gtk::Button::builder()
        .label("实时日志")
        .valign(gtk::Align::Center)
        .build();
    diagnostics_header.append(&log_refresh_btn);
    diagnostics_header.append(&live_log_btn);

    let log_full = log_view(&log_buffer);
    let full_log_scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .min_content_height(360)
        .child(&log_full)
        .css_classes(["log-frame", "full-log"])
        .build();
    diagnostics_page.append(&full_log_scroller);

    let connection_action = gio::SimpleAction::new("show-connection", None);
    let connection_stack = stack.clone();
    connection_action
        .connect_activate(move |_, _| connection_stack.set_visible_child_name("connection"));
    app.add_action(&connection_action);
    app.set_accels_for_action("app.show-connection", &["<Control>1"]);

    let diagnostics_action = gio::SimpleAction::new("show-diagnostics", None);
    let diagnostics_stack = stack.clone();
    diagnostics_action
        .connect_activate(move |_, _| diagnostics_stack.set_visible_child_name("diagnostics"));
    app.add_action(&diagnostics_action);
    app.set_accels_for_action("app.show-diagnostics", &["<Control>2"]);

    let ui = AppUi {
        window,
        stack,
        username,
        password,
        nic,
        nic_model,
        dhcp,
        save_password,
        autostart,
        link_panel,
        status_icon,
        status_title,
        status_detail,
        status_badge,
        status_spinner,
        cable_stage,
        client_stage,
        process_stage,
        uptime_stage,
        client_row,
        interface_row,
        service_row,
        action_btn,
        disconnect_btn,
        save_btn,
        header_refresh_btn,
        log_refresh_btn,
        live_log_btn,
        diagnostics_btn,
        connectivity_btn,
        restart_btn,
        client_folder_btn,
        help_btn,
        sidebar_status: sidebar.status_label.clone(),
        client_banner,
        log_buffer,
        log_preview,
        log_full,
        toasts,
        nics,
        last_status: Rc::new(RefCell::new(None)),
        busy: Rc::new(Cell::new(false)),
        refreshing: Rc::new(Cell::new(false)),
        autostart_guard: Rc::new(Cell::new(false)),
    };

    install_breakpoints(
        &ui.window,
        &connection_clamp,
        &sidebar,
        &columns,
        &link_top,
        &action_wrap,
        &ui.action_btn,
        &header_switcher,
        &bottom_switcher,
        &connection_content,
        &diagnostics_page,
    );
    connect_actions(&ui);
    settings::install_settings_action(app, &ui);
    connect_sidebar(&ui, &sidebar, &connection_page);
    update_controls(&ui);
    refresh_status(&ui);
    ui.window.present();
}

pub fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("../data/style.css"));

    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
