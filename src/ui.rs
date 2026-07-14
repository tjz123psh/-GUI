use crate::{config, system};
use adw::prelude::*;
use gtk::{gio, glib};
use gtk4 as gtk;
use libadwaita as adw;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone)]
struct StageUi {
    dot: gtk::Box,
    value: gtk::Label,
}

struct SidebarUi {
    root: gtk::Box,
    compact_root: gtk::Box,
    status_label: gtk::Label,
    all_buttons: Vec<gtk::Button>,
    status_buttons: Vec<gtk::Button>,
    auth_buttons: Vec<gtk::Button>,
    runtime_buttons: Vec<gtk::Button>,
    logs_buttons: Vec<gtk::Button>,
    settings_buttons: Vec<gtk::Button>,
    about_buttons: Vec<gtk::Button>,
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
        .button_label("查看安装方法")
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
    connect_sidebar(&ui, &sidebar, &connection_page);
    update_controls(&ui);
    refresh_status(&ui);
    ui.window.present();
}

#[allow(clippy::too_many_arguments)]
fn install_breakpoints(
    window: &adw::ApplicationWindow,
    connection_clamp: &adw::Clamp,
    sidebar: &SidebarUi,
    columns: &gtk::Box,
    link_top: &gtk::Box,
    action_wrap: &gtk::Box,
    action_btn: &gtk::Button,
    header_switcher: &adw::ViewSwitcher,
    bottom_switcher: &adw::ViewSwitcherBar,
    connection_content: &gtk::Box,
    diagnostics_page: &gtk::Box,
) {
    let standard_condition = adw::BreakpointCondition::new_and(
        adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MinWidth,
            720.0,
            adw::LengthUnit::Sp,
        ),
        adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            1099.0,
            adw::LengthUnit::Sp,
        ),
    );
    let standard = adw::Breakpoint::new(standard_condition);
    standard.add_setter(&sidebar.root, "visible", Some(&false.to_value()));
    standard.add_setter(&sidebar.compact_root, "visible", Some(&true.to_value()));
    standard.add_setter(
        columns,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    standard.add_setter(
        link_top,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    standard.add_setter(action_wrap, "width-request", Some(&(-1_i32).to_value()));
    standard.add_setter(action_wrap, "halign", Some(&gtk::Align::Fill.to_value()));
    standard.add_setter(action_btn, "hexpand", Some(&true.to_value()));
    standard.add_setter(connection_clamp, "maximum-size", Some(&820_i32.to_value()));
    window.add_breakpoint(standard);

    let expanded_condition = adw::BreakpointCondition::new_and(
        adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MinWidth,
            1100.0,
            adw::LengthUnit::Sp,
        ),
        adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            1399.0,
            adw::LengthUnit::Sp,
        ),
    );
    let expanded = adw::Breakpoint::new(expanded_condition);
    expanded.add_setter(&sidebar.root, "visible", Some(&false.to_value()));
    expanded.add_setter(&sidebar.compact_root, "visible", Some(&true.to_value()));
    window.add_breakpoint(expanded);

    let compact = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 719sp").expect("valid breakpoint"),
    );
    compact.add_setter(
        link_top,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    compact.add_setter(&sidebar.root, "visible", Some(&false.to_value()));
    compact.add_setter(&sidebar.compact_root, "visible", Some(&false.to_value()));
    compact.add_setter(
        columns,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    compact.add_setter(connection_clamp, "maximum-size", Some(&820_i32.to_value()));
    compact.add_setter(action_wrap, "width-request", Some(&(-1_i32).to_value()));
    compact.add_setter(action_wrap, "halign", Some(&gtk::Align::Fill.to_value()));
    compact.add_setter(action_btn, "hexpand", Some(&true.to_value()));
    compact.add_setter(header_switcher, "visible", Some(&false.to_value()));
    compact.add_setter(bottom_switcher, "reveal", Some(&true.to_value()));
    for widget in [connection_content, diagnostics_page] {
        compact.add_setter(widget, "margin-start", Some(&16_i32.to_value()));
        compact.add_setter(widget, "margin-end", Some(&16_i32.to_value()));
        compact.add_setter(widget, "margin-top", Some(&16_i32.to_value()));
        compact.add_setter(widget, "margin-bottom", Some(&16_i32.to_value()));
    }
    window.add_breakpoint(compact);
}

fn connect_actions(ui: &AppUi) {
    let username = ui.username.clone();
    username.connect_changed(move |entry| entry.remove_css_class("error"));

    let nic_ui = ui.clone();
    ui.nic.connect_selected_notify(move |_| {
        let status = nic_ui.last_status.borrow().clone();
        if let Some(status) = status {
            apply_status(&nic_ui, status);
            update_controls(&nic_ui);
        }
    });

    let action_ui = ui.clone();
    ui.action_btn.connect_clicked(move |_| {
        let Some(settings) = collect_settings(&action_ui) else {
            return;
        };
        if let Err(err) = config::save(&settings) {
            toast(&action_ui, &format!("设置保存失败：{err}"));
            return;
        }
        let password = action_ui.password.text().to_string();
        action_ui.password.set_text("");
        run_action(
            &action_ui,
            "正在请求授权并启动认证…",
            "认证命令执行失败",
            "认证命令已完成，正在确认进程状态",
            move || system::authenticate(&settings, &password),
            |_| {},
        );
    });

    let disconnect_ui = ui.clone();
    ui.disconnect_btn.connect_clicked(move |_| {
        run_action(
            &disconnect_ui,
            "正在停止认证进程…",
            "断开认证失败",
            "认证进程已停止",
            system::disconnect,
            |_| {},
        );
    });

    let save_ui = ui.clone();
    ui.save_btn.connect_clicked(move |_| {
        let Some(settings) = collect_settings(&save_ui) else {
            return;
        };
        match config::save(&settings) {
            Ok(()) => toast(&save_ui, "设置已保存（不包含密码）"),
            Err(err) => toast(&save_ui, &format!("设置保存失败：{err}")),
        }
    });

    let autostart_ui = ui.clone();
    ui.autostart.connect_active_notify(move |row| {
        if autostart_ui.autostart_guard.get()
            || autostart_ui.busy.get()
            || autostart_ui.refreshing.get()
        {
            return;
        }

        if row.is_active() {
            let Some(settings) = collect_settings(&autostart_ui) else {
                set_autostart_switch(&autostart_ui, false);
                return;
            };
            if let Err(err) = config::save(&settings) {
                toast(&autostart_ui, &format!("设置保存失败：{err}"));
                set_autostart_switch(&autostart_ui, false);
                return;
            }
            run_action(
                &autostart_ui,
                "正在启用开机认证…",
                "启用开机认证失败",
                "开机认证已启用",
                move || system::enable_service(&settings),
                |_| {},
            );
        } else {
            run_action(
                &autostart_ui,
                "正在停用开机认证…",
                "停用开机认证失败",
                "开机认证已停用",
                system::disable_service,
                |_| {},
            );
        }
    });

    for button in [&ui.header_refresh_btn, &ui.log_refresh_btn] {
        let refresh_ui = ui.clone();
        button.connect_clicked(move |_| refresh_status(&refresh_ui));
    }

    let logs_ui = ui.clone();
    ui.live_log_btn
        .connect_clicked(move |_| match system::open_live_log() {
            Ok(()) => toast(&logs_ui, "已打开实时日志窗口"),
            Err(err) => toast(&logs_ui, &format!("无法打开实时日志：{err}")),
        });

    let stack = ui.stack.clone();
    ui.diagnostics_btn
        .connect_clicked(move |_| stack.set_visible_child_name("diagnostics"));

    let connectivity_ui = ui.clone();
    ui.connectivity_btn.connect_clicked(move |_| {
        run_action(
            &connectivity_ui,
            "正在测试网络连通性…",
            "网络连通测试失败",
            "网络可以访问公网",
            system::test_connectivity,
            |_| {},
        );
    });

    let restart_ui = ui.clone();
    ui.restart_btn.connect_clicked(move |_| {
        run_action(
            &restart_ui,
            "正在重启认证服务…",
            "重启认证服务失败",
            "认证服务已重启",
            system::restart_service,
            |_| {},
        );
    });

    let client_folder_ui = ui.clone();
    ui.client_folder_btn
        .connect_clicked(move |_| match system::open_client_folder() {
            Ok(()) => toast(&client_folder_ui, "已打开客户端目录"),
            Err(err) => toast(&client_folder_ui, &format!("无法打开客户端目录：{err}")),
        });

    let help_ui = ui.clone();
    ui.help_btn
        .connect_clicked(move |_| match system::open_help() {
            Ok(()) => toast(&help_ui, "已打开帮助文档"),
            Err(err) => toast(&help_ui, &format!("无法打开帮助文档：{err}")),
        });

    let banner_window = ui.window.clone();
    ui.client_banner.connect_button_clicked(move |_| {
        let dialog = adw::AlertDialog::builder()
            .heading("安装官方锐捷客户端")
            .body("把 RG_Supplicant_For_Linux*.zip 放到 ~/Downloads，然后重新运行本仓库的 scripts/install.sh。")
            .build();
        dialog.add_response("close", "知道了");
        dialog.present(Some(&banner_window));
    });
}

fn run_action<F, S>(
    ui: &AppUi,
    pending: &str,
    error_prefix: &'static str,
    success_message: &'static str,
    work: F,
    on_success: S,
) where
    F: FnOnce() -> anyhow::Result<()> + Send + 'static,
    S: FnOnce(&AppUi) + 'static,
{
    if ui.busy.replace(true) {
        return;
    }
    ui.status_detail.set_text(pending);
    ui.status_spinner.set_visible(true);
    set_badge(ui, "处理中", "badge-working");
    update_controls(ui);

    let task_ui = ui.clone();
    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(work).await;
        let succeeded = match result {
            Ok(Ok(())) => {
                on_success(&task_ui);
                toast(&task_ui, success_message);
                true
            }
            Ok(Err(err)) => {
                toast(&task_ui, &format!("{error_prefix}：{err}"));
                false
            }
            Err(_) => {
                toast(&task_ui, &format!("{error_prefix}：后台任务异常终止"));
                false
            }
        };
        if succeeded {
            glib::timeout_future(Duration::from_millis(700)).await;
        }
        task_ui.busy.set(false);
        refresh_status(&task_ui);
    });
}

