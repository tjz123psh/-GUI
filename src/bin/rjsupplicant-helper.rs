use anyhow::{Context, Result};
use rjsupplicant_gui::client_install;
use rjsupplicant_gui::privileged::{
    AuthOptions, CLIENT_DIR, CLIENT_WRAPPER_PATH, HelperRequest, SERVICE_PATH, client_binary_path,
    client_log_path, service_content_uses_owned_paths, service_file,
};
use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::process::{Command, ExitCode};

const SYSTEMCTL: &str = "/usr/bin/systemctl";
const SERVICE_NAME: &str = "rjsupplicant.service";
const MAX_PASSWORD_BYTES: usize = 4096;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("rjsupplicant-helper: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    ensure_root()?;
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match HelperRequest::parse(&args)? {
        HelperRequest::InstallClient(zip_path) => client_install::install_official_client(
            &zip_path,
            Path::new(CLIENT_DIR),
            Path::new(CLIENT_WRAPPER_PATH),
        ),
        HelperRequest::Authenticate(mut options) => {
            options.password = read_auth_password()?;
            authenticate(&options)
        }
        HelperRequest::Disconnect => {
            let result = disconnect();
            restore_network_services();
            result
        }
        HelperRequest::EnableService(options) => {
            let result = enable_service(&options);
            restore_network_services();
            result
        }
        HelperRequest::DisableService => {
            let result = disable_service();
            restore_network_services();
            result
        }
        HelperRequest::RestartService => {
            ensure_client_installed()?;
            ensure_service_is_safe()?;
            let result = run_checked(SYSTEMCTL, &["restart", SERVICE_NAME], "重启认证服务失败");
            restore_network_services();
            result
        }
        HelperRequest::RestoreNetwork => {
            // 供 service 的 ExecStartPost 调用：等 8 秒让客户端完成启动与
            // NM 停止操作后再恢复，复刻手动认证的 DHCP 注入时序。
            std::thread::sleep(std::time::Duration::from_secs(8));
            restore_network_services();
            Ok(())
        }
    }
}

