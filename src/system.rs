// 后端能力模块：为特权 helper、systemd 服务与官方客户端操作提供纯逻辑。
// 旧版前端删除后，部分入口（连接性测试/日志/重启等）暂时没有界面调用方，
// 它们属于后端能力清单，待新前端按设计接入后再逐个点亮。
#![allow(dead_code)]

use crate::config::{self, SERVICE, Settings};
use anyhow::{Context, Result};
use rjsupplicant_gui::privileged::{
    self, AuthLineKind, AuthOptions, CLIENT_DIR, HELPER_PATH, HelperRequest,
};
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const PKEXEC_PATH: &str = "/usr/bin/pkexec";
const SUDO_PATH: &str = "/usr/bin/sudo";
const SYSTEMCTL_PATH: &str = "/usr/bin/systemctl";
const IP_BIN: &str = "/usr/bin/ip";
/// 提权等待上限：pkexec 授权弹窗与终端 `sudo` 两条路径共用同一个上界。
/// 用户一直不响应授权/密码提示时终止等待并报错，避免 GUI 停留在"忙"状态、
/// 所有点击被吞掉却无法取消（旧实现无超时；终端回退改成"等待结果"后一度无上界）。
const ELEVATION_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// 状态类命令（systemctl/nmcli/journalctl/ps）的硬超时：任何一个卡住都不能
/// 让 10 秒轮询永久停在 refreshing=true，把界面冻结在旧状态。
const STATUS_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientProcessState {
    Running,
    NotRunning,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct ClientStatus {
    pub client_installed: bool,
    pub client_requires_migration: bool,
    pub service_requires_migration: bool,
    /// 服务内容安全但模板过旧（缺少「已联网跳过」的 ExecCondition）：
    /// 只需重新开启一次开机认证完成迁移，与"不安全"区分展示。
    pub service_template_outdated: bool,
    pub client_process: ClientProcessState,
    /// 兼容旧前端：等价于 `client_process == ClientProcessState::Running`。
    pub client_running: bool,
    pub client_uptime_seconds: Option<u64>,
    pub service_enabled: String,
    pub service_active: String,
    pub service_sub: String,
    pub service_result: String,
    pub last_log: String,
    /// NetworkManager 无线射频是否启用。射频状态由 NM 持久化，
    /// 用户手动关 Wi-Fi 后重启仍保持关闭（实测事故源头之一）。
    /// `None` 表示 nmcli 缺失或输出无法判断，调用方不得按"已启用"或"已关闭"误报。
    pub wifi_radio_enabled: Option<bool>,
}

/// 认证 helper 退出码 3 对应的错误：认证结果未确认（既不是成功也不是失败）。
#[derive(Debug)]
pub struct AuthUnconfirmed;

impl std::fmt::Display for AuthUnconfirmed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("认证结果未确认（AuthUnconfirmed）")
    }
}

impl std::error::Error for AuthUnconfirmed {}

const AUTH_UNCONFIRMED_MARKER: &str = "AuthUnconfirmed";

#[derive(Clone, Debug)]
pub enum Action {
    InstallClient,
    Authenticate,
    Disconnect,
    EnableService,
    DisableService,
    RestartService,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ServiceProperties {
    load_state: String,
    unit_file_state: String,
    active_state: String,
    sub_state: String,
    result: String,
}

impl Default for ServiceProperties {
    fn default() -> Self {
        Self {
            load_state: "unknown".to_string(),
            unit_file_state: "unknown".to_string(),
            active_state: "unknown".to_string(),
            sub_state: "unknown".to_string(),
            result: "unknown".to_string(),
        }
    }
}

fn parse_service_properties(text: &str) -> ServiceProperties {
    let mut properties = ServiceProperties::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let value = if value.is_empty() { "unknown" } else { value };
        let slot = match key.trim() {
            "LoadState" => &mut properties.load_state,
            "UnitFileState" => &mut properties.unit_file_state,
            "ActiveState" => &mut properties.active_state,
            "SubState" => &mut properties.sub_state,
            "Result" => &mut properties.result,
            _ => continue,
        };
        *slot = value.to_string();
    }
    properties
}