fn collect_settings(ui: &AppUi) -> Option<config::Settings> {
    let settings = config::Settings {
        username: ui.username.text().trim().to_string(),
        nic: selected_nic(ui),
        dhcp: ui.dhcp.is_active(),
        save_password: ui.save_password.is_active(),
    };
    if let Err(err) = config::validate(&settings) {
        ui.username.add_css_class("error");
        ui.username.grab_focus();
        toast(ui, &err.to_string());
        return None;
    }
    Some(settings)
}

fn refresh_status(ui: &AppUi) {
    refresh_interfaces(ui);
    if ui.refreshing.replace(true) {
        return;
    }
    ui.status_spinner.set_visible(true);
    update_controls(ui);
    let refresh_ui = ui.clone();
    glib::spawn_future_local(async move {
        match gio::spawn_blocking(system::load_status).await {
            Ok(status) => apply_status(&refresh_ui, status),
            Err(_) => toast(&refresh_ui, "状态读取任务异常终止"),
        }
        refresh_ui.refreshing.set(false);
        if !refresh_ui.busy.get() {
            refresh_ui.status_spinner.set_visible(false);
        }
        update_controls(&refresh_ui);
    });
}

fn apply_status(ui: &AppUi, status: system::ClientStatus) {
    let nic = selected_nic(ui);
    let carrier = system::interface_has_carrier(&nic);
    let enabled = service_is_enabled(&status.service_enabled);
    let failed = status.service_active.trim() == "failed";

    set_stage(
        &ui.cable_stage,
        if carrier { "已连接" } else { "未连接" },
        if carrier {
            "stage-success"
        } else {
            "stage-idle"
        },
    );
    set_stage(
        &ui.client_stage,
        if status.client_installed {
            "已就绪"
        } else {
            "未安装"
        },
        if status.client_installed {
            "stage-success"
        } else {
            "stage-error"
        },
    );
    set_stage(
        &ui.process_stage,
        if status.client_running {
            "运行中"
        } else {
            "未运行"
        },
        if status.client_running {
            "stage-success"
        } else {
            "stage-idle"
        },
    );
    set_stage(
        &ui.uptime_stage,
        &status
            .client_uptime_seconds
            .map(format_duration)
            .unwrap_or_else(|| "00:00:00".to_string()),
        if status.client_running {
            "stage-success"
        } else {
            "stage-idle"
        },
    );

    ui.client_row.set_subtitle(if !status.client_installed {
        "未安装"
    } else if status.client_running {
        "已安装 · 认证进程运行中"
    } else {
        "已安装 · 认证进程未运行"
    });
    ui.interface_row.set_subtitle(&format!(
        "{} · {}",
        nic,
        if carrier {
            "网线已连接"
        } else {
            "未检测到网线"
        }
    ));
    ui.service_row.set_subtitle(if enabled {
        "已启用，将在开机联网后自动认证"
    } else {
        "未启用"
    });
    set_autostart_switch(ui, enabled);
    ui.sidebar_status.set_text(if status.client_running {
        "认证运行中"
    } else if carrier {
        "等待认证"
    } else {
        "网络未连接"
    });

    if !status.client_installed {
        set_connection_state(
            ui,
            "缺少官方客户端",
            "安装学校提供的 Linux 客户端后才能发起认证",
            "需安装",
            "state-error",
            "badge-error",
            "dialog-warning-symbolic",
        );
    } else if failed {
        set_connection_state(
            ui,
            "开机认证启动失败",
            "打开诊断日志检查账号、密码或网卡设置",
            "故障",
            "state-error",
            "badge-error",
            "dialog-error-symbolic",
        );
    } else if status.client_running {
        set_connection_state(
            ui,
            "认证进程正在运行",
            if carrier {
                "认证结果以官方客户端日志为准"
            } else {
                "进程仍在运行，但当前网卡没有检测到网线"
            },
            "运行中",
            "state-active",
            "badge-active",
            "network-transmit-receive-symbolic",
        );
    } else if carrier {
        set_connection_state(
            ui,
            "可以开始认证",
            "网线和官方客户端均已就绪",
            "待连接",
            "state-ready",
            "badge-ready",
            "network-wired-symbolic",
        );
    } else {
        set_connection_state(
            ui,
            "等待有线网络",
            "连接网线，或在下方选择其他有线接口",
            "未连接",
            "state-idle",
            "badge-idle",
            "network-offline-symbolic",
        );
    }

    ui.client_banner.set_revealed(!status.client_installed);
    ui.log_buffer.set_text(&status.last_log);
    scroll_log_to_end(&ui.log_preview);
    scroll_log_to_end(&ui.log_full);
    *ui.last_status.borrow_mut() = Some(status);
}

