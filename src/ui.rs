use crate::{config, system};
use adw::prelude::*;
use gtk4 as gtk;
use libadwaita as adw;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
struct AppUi {
    username: gtk::Entry,
    password: gtk::PasswordEntry,
    nic: gtk::DropDown,
    dhcp: gtk::Switch,
    save_password: gtk::Switch,
    status: gtk::Label,
    status_hint: gtk::Label,
    log: gtk::TextView,
    toasts: adw::ToastOverlay,
    nics: Rc<RefCell<Vec<String>>>,
}

pub fn build(app: &adw::Application) {
    let settings = config::load();
    let nics = Rc::new(RefCell::new(system::wired_interfaces()));
    let toasts = adw::ToastOverlay::new();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("锐捷有线认证")
        .default_width(980)
        .default_height(720)
        .content(&toasts)
        .build();

    let shell = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["app-shell"])
        .build();
    toasts.set_child(Some(&shell));

    shell.append(&app_header());

    let page = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    shell.append(&page);

    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(24)
        .margin_end(24)
        .build();
    page.set_child(Some(&root));

    root.append(&brand_header());
    root.append(&hero_panel());

    let main_grid = gtk::Grid::builder()
        .column_spacing(18)
        .row_spacing(18)
        .hexpand(true)
        .build();
    root.append(&main_grid);

    let form_panel = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .hexpand(true)
        .css_classes(["panel"])
        .build();
    main_grid.attach(&form_panel, 0, 0, 1, 1);

    form_panel.append(&section_header(
        "账号与认证",
        "密码留空时使用官方客户端已保存的密码；只有首次保存或修改时才填写。",
    ));

    let username = gtk::Entry::builder()
        .placeholder_text("请输入校园网账号")
        .text(&settings.username)
        .width_request(260)
        .css_classes(["compact-input"])
        .build();
    form_panel.append(&setting_row(
        "avatar-default-symbolic",
        "校园网账号",
        "用于有线网络认证的校园网账号",
        &username,
        false,
    ));

    let password = gtk::PasswordEntry::builder()
        .placeholder_text("不修改则留空")
        .show_peek_icon(true)
        .width_request(260)
        .css_classes(["compact-input"])
        .build();
    form_panel.append(&setting_row(
        "dialog-password-symbolic",
        "本次修改密码",
        "留空会复用官方客户端保存的密码，不会覆盖密码",
        &password,
        false,
    ));

    let nic_model = gtk::StringList::new(
        &nics
            .borrow()
            .iter()
            .map(String::as_str)
            .collect::<Vec<&str>>(),
    );
    let nic = gtk::DropDown::builder()
        .model(&nic_model)
        .width_request(150)
        .css_classes(["compact-select"])
        .build();
    select_default_nic(&nic, &nics.borrow(), &settings.nic);
    form_panel.append(&setting_row(
        "network-wired-symbolic",
        "有线网卡",
        "通常是 eno1，插线后可刷新确认",
        &nic,
        false,
    ));

    let dhcp = gtk::Switch::builder()
        .active(settings.dhcp)
        .valign(gtk::Align::Center)
        .build();
    form_panel.append(&setting_row(
        "network-workgroup-symbolic",
        "DHCP",
        "学校有线网一般保持开启",
        &dhcp,
        false,
    ));

    let save_password = gtk::Switch::builder()
        .active(settings.save_password)
        .valign(gtk::Align::Center)
        .build();
    form_panel.append(&setting_row(
        "changes-allow-symbolic",
        "保存密码到官方客户端",
        "开启后，本次填写的密码会交给官方客户端保存",
        &save_password,
        true,
    ));

    let side_panel = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .width_request(340)
        .css_classes(["panel"])
        .build();
    main_grid.attach(&side_panel, 1, 0, 1, 1);

    side_panel.append(&status_card());

    let status = gtk::Label::builder()
        .label("客户端：检查中\n服务：检查中\n网卡：eno1")
        .halign(gtk::Align::Start)
        .margin_start(18)
        .margin_end(18)
        .wrap(true)
        .css_classes(["status-copy"])
        .build();
    let status_hint = gtk::Label::builder()
        .label("准备就绪")
        .halign(gtk::Align::Start)
        .margin_start(18)
        .margin_end(18)
        .wrap(true)
        .css_classes(["hint"])
        .build();
    side_panel.append(&status);
    side_panel.append(&status_hint);

    let buttons = gtk::Grid::builder()
        .column_spacing(12)
        .row_spacing(12)
        .margin_top(4)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .hexpand(true)
        .build();
    side_panel.append(&buttons);

    let auth_btn = action_button(
        "network-transmit-receive-symbolic",
        "连接网络",
        "使用当前账号、网卡和 DHCP 设置发起有线认证",
        true,
        false,
    );
    let save_btn = action_button(
        "document-save-symbolic",
        "保存设置",
        "只保存账号、网卡和开关设置，不保存密码",
        false,
        false,
    );
    let refresh_btn = action_button(
        "view-refresh-symbolic",
        "刷新状态",
        "重新读取客户端、systemd 服务和日志状态",
        false,
        false,
    );
    let disconnect_btn = action_button(
        "network-offline-symbolic",
        "断开连接",
        "退出正在运行的锐捷认证客户端",
        false,
        false,
    );
    let logs_btn = action_button(
        "text-x-generic-symbolic",
        "实时日志",
        "打开 rjsupplicant.service 的实时日志窗口",
        false,
        false,
    );
    let enable_btn = action_button(
        "system-run-symbolic",
        "开机自启",
        "按当前账号、网卡和 DHCP 设置重写并启用 rjsupplicant.service",
        false,
        false,
    );
    let disable_btn = action_button(
        "window-close-symbolic",
        "取消自启",
        "禁用并停止 rjsupplicant.service",
        false,
        true,
    );

    buttons.attach(&auth_btn, 0, 0, 2, 1);
    buttons.attach(&save_btn, 0, 1, 1, 1);
    buttons.attach(&refresh_btn, 1, 1, 1, 1);
    buttons.attach(&disconnect_btn, 0, 2, 1, 1);
    buttons.attach(&logs_btn, 1, 2, 1, 1);
    buttons.attach(&enable_btn, 0, 3, 1, 1);
    buttons.attach(&disable_btn, 1, 3, 1, 1);

    let log_panel = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .css_classes(["panel", "log-panel"])
        .build();
    root.append(&log_panel);

    log_panel.append(&section_header("运行日志", "最近的认证服务输出"));

    let log = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .vexpand(true)
        .css_classes(["log-view"])
        .build();
    log.buffer().set_text("准备就绪。");

    let log_scroller = gtk::ScrolledWindow::builder()
        .min_content_height(180)
        .vexpand(true)
        .child(&log)
        .build();
    log_panel.append(&log_scroller);

    let ui = AppUi {
        username,
        password,
        nic,
        dhcp,
        save_password,
        status,
        status_hint,
        log,
        toasts,
        nics,
    };

    connect_actions(
        &ui,
        &auth_btn,
        &save_btn,
        &disconnect_btn,
        &refresh_btn,
        &enable_btn,
        &disable_btn,
        &logs_btn,
    );
    refresh_status(&ui);

    window.present();
}

