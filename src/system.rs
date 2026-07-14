use crate::config::{self, SERVICE, Settings};
use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SERVICE_PATH: &str = "/etc/systemd/system/rjsupplicant.service";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ClientStatus {
    pub client_installed: bool,
    pub client_running: bool,
    pub client_uptime_seconds: Option<u64>,
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
    RestartService,
}

pub fn load_status() -> ClientStatus {
    let (client_running, client_uptime_seconds) = client_process_info();
    ClientStatus {
        client_installed: config::client_path().exists() && config::client_binary_path().exists(),
        client_running,
        client_uptime_seconds,
        service_enabled: command_text("systemctl", &["is-enabled", SERVICE])
            .unwrap_or_else(|| "unknown".to_string()),
        service_active: command_text("systemctl", &["is-active", SERVICE])
            .unwrap_or_else(|| "unknown".to_string()),
        last_log: recent_log(),
    }
}

pub fn wired_interfaces() -> Vec<String> {
    let mut ethernet = fs::read_dir("/sys/class/net")
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let path = entry.path();
            let link_type = fs::read_to_string(path.join("type")).ok()?;
            if name == "lo" || link_type.trim() != "1" || path.join("wireless").exists() {
                return None;
            }
            Some((name, path.join("device").exists()))
        })
        .collect::<Vec<_>>();

    ethernet.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let has_physical = ethernet.iter().any(|(_, physical)| *physical);
    let mut names = ethernet
        .into_iter()
        .filter(|(_, physical)| !has_physical || *physical)
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    if names.is_empty() {
        names.push("eno1".to_string());
    }
    names
}

pub fn interface_has_carrier(name: &str) -> bool {
    fs::read_to_string(Path::new("/sys/class/net").join(name).join("carrier"))
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
}

pub fn authenticate(settings: &Settings, password: &str) -> Result<()> {
    config::validate(settings)?;
    ensure_client_installed()?;
    let spec = authenticate_command(settings, password);
    run_elevated_wait(Action::Authenticate, &spec.program, &spec.args)
}

pub fn disconnect() -> Result<()> {
    ensure_client_installed()?;
    if command_text("systemctl", &["is-active", SERVICE]).as_deref() == Some("active") {
        let spec = stop_service_command();
        return run_elevated_wait(Action::Disconnect, &spec.program, &spec.args);
    }
    let spec = disconnect_command();
    run_elevated_wait(Action::Disconnect, &spec.program, &spec.args)
}

pub fn enable_service(settings: &Settings) -> Result<()> {
    install_service(settings)?;
    let spec = enable_service_command();
    run_elevated_wait(Action::EnableService, &spec.program, &spec.args)
}

pub fn disable_service() -> Result<()> {
    let spec = disable_service_command();
    run_elevated_wait(Action::DisableService, &spec.program, &spec.args)
}

pub fn restart_service() -> Result<()> {
    ensure_client_installed()?;
    run_elevated_wait(
        Action::RestartService,
        "systemctl",
        &["restart".to_string(), SERVICE.to_string()],
    )
}