fn set_connection_state(
    ui: &AppUi,
    title: &str,
    detail: &str,
    badge: &str,
    panel_class: &str,
    badge_class: &str,
    icon: &str,
) {
    for class in ["state-idle", "state-ready", "state-active", "state-error"] {
        ui.link_panel.remove_css_class(class);
    }
    ui.link_panel.add_css_class(panel_class);
    ui.status_icon.set_icon_name(Some(icon));
    ui.status_title.set_text(title);
    ui.status_detail.set_text(detail);
    set_badge(ui, badge, badge_class);
}

fn set_badge(ui: &AppUi, text: &str, class: &str) {
    for old in [
        "badge-idle",
        "badge-ready",
        "badge-active",
        "badge-error",
        "badge-working",
    ] {
        ui.status_badge.remove_css_class(old);
    }
    ui.status_badge.add_css_class(class);
    ui.status_badge.set_text(text);
}

fn set_stage(stage: &StageUi, text: &str, class: &str) {
    for old in ["stage-idle", "stage-success", "stage-error"] {
        stage.dot.remove_css_class(old);
    }
    stage.dot.add_css_class(class);
    stage.value.set_text(text);
}

fn set_autostart_switch(ui: &AppUi, enabled: bool) {
    ui.autostart_guard.set(true);
    ui.autostart.set_active(enabled);
    ui.autostart_guard.set(false);
}

