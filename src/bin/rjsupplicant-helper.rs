use anyhow::{Context, Result};
use rjsupplicant_gui::client_install;
use rjsupplicant_gui::privileged::{
    AuthOptions, CLIENT_DIR, CLIENT_WRAPPER_PATH, HelperRequest, SERVICE_PATH, client_binary_path,
    service_content_uses_owned_paths, service_file,
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
        HelperRequest::Disconnect => disconnect(),
        HelperRequest::EnableService(options) => enable_service(&options),
        HelperRequest::DisableService => disable_service(),
        HelperRequest::RestartService => {
            ensure_client_installed()?;
            ensure_service_is_safe()?;
            run_checked(SYSTEMCTL, &["restart", SERVICE_NAME], "重启认证服务失败")
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
    run_owned_checked(CLIENT_WRAPPER_PATH, &args, "有线认证失败")
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

fn run_owned_checked(program: &str, args: &[String], context: &str) -> Result<()> {
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
}