fn load_service_properties() -> ServiceProperties {
    // 一次性读取全部字段，避免 is-enabled/is-active 两次调用之间状态漂移。
    let args = [
        "show",
        "-p",
        "LoadState",
        "-p",
        "UnitFileState",
        "-p",
        "ActiveState",
        "-p",
        "SubState",
        "-p",
        "Result",
        "--",
        SERVICE,
    ];
    let mut command = Command::new(SYSTEMCTL_PATH);
    command.args(args);
    let Some(output) = command_output_with_timeout(command, STATUS_COMMAND_TIMEOUT) else {
        return ServiceProperties::default();
    };
    if !output.status.success() {
        return ServiceProperties::default();
    }
    parse_service_properties(&String::from_utf8_lossy(&output.stdout))
}

pub fn load_status() -> ClientStatus {
    let (client_process, client_uptime_seconds) = client_process_info();
    let client_running = client_process == ClientProcessState::Running;
    let privileged_ready = privileged_client_ready();
    let legacy_ready = legacy_client_ready();
    let service = load_service_properties();
    let wifi_radio_enabled = command_text("/usr/bin/nmcli", &["-t", "-f", "WIFI", "radio"])
        .and_then(|value| match value.trim() {
            "enabled" => Some(true),
            "disabled" => Some(false),
            _ => None,
        });
    ClientStatus {
        client_installed: privileged_ready || legacy_ready,
        client_requires_migration: helper_installed() && !privileged_ready && legacy_ready,
        service_requires_migration: privileged_ready && installed_service_is_unsafe(),
        service_template_outdated: privileged_ready && installed_service_is_outdated(),
        client_process,
        client_running,
        client_uptime_seconds,
        service_enabled: service.unit_file_state,
        service_active: service.active_state,
        service_sub: service.sub_state,
        service_result: service.result,
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
            if is_excluded_wired_interface(&name)
                || link_type.trim() != "1"
                || path.join("wireless").exists()
            {
                return None;
            }
            // `device` 存在的是物理网卡/USB 网卡；VLAN、bridge、bond 没有该链接，
            // 但仍要保留，只是排在物理网卡之后。
            Some((name, path.join("device").exists()))
        })
        .collect::<Vec<_>>();

    ethernet.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    ethernet.into_iter().map(|(name, _)| name).collect()
}