fn update_controls(ui: &AppUi) {
    let unavailable = ui.busy.get() || ui.refreshing.get();
    for button in [
        &ui.action_btn,
        &ui.disconnect_btn,
        &ui.save_btn,
        &ui.header_refresh_btn,
        &ui.log_refresh_btn,
        &ui.connectivity_btn,
        &ui.restart_btn,
        &ui.client_folder_btn,
        &ui.help_btn,
    ] {
        button.set_sensitive(!unavailable);
    }
    ui.autostart.set_sensitive(!unavailable);
    ui.live_log_btn.set_sensitive(!unavailable);

    if unavailable {
        return;
    }
    let status = ui.last_status.borrow();
    if let Some(status) = status.as_ref() {
        ui.action_btn
            .set_sensitive(status.client_installed && !status.client_running);
        ui.disconnect_btn
            .set_sensitive(status.client_installed && status.client_running);
        ui.autostart.set_sensitive(status.client_installed);
        ui.restart_btn.set_sensitive(status.client_installed);
        ui.client_folder_btn.set_sensitive(status.client_installed);
    } else {
        ui.action_btn.set_sensitive(false);
        ui.disconnect_btn.set_sensitive(false);
        ui.autostart.set_sensitive(false);
        ui.restart_btn.set_sensitive(false);
        ui.client_folder_btn.set_sensitive(false);
    }
}

