// 后端能力模块：为特权 helper、systemd 服务与官方客户端操作提供纯逻辑。
// 旧版前端删除后，部分入口（连接性测试/日志/重启等）暂时没有界面调用方，
// 它们属于后端能力清单，待新前端按设计接入后再逐个点亮。
#![allow(dead_code)]

use crate::config::{self, SERVICE, Settings};
use anyhow::{Context, Result};
use rjsupplicant_gui::privileged::{self, AuthOptions, CLIENT_DIR, HELPER_PATH, HelperRequest};
use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PKEXEC_PATH: &str = "/usr/bin/pkexec";
const SUDO_PATH: &str = "/usr/bin/sudo";
const SYSTEMCTL_PATH: &str = "/usr/bin/systemctl";
const IP_BIN: &str = "/usr/bin/ip";
/// 提权等待上限：pkexec 授权弹窗与终端 `sudo` 两条路径共用同一个上界。
/// 用户一直不响应授权/密码提示时终止等待并报错，避免 GUI 停留在"忙"状态、
/// 所有点击被吞掉却无法取消（旧实现无超时；终端回退改成"等待结果"后一度无上界）。
const ELEVATION_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ClientStatus {
    pub client_installed: bool,
    pub client_requires_migration: bool,
    pub service_requires_migration: bool,
    pub client_running: bool,
    pub client_uptime_seconds: Option<u64>,
    pub service_enabled: String,
    pub service_active: String,
    pub last_log: String,
    /// NetworkManager 无线射频是否启用。射频状态由 NM 持久化，
    /// 用户手动关 Wi-Fi 后重启仍保持关闭（实测事故源头之一）。
    pub wifi_radio_enabled: bool,
}

#[derive(Clone, Debug)]
pub enum Action {
    InstallClient,
    Authenticate,
    Disconnect,
    EnableService,
    DisableService,
    RestartService,
}