fn is_excluded_wired_interface(name: &str) -> bool {
    name == "lo"
        || [
            "docker", "virbr", "vmnet", "veth", "br-", "tun", "tap", "wg",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// 网卡物理链路是否已连接；读不到 carrier（网卡不存在等）返回 `None`，
/// 与"明确无网线"（`Some(false)`）区分：只有后者才值得提前拦截认证。
pub fn interface_carrier(name: &str) -> Option<bool> {
    fs::read_to_string(Path::new("/sys/class/net").join(name).join("carrier"))
        .ok()
        .map(|value| value.trim() == "1")
}

pub fn interface_has_carrier(name: &str) -> bool {
    interface_carrier(name) == Some(true)
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

/// 网卡是否已有 IPv4 地址。先看 sysfs 网卡是否存在，再用 `ip -4` 读取；
/// 子进程有硬超时，避免网络栈异常时卡住状态刷新。
pub fn interface_has_ipv4(name: &str) -> bool {
    let name = name.trim();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\0')
        || !Path::new("/sys/class/net").join(name).is_dir()
    {
        return false;
    }
    let mut command = Command::new(IP_BIN);
    command.args(["-4", "-o", "addr", "show", "dev", name]);
    let Some(output) = command_output_with_timeout(command, Duration::from_millis(800)) else {
        return false;
    };
    output.status.success() && ipv4_output_has_address(&String::from_utf8_lossy(&output.stdout))
}

fn ipv4_output_has_address(text: &str) -> bool {
    text.lines().any(|line| {
        let mut words = line.split_whitespace();
        words.any(|word| word == "inet") && words.any(|word| word.contains('.'))
    })
}

/// 网卡是否有默认路由。直接解析 `/proc/net/route`，不做任何阻塞调用。
pub fn interface_has_default_route(name: &str) -> bool {
    let Ok(text) = fs::read_to_string("/proc/net/route") else {
        return false;
    };
    parse_default_route(&text, name.trim())
}

fn parse_default_route(text: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    text.lines().skip(1).any(|line| {
        let mut fields = line.split_whitespace();
        let Some(interface) = fields.next() else {
            return false;
        };
        let Some(destination) = fields.next() else {
            return false;
        };
        let Some(gateway) = fields.next() else {
            return false;
        };
        interface == name && destination == "00000000" && gateway != "00000000"
    })
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
    // 网线未连接时不发起认证（也避免无意义的 polkit 弹窗）：官方客户端在该
    // 状态下可能直接 SIGSEGV（实测 12 次崩溃中有 3 次发生在无网线窗口）。
    if interface_carrier(settings.nic.trim()) == Some(false) {
        anyhow::bail!(
            "{} 没有检测到网线连接，请检查网线后重试",
            settings.nic.trim()
        );
    }
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

/// 判断错误是否表示"helper 退出码 3：认证结果未确认"。
/// 调用方据此保留忙碌/待确认状态，而不是当成明确失败去清空会话或提示密码错误。
pub fn is_auth_unconfirmed(err: &anyhow::Error) -> bool {
    if err.downcast_ref::<AuthUnconfirmed>().is_some() {
        return true;
    }
    err.chain().any(|cause| {
        let text = cause.to_string();
        text.contains(AUTH_UNCONFIRMED_MARKER) || text.contains("认证结果未确认")
    })
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
    let path = Path::new(program);
    if !path.is_absolute() {
        anyhow::bail!("拒绝提权执行相对路径程序：{program}");
    }
    // 只校验最终文件不够：父目录可写时，攻击者可以在 symlink_metadata 检查
    // 通过后、pkexec/sudo 真正 execve 前替换文件或目录。逐级校验到 `/`，
    // 任何一级非 root-owned、组/其他可写或符号链接都拒绝。
    for ancestor in path.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(ancestor)
            .with_context(|| format!("无法检查提权路径的父目录：{}", ancestor.display()))?;
        if !metadata.file_type().is_dir() || !trusted_root_owner(&metadata) {
            anyhow::bail!(
                "拒绝提权执行位于不可信目录下的程序：{}（父目录 {} 非 root-owned 或他人可写）",
                program,
                ancestor.display()
            );
        }
    }
    let metadata = fs::symlink_metadata(program)
        .with_context(|| format!("无法检查待提权执行的程序：{program}"))?;
    if !metadata.file_type().is_file() || !trusted_root_owner(&metadata) {
        anyhow::bail!(
            "拒绝提权执行非 root-owned 或他人可写的程序：{program}；请用 scripts/install.sh 安装 root-owned helper 后重试"
        );
    }
    Ok(())
}

#[cfg(unix)]
fn trusted_root_owner(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.uid() == 0 && metadata.mode() & 0o022 == 0
}

/// 终端回退：kitty/foot/alacritty/xterm 在被执行命令退出后才关闭，因此等待终端
/// 进程即可拿到真实结果。旧实现在 `spawn()` 后直接返回成功，会把失败伪装成完成；
/// 等待同样受 `ELEVATION_WAIT_TIMEOUT` 上界约束，避免密码提示挂起时 GUI 永久忙碌。
fn run_terminal_elevated(action: &Action, program: &str, args: &[String]) -> Result<()> {
    let title = action_label(action);
    let mut command_args = vec![SUDO_PATH.to_string(), program.to_string()];
    command_args.extend(args.iter().cloned());
    let mut child = spawn_terminal(title, &command_args)?;
    let Some(status) = wait_for_child(&mut child, ELEVATION_WAIT_TIMEOUT, "终端中的管理员命令")?
    else {
        anyhow::bail!("{title} 在终端中等待超时，请确认该终端窗口（可能仍在等待密码）后重试");
    };
    status_result(action, status).context("终端中的管理员命令执行失败")
}

/// 统一把子进程退出状态转成错误：认证动作的退出码 3 必须保留"未确认"语义。
fn status_result(action: &Action, status: ExitStatus) -> Result<()> {
    if status.success() {
        return Ok(());
    }
    if matches!(action, Action::Authenticate) && status.code() == Some(3) {
        return Err(anyhow::Error::new(AuthUnconfirmed));
    }
    anyhow::bail!("{} 执行失败：{}", action_label(action), status)
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

        return status_result(&action, status);
    }

    run_terminal_elevated(&action, program, args)
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
        .replace('%', "%%")
        .replace('$', "$$")
        .replace(['\n', '\r', '\0'], " ");
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
    let client = fs::read(client_log_path())
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
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

/// 官方日志中最近一次明确判定：`(时间, 是否成功, 详情)`。
/// 只读末尾 64KiB，避免日志无限增长后每次刷新都整文件读入。
const AUTH_LOG_TAIL_BYTES: u64 = 64 * 1024;

pub fn last_auth_from_log() -> Option<(String, bool, String)> {
    let text = read_file_tail(&client_log_path(), AUTH_LOG_TAIL_BYTES)?;
    parse_last_auth(&text)
}

fn read_file_tail(path: &Path, max_bytes: u64) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = length.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0 {
        // 截断点可能落在半行：丢掉第一行残片，避免把残片误认成判定行。
        let newline = text.find('\n')?;
        text.drain(..=newline);
    }
    Some(text)
}

/// 从后往前找第一条终局判定；Transient（重试提示）跳过继续向前找。
/// 判定本身由 helper/GUI 共用的 `classify_auth_line` 完成，中英文日志同一套规则。
fn parse_last_auth(text: &str) -> Option<(String, bool, String)> {
    for line in text.lines().rev() {
        let (time, body) = split_log_timestamp(line.trim());
        if body.is_empty() {
            continue;
        }
        let detail = body.chars().take(32).collect::<String>();
        match privileged::classify_auth_line(&body) {
            Some(AuthLineKind::Success) => return Some((time, true, detail)),
            Some(AuthLineKind::TerminalFailure) => {
                // classify_auth_line 对终局失败用 contains；官方成功横幅里的
                // “上次认证失败的原因已修复”也含标记，但不是判定行。判定行
                // 必须以实际 marker 开头。
                if privileged::auth_line_reason(&body)
                    .is_some_and(|marker| body.starts_with(marker))
                {
                    return Some((time, false, detail));
                }
            }
            Some(AuthLineKind::TransientFailure) | None => {}
        }
    }
    None
}

/// 切出日志行前缀 "YYYY-MM-DD HH:MM:SS"，返回 ("MM-DD HH:MM", 其余内容)。
/// 少数状态行（`网线没有连接上`、`无法获取动态IP地址`）带两个时间戳——官方
/// 客户端用另一条通道打印——这里把第二层也剥掉，详情里才不会重复显示时间。
/// 前缀格式不符时返回 ("", 原行)。
fn split_log_timestamp(line: &str) -> (String, String) {
    let Some((time, rest)) = strip_timestamp_prefix(line) else {
        return (String::new(), line.to_string());
    };
    let body = strip_timestamp_prefix(&rest)
        .map(|(_, inner)| inner)
        .unwrap_or(rest);
    (time, body)
}

fn strip_timestamp_prefix(line: &str) -> Option<(String, String)> {
    let mut parts = line.splitn(3, ' ');
    let (date, clock, rest) = (parts.next()?, parts.next()?, parts.next()?);
    if date.len() == 10
        && clock.len() == 8
        && date.is_ascii()
        && clock.is_ascii()
        && date.chars().all(|ch| ch.is_ascii_digit() || ch == '-')
        && clock.chars().all(|ch| ch.is_ascii_digit() || ch == ':')
    {
        return Some((
            format!("{} {}", &date[5..], &clock[..5]),
            rest.trim().to_string(),
        ));
    }
    None
}

fn client_process_info() -> (ClientProcessState, Option<u64>) {
    let entries = match fs::read_dir("/proc") {
        Ok(entries) => entries,
        // /proc 读不了时无法判断，绝不能谎报“未运行”。
        Err(_) => return (ClientProcessState::Unknown, None),
    };
    let mut inaccessible = false;
    let mut found_pid = None;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let pid = file_name.to_string_lossy();
        if pid.is_empty() || !pid.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let comm = match fs::read_to_string(entry.path().join("comm")) {
            Ok(comm) => comm,
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                // hidepid 等安全策略可能隐藏真正的客户端进程。
                inaccessible = true;
                continue;
            }
            Err(_) => continue,
        };
        if comm.trim() != "rjsupplicant" {
            continue;
        }
        let stat = match fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            // 权限受限时无法判断僵尸状态，但 comm 已匹配；继续按运行中处理，
            // 不能让 root 客户的 /proc 隐藏把真实会话报成“未知”。
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => String::new(),
            Err(_) => continue,
        };
        // 僵尸/死亡进程的 comm 仍是 rjsupplicant，但已经不持有任何会话。
        if matches!(proc_stat_state(&stat), Some('Z' | 'X')) {
            continue;
        }
        let exe = match fs::read_link(entry.path().join("exe")) {
            Ok(exe) => exe,
            // root helper 启动的客户端以 root 运行；普通用户读 /proc/<pid>/exe
            // 会得到 EACCES。comm 已匹配且非僵尸，不能再报“未知”。
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                found_pid = Some(pid.into_owned());
                break;
            }
            Err(_) => continue,
        };
        if !is_client_exe_target(&exe) {
            continue;
        }
        found_pid = Some(pid.into_owned());
        break;
    }

    let state = aggregate_client_process_state(found_pid.is_some(), inaccessible);
    if state != ClientProcessState::Running {
        return (state, None);
    }
    let pid = found_pid.expect("Running 状态必然有 pid");
    let uptime = command_text("ps", &["-o", "etimes=", "-p", &pid])
        .and_then(|value| value.trim().parse::<u64>().ok());
    (ClientProcessState::Running, uptime)
}