fn refresh_interfaces(ui: &AppUi) {
    let previous = selected_nic(ui);
    let fresh = system::wired_interfaces();
    if *ui.nics.borrow() == fresh {
        return;
    }
    while ui.nic_model.n_items() > 0 {
        ui.nic_model.remove(0);
    }
    for name in &fresh {
        ui.nic_model.append(name);
    }
    ui.nic
        .set_selected(preferred_nic_index(&fresh, &previous) as u32);
    *ui.nics.borrow_mut() = fresh;
}

fn selected_nic(ui: &AppUi) -> String {
    ui.nics
        .borrow()
        .get(ui.nic.selected() as usize)
        .cloned()
        .unwrap_or_else(|| "eno1".to_string())
}

fn preferred_nic_index(nics: &[String], preferred: &str) -> usize {
    nics.iter()
        .position(|name| !preferred.is_empty() && name == preferred)
        .or_else(|| nics.iter().position(|name| name == "eno1"))
        .unwrap_or(0)
}

fn service_is_enabled(value: &str) -> bool {
    matches!(
        value.trim(),
        "enabled" | "enabled-runtime" | "linked" | "linked-runtime" | "alias"
    )
}

fn format_duration(total_seconds: u64) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn toast(ui: &AppUi, message: &str) {
    ui.toasts.add_toast(adw::Toast::new(message));
}

fn scroll_log_to_end(view: &gtk::TextView) {
    let mut end = view.buffer().end_iter();
    view.scroll_to_iter(&mut end, 0.0, false, 0.0, 1.0);
}