fn ensure_root() -> Result<()> {
    let status = fs::read_to_string("/proc/self/status").context("无法读取进程身份")?;
    let effective_uid = status
        .lines()
        .find(|line| line.starts_with("Uid:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .and_then(|value| value.parse::<u32>().ok())
        .context("无法判断有效用户身份")?;
    if effective_uid != 0 {
        anyhow::bail!("该 helper 只能通过 pkexec 或 root 调用");
    }
    Ok(())
}

fn authenticate(options: &AuthOptions) -> Result<()> {
    ensure_client_installed()?;
    let args = client_arguments(options, true);
    let log_path = client_log_path();
    let mut log_offset = fs::metadata(&log_path).map(|meta| meta.len()).unwrap_or(0);

    // 官方客户端启动时会主动 `systemctl stop NetworkManager`（strace 实测），
    // 且其内置 DHCP（2014 二进制）在现代内核上不发出任何报文（pcap 实测）。
    // 约 8 秒后恢复 NetworkManager：NM 的内部 DHCP 拿到地址并建立 eno1 默认
    // 路由，客户端轮询 /proc/net/route 确认后即认证成功并保持会话（实机验证）。
    let mut child = Command::new(CLIENT_WRAPPER_PATH)
        .args(&args)
        .spawn()
        .with_context(|| "有线认证失败：无法启动客户端")?;
    // 客户端可能在任何一次判定之前就退出（网线未插、认证服务器不通、崩溃），
    // 那时它已经停掉了 NetworkManager；用守卫兜住所有出口，避免无线被静默切断。
    let _network_guard = NetworkRestorer;
    await_auth_result(&mut child, &log_path, &mut log_offset)
}

/// DHCP 注入时序节点：客户端在此期间完成启动并停掉 NetworkManager。
const DHCP_RESTORE_DELAY: std::time::Duration = std::time::Duration::from_secs(8);

/// 认证结果判定时长：失败路径约 45 秒（DHCP 超时）后客户端退出；
/// 成功路径客户端保持前台会话运行，必须尽早返回，否则 GUI 的
/// pkexec 等待会在 120 秒超时后杀掉整个已认证会话。
const AUTH_RESULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

const AUTH_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

/// 轮询官方客户端日志直到得出结果；网络恢复由调用方的 `NetworkRestorer` 负责，
/// 这里只在 8 秒节点显式提前恢复一次，使所有出口都不依赖分支写法。
fn await_auth_result(
    child: &mut std::process::Child,
    log_path: &Path,
    log_offset: &mut u64,
) -> Result<()> {
    // 客户端退出码不可靠（DHCP 失败也返回 0），成败以启动后新增的官方日志判定。
    let started = std::time::Instant::now();
    let mut restored = false;
    loop {
        let outcome = classify_auth(
            child
                .try_wait()
                .with_context(|| "有线认证失败：等待客户端退出失败")?
                .is_some(),
            new_log_tail(log_path, log_offset).as_deref(),
        );
        match outcome {
            // 客户端已退出（失败或崩溃）：返回后由 GUI 状态轮询呈现真实状态。
            AuthOutcome::ClientExited | AuthOutcome::Succeeded => return Ok(()),
            AuthOutcome::Failed(reason) => {
                // 判到失败但客户端还活着：它已带 `-p` 明文口令且不会建立会话，
                // 留在系统里只是一个持有口令的 root 孤儿进程。
                terminate_client(child);
                return Err(anyhow::anyhow!("{reason}"));
            }
            AuthOutcome::Pending => {}
        }
        // 8 秒节点必须在这里显式恢复：客户端要等 NM 注入路由才会写出「认证成功」，
        // 只靠守卫在函数返回时恢复会让本轮认证一直等到超时。
        if !restored && started.elapsed() >= DHCP_RESTORE_DELAY {
            restore_network_services();
            restored = true;
        }
        // 总预算保持为原两阶段的 8 秒 + 60 秒，避免改动已被实机验证过的
        // 失败判定窗口（客户端 DHCP 超时约 45 秒）。
        // 超时说明本轮判不出结果，此时**不**杀客户端：慢网络上"再等一会就成功"
        // 与"卡死"无法区分，误杀会真把一次能成的认证打断。
        if started.elapsed() >= DHCP_RESTORE_DELAY + AUTH_RESULT_TIMEOUT {
            return Ok(());
        }
        std::thread::sleep(AUTH_POLL_INTERVAL);
    }
}

/// 终止并回收官方客户端。只用于"已判定失败但进程仍在"这一种状态。
fn terminate_client(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[derive(Debug, Eq, PartialEq)]
enum AuthOutcome {
    Pending,
    ClientExited,
    Succeeded,
    Failed(String),
}

/// 单轮判定优先级：进程已退出 > 认证成功 > 失败标记。
/// 早退排最前，是为了不把客户端崩溃后残留的日志行当成本轮结果；
/// 「认证成功」排在失败标记之前，是为了避免官方客户端粘贴的管理中心提示里
/// 出现「认证失败」字样时，把一次真实成功翻成失败。
fn classify_auth(client_exited: bool, new_log: Option<&str>) -> AuthOutcome {
    if client_exited {
        return AuthOutcome::ClientExited;
    }
    let Some(new_log) = new_log else {
        return AuthOutcome::Pending;
    };
    if new_log.contains("认证成功") {
        return AuthOutcome::Succeeded;
    }
    match auth_failure_reason(new_log) {
        Some(reason) => AuthOutcome::Failed(reason),
        None => AuthOutcome::Pending,
    }
}

/// 读取 `offset` 之后的新增日志，并把 `offset` 推进到本次消费到的位置。
/// 推进是必须的：不推进时每次 200ms 轮询都重读全部历史，日志越长读放大越严重。
fn new_log_tail(path: &Path, offset: &mut u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    if size < *offset {
        // 日志被清空或轮转：旧偏移已无意义，从头读，否则此后永远判不到新内容。
        *offset = 0;
    }
    if size == *offset {
        return None;
    }
    file.seek(SeekFrom::Start(*offset)).ok()?;
    let mut text = String::new();
    if file.read_to_string(&mut text).is_err() {
        // 含非法 UTF-8 字节：这段永远解析不出来，同样跳过它，避免每次轮询
        // 都在同一处失败并返回 None（旧行为会让成败判定永久失效）。
        *offset = size;
        return None;
    }
    *offset = size;
    Some(text)
}

fn auth_failure_reason(new_log: &str) -> Option<String> {
    const MARKERS: &[&str] = &[
        "网线没有连接上",
        "无法连接认证服务器",
        "认证失败",
        "无法获取动态IP地址",
    ];
    MARKERS
        .iter()
        .find(|marker| new_log.contains(**marker))
        .map(|marker| marker.to_string())
}

fn read_auth_password() -> Result<Option<String>> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        let echo_guard = TerminalEchoGuard::new(stdin.as_raw_fd())?;
        eprint!("校园网密码（直接回车复用官方客户端已保存的密码）：");
        std::io::stderr().flush()?;
        let mut input = String::new();
        let read_result = stdin.read_line(&mut input);
        drop(echo_guard);
        eprintln!();
        read_result?;
        while matches!(input.as_bytes().last(), Some(b'\n' | b'\r')) {
            input.pop();
        }
        return validate_password_input(input.into_bytes());
    }

    let mut input = Vec::new();
    stdin
        .take((MAX_PASSWORD_BYTES + 1) as u64)
        .read_to_end(&mut input)?;
    validate_password_input(input)
}

fn validate_password_input(input: Vec<u8>) -> Result<Option<String>> {
    if input.len() > MAX_PASSWORD_BYTES {
        anyhow::bail!("校园网密码过长");
    }
    let password = String::from_utf8(input).context("校园网密码不是有效 UTF-8")?;
    if password.contains('\0') {
        anyhow::bail!("校园网密码包含不支持的空字符");
    }
    Ok((!password.is_empty()).then_some(password))
}

/// 离开作用域时恢复被官方客户端停掉的 NetworkManager。
/// 与下面的 `TerminalEchoGuard` 同一模式：把清理绑到函数所有出口
/// （含 `?` 提前返回），不再逐分支手工调用，避免漏掉某条路径。
/// 幂等：NetworkManager 已在运行时 `systemctl start` 是空操作。
struct NetworkRestorer;

impl Drop for NetworkRestorer {
    fn drop(&mut self) {
        restore_network_services();
    }
}

struct TerminalEchoGuard {
    fd: std::os::unix::io::RawFd,
    original: libc::termios,
}

impl TerminalEchoGuard {
    fn new(fd: std::os::unix::io::RawFd) -> Result<Self> {
        // SAFETY: tcgetattr initializes the provided termios value for a valid terminal fd.
        let mut original = unsafe { std::mem::zeroed::<libc::termios>() };
        // SAFETY: original points to writable storage and fd comes from an active Stdin handle.
        if unsafe { libc::tcgetattr(fd, &mut original) } != 0 {
            return Err(std::io::Error::last_os_error()).context("无法读取终端输入设置");
        }
        let mut hidden = original;
        hidden.c_lflag &= !libc::ECHO;
        // SAFETY: hidden is a valid termios value obtained from the same terminal.
        if unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &hidden) } != 0 {
            return Err(std::io::Error::last_os_error()).context("无法隐藏终端密码输入");
        }
        Ok(Self { fd, original })
    }
}

