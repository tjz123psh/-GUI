use crate::config::{self, SERVICE, Settings};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ClientStatus {
    pub client_installed: bool,
    pub service_enabled: String,
    pub service_active: String,
    pub last_log: String,
}

#[derive(Clone, Debug)]
pub enum Action {
    Authenticate,
    Disconnect,
    EnableService,
    DisableService,
}

pub fn load_status() -> ClientStatus {
    ClientStatus {
        client_installed: config::client_path().exists() && config::client_binary_path().exists(),
        service_enabled: command_text("systemctl", &["is-enabled", SERVICE])
            .unwrap_or_else(|| "unknown".to_string()),
        service_active: command_text("systemctl", &["is-active", SERVICE])
            .unwrap_or_else(|| "unknown".to_string()),
        last_log: recent_log(),
    }
}

pub fn wired_interfaces() -> Vec<String> {
    let mut names = fs::read_dir("/sys/class/net")
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name != "lo")
        .collect::<Vec<_>>();

    names.sort();
    if names.is_empty() {
        names.push("eno1".to_string());
    }
    names
}

pub fn authenticate(settings: &Settings, password: &str) -> Result<()> {
    let spec = authenticate_command(settings, password);
    run_elevated(Action::Authenticate, &spec.program, &spec.args)
}

pub fn disconnect() -> Result<()> {
    let spec = disconnect_command();
    run_elevated(Action::Disconnect, &spec.program, &spec.args)
}

pub fn enable_service() -> Result<()> {
    let spec = enable_service_command();
    run_elevated(Action::EnableService, &spec.program, &spec.args)
}

pub fn disable_service() -> Result<()> {
    let spec = disable_service_command();
    run_elevated(Action::DisableService, &spec.program, &spec.args)
}

pub fn open_live_log() -> Result<()> {
    run_terminal(
        "锐捷认证日志",
        &[
            "journalctl".to_string(),
            "-u".to_string(),
            SERVICE.to_string(),
            "-n".to_string(),
            "120".to_string(),
            "-f".to_string(),
        ],
    )
}

fn run_elevated(action: Action, program: &str, args: &[String]) -> Result<()> {
    if command_exists("pkexec") {
        Command::new("pkexec")
            .arg(program)
            .args(args)
            .spawn()
            .with_context(|| format!("无法启动系统授权：{}", action_label(&action)))?;
        return Ok(());
    }

    let mut terminal_args = vec!["sudo".to_string(), program.to_string()];
    terminal_args.extend(args.iter().cloned());
    run_terminal(action_label(&action), &terminal_args)
}

pub fn authenticate_command(settings: &Settings, password: &str) -> CommandSpec {
    let dhcp = if settings.dhcp { "1" } else { "0" };
    let save = if settings.save_password { "1" } else { "0" };

    let mut args = vec![
        "-a".to_string(),
        "1".to_string(),
        "-d".to_string(),
        dhcp.to_string(),
        "-n".to_string(),
        settings.nic.clone(),
        "-u".to_string(),
        settings.username.clone(),
        "-S".to_string(),
        save.to_string(),
    ];

    if !password.trim().is_empty() {
        args.push("-p".to_string());
        args.push(password.to_string());
    }

    CommandSpec {
        program: config::path_string(&config::client_path()),
        args,
    }
}

pub fn disconnect_command() -> CommandSpec {
    CommandSpec {
        program: config::path_string(&config::client_path()),
        args: vec!["-q".to_string()],
    }
}

pub fn enable_service_command() -> CommandSpec {
    CommandSpec {
        program: "systemctl".to_string(),
        args: vec![
            "enable".to_string(),
            "--now".to_string(),
            SERVICE.to_string(),
        ],
    }
}

pub fn disable_service_command() -> CommandSpec {
    CommandSpec {
        program: "systemctl".to_string(),
        args: vec![
            "disable".to_string(),
            "--now".to_string(),
            SERVICE.to_string(),
        ],
    }
}