fn sidebar_navigation() -> SidebarUi {
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .width_request(240)
        .css_classes(["sidebar"])
        .build();

    let brand = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .margin_top(22)
        .margin_bottom(24)
        .margin_start(20)
        .margin_end(16)
        .build();
    root.append(&brand);
    let brand_mark = gtk::Box::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .css_classes(["sidebar-brand-mark"])
        .build();
    let brand_icon = gtk::Image::from_icon_name("insert-link-symbolic");
    brand_icon.set_pixel_size(22);
    brand_mark.append(&brand_icon);
    brand.append(&brand_mark);
    let brand_copy = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(1)
        .build();
    brand_copy.append(
        &gtk::Label::builder()
            .label("锐捷有线认证")
            .halign(gtk::Align::Start)
            .css_classes(["sidebar-brand-title"])
            .build(),
    );
    brand_copy.append(
        &gtk::Label::builder()
            .label("GDUFS 校园有线网")
            .halign(gtk::Align::Start)
            .css_classes(["sidebar-brand-subtitle"])
            .build(),
    );
    brand.append(&brand_copy);

    let nav = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_start(14)
        .margin_end(14)
        .build();
    root.append(&nav);
    let (status_btn, _) = sidebar_nav_button("network-wired-symbolic", "连接状态", true);
    let (auth_btn, _) = sidebar_nav_button("avatar-default-symbolic", "认证设置", false);
    let (runtime_btn, _) = sidebar_nav_button("system-run-symbolic", "运行状态", false);
    let (logs_btn, _) = sidebar_nav_button("utilities-terminal-symbolic", "日志查看", false);
    let (settings_btn, _) = sidebar_nav_button("emblem-system-symbolic", "设置中心", false);
    let (about_btn, _) = sidebar_nav_button("help-about-symbolic", "关于我们", false);
    for button in [
        &status_btn,
        &auth_btn,
        &runtime_btn,
        &logs_btn,
        &settings_btn,
        &about_btn,
    ] {
        nav.append(button);
    }

    let spacer = gtk::Box::builder().vexpand(true).build();
    root.append(&spacer);

    let bytes = glib::Bytes::from_static(include_bytes!("../data/sidebar-landscape.png"));
    let stream = gio::MemoryInputStream::from_bytes(&bytes);
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_stream(&stream, gio::Cancellable::NONE)
        .expect("embedded sidebar artwork");
    let texture = gtk::gdk::Texture::for_pixbuf(&pixbuf);
    let picture = gtk::Picture::for_paintable(&texture);
    picture.set_content_fit(gtk::ContentFit::Cover);
    picture.set_width_request(240);
    picture.set_height_request(300);

    let artwork = gtk::Overlay::new();
    artwork.set_child(Some(&picture));
    root.append(&artwork);
    let network_card = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .halign(gtk::Align::Fill)
        .valign(gtk::Align::End)
        .margin_start(14)
        .margin_end(14)
        .margin_bottom(16)
        .css_classes(["sidebar-status-card"])
        .build();
    artwork.add_overlay(&network_card);
    let network_icon = gtk::Image::from_icon_name("network-wireless-signal-excellent-symbolic");
    network_icon.set_pixel_size(22);
    network_card.append(&network_icon);
    let status_copy = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(1)
        .build();
    status_copy.append(
        &gtk::Label::builder()
            .label("网络状态")
            .halign(gtk::Align::Start)
            .css_classes(["sidebar-status-caption"])
            .build(),
    );
    let status_label = gtk::Label::builder()
        .label("检查中")
        .halign(gtk::Align::Start)
        .css_classes(["sidebar-status-value"])
        .build();
    status_copy.append(&status_label);
    network_card.append(&status_copy);

    let compact_root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .width_request(72)
        .visible(false)
        .css_classes(["compact-sidebar"])
        .build();
    let compact_brand = gtk::Box::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .margin_top(18)
        .margin_bottom(20)
        .css_classes(["compact-brand"])
        .build();
    compact_brand.append(&gtk::Image::from_icon_name("insert-link-symbolic"));
    compact_root.append(&compact_brand);
    let compact_nav = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    compact_root.append(&compact_nav);
    let compact_status = compact_nav_button("network-wired-symbolic", "连接状态", true);
    let compact_auth = compact_nav_button("avatar-default-symbolic", "认证设置", false);
    let compact_runtime = compact_nav_button("system-run-symbolic", "运行状态", false);
    let compact_logs = compact_nav_button("utilities-terminal-symbolic", "日志查看", false);
    let compact_settings = compact_nav_button("emblem-system-symbolic", "设置中心", false);
    let compact_about = compact_nav_button("help-about-symbolic", "关于我们", false);
    for button in [
        &compact_status,
        &compact_auth,
        &compact_runtime,
        &compact_logs,
        &compact_settings,
        &compact_about,
    ] {
        compact_nav.append(button);
    }

    let all_buttons = vec![
        status_btn.clone(),
        auth_btn.clone(),
        runtime_btn.clone(),
        logs_btn.clone(),
        settings_btn.clone(),
        about_btn.clone(),
        compact_status.clone(),
        compact_auth.clone(),
        compact_runtime.clone(),
        compact_logs.clone(),
        compact_settings.clone(),
        compact_about.clone(),
    ];

    SidebarUi {
        root,
        compact_root,
        status_label,
        all_buttons,
        status_buttons: vec![status_btn, compact_status],
        auth_buttons: vec![auth_btn, compact_auth],
        runtime_buttons: vec![runtime_btn, compact_runtime],
        logs_buttons: vec![logs_btn, compact_logs],
        settings_buttons: vec![settings_btn, compact_settings],
        about_buttons: vec![about_btn, compact_about],
    }
}

