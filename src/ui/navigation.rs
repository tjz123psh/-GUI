use super::AppUi;
use crate::config;
use adw::prelude::*;
use gtk::{gio, glib};
use gtk4 as gtk;
use libadwaita as adw;

pub(super) struct SidebarUi {
    pub(super) root: gtk::Box,
    pub(super) compact_root: gtk::Box,
    pub(super) status_label: gtk::Label,
    pub(super) all_buttons: Vec<gtk::Button>,
    pub(super) status_buttons: Vec<gtk::Button>,
    pub(super) auth_buttons: Vec<gtk::Button>,
    pub(super) runtime_buttons: Vec<gtk::Button>,
    pub(super) logs_buttons: Vec<gtk::Button>,
    pub(super) settings_buttons: Vec<gtk::Button>,
    pub(super) about_buttons: Vec<gtk::Button>,
}

pub(super) fn sidebar_navigation() -> SidebarUi {
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

    let bytes = glib::Bytes::from_static(include_bytes!("../../data/sidebar-landscape.png"));
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

pub(super) fn connect_sidebar(
    ui: &AppUi,
    sidebar: &SidebarUi,
    connection_page: &gtk::ScrolledWindow,
) {
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
                    crate::system::client_display_path().display()
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
                .body(format!(
                    "面向 GDUFS 校园有线网的 GTK 客户端\n版本 {}\n\n基于学校提供的官方 Linux 认证程序。",
                    env!("CARGO_PKG_VERSION")
                ))
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