fn run_terminal(title: &str, args: &[String]) -> Result<()> {
    if command_exists("kitty") {
        Command::new("kitty")
            .args(["--title", title, "-e"])
            .args(args)
            .spawn()
            .context("无法打开 kitty")?;
        return Ok(());
    }

    if command_exists("foot") {
        Command::new("foot")
            .args(["--title", title])
            .args(args)
            .spawn()
            .context("无法打开 foot")?;
        return Ok(());
    }

    if command_exists("alacritty") {
        Command::new("alacritty")
            .args(["--title", title, "-e"])
            .args(args)
            .spawn()
            .context("无法打开 alacritty")?;
        return Ok(());
    }

    if command_exists("xterm") {
        Command::new("xterm")
            .args(["-T", title, "-e"])
            .args(args)
            .spawn()
            .context("无法打开 xterm")?;
        return Ok(());
    }

    anyhow::bail!("找不到 pkexec 或可用终端，无法执行需要管理员权限的命令");
}

fn recent_log() -> String {
    command_text("journalctl", &["-u", SERVICE, "-n", "80", "--no-pager"])
        .filter(|text| !text.is_empty())
        .or_else(|| fs::read_to_string(config::log_path()).ok())
        .unwrap_or_else(|| "暂无日志。".to_string())
}

fn command_text(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).to_string()
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };

    Some(text.trim().to_string())
}

fn command_exists(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).exists();
    }

    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&paths)
        .map(|path| path.join(program))
        .any(is_executable_file)
}

fn is_executable_file(path: PathBuf) -> bool {
    path.is_file()
        && path
            .metadata()
            .map(|metadata| {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    true
                }
            })
            .unwrap_or(false)
}

fn action_label(action: &Action) -> &'static str {
    match action {
        Action::Authenticate => "锐捷有线认证",
        Action::Disconnect => "断开锐捷认证",
        Action::EnableService => "启用锐捷开机自启",
        Action::DisableService => "禁用锐捷开机自启",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            username: "20260001".to_string(),
            nic: "enp4s0".to_string(),
            dhcp: true,
            save_password: true,
        }
    }

    #[test]
    fn builds_authenticate_command_without_password() {
        let spec = authenticate_command(&settings(), "");

        assert_eq!(spec.program, config::path_string(&config::client_path()));
        assert_eq!(
            spec.args,
            [
                "-a", "1", "-d", "1", "-n", "enp4s0", "-u", "20260001", "-S", "1"
            ]
        );
    }

    #[test]
    fn builds_authenticate_command_with_password_only_when_present() {
        let spec = authenticate_command(&settings(), "secret");

        assert_eq!(
            spec.args,
            [
                "-a", "1", "-d", "1", "-n", "enp4s0", "-u", "20260001", "-S", "1", "-p", "secret"
            ]
        );
    }

    #[test]
    fn builds_static_ip_no_save_authenticate_command() {
        let mut settings = settings();
        settings.dhcp = false;
        settings.save_password = false;

        let spec = authenticate_command(&settings, "  ");

        assert_eq!(
            spec.args,
            [
                "-a", "1", "-d", "0", "-n", "enp4s0", "-u", "20260001", "-S", "0"
            ]
        );
    }

    #[test]
    fn builds_disconnect_command() {
        assert_eq!(
            disconnect_command(),
            CommandSpec {
                program: config::path_string(&config::client_path()),
                args: vec!["-q".to_string()]
            }
        );
    }

    #[test]
    fn builds_service_commands() {
        assert_eq!(
            enable_service_command(),
            CommandSpec {
                program: "systemctl".to_string(),
                args: vec![
                    "enable".to_string(),
                    "--now".to_string(),
                    SERVICE.to_string()
                ]
            }
        );
        assert_eq!(
            disable_service_command(),
            CommandSpec {
                program: "systemctl".to_string(),
                args: vec![
                    "disable".to_string(),
                    "--now".to_string(),
                    SERVICE.to_string()
                ]
            }
        );
    }
}