fn sidebar_nav_button(icon: &str, text: &str, active: bool) -> (gtk::Button, gtk::Label) {
    let button = gtk::Button::builder()
        .hexpand(true)
        .css_classes(["sidebar-nav"])
        .build();
    if active {
        button.add_css_class("active");
    }
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .halign(gtk::Align::Start)
        .build();
    content.append(&gtk::Image::from_icon_name(icon));
    let label = gtk::Label::builder().label(text).build();
    content.append(&label);
    button.set_child(Some(&content));
    (button, label)
}

fn compact_nav_button(icon: &str, tooltip: &str, active: bool) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .halign(gtk::Align::Center)
        .css_classes(["compact-nav"])
        .build();
    if active {
        button.add_css_class("active");
    }
    button
}

fn connect_sidebar(ui: &AppUi, sidebar: &SidebarUi, connection_page: &gtk::ScrolledWindow) {
    for trigger in &sidebar.status_buttons {
        let stack = ui.stack.clone();
        let adjustment = connection_page.vadjustment();
        let all = sidebar.all_buttons.clone();
        let selected = sidebar.status_buttons.clone();
        trigger.connect_clicked(move |_| {
            select_sidebar_buttons(&all, &selected);
            stack.set_visible_child_name("connection");
            adjustment.set_value(0.0);
        });
    }

    for trigger in &sidebar.auth_buttons {
        let stack = ui.stack.clone();
        let entry = ui.username.clone();
        let all = sidebar.all_buttons.clone();
        let selected = sidebar.auth_buttons.clone();
        trigger.connect_clicked(move |_| {
            select_sidebar_buttons(&all, &selected);
            stack.set_visible_child_name("connection");
            entry.grab_focus();
        });
    }

    for trigger in &sidebar.runtime_buttons {
        let stack = ui.stack.clone();
        let switch = ui.autostart.clone();
        let all = sidebar.all_buttons.clone();
        let selected = sidebar.runtime_buttons.clone();
        trigger.connect_clicked(move |_| {
            select_sidebar_buttons(&all, &selected);
            stack.set_visible_child_name("connection");
            switch.grab_focus();
        });
    }

    for trigger in &sidebar.logs_buttons {
        let stack = ui.stack.clone();
        let all = sidebar.all_buttons.clone();
        let selected = sidebar.logs_buttons.clone();
        trigger.connect_clicked(move |_| {
            select_sidebar_buttons(&all, &selected);
            stack.set_visible_child_name("diagnostics");
        });
    }

    for trigger in &sidebar.settings_buttons {
        let window = ui.window.clone();
        trigger.connect_clicked(move |_| {
            let dialog = adw::AlertDialog::builder()
                .heading("设置中心")
                .body(format!(
                    "配置文件：{}\n官方客户端：{}\n\n账号和连接选项可在“认证设置”中修改。",
                    config::settings_path().display(),
                    config::client_path().display()
                ))
                .build();
            dialog.add_response("close", "关闭");
            dialog.present(Some(&window));
        });
    }

    for trigger in &sidebar.about_buttons {
        let window = ui.window.clone();
        trigger.connect_clicked(move |_| {
            let dialog = adw::AlertDialog::builder()
                .heading("锐捷有线认证")
                .body("面向 GDUFS 校园有线网的 GTK 客户端\n版本 0.2.0\n\n基于学校提供的官方 Linux 认证程序。")
                .build();
            dialog.add_response("close", "关闭");
            dialog.present(Some(&window));
        });
    }
}