fn connect_actions(
    ui: &AppUi,
    auth_btn: &gtk::Button,
    save_btn: &gtk::Button,
    disconnect_btn: &gtk::Button,
    refresh_btn: &gtk::Button,
    enable_btn: &gtk::Button,
    disable_btn: &gtk::Button,
    logs_btn: &gtk::Button,
) {
    let auth_ui = ui.clone();
    auth_btn.connect_clicked(move |_| {
        let Some(settings) = collect_settings(&auth_ui) else {
            return;
        };

        if let Err(err) = config::save(&settings) {
            toast(&auth_ui, &format!("设置保存失败：{err}"));
            return;
        }

        match system::authenticate(&settings, auth_ui.password.text().as_str()) {
            Ok(()) => {
                let password_note = if auth_ui.password.text().is_empty() {
                    "未传入密码，使用已保存密码"
                } else {
                    "已传入本次填写的密码"
                };
                append_log(
                    &auth_ui,
                    &format!(
                        "已请求连接：账号={}，网卡={}，{}。",
                        settings.username, settings.nic, password_note
                    ),
                );
                auth_ui.password.set_text("");
                toast(&auth_ui, "已发起认证");
                refresh_status(&auth_ui);
            }
            Err(err) => toast(&auth_ui, &format!("认证启动失败：{err}")),
        }
    });

    let save_ui = ui.clone();
    save_btn.connect_clicked(move |_| {
        let Some(settings) = collect_settings(&save_ui) else {
            return;
        };

        match config::save(&settings) {
            Ok(()) => {
                append_log(&save_ui, "设置已保存。");
                toast(&save_ui, "设置已保存");
            }
            Err(err) => toast(&save_ui, &format!("设置保存失败：{err}")),
        }
    });

    let disconnect_ui = ui.clone();
    disconnect_btn.connect_clicked(move |_| match system::disconnect() {
        Ok(()) => {
            append_log(&disconnect_ui, "已请求断开认证。");
            toast(&disconnect_ui, "已请求断开");
            refresh_status(&disconnect_ui);
        }
        Err(err) => toast(&disconnect_ui, &format!("断开失败：{err}")),
    });

    let refresh_ui = ui.clone();
    refresh_btn.connect_clicked(move |_| refresh_status(&refresh_ui));

    let enable_ui = ui.clone();
    enable_btn.connect_clicked(move |_| {
        let Some(settings) = collect_settings(&enable_ui) else {
            return;
        };

        if let Err(err) = config::save(&settings) {
            toast(&enable_ui, &format!("设置保存失败：{err}"));
            return;
        }

        match system::enable_service(&settings) {
            Ok(()) => {
                append_log(
                    &enable_ui,
                    &format!(
                        "已按当前设置启用开机自启：账号={}，网卡={}，DHCP={}。",
                        settings.username, settings.nic, settings.dhcp
                    ),
                );
                toast(&enable_ui, "已启用自启");
                refresh_status(&enable_ui);
            }
            Err(err) => toast(&enable_ui, &format!("启用自启失败：{err}")),
        }
    });

    let disable_ui = ui.clone();
    disable_btn.connect_clicked(move |_| match system::disable_service() {
        Ok(()) => {
            append_log(&disable_ui, "已请求取消开机自启。");
            toast(&disable_ui, "已请求取消自启");
            refresh_status(&disable_ui);
        }
        Err(err) => toast(&disable_ui, &format!("取消自启失败：{err}")),
    });

    let logs_ui = ui.clone();
    logs_btn.connect_clicked(move |_| match system::open_live_log() {
        Ok(()) => append_log(&logs_ui, "已打开实时日志窗口。"),
        Err(err) => toast(&logs_ui, &format!("日志窗口打开失败：{err}")),
    });
}