impl Drop for TerminalEchoGuard {
    fn drop(&mut self) {
        // SAFETY: original was returned by tcgetattr for this fd; restoration is best effort.
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSANOW, &self.original);
        }
    }
}

fn disconnect() -> Result<()> {
    ensure_client_installed()?;
    if command_succeeds(SYSTEMCTL, &["is-active", "--quiet", SERVICE_NAME]) {
        ensure_service_is_safe()?;
        return run_checked(SYSTEMCTL, &["stop", SERVICE_NAME], "停止认证服务失败");
    }
    run_checked(CLIENT_WRAPPER_PATH, &["-q"], "断开有线认证失败")
}

fn enable_service(options: &AuthOptions) -> Result<()> {
    ensure_client_installed()?;
    write_service_file(&service_file(options))?;
    run_checked(SYSTEMCTL, &["daemon-reload"], "重新加载 systemd 失败")?;
    run_checked(SYSTEMCTL, &["enable", SERVICE_NAME], "启用开机认证失败")?;
    run_checked(SYSTEMCTL, &["restart", SERVICE_NAME], "启动开机认证失败")
}

fn disable_service() -> Result<()> {
    ensure_service_is_safe()?;
    run_checked(
        SYSTEMCTL,
        &["disable", "--now", SERVICE_NAME],
        "禁用开机认证失败",
    )
}