fn select_sidebar_buttons(buttons: &[gtk::Button], selected: &[gtk::Button]) {
    for button in buttons {
        button.remove_css_class("active");
    }
    for button in selected {
        button.add_css_class("active");
    }
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .css_classes(["flat"])
        .build()
}

fn text_action_button(icon: &str, label: &str, classes: &[&str]) -> gtk::Button {
    let button = gtk::Button::builder()
        .hexpand(true)
        .css_classes(classes)
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::Center)
        .build();
    content.append(&gtk::Image::from_icon_name(icon));
    content.append(&gtk::Label::new(Some(label)));
    button.set_child(Some(&content));
    button
}

fn quick_action_button(icon: &str, title: &str, subtitle: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .hexpand(true)
        .css_classes(["quick-action"])
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .build();
    let icon_wrap = gtk::Box::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .css_classes(["quick-action-icon"])
        .build();
    icon_wrap.append(&gtk::Image::from_icon_name(icon));
    content.append(&icon_wrap);
    let copy = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .build();
    copy.append(
        &gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .css_classes(["quick-action-title"])
            .build(),
    );
    copy.append(
        &gtk::Label::builder()
            .label(subtitle)
            .halign(gtk::Align::Start)
            .wrap(true)
            .xalign(0.0)
            .css_classes(["quick-action-subtitle"])
            .build(),
    );
    content.append(&copy);
    content.append(&gtk::Image::from_icon_name("go-next-symbolic"));
    button.set_child(Some(&content));
    button
}

fn stage_widget(icon: &str, title: &str) -> (gtk::Box, StageUi) {
    let item = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .css_classes(["stage-item"])
        .build();
    let dot = gtk::Box::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .css_classes(["stage-dot", "stage-idle"])
        .build();
    dot.append(&gtk::Image::from_icon_name(icon));
    item.append(&dot);
    let copy = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(1)
        .build();
    copy.append(
        &gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .css_classes(["stage-title"])
            .build(),
    );
    let value = gtk::Label::builder()
        .label("检查中")
        .halign(gtk::Align::Start)
        .css_classes(["stage-value"])
        .build();
    copy.append(&value);
    item.append(&copy);
    (item, StageUi { dot, value })
}

fn section_heading(title: &str, description: &str) -> gtk::Box {
    let copy = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(3)
        .build();
    copy.append(
        &gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .css_classes(["section-title"])
            .build(),
    );
    copy.append(
        &gtk::Label::builder()
            .label(description)
            .halign(gtk::Align::Start)
            .wrap(true)
            .xalign(0.0)
            .css_classes(["section-description"])
            .build(),
    );
    copy
}

fn status_row(icon: &str, title: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle("检查中")
        .build();
    row.add_prefix(&gtk::Image::from_icon_name(icon));
    row
}

fn log_view(buffer: &gtk::TextBuffer) -> gtk::TextView {
    gtk::TextView::builder()
        .buffer(buffer)
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .left_margin(14)
        .right_margin(14)
        .top_margin(12)
        .bottom_margin(12)
        .css_classes(["log-view"])
        .build()
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