fn collect_settings(ui: &AppUi) -> Option<config::Settings> {
    let username = ui.username.text().trim().to_string();
    if username.is_empty() {
        toast(ui, "请先输入校园网账号");
        ui.username.grab_focus();
        return None;
    }

    Some(config::Settings {
        username,
        nic: selected_nic(ui),
        dhcp: ui.dhcp.is_active(),
        save_password: ui.save_password.is_active(),
    })
}

fn refresh_status(ui: &AppUi) {
    let status = system::load_status();
    let client = if status.client_installed {
        "已安装"
    } else {
        "未找到"
    };
    let active = normalize_status(&status.service_active);
    let enabled = normalize_status(&status.service_enabled);

    ui.status.set_text(&format!(
        "客户端：{}\n服务：{} / {}\n网卡：{}",
        client,
        enabled,
        active,
        selected_nic(ui)
    ));
    ui.status_hint
        .set_text(match status.service_active.as_str() {
            "active" => "服务正在运行，网络认证已发起。",
            "inactive" => "服务未运行，点击连接网络即可发起认证。",
            "failed" => "服务处于失败状态，请查看运行日志。",
            _ => "状态已刷新。",
        });
    ui.log.buffer().set_text(&status.last_log);
}

fn selected_nic(ui: &AppUi) -> String {
    let selected = ui.nic.selected() as usize;
    ui.nics
        .borrow()
        .get(selected)
        .cloned()
        .unwrap_or_else(|| "eno1".to_string())
}

fn select_default_nic(dropdown: &gtk::DropDown, nics: &[String], preferred: &str) {
    let index = nics
        .iter()
        .position(|name| !preferred.is_empty() && name == preferred)
        .or_else(|| nics.iter().position(|name| name == "eno1"))
        .unwrap_or(0);
    dropdown.set_selected(index as u32);
}

fn normalize_status(status: &str) -> &str {
    match status {
        "enabled" => "已自启",
        "disabled" => "未自启",
        "active" => "运行中",
        "inactive" => "已停止",
        "failed" => "失败",
        _ => "未知",
    }
}

fn toast(ui: &AppUi, message: &str) {
    ui.toasts.add_toast(adw::Toast::new(message));
}

fn append_log(ui: &AppUi, line: &str) {
    let buffer = ui.log.buffer();
    let current = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string();
    let next = if current.is_empty() {
        line.to_string()
    } else {
        format!("{}\n{}", current, line)
    };
    buffer.set_text(&next);
}

fn app_header() -> adw::HeaderBar {
    let header = adw::HeaderBar::new();
    header.add_css_class("chrome-bar");

    let center = gtk::Box::builder().width_request(1).build();
    header.set_title_widget(Some(&center));
    header
}