fn client_arguments(options: &AuthOptions, include_password: bool) -> Vec<String> {
    let mut args = vec![
        "-a".to_string(),
        "1".to_string(),
        "-d".to_string(),
        if options.dhcp { "1" } else { "0" }.to_string(),
        "-n".to_string(),
        options.nic.clone(),
        "-u".to_string(),
        options.username.clone(),
        "-S".to_string(),
        if options.save_password { "1" } else { "0" }.to_string(),
    ];
    if include_password && let Some(password) = options.password.as_ref() {
        args.push("-p".to_string());
        args.push(password.clone());
    }
    args
}

fn ensure_client_installed() -> Result<()> {
    if is_secure_root_executable(Path::new(CLIENT_WRAPPER_PATH))
        && is_secure_root_executable(&client_binary_path())
    {
        return Ok(());
    }
    anyhow::bail!("root-owned 官方客户端未安装或权限不安全")
}

fn is_secure_root_executable(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| {
            metadata.file_type().is_file()
                && metadata.uid() == 0
                && metadata.mode() & 0o022 == 0
                && metadata.mode() & 0o111 != 0
        })
        .unwrap_or(false)
}

fn ensure_service_is_safe() -> Result<()> {
    let metadata = fs::symlink_metadata(SERVICE_PATH).context("无法检查 systemd 服务文件")?;
    if !metadata.file_type().is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        anyhow::bail!("拒绝操作非 root-owned 或可写的旧 systemd 服务，请先重新启用以完成迁移");
    }
    let content = fs::read_to_string(SERVICE_PATH).context("无法读取 systemd 服务文件")?;
    if !service_content_uses_owned_paths(&content) {
        anyhow::bail!("拒绝执行引用旧用户路径的 systemd 服务，请先重新启用以完成迁移");
    }
    Ok(())
}