pub fn load_status() -> ClientStatus {
    let (client_running, client_uptime_seconds) = client_process_info();
    let privileged_ready = privileged_client_ready();
    let legacy_ready = legacy_client_ready();
    let service_enabled = command_text(SYSTEMCTL_PATH, &["is-enabled", SERVICE])
        .unwrap_or_else(|| "unknown".to_string());
    let service_active = command_text(SYSTEMCTL_PATH, &["is-active", SERVICE])
        .unwrap_or_else(|| "unknown".to_string());
    // 未知时按"已启用"处理（不误报）；nmcli 随 NetworkManager 安装。
    let wifi_radio_enabled = command_text("/usr/bin/nmcli", &["-t", "-f", "WIFI", "radio"])
        .map(|value| value.trim() == "enabled")
        .unwrap_or(true);
    ClientStatus {
        client_installed: privileged_ready || legacy_ready,
        client_requires_migration: helper_installed() && !privileged_ready && legacy_ready,
        service_requires_migration: privileged_ready && installed_service_is_unsafe(),
        client_running,
        client_uptime_seconds,
        service_enabled,
        service_active,
        last_log: recent_log(),
        wifi_radio_enabled,
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

/// 读取网卡当前 IPv4 地址（如 "192.168.1.5"）。无地址/无网卡时返回 None。
pub fn interface_ipv4(name: &str) -> Option<String> {
    let output = Command::new(IP_BIN)
        .args(["-4", "-o", "addr", "show", "dev", name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            let _index = parts.next()?;
            let _device = parts.next()?;
            if parts.next()? != "inet" {
                return None;
            }
            parts.next()?.split('/').next().map(|addr| addr.to_string())
        })
}

/// 读取网卡默认网关（如 "192.168.1.1"）。无路由时返回 None。
pub fn interface_gateway(name: &str) -> Option<String> {
    let output = Command::new(IP_BIN)
        .args(["route", "show", "default", "dev", name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut words = text.split_whitespace();
    while let Some(word) = words.next() {
        if word == "via" {
            return words.next().map(str::to_string);
        }
    }
    None
}

pub fn install_official_client(zip_path: &Path) -> Result<()> {
    if helper_installed() {
        let zip_path = fs::canonicalize(zip_path).context("无法读取选择的安装包路径")?;
        let spec = helper_command(HelperRequest::InstallClient(zip_path));
        return run_elevated_wait(Action::InstallClient, &spec.program, &spec.args);
    }
    rjsupplicant_gui::client_install::install_official_client(
        zip_path,
        &config::data_dir(),
        &config::client_path(),
    )
}

pub fn authenticate(settings: &Settings, password: &str) -> Result<()> {
    config::validate(settings)?;
    ensure_client_installed()?;
    let use_helper = privileged_client_ready();
    let spec = authenticate_command_for(settings, password, use_helper);
    if use_helper {
        return run_elevated_wait_with_input(
            Action::Authenticate,
            &spec.program,
            &spec.args,
            Some(password.as_bytes()),
        );
    }
    run_elevated_wait(Action::Authenticate, &spec.program, &spec.args)
}

pub fn disconnect() -> Result<()> {
    ensure_client_installed()?;
    if privileged_client_ready() {
        let spec = helper_command(HelperRequest::Disconnect);
        return run_elevated_wait(Action::Disconnect, &spec.program, &spec.args);
    }
    if command_text(SYSTEMCTL_PATH, &["is-active", SERVICE]).as_deref() == Some("active") {
        let spec = stop_service_command();
        return run_elevated_wait(Action::Disconnect, &spec.program, &spec.args);
    }
    let spec = disconnect_command();
    run_elevated_wait(Action::Disconnect, &spec.program, &spec.args)
}

pub fn enable_service(settings: &Settings) -> Result<()> {
    config::validate(settings)?;
    ensure_client_installed()?;
    if privileged_client_ready() {
        let spec = helper_command(HelperRequest::EnableService(privileged_options(
            settings, None,
        )));
        return run_elevated_wait(Action::EnableService, &spec.program, &spec.args);
    }
    anyhow::bail!("旧版客户端不能启用开机认证，请通过顶部提示重新选择官方 ZIP 完成安全迁移")
}

pub fn disable_service() -> Result<()> {
    if helper_installed() {
        let spec = helper_command(HelperRequest::DisableService);
        return run_elevated_wait(Action::DisableService, &spec.program, &spec.args);
    }
    let spec = disable_service_command();
    run_elevated_wait(Action::DisableService, &spec.program, &spec.args)
}

pub fn restart_service() -> Result<()> {
    ensure_client_installed()?;
    if privileged_client_ready() {
        let spec = helper_command(HelperRequest::RestartService);
        return run_elevated_wait(Action::RestartService, &spec.program, &spec.args);
    }
    anyhow::bail!("旧版客户端不能重启开机认证，请先完成 root-owned 客户端迁移")
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
    open_with_default_app(client_data_dir())
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

/// 提权边界：任何交给 pkexec/`sudo` 执行的程序都必须 root-owned 且组/其他不可写。
/// 与 helper 侧 `is_secure_root_executable`、`install.sh` 的同名检查保持一致：
/// 否则一个能写 `$HOME` 的进程只要替换掉 legacy wrapper，就能借用户的一次授权拿到 root。
fn ensure_root_owned_program(program: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(program)
        .with_context(|| format!("无法检查待提权执行的程序：{program}"))?;
    if !metadata.file_type().is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        anyhow::bail!(
            "拒绝提权执行非 root-owned 或他人可写的程序：{program}；请用 scripts/install.sh 安装 root-owned helper 后重试"
        );
    }
    Ok(())
}

/// 终端回退：kitty/foot/alacritty/xterm 在被执行命令退出后才关闭，因此等待终端
/// 进程即可拿到真实结果。旧实现在 `spawn()` 后直接返回成功，会把失败伪装成完成；
/// 等待同样受 `ELEVATION_WAIT_TIMEOUT` 上界约束，避免密码提示挂起时 GUI 永久忙碌。
fn run_terminal_elevated(title: &str, program: &str, args: &[String]) -> Result<()> {
    let mut command_args = vec![SUDO_PATH.to_string(), program.to_string()];
    command_args.extend(args.iter().cloned());
    let mut child = spawn_terminal(title, &command_args)?;
    let Some(status) = wait_for_child(&mut child, ELEVATION_WAIT_TIMEOUT, "终端中的管理员命令")?
    else {
        anyhow::bail!("{title} 在终端中等待超时，请确认该终端窗口（可能仍在等待密码）后重试");
    };
    if status.success() {
        return Ok(());
    }
    anyhow::bail!("终端中的 sudo 执行失败：{status}")
}

/// 有界等待提权子进程：正常结束返回其状态；超时则终止并回收直接子进程后返回
/// `None`，由调用方给出可操作的报错。两条提权路径共用，避免任何一条让 GUI 无限忙碌。
/// 注意：`kill` 只作用于直接子进程（pkexec 或终端模拟器），由它派生的
/// root 侧进程不会被回收——这一限制在两条路径上相同。
fn wait_for_child(
    child: &mut std::process::Child,
    timeout: std::time::Duration,
    label: &str,
) -> Result<Option<std::process::ExitStatus>> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("无法等待{label}结束"))?
        {
            return Ok(Some(status));
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn run_elevated_wait(action: Action, program: &str, args: &[String]) -> Result<()> {
    run_elevated_wait_with_input(action, program, args, None)
}

fn run_elevated_wait_with_input(
    action: Action,
    program: &str,
    args: &[String],
    input: Option<&[u8]>,
) -> Result<()> {
    ensure_root_owned_program(program)?;
    if command_exists(PKEXEC_PATH) {
        let mut command = Command::new(PKEXEC_PATH);
        command.arg(program).args(args);
        if input.is_some() {
            command.stdin(Stdio::piped());
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("无法启动系统授权：{}", action_label(&action)))?;
        if let Some(input) = input {
            let write_result = child
                .stdin
                .take()
                .context("无法打开 helper 密码输入通道")?
                .write_all(input);
            if let Err(err) = write_result {
                let _ = child.kill();
                let _ = child.wait();
                return Err(err).context("无法写入 helper 密码输入通道");
            }
        }
        // 等待授权结果，但带上界：用户不响应 polkit 弹窗时 120 秒后终止并报错，
        // 而不是让 GUI 无限停留在忙碌状态。
        let Some(status) = wait_for_child(&mut child, ELEVATION_WAIT_TIMEOUT, "系统授权")?
        else {
            anyhow::bail!("{} 授权等待超时，请重试", action_label(&action));
        };

        if status.success() {
            return Ok(());
        }

        anyhow::bail!("{} 执行失败：{}", action_label(&action), status);
    }

    run_terminal_elevated(action_label(&action), program, args)
}

#[cfg(test)]
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
         Type=simple\n\
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

#[cfg(test)]
fn systemd_quote(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    format!("\"{escaped}\"")
}

fn ensure_client_installed() -> Result<()> {
    if client_installed() {
        return Ok(());
    }

    anyhow::bail!("官方客户端未安装，请先运行 scripts/install.sh 并放入官方客户端 zip")
}

fn authenticate_command_for(settings: &Settings, password: &str, use_helper: bool) -> CommandSpec {
    if use_helper {
        return helper_command(HelperRequest::Authenticate(privileged_options(
            settings, None,
        )));
    }
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

fn privileged_options(settings: &Settings, password: Option<String>) -> AuthOptions {
    AuthOptions {
        username: settings.username.trim().to_string(),
        nic: settings.nic.trim().to_string(),
        dhcp: settings.dhcp,
        save_password: settings.save_password,
        password,
    }
}

fn helper_command(request: HelperRequest) -> CommandSpec {
    CommandSpec {
        program: HELPER_PATH.to_string(),
        args: request.arguments(),
    }
}

pub fn disconnect_command() -> CommandSpec {
    CommandSpec {
        program: config::path_string(&config::client_path()),
        args: vec!["-q".to_string()],
    }
}

#[cfg(test)]
pub fn enable_service_command() -> CommandSpec {
    CommandSpec {
        program: SYSTEMCTL_PATH.to_string(),
        args: vec![
            "enable".to_string(),
            "--now".to_string(),
            SERVICE.to_string(),
        ],
    }
}

pub fn disable_service_command() -> CommandSpec {
    CommandSpec {
        program: SYSTEMCTL_PATH.to_string(),
        args: vec![
            "disable".to_string(),
            "--now".to_string(),
            SERVICE.to_string(),
        ],
    }
}

pub fn stop_service_command() -> CommandSpec {
    CommandSpec {
        program: SYSTEMCTL_PATH.to_string(),
        args: vec!["stop".to_string(), SERVICE.to_string()],
    }
}

/// 在可用终端里启动一条命令，返回子进程句柄供调用方决定是等待还是丢弃。
fn spawn_terminal(title: &str, args: &[String]) -> Result<std::process::Child> {
    if command_exists("kitty") {
        return Command::new("kitty")
            .args(["--title", title, "-e"])
            .args(args)
            .spawn()
            .context("无法打开 kitty");
    }

    if command_exists("foot") {
        return Command::new("foot")
            .args(["--title", title])
            .args(args)
            .spawn()
            .context("无法打开 foot");
    }

    if command_exists("alacritty") {
        return Command::new("alacritty")
            .args(["--title", title, "-e"])
            .args(args)
            .spawn()
            .context("无法打开 alacritty");
    }

    if command_exists("xterm") {
        return Command::new("xterm")
            .args(["-T", title, "-e"])
            .args(args)
            .spawn()
            .context("无法打开 xterm");
    }

    anyhow::bail!("找不到可用终端（kitty/foot/alacritty/xterm），无法执行需要终端的命令")
}

/// 启动后即不关注的终端用法（例如 `journalctl -f` 会一直跟随）。
fn run_terminal(title: &str, args: &[String]) -> Result<()> {
    spawn_terminal(title, args).map(|_| ())
}

fn recent_log() -> String {
    let journal = command_text("journalctl", &["-u", SERVICE, "-n", "60", "--no-pager"])
        .filter(|text| !text.is_empty() && !text.contains("-- No entries --"));
    let client = fs::read_to_string(client_log_path())
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
        // 与 PATH 查找分支保持同一判据：目录或无执行位的文件都不算可用命令，
        // 否则 pkexec 缺失会被误判为“可用”，绕过上面那条显式失败。
        return is_executable_file(PathBuf::from(program));
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
        Action::InstallClient => "安装官方锐捷客户端",
        Action::Authenticate => "锐捷有线认证",
        Action::Disconnect => "断开锐捷认证",
        Action::EnableService => "启用锐捷开机自启",
        Action::DisableService => "禁用锐捷开机自启",
        Action::RestartService => "重启锐捷认证服务",
    }
}

fn helper_installed() -> bool {
    is_executable_file(PathBuf::from(HELPER_PATH))
}

fn privileged_client_ready() -> bool {
    helper_installed()
        && is_executable_file(PathBuf::from(privileged::CLIENT_WRAPPER_PATH))
        && is_executable_file(privileged::client_binary_path())
}

fn legacy_client_ready() -> bool {
    is_executable_file(config::client_path()) && is_executable_file(config::client_binary_path())
}

fn client_installed() -> bool {
    privileged_client_ready() || legacy_client_ready()
}

fn client_data_dir() -> PathBuf {
    if privileged_client_ready() {
        PathBuf::from(CLIENT_DIR)
    } else {
        config::data_dir()
    }
}

fn client_log_path() -> PathBuf {
    if privileged_client_ready() {
        privileged::client_log_path()
    } else {
        config::log_path()
    }
}

fn installed_service_is_unsafe() -> bool {
    let path = Path::new(privileged::SERVICE_PATH);
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return false,
        Err(_) => return true,
    };
    if !metadata.file_type().is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return true;
    }
    fs::read_to_string(path)
        .map(|content| !privileged::service_content_uses_owned_paths(&content))
        .unwrap_or(true)
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
        let spec = authenticate_command_for(&settings(), "", false);

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
        let spec = authenticate_command_for(&settings(), "secret", false);

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

        let spec = authenticate_command_for(&settings, "  ", false);

        assert_eq!(
            spec.args,
            [
                "-a", "1", "-d", "0", "-n", "enp4s0", "-u", "20260001", "-S", "0"
            ]
        );
    }

    #[test]
    fn privileged_authenticate_uses_fixed_helper_without_password_argument() {
        let spec = authenticate_command_for(&settings(), "secret", true);

        assert_eq!(spec.program, HELPER_PATH);
        assert!(!spec.args.iter().any(|argument| argument == "secret"));
        assert_eq!(
            HelperRequest::parse(&spec.args).expect("parse helper arguments"),
            HelperRequest::Authenticate(AuthOptions {
                username: "20260001".to_string(),
                nic: "enp4s0".to_string(),
                dhcp: true,
                save_password: true,
                password: None,
            })
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
                program: SYSTEMCTL_PATH.to_string(),
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
                program: SYSTEMCTL_PATH.to_string(),
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
                program: SYSTEMCTL_PATH.to_string(),
                args: vec!["stop".to_string(), SERVICE.to_string()]
            }
        );
    }

    #[test]
    fn builds_service_file_from_current_settings() {
        let content = service_file(&settings());

        assert!(content.contains("Type=simple"));
        assert!(content.contains("ExecStart="));
        assert!(content.contains("-a 1 -d 1 -n \"enp4s0\" -u \"20260001\" -S 1"));
        assert!(content.contains("ExecStop="));
        assert!(content.contains("WantedBy=multi-user.target"));
    }

    #[test]
    fn escapes_systemd_specifiers_and_quotes() {
        assert_eq!(systemd_quote("a%\\\"b"), "\"a%%\\\\\\\"b\"");
    }

    #[test]
    fn refuses_to_escalate_a_program_that_others_can_replace() {
        use std::os::unix::fs::PermissionsExt;

        // 用 0777 而不是“属主非 root”构造用例，使断言与运行测试的用户无关。
        let path =
            std::env::temp_dir().join(format!("rjsupplicant-gui-elevation-{}", std::process::id()));
        fs::write(&path, "#!/bin/sh\nexit 0\n").expect("写入临时程序");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o777)).expect("放开写权限");
        let message = ensure_root_owned_program(&config::path_string(&path))
            .expect_err("他人可写的程序不应被接受为提权目标");
        let detail = message.to_string();
        fs::remove_file(&path).expect("清理临时程序");

        assert!(
            detail.contains("拒绝提权执行"),
            "错误未说明拒绝原因：{detail}"
        );
        assert!(
            ensure_root_owned_program("/definitely/not/installed/rjsupplicant-helper").is_err(),
            "不存在的程序不应被当成可信提权目标"
        );
    }

    #[test]
    fn absolute_command_path_requires_executable_bit() {
        use std::os::unix::fs::PermissionsExt;

        let path = std::env::temp_dir().join(format!(
            "rjsupplicant-gui-command-exists-{}",
            std::process::id()
        ));
        fs::write(&path, "#!/bin/sh\n").expect("写入临时可执行文件");
        let bare = config::path_string(&path);

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("去掉执行位");
        let without_exec_bit = command_exists(&bare);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("加上执行位");
        let with_exec_bit = command_exists(&bare);
        fs::remove_file(&path).expect("清理临时可执行文件");

        assert!(
            !without_exec_bit,
            "无执行位的文件不应被当成可用命令：{bare}"
        );
        assert!(with_exec_bit, "可执行文件应被识别为可用命令：{bare}");
        assert!(
            !command_exists("/etc"),
            "目录不应被当成可用命令（pkexec 缺失时的误判入口）"
        );
    }

    #[test]
    fn elevation_wait_bounds_a_stuck_child_and_reaps_it() {
        // 终端里的 sudo 提示挂起时不能有“无上界等待”：超时必须终止并回收直接子进程。
        let mut stuck = Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("启动 sleep 模拟挂起的提权子进程");
        let outcome = wait_for_child(&mut stuck, std::time::Duration::from_millis(200), "测试")
            .expect("有界等待本身不应失败");
        assert!(outcome.is_none(), "挂起的子进程应在超时后返回 None");
        assert!(
            stuck.try_wait().expect("复查子进程状态").is_some(),
            "超时后子进程必须已被回收，不能留在系统里挂起"
        );

        let mut finished = Command::new("/bin/true")
            .spawn()
            .expect("启动 /bin/true 模拟正常结束的提权子进程");
        let outcome = wait_for_child(&mut finished, std::time::Duration::from_secs(5), "测试")
            .expect("有界等待本身不应失败");
        assert!(
            outcome.expect("正常结束应返回退出状态").success(),
            "正常结束的子进程不应被判为失败"
        );
    }
}