pub fn test_connectivity() -> Result<()> {
    if !command_exists("ping") {
        anyhow::bail!("找不到 ping，请安装 iputils");
    }
    let status = Command::new("ping")
        .args(["-c", "1", "-W", "2", "223.5.5.5"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("无法启动网络连通测试")?;
    if status.success() {
        return Ok(());
    }
    anyhow::bail!("无法访问测试地址 223.5.5.5，请检查认证状态")
}

pub fn open_client_folder() -> Result<()> {
    open_with_default_app(config::data_dir())
}

pub fn open_help() -> Result<()> {
    open_with_default_app("https://etr.gdufs.edu.cn/info/1303/5137.htm")
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

fn run_elevated_wait(action: Action, program: &str, args: &[String]) -> Result<()> {
    if command_exists("pkexec") {
        let status = Command::new("pkexec")
            .arg(program)
            .args(args)
            .status()
            .with_context(|| format!("无法启动系统授权：{}", action_label(&action)))?;

        if status.success() {
            return Ok(());
        }

        anyhow::bail!("{} 执行失败：{}", action_label(&action), status);
    }

    run_elevated(action, program, args)
}

fn install_service(settings: &Settings) -> Result<()> {
    config::validate(settings)?;
    ensure_client_installed()?;

    write_root_file(SERVICE_PATH, &service_file(settings))?;
    run_elevated_wait(
        Action::EnableService,
        "systemctl",
        &["daemon-reload".to_string()],
    )
}

fn write_root_file(path: &str, content: &str) -> Result<()> {
    if !command_exists("pkexec") {
        anyhow::bail!("找不到 pkexec，无法写入 {}", path);
    }

    let mut child = Command::new("pkexec")
        .arg("tee")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .with_context(|| format!("无法请求管理员授权写入 {}", path))?;

    child
        .stdin
        .as_mut()
        .context("无法写入 systemd 服务内容")?
        .write_all(content.as_bytes())?;

    let status = child.wait()?;
    if status.success() {
        return Ok(());
    }

    anyhow::bail!("写入 {} 失败：{}", path, status);
}

pub fn service_file(settings: &Settings) -> String {
    let dhcp = if settings.dhcp { "1" } else { "0" };
    let save = if settings.save_password { "1" } else { "0" };
    let client = systemd_quote(&config::path_string(&config::client_path()));
    let nic = systemd_quote(settings.nic.trim());
    let username = systemd_quote(settings.username.trim());
    let workdir = systemd_quote(&config::path_string(
        config::client_binary_path()
            .parent()
            .unwrap_or_else(|| Path::new("/")),
    ));

    format!(
        "[Unit]\n\
         Description=Ruijie RG-SU wired authentication client\n\
         Documentation=https://etr.gdufs.edu.cn/info/1303/5137.htm\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=forking\n\
         GuessMainPID=yes\n\
         ExecStart={} -a 1 -d {} -n {} -u {} -S {}\n\
         ExecStop={} -q\n\
         Restart=on-failure\n\
         RestartSec=10\n\
         TimeoutStartSec=30\n\
         TimeoutStopSec=15\n\
         WorkingDirectory={}\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        client, dhcp, nic, username, save, client, workdir
    )
}

fn systemd_quote(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    format!("\"{escaped}\"")
}

fn ensure_client_installed() -> Result<()> {
    if config::client_path().exists() && config::client_binary_path().exists() {
        return Ok(());
    }

    anyhow::bail!("官方客户端未安装，请先运行 scripts/install.sh 并放入官方客户端 zip")
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

pub fn stop_service_command() -> CommandSpec {
    CommandSpec {
        program: "systemctl".to_string(),
        args: vec!["stop".to_string(), SERVICE.to_string()],
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
    let journal = command_text("journalctl", &["-u", SERVICE, "-n", "60", "--no-pager"])
        .filter(|text| !text.is_empty() && !text.contains("-- No entries --"));
    let client = fs::read_to_string(config::log_path())
        .ok()
        .filter(|text| !text.trim().is_empty())
        .map(|text| tail_lines(&text, 80));

    match (client, journal) {
        (Some(client), Some(journal)) => {
            format!("官方客户端日志\n{client}\n\nsystemd 日志\n{journal}")
        }
        (Some(client), None) => client,
        (None, Some(journal)) => journal,
        (None, None) => "暂无日志。".to_string(),
    }
}

fn tail_lines(text: &str, count: usize) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

fn client_process_info() -> (bool, Option<u64>) {
    let pid = fs::read_dir("/proc")
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .chars()
                .all(|ch| ch.is_ascii_digit())
        })
        .find_map(|entry| {
            fs::read_to_string(entry.path().join("comm"))
                .ok()
                .filter(|name| name.trim() == "rjsupplicant")
                .map(|_| entry.file_name().to_string_lossy().into_owned())
        });
    let Some(pid) = pid else {
        return (false, None);
    };
    let uptime = command_text("ps", &["-o", "etimes=", "-p", &pid])
        .and_then(|value| value.trim().parse::<u64>().ok());
    (true, uptime)
}

fn open_with_default_app(target: impl AsRef<std::ffi::OsStr>) -> Result<()> {
    if !command_exists("gio") {
        anyhow::bail!("找不到 gio，无法打开目标");
    }
    Command::new("gio")
        .arg("open")
        .arg(target)
        .spawn()
        .context("无法调用系统默认应用")?;
    Ok(())
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
        Action::RestartService => "重启锐捷认证服务",
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
        assert_eq!(
            stop_service_command(),
            CommandSpec {
                program: "systemctl".to_string(),
                args: vec!["stop".to_string(), SERVICE.to_string()]
            }
        );
    }

    #[test]
    fn builds_service_file_from_current_settings() {
        let content = service_file(&settings());

        assert!(content.contains("Type=forking"));
        assert!(content.contains("ExecStart="));
        assert!(content.contains("-a 1 -d 1 -n \"enp4s0\" -u \"20260001\" -S 1"));
        assert!(content.contains("ExecStop="));
        assert!(content.contains("WantedBy=multi-user.target"));
    }

    #[test]
    fn escapes_systemd_specifiers_and_quotes() {
        assert_eq!(systemd_quote("a%\\\"b"), "\"a%%\\\\\\\"b\"");
    }
}