fn aggregate_client_process_state(found: bool, inaccessible: bool) -> ClientProcessState {
    if found {
        ClientProcessState::Running
    } else if inaccessible {
        // 有进程信息因权限读不到：可能是客户端，也可能不是——只能报未知。
        ClientProcessState::Unknown
    } else {
        ClientProcessState::NotRunning
    }
}

/// `/proc/<pid>/stat` 的 state 是 `comm` 右括号后的第一个字段；
/// 用 rfind 处理 comm 中可能出现的空格/括号。
fn proc_stat_state(stat: &str) -> Option<char> {
    let close = stat.rfind(')')?;
    stat[close + 1..].split_whitespace().next()?.chars().next()
}

fn is_client_exe_target(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "rjsupplicant" || name == "rjsupplicant (deleted)")
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
    let mut command = Command::new(program);
    command.args(args);
    let output = command_output_with_timeout(command, STATUS_COMMAND_TIMEOUT)?;
    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).to_string()
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };

    Some(text.trim().to_string())
}

/// 有界读取短命令输出：到期直接 kill，避免 `ip` 等命令卡住状态刷新。
fn command_output_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> Option<std::process::Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().ok()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
        let now = Instant::now();
        if now >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep((deadline - now).min(Duration::from_millis(20)));
    }
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