fn write_service_file(content: &str) -> Result<()> {
    let path = Path::new(SERVICE_PATH);
    let parent = path.parent().context("无法确定 systemd 服务目录")?;
    let temporary = parent.join(format!(".rjsupplicant.service.{}.tmp", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true).mode(0o600);
    let result = (|| {
        let mut file = options.open(&temporary).context("无法创建临时服务文件")?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o644))?;
        fs::rename(&temporary, path).context("无法安装 systemd 服务文件")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn command_succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// 官方客户端启动时会主动停止 NetworkManager（strace 实测确认，属其设计行为）；
/// 在客户端可能运行过的每个动作后恢复，避免本机无线网络被连带断开。
/// 幂等：NetworkManager 已运行时 `start` 是空操作。
fn restore_network_services() {
    let _ = Command::new(SYSTEMCTL)
        .args(["start", "NetworkManager.service"])
        .status();
}

fn run_checked(program: &str, args: &[&str], context: &str) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("{context}：无法启动命令"))?;
    if status.success() {
        return Ok(());
    }
    anyhow::bail!("{context}：{status}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_client_arguments_without_empty_password() {
        let options = AuthOptions {
            username: "20260001".to_string(),
            nic: "enp4s0".to_string(),
            dhcp: false,
            save_password: true,
            password: None,
        };
        assert_eq!(
            client_arguments(&options, true),
            [
                "-a", "1", "-d", "0", "-n", "enp4s0", "-u", "20260001", "-S", "1"
            ]
        );
    }

    #[test]
    fn effective_uid_parser_finds_current_process() {
        let status = fs::read_to_string("/proc/self/status").expect("read process status");
        assert!(status.lines().any(|line| line.starts_with("Uid:")));
    }

    #[test]
    fn validates_password_from_standard_input() {
        assert_eq!(
            validate_password_input(Vec::new()).expect("empty input"),
            None
        );
        assert_eq!(
            validate_password_input("secret 密码".as_bytes().to_vec()).expect("password input"),
            Some("secret 密码".to_string())
        );
        assert!(validate_password_input(vec![0]).is_err());
        assert!(validate_password_input(vec![b'x'; MAX_PASSWORD_BYTES + 1]).is_err());
    }

    #[test]
    fn detects_auth_failure_markers() {
        assert_eq!(
            auth_failure_reason("网线没有连接上，请检查网卡连接").as_deref(),
            Some("网线没有连接上")
        );
        assert_eq!(
            auth_failure_reason("2026-09-01 17:15:44 无法获取动态IP地址").as_deref(),
            Some("无法获取动态IP地址")
        );
        assert_eq!(
            auth_failure_reason("认证失败无法连接认证服务器。").as_deref(),
            Some("无法连接认证服务器")
        );
        assert_eq!(
            auth_failure_reason("认证成功\n管理中心提示： 欢迎使用广外大网络！").as_deref(),
            None
        );
        assert_eq!(auth_failure_reason("").as_deref(), None);
    }

    #[test]
    fn new_log_tail_advances_offset_and_survives_truncation() {
        use std::io::Write;

        let path = std::env::temp_dir().join(format!(
            "rjsupplicant-helper-log-tail-{}",
            std::process::id()
        ));

        fs::write(&path, "第一轮\n").expect("写入初始日志");
        let mut offset = fs::metadata(&path).expect("读取日志长度").len();
        assert_eq!(new_log_tail(&path, &mut offset), None, "起点之后没有新内容");

        // 追加后只返回新增部分，且偏移必须前进，否则每 200ms 都在重读历史。
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("打开日志追加")
            .write_all("认证成功\n".as_bytes())
            .expect("追加认证成功");
        assert_eq!(
            new_log_tail(&path, &mut offset).as_deref(),
            Some("认证成功\n")
        );
        assert_eq!(
            offset,
            fs::metadata(&path).expect("读取日志长度").len(),
            "读取后偏移应推进到文件末尾"
        );
        assert_eq!(
            new_log_tail(&path, &mut offset),
            None,
            "偏移未前进会重复读到旧的认证成功行"
        );

        // 日志被清空/轮转（长度变短）后必须能重新判定，而不是永久返回 None。
        fs::write(&path, "x\n").expect("重写为更短的日志");
        assert_eq!(new_log_tail(&path, &mut offset).as_deref(), Some("x\n"));

        // 非法 UTF-8 段应被跳过（并把偏移推过它），其后的正常内容仍可判定。
        fs::write(&path, [0xff_u8; 8]).expect("写入非法字节");
        assert_eq!(new_log_tail(&path, &mut offset), None, "坏字节段无法解析");
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("打开日志追加")
            .write_all("认证成功\n".as_bytes())
            .expect("追加认证成功");
        assert_eq!(
            new_log_tail(&path, &mut offset).as_deref(),
            Some("认证成功\n"),
            "跳过坏字节后应能继续判定"
        );

        fs::remove_file(&path).expect("清理临时日志");
    }

    #[test]
    fn early_client_exit_is_not_confused_with_auth_success() {
        // 客户端崩溃后残留的“认证成功”不能被当成本轮结果；这条路径同时
        // 必须触发网络恢复，由 authenticate 里的 NetworkRestorer 守卫保证。
        assert_eq!(
            classify_auth(true, Some("认证成功")),
            AuthOutcome::ClientExited
        );
        assert_eq!(classify_auth(true, None), AuthOutcome::ClientExited);
    }

    #[test]
    fn classifies_running_client_by_new_log_only() {
        assert_eq!(classify_auth(false, None), AuthOutcome::Pending);
        assert_eq!(classify_auth(false, Some("")), AuthOutcome::Pending);
        assert_eq!(
            classify_auth(false, Some("网线没有连接上，请检查网卡连接")),
            AuthOutcome::Failed("网线没有连接上".to_string())
        );
        assert_eq!(
            classify_auth(false, Some("认证成功\n管理中心提示： 欢迎使用广外大网络！")),
            AuthOutcome::Succeeded
        );
        // 官方提示里带“认证失败”字样时，一次真实成功不能被翻成失败。
        assert_eq!(
            classify_auth(false, Some("认证成功\n上次认证失败的原因已修复")),
            AuthOutcome::Succeeded
        );
    }
}
