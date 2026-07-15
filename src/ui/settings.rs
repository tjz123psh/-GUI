use super::AppUi;
use super::runtime::{preferred_nic_index, toast};
use crate::config;
use adw::prelude::*;
use gtk::gio;
use gtk4 as gtk;
use libadwaita as adw;

pub(super) fn install_settings_action(app: &adw::Application, ui: &AppUi) {
    let action = gio::SimpleAction::new("settings", None);
    let action_ui = ui.clone();
    action.connect_activate(move |_, _| show_settings_dialog(&action_ui));
    app.add_action(&action);
    app.set_accels_for_action("app.settings", &["<Control>comma"]);
}

pub(super) fn show_settings_dialog(ui: &AppUi) {
    let interfaces = ui.nics.borrow().clone();
    let current_nic = ui
        .nic_model
        .string(ui.nic.selected())
        .map(|value| value.to_string())
        .unwrap_or_else(|| "eno1".to_string());
    let interface_model =
        gtk::StringList::new(&interfaces.iter().map(String::as_str).collect::<Vec<_>>());

    let dialog = adw::PreferencesDialog::builder()
        .title("设置")
        .content_width(520)
        .content_height(560)
        .search_enabled(false)
        .css_classes(["settings-dialog"])
        .build();
    let page = adw::PreferencesPage::builder()
        .title("连接")
        .icon_name("network-wired-symbolic")
        .css_classes(["settings-page"])
        .build();

    let account_group = adw::PreferencesGroup::builder()
        .title("认证设置")
        .description("密码只在连接时输入，不会保存在本应用中")
        .build();
    let username = adw::EntryRow::builder()
        .title("校园网账号")
        .text(ui.username.text())
        .input_purpose(gtk::InputPurpose::FreeForm)
        .build();
    let nic = adw::ComboRow::builder()
        .title("有线网卡")
        .subtitle("选择连接校园有线网络的接口")
        .model(&interface_model)
        .selected(preferred_nic_index(&interfaces, &current_nic) as u32)
        .build();
    account_group.add(&username);
    account_group.add(&nic);
    page.add(&account_group);

    let connection_group = adw::PreferencesGroup::builder().title("连接选项").build();
    let dhcp = adw::SwitchRow::builder()
        .title("自动获取网络地址")
        .subtitle("校园有线网络通常需要 DHCP")
        .active(ui.dhcp.is_active())
        .build();
    let save_password = adw::SwitchRow::builder()
        .title("交给官方客户端保存密码")
        .subtitle("关闭后，本次密码仅用于当前认证")
        .active(ui.save_password.is_active())
        .build();
    connection_group.add(&dhcp);
    connection_group.add(&save_password);
    page.add(&connection_group);

    let action_group = adw::PreferencesGroup::new();
    let save = adw::ButtonRow::builder()
        .title("保存设置")
        .start_icon_name("document-save-symbolic")
        .activatable(true)
        .css_classes(["suggested-action"])
        .build();
    action_group.add(&save);
    page.add(&action_group);

    username.connect_changed(|row| row.remove_css_class("error"));

    let save_dialog = dialog.downgrade();
    let save_ui = ui.clone();
    let save_model = interface_model.clone();
    let save_username = username.clone();
    let save_nic = nic.clone();
    let save_dhcp = dhcp.clone();
    let save_password_row = save_password.clone();
    save.connect_activated(move |_| {
        let Some(save_dialog) = save_dialog.upgrade() else {
            return;
        };
        let Some(nic_name) = save_model.string(save_nic.selected()) else {
            save_dialog.add_toast(adw::Toast::new("请选择有线网卡"));
            return;
        };
        let settings = config::Settings {
            username: save_username.text().trim().to_string(),
            nic: nic_name.to_string(),
            dhcp: save_dhcp.is_active(),
            save_password: save_password_row.is_active(),
        };
        if let Err(err) = config::validate(&settings) {
            save_username.add_css_class("error");
            save_username.grab_focus();
            save_dialog.add_toast(adw::Toast::new(&err.to_string()));
            return;
        }
        if let Err(err) = config::save(&settings) {
            save_dialog.add_toast(adw::Toast::new(&format!("设置保存失败：{err}")));
            return;
        }

        save_ui.username.set_text(&settings.username);
        save_ui
            .nic
            .set_selected(preferred_nic_index(&save_ui.nics.borrow(), &settings.nic) as u32);
        save_ui.dhcp.set_active(settings.dhcp);
        save_ui.save_password.set_active(settings.save_password);
        let _ = save_dialog.close();
        toast(&save_ui, "设置已保存（不包含密码）");
    });

    dialog.add(&page);
    dialog.set_default_widget(Some(&save));
    dialog.present(Some(&ui.window));
    username.grab_focus();
}