/// 服务内容安全、但缺少「已联网跳过」的 ExecCondition（旧模板）。
/// 只提示迁移、不当作不安全：旧模板仍可被安全地禁用/重启，否则用户
/// 无法关闭一个等待迁移的服务（disable/restart 走同一安全校验）。
fn installed_service_is_outdated() -> bool {
    let Ok(content) = fs::read_to_string(privileged::SERVICE_PATH) else {
        return false;
    };
    privileged::service_content_uses_owned_paths(&content)
        && !privileged::service_is_current_template(&content)
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

    #[test]
    fn parses_last_auth_from_log_tail() {
        // 取最后一条终局判定：失败在前、成功在后 → 报成功。
        let log = "2026-09-07 10:00:10 认证失败您的账户已欠费\n\
                   2026-10-01 09:03:50 认证成功\n";
        assert_eq!(
            parse_last_auth(log),
            Some(("10-01 09:03".to_string(), true, "认证成功".to_string()))
        );

        // 无关行不参与判定（不能把"正在停止系统服务"之类当结果）。
        let unrelated = "2026-10-01 16:48:50 正在停止系统(NetworkManager)服务\n\
                         2026-10-01 16:49:13 认证方式    有线认证\n";
        assert_eq!(parse_last_auth(unrelated), None);

        // "网线没有连接上"是重试提示（带双时间戳），不是终局结果：即使它是
        // 最后一条标记行，也必须向前找到真正的判定。
        let transient = "2026-10-01 09:03:50 认证成功\n\
                         2026-10-01 16:48:51 2026-10-01 16:48:51 网线没有连接上，请检查网卡连接\n";
        assert_eq!(
            parse_last_auth(transient),
            Some(("10-01 09:03".to_string(), true, "认证成功".to_string()))
        );

        // DHCP 超时是终局失败，且详情里不带重复的时间戳。
        let failure = "2026-09-01 17:35:10 2026-09-01 17:35:10 无法获取动态IP地址\n";
        let (time, ok, detail) = parse_last_auth(failure).expect("应解析出失败记录");
        assert_eq!(time, "09-01 17:35");
        assert!(!ok);
        assert_eq!(detail, "无法获取动态IP地址");

        // 成功横幅里的说明文字不是判定行。
        assert_eq!(parse_last_auth("上次认证失败的原因已修复\n"), None);

        // 没有时间前缀的判定行仍然参与判定，只是不显示时间。
        let (time, ok, _) = parse_last_auth("认证失败\n").expect("无时间前缀也能解析");
        assert!(time.is_empty());
        assert!(!ok);
    }

    #[test]
    fn parses_chinese_and_english_auth_results() {
        // 英文环境：Success / Authentication failed 与中文标记必须等价。
        let english = "2026-10-01 09:03:50 Success\n\
                       2026-10-01 09:04:00 Supplicate failure\n\
                       2026-10-01 09:05:00 Authentication failed\n";
        assert_eq!(
            parse_last_auth(english),
            Some((
                "10-01 09:05".to_string(),
                false,
                "Authentication failed".to_string()
            ))
        );

        // Transient（Supplicate failure）不是终局：必须继续向前找到真正的成功。
        let retry_after_success =
            "2026-10-01 09:03:50 Success\n2026-10-01 09:04:00 Supplicate failure\n";
        assert_eq!(
            parse_last_auth(retry_after_success),
            Some(("10-01 09:03".to_string(), true, "Success".to_string()))
        );

        for transient in [
            "2026-10-01 09:04:00 Adapter links down\n",
            "2026-10-01 09:04:00 Adapter cann't work\n",
            "2026-10-01 09:04:00 IP has chaged\n",
        ] {
            let log = format!("2026-10-01 09:03:50 认证成功\n{transient}");
            assert_eq!(
                parse_last_auth(&log),
                Some(("10-01 09:03".to_string(), true, "认证成功".to_string())),
                "{transient:?} 是重试提示，不应成为最近认证结果"
            );
        }
    }

    #[test]
    fn maps_client_process_states_without_false_negatives() {
        assert_eq!(
            aggregate_client_process_state(true, false),
            ClientProcessState::Running
        );
        assert_eq!(
            aggregate_client_process_state(true, true),
            ClientProcessState::Running,
            "找到运行中的客户端时，其他进程不可读不应影响结论"
        );
        assert_eq!(
            aggregate_client_process_state(false, true),
            ClientProcessState::Unknown,
            "权限不足只能报未知，不能报未运行"
        );
        assert_eq!(
            aggregate_client_process_state(false, false),
            ClientProcessState::NotRunning
        );

        // /proc/<pid>/stat 的 comm 可能包含空格/括号，state 取最后一个 ')' 之后。
        assert_eq!(proc_stat_state("123 (rj supp (licant)) Z 1 2 3"), Some('Z'));
        assert_eq!(proc_stat_state("123 (rjsupplicant) R 1 2 3"), Some('R'));
        assert!(is_client_exe_target(Path::new(
            "/usr/lib/rjsupplicant/rjsupplicant"
        )));
        assert!(!is_client_exe_target(Path::new("/bin/bash")));
    }

    #[test]
    fn parses_systemctl_show_service_properties() {
        let properties = parse_service_properties(
            "LoadState=loaded\n\
             UnitFileState=enabled\n\
             ActiveState=active\n\
             SubState=running\n\
             Result=success\n",
        );
        assert_eq!(properties.load_state, "loaded");
        assert_eq!(properties.unit_file_state, "enabled");
        assert_eq!(properties.active_state, "active");
        assert_eq!(properties.sub_state, "running");
        assert_eq!(properties.result, "success");

        // 命令失败/没有输出时各字段都是 unknown，调用方不会误报为正常。
        let unknown = parse_service_properties("");
        assert_eq!(unknown, ServiceProperties::default());
        let empty_values = parse_service_properties("UnitFileState=\nActiveState=\n");
        assert_eq!(empty_values.unit_file_state, "unknown");
        assert_eq!(empty_values.active_state, "unknown");
    }

    #[test]
    fn parses_default_route_from_proc_net_route() {
        let routes = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n\
                      eno1\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
                      eno1\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n";
        assert!(parse_default_route(routes, "eno1"));
        assert!(!parse_default_route(routes, "eno2"));
        assert!(!parse_default_route(routes, ""));
        let no_gateway = "Iface\tDestination\tGateway\n\
                          eno1\t00000000\t00000000\n";
        assert!(!parse_default_route(no_gateway, "eno1"));
    }

    #[test]
    fn detects_ipv4_address_in_ip_output() {
        assert!(ipv4_output_has_address(
            "2: eno1    inet 192.168.1.5/24 brd 192.168.1.255 scope global eno1\n"
        ));
        assert!(!ipv4_output_has_address(
            "2: eno1    inet6 fe80::1/64 scope link\n"
        ));
        assert!(!ipv4_output_has_address(""));
        assert!(!interface_has_ipv4("rj-nonexistent0"));
        assert!(!interface_has_default_route("rj-nonexistent0"));
    }

    #[test]
    fn wired_interface_filter_keeps_vlan_bridge_bond_and_usb() {
        for virtual_name in [
            "lo", "docker0", "virbr0", "vmnet1", "veth123", "br-abcd", "tun0", "tap0", "wg0",
        ] {
            assert!(
                is_excluded_wired_interface(virtual_name),
                "{virtual_name} 不应出现在有线网卡列表"
            );
        }
        for real_name in ["eno1", "enp4s0", "usb0", "eno1.100", "br0", "bond0"] {
            assert!(
                !is_excluded_wired_interface(real_name),
                "{real_name} 应保留在有线网卡列表"
            );
        }
    }

    #[test]
    fn bounded_command_output_captures_and_times_out() {
        let mut echo = Command::new("/bin/sh");
        echo.args(["-c", "printf ok"]);
        let output =
            command_output_with_timeout(echo, Duration::from_secs(1)).expect("短命令应正常结束");
        assert!(output.status.success());
        assert_eq!(output.stdout, b"ok");

        let mut slow = Command::new("/bin/sleep");
        slow.arg("5");
        let started = Instant::now();
        assert!(command_output_with_timeout(slow, Duration::from_millis(50)).is_none());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn reads_only_the_log_tail() {
        let path =
            std::env::temp_dir().join(format!("rjsupplicant-gui-log-tail-{}", std::process::id()));
        let content = format!("{}\nTAIL\n", "x".repeat(256));
        fs::write(&path, &content).expect("写入临时日志");
        let tail = read_file_tail(&path, 32).expect("应读取末尾");
        fs::remove_file(&path).expect("清理临时日志");
        assert!(!tail.contains('x'), "只应保留末尾 32 字节：{tail:?}");
        assert!(tail.ends_with("TAIL\n"));
    }

    #[test]
    fn auth_unconfirmed_is_recognized() {
        let err = anyhow::anyhow!(AUTH_UNCONFIRMED_MARKER);
        assert!(is_auth_unconfirmed(&err));
        assert!(is_auth_unconfirmed(&anyhow::anyhow!("认证结果未确认")));
        assert!(!is_auth_unconfirmed(&anyhow::anyhow!("认证失败")));

        let status = Command::new("sh")
            .args(["-c", "exit 3"])
            .status()
            .expect("启动 sh 模拟 helper 退出码 3");
        let err = status_result(&Action::Authenticate, status)
            .expect_err("认证退出码 3 必须返回未确认错误");
        assert!(is_auth_unconfirmed(&err));
    }
}