fn brand_header() -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Start)
        .css_classes(["brand-row"])
        .build();
    row.append(&icon_tile("network-wired-symbolic", "brand-icon"));

    let copy = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(1)
        .build();
    copy.append(
        &gtk::Label::builder()
            .label("锐捷有线认证")
            .halign(gtk::Align::Start)
            .css_classes(["app-title"])
            .build(),
    );
    copy.append(
        &gtk::Label::builder()
            .label("GDUFS Wired Network Client")
            .halign(gtk::Align::Start)
            .css_classes(["caption"])
            .build(),
    );
    row.append(&copy);
    row
}

fn hero_panel() -> gtk::Box {
    let panel = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(16)
        .halign(gtk::Align::Fill)
        .css_classes(["hero"])
        .build();
    panel.append(&icon_tile("network-wired-symbolic", "hero-icon"));

    let copy = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Start)
        .hexpand(true)
        .build();
    copy.append(
        &gtk::Label::builder()
            .label("校园有线网络")
            .halign(gtk::Align::Start)
            .css_classes(["hero-title"])
            .build(),
    );
    copy.append(
        &gtk::Label::builder()
            .label("一键连接、断开和管理开机自启。密码只在需要首次保存或修改时填写。")
            .halign(gtk::Align::Start)
            .wrap(true)
            .css_classes(["body-copy"])
            .build(),
    );
    panel.append(&copy);
    panel
}

fn status_card() -> gtk::Box {
    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(14)
        .margin_top(18)
        .margin_bottom(2)
        .margin_start(18)
        .margin_end(18)
        .tooltip_text("显示官方客户端、systemd 服务和当前网卡状态")
        .build();
    card.append(&icon_tile("network-wired-symbolic", "status-icon"));

    let copy = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .valign(gtk::Align::Center)
        .build();
    copy.append(
        &gtk::Label::builder()
            .label("当前状态")
            .halign(gtk::Align::Start)
            .css_classes(["section-title"])
            .build(),
    );
    copy.append(
        &gtk::Label::builder()
            .label("认证服务与客户端状态")
            .halign(gtk::Align::Start)
            .css_classes(["caption"])
            .build(),
    );
    card.append(&copy);
    card
}

fn section_header(title: &str, subtitle: &str) -> gtk::Box {
    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(5)
        .margin_top(18)
        .margin_bottom(14)
        .margin_start(18)
        .margin_end(18)
        .build();
    header.append(
        &gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .css_classes(["section-title"])
            .build(),
    );
    header.append(
        &gtk::Label::builder()
            .label(subtitle)
            .halign(gtk::Align::Start)
            .wrap(true)
            .css_classes(["caption"])
            .build(),
    );
    header
}

fn setting_row<W: IsA<gtk::Widget>>(
    icon: &str,
    title: &str,
    subtitle: &str,
    control: &W,
    last: bool,
) -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(14)
        .margin_start(18)
        .margin_end(18)
        .margin_top(0)
        .margin_bottom(0)
        .height_request(76)
        .css_classes(["setting-row"])
        .build();
    if last {
        row.add_css_class("last-row");
    }

    let icon_slot = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .width_request(18)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .build();
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(16);
    image.add_css_class("row-icon");
    icon_slot.append(&image);
    row.append(&icon_slot);

    let text = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(3)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();
    text.append(
        &gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .css_classes(["row-title"])
            .build(),
    );
    text.append(
        &gtk::Label::builder()
            .label(subtitle)
            .halign(gtk::Align::Start)
            .wrap(true)
            .css_classes(["row-subtitle"])
            .build(),
    );
    row.append(&text);

    let control_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::End)
        .build();
    control_box.append(control);
    row.append(&control_box);

    row
}

fn action_button(
    icon: &str,
    label: &str,
    tooltip: &str,
    primary: bool,
    danger: bool,
) -> gtk::Button {
    let button = gtk::Button::builder()
        .hexpand(true)
        .tooltip_text(tooltip)
        .css_classes(["action-button"])
        .build();
    button.set_cursor_from_name(Some("pointer"));
    if primary {
        button.add_css_class("primary-action");
    }
    if danger {
        button.add_css_class("danger-action");
    }
    button.set_child(Some(&button_content(icon, label)));
    button
}

fn button_content(icon: &str, label: &str) -> gtk::Box {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(16);
    content.append(&image);
    content.append(
        &gtk::Label::builder()
            .label(label)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build(),
    );
    content
}

fn icon_tile(icon: &str, css_class: &str) -> gtk::Box {
    let tile = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .css_classes([css_class])
        .build();
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(26);
    image.set_halign(gtk::Align::Center);
    image.set_valign(gtk::Align::Center);
    tile.append(&image);
    tile
}

