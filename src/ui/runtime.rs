use super::{AppUi, StageUi};
use crate::{config, system};
use adw::prelude::*;
use gtk::{gio, glib};
use gtk4 as gtk;
use libadwaita as adw;
use std::time::Duration;

pub(super) fn connect_actions(ui: &AppUi) {
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

    let banner_ui = ui.clone();
    ui.client_banner.connect_button_clicked(move |_| {
        choose_official_client(&banner_ui);
    });
}

fn choose_official_client(ui: &AppUi) {
    if ui.busy.get() || ui.refreshing.get() {
        return;
    }

    let zip_filter = gtk::FileFilter::new();
    zip_filter.set_name(Some("ZIP 安装包"));
    zip_filter.add_mime_type("application/zip");
    zip_filter.add_pattern("*.zip");
    zip_filter.add_pattern("*.ZIP");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&zip_filter);
    let dialog = gtk::FileDialog::builder()
        .title("选择官方锐捷 Linux 客户端")
        .accept_label("安装")
        .filters(&filters)
        .default_filter(&zip_filter)
        .modal(true)
        .build();

    let dialog_ui = ui.clone();
    glib::spawn_future_local(async move {
        match dialog.open_future(Some(&dialog_ui.window)).await {
            Ok(file) => {
                let Some(path) = file.path() else {
                    toast(&dialog_ui, "只能安装本机文件，请重新选择 ZIP 安装包");
                    return;
                };
                install_client_archive(&dialog_ui, path);
            }
            Err(err) if err.matches(gtk::DialogError::Dismissed) => {}
            Err(err) => toast(&dialog_ui, &format!("无法选择安装包：{err}")),
        }
    });
}

fn install_client_archive(ui: &AppUi, path: std::path::PathBuf) {
    if ui.busy.replace(true) {
        return;
    }
    ui.client_banner.set_title("正在安装官方锐捷客户端…");
    ui.status_detail.set_text("正在校验并解压官方客户端");
    ui.status_spinner.set_visible(true);
    set_badge(ui, "安装中", "badge-working");
    update_controls(ui);

    let task_ui = ui.clone();
    glib::spawn_future_local(async move {
        match gio::spawn_blocking(move || system::install_official_client(&path)).await {
            Ok(Ok(())) => {
                task_ui.client_banner.set_revealed(false);
                toast(&task_ui, "官方锐捷客户端已安装");
            }
            Ok(Err(err)) => {
                task_ui
                    .client_banner
                    .set_title("未安装官方锐捷客户端，连接功能不可用");
                toast(&task_ui, &format!("安装官方客户端失败：{err}"));
            }
            Err(_) => {
                task_ui
                    .client_banner
                    .set_title("未安装官方锐捷客户端，连接功能不可用");
                toast(&task_ui, "安装官方客户端失败：后台任务异常终止");
            }
        }
        task_ui.busy.set(false);
        refresh_status(&task_ui);
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

pub(super) fn refresh_status(ui: &AppUi) {
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
    let enabled = service_is_enabled(&status.service_enabled) && !status.service_requires_migration;
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
        if status.client_requires_migration {
            "需迁移"
        } else if status.client_installed {
            "已就绪"
        } else {
            "未安装"
        },
        if status.client_installed && !status.client_requires_migration {
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
    } else if status.client_requires_migration {
        "旧版客户端可用 · 需迁移到系统安全路径"
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
    ui.service_row
        .set_subtitle(if status.service_requires_migration {
            "旧版服务需迁移 · 打开开关以重写安全服务"
        } else if enabled {
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

    if status.client_requires_migration {
        ui.client_banner
            .set_title("检测到旧版客户端，重新选择安装包以完成安全迁移");
    } else {
        ui.client_banner
            .set_title("未安装官方锐捷客户端，连接功能不可用");
    }
    ui.client_banner
        .set_revealed(!status.client_installed || status.client_requires_migration);
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

pub(super) fn update_controls(ui: &AppUi) {
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
    ui.client_banner.set_sensitive(!unavailable);

    if unavailable {
        return;
    }
    let status = ui.last_status.borrow();
    if let Some(status) = status.as_ref() {
        ui.action_btn
            .set_sensitive(status.client_installed && !status.client_running);
        ui.disconnect_btn
            .set_sensitive(status.client_installed && status.client_running);
        ui.autostart
            .set_sensitive(status.client_installed && !status.client_requires_migration);
        ui.restart_btn.set_sensitive(
            status.client_installed
                && !status.client_requires_migration
                && !status.service_requires_migration,
        );
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

pub(super) fn preferred_nic_index(nics: &[String], preferred: &str) -> usize {
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