pub fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        r#"
        window,
        .app-shell {
          background: #f7f8f4;
          color: #1d2721;
        }

        headerbar {
          background: #f7f8f4;
          box-shadow: none;
          border-bottom: 1px solid rgba(47, 84, 62, 0.10);
        }

        .chrome-bar {
          min-height: 34px;
          padding-top: 0;
          padding-bottom: 0;
        }

        .brand-row {
          min-height: 46px;
        }

        .app-title {
          font-size: 18px;
          font-weight: 800;
          color: #1d2721;
        }

        .caption {
          font-size: 13px;
          color: #6d7871;
        }

        .body-copy {
          font-size: 14px;
          color: #4e5d55;
        }

        .hero {
          padding: 18px;
          border-radius: 10px;
          background: linear-gradient(90deg, #fbfdf9, #eef8f0);
          border: 1px solid rgba(65, 128, 88, 0.18);
        }

        .hero-title {
          font-size: 20px;
          font-weight: 800;
        }

        .brand-icon,
        .hero-icon,
        .status-icon {
          color: #29945d;
          background: #dff4e7;
          border: 1px solid rgba(41, 148, 93, 0.20);
          border-radius: 10px;
          min-width: 52px;
          min-height: 52px;
          padding: 0;
        }

        .brand-icon {
          min-width: 42px;
          min-height: 42px;
        }

        .status-icon {
          min-width: 64px;
          min-height: 64px;
          border-radius: 999px;
        }

        .panel {
          border-radius: 10px;
          background: #ffffff;
          border: 1px solid rgba(44, 67, 55, 0.12);
          box-shadow: 0 8px 24px rgba(29, 39, 33, 0.045);
        }

        .section-title {
          font-size: 16px;
          font-weight: 800;
        }

        .setting-row {
          border-bottom: 1px solid rgba(44, 67, 55, 0.10);
        }

        .last-row {
          border-bottom: none;
        }

        .row-icon {
          color: #80908a;
        }

        .row-title {
          font-size: 15px;
          font-weight: 700;
          color: #1d2721;
        }

        .row-subtitle {
          font-size: 13px;
          color: #6d7871;
        }

        .compact-input,
        .compact-select {
          min-height: 44px;
          border-radius: 8px;
        }

        .compact-input:focus,
        .compact-select:focus {
          outline: 2px solid alpha(#65c18b, 0.36);
          outline-offset: 2px;
          border-color: #65c18b;
        }

        switch:checked {
          background: #65c18b;
        }

        .status-copy {
          font-size: 14px;
          font-weight: 700;
          line-height: 1.45;
        }

        .hint {
          font-size: 13px;
          color: #6d7871;
        }

        .action-button {
          min-height: 48px;
          border-radius: 8px;
          font-weight: 700;
          background: #fbfcfa;
          border: 1px solid rgba(44, 67, 55, 0.12);
          color: #314139;
          box-shadow: 0 1px 0 rgba(29, 39, 33, 0.04);
        }

        .action-button:hover {
          background: #eef7f1;
          border-color: rgba(41, 148, 93, 0.28);
          box-shadow: 0 6px 16px rgba(44, 67, 55, 0.08);
        }

        .action-button:active {
          background: #e1eee6;
          border-color: rgba(41, 148, 93, 0.38);
          box-shadow: inset 0 1px 3px rgba(29, 39, 33, 0.12);
        }

        .action-button:focus-visible {
          outline: 2px solid alpha(#65c18b, 0.45);
          outline-offset: 2px;
        }

        .primary-action {
          color: #ffffff;
          background: #65c18b;
          border-color: #65c18b;
          box-shadow: 0 8px 18px rgba(101, 193, 139, 0.26);
        }

        .primary-action:hover {
          background: #57b67f;
          border-color: #57b67f;
          box-shadow: 0 10px 24px rgba(101, 193, 139, 0.34);
        }

        .primary-action:active {
          background: #48a970;
          border-color: #48a970;
          box-shadow: inset 0 2px 5px rgba(24, 93, 57, 0.22);
        }

        .danger-action {
          color: #c9362b;
        }

        .danger-action:hover {
          background: #fff2f0;
          border-color: rgba(201, 54, 43, 0.22);
        }

        .danger-action:active {
          background: #ffe5e1;
          border-color: rgba(201, 54, 43, 0.32);
        }

        .log-panel {
          padding-bottom: 14px;
        }

        .log-view {
          padding: 12px;
          border-radius: 8px;
          background: #fbfcfa;
          color: #48554f;
          font-size: 12px;
        }
        "#,
    );

    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
