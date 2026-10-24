use anyhow::Result;
use std::path::{Path, PathBuf};

pub const HELPER_PATH: &str = "/usr/lib/rjsupplicant-gui/rjsupplicant-helper";
pub const CLIENT_DIR: &str = "/usr/lib/rjsupplicant";
pub const CLIENT_WRAPPER_PATH: &str = "/usr/lib/rjsupplicant-gui/rjsupplicant";
pub const SERVICE_PATH: &str = "/etc/systemd/system/rjsupplicant.service";

#[derive(Clone, Eq, PartialEq)]
pub struct AuthOptions {
    pub username: String,
    pub nic: String,
    pub dhcp: bool,
    pub save_password: bool,
    pub password: Option<String>,
}

/// 手写 Debug：`password` 永远只打印 `<redacted>`，避免任何日志/错误链
/// 意外把明文口令带入 journal 或 GUI toast。
impl std::fmt::Debug for AuthOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthOptions")
            .field("username", &self.username)
            .field("nic", &self.nic)
            .field("dhcp", &self.dhcp)
            .field("save_password", &self.save_password)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// 官方客户端 run.log 的单行判定结果。
///
/// 日志语言随运行环境变化：本机 `en_US.UTF-8` 下客户端实际写
/// `Success` / `Supplicate failure`，中文环境写 `认证成功` / `认证失败`。
/// 历史上这里只匹配中文，导致英文环境下一次已成功的认证要等到 60 秒超时，
/// 期间 GUI 一直显示“未连接”。marker 表必须由 helper 与 GUI 共用。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthLineKind {
    /// 本轮认证成功（终局）。
    Success,
    /// 客户端仍在重试的中间状态：断链重连、单次认证失败、IP 变更等。
    /// 不能据此终止客户端，也不能写进“最近认证”历史。
    TransientFailure,
    /// 本轮认证的终局失败。
    TerminalFailure,
}

/// 成功判定必须是整行（去时间戳后）匹配：官方横幅里可能出现
/// `quit rjsupplicant success.` 之类的非判定文本，`contains` 会误报。
const AUTH_SUCCESS_LINES: &[&str] = &["认证成功", "Success", "Supplicate success"];

/// 重试中的提示。`Supplicate failure` 实测每次后面都继续认证并最终成功，
/// 属于客户端内部重试，不能当成终局失败。
const AUTH_TRANSIENT_MARKERS: &[&str] = &[
    "网线没有连接上",
    "Adapter links down",
    "Adapter cann't work",
    "网卡已被禁用",
    "Supplicate failure",
    "IP has chaged",
    "IP地址发生变化",
];

/// 终局失败（中英文两套，取自官方 `uiCHS.ini` / `uiENG.ini`）。
const AUTH_TERMINAL_MARKERS: &[&str] = &[
    "认证失败",
    "无法连接认证服务器",
    "Can't connect radius server",
    "无法获取动态IP地址",
    "Unable to get dynamic IP address",
    "Authentication failure",
    "Authentication failed",
    "IP地址信息获取错误",
    "Failed to obtain IP information",
    "认证已关闭",
    "Supplicate close",
];

/// 兼容旧调用方的成功标记；新代码请用 [`classify_auth_line`]。
pub const AUTH_SUCCESS_MARKER: &str = "认证成功";

/// 兼容旧调用方的失败标记；新代码请用 [`classify_auth_line`]（它区分
/// 重试中的 `Supplicate failure` 与真正的终局失败）。
pub const AUTH_FAILURE_MARKERS: &[&str] = &[
    "网线没有连接上",
    "无法连接认证服务器",
    "认证失败",
    "无法获取动态IP地址",
];

/// 判定一行（已去掉时间戳）的语义；无法识别返回 `None`。
pub fn classify_auth_line(body: &str) -> Option<AuthLineKind> {
    let line = body.trim();
    if line.is_empty() {
        return None;
    }
    if AUTH_SUCCESS_LINES
        .iter()
        .any(|marker| line.eq_ignore_ascii_case(marker))
    {
        return Some(AuthLineKind::Success);
    }
    if match_marker(line, AUTH_TERMINAL_MARKERS).is_some() {
        return Some(AuthLineKind::TerminalFailure);
    }
    if match_marker(line, AUTH_TRANSIENT_MARKERS).is_some() {
        return Some(AuthLineKind::TransientFailure);
    }
    None
}

/// 一段新增日志中优先级最高的判定：成功 > 终局失败 > 重试中。
/// 成功优先，避免同一段里先出现一次 `Supplicate failure`、随后成功时被翻成失败。
pub fn classify_auth_text(text: &str) -> Option<AuthLineKind> {
    let mut fallback = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let body = match split_leading_timestamp(line) {
            Some((_, body)) => split_leading_timestamp(&body).map_or(body, |(_, inner)| inner),
            None => line.to_string(),
        };
        match classify_auth_line(&body) {
            Some(AuthLineKind::Success) => return Some(AuthLineKind::Success),
            Some(AuthLineKind::TerminalFailure) => {
                fallback.get_or_insert(AuthLineKind::TerminalFailure);
            }
            Some(AuthLineKind::TransientFailure) => {
                fallback.get_or_insert(AuthLineKind::TransientFailure);
            }
            None => {}
        }
    }
    fallback
}

/// 命中 `markers` 中任一标记时返回该标记（用于错误文案）。
pub fn auth_line_reason(body: &str) -> Option<&'static str> {
    let line = body.trim();
    match classify_auth_line(line)? {
        AuthLineKind::TerminalFailure => match_marker(line, AUTH_TERMINAL_MARKERS),
        AuthLineKind::TransientFailure => match_marker(line, AUTH_TRANSIENT_MARKERS),
        AuthLineKind::Success => None,
    }
}

fn match_marker(line: &str, markers: &[&'static str]) -> Option<&'static str> {
    markers
        .iter()
        .find(|marker| line.contains(**marker))
        .copied()
}

/// 剥掉 `YYYY-MM-DD HH:MM:SS ` 前缀；格式不符时返回 `None`。
fn split_leading_timestamp(line: &str) -> Option<(String, String)> {
    let mut parts = line.splitn(3, ' ');
    let (date, clock, rest) = (parts.next()?, parts.next()?, parts.next()?);
    if date.len() == 10
        && clock.len() == 8
        && date.chars().all(|ch| ch.is_ascii_digit() || ch == '-')
        && clock.chars().all(|ch| ch.is_ascii_digit() || ch == ':')
    {
        return Some((format!("{date} {clock}"), rest.trim().to_string()));
    }
    None
}

/// 「最近认证」历史解析使用的终局失败标记。历史上 `网线没有连接上` 曾被当成
/// 终局失败，实际是链路重协商期间的重试提示；现在统一由 [`classify_auth_line`]
/// 区分终局/重试，本常量仅保留给旧测试与文档引用。
pub const AUTH_HISTORY_FAILURE_MARKERS: &[&str] =
    &["认证失败", "无法连接认证服务器", "无法获取动态IP地址"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HelperRequest {
    InstallClient(PathBuf),
    /// 仅由 install.sh 以 sudo 调用 root helper：重写官方客户端 wrapper，
    /// 把既有安装升级到带 getifaddrs 垫片加载逻辑的新版。与 restore-network、
    /// should-authenticate 同类，只由 root 侧调用，因此不进 polkit 动作表。
    InstallWrapper,
    Authenticate(AuthOptions),
    Disconnect,
    EnableService(AuthOptions),
    DisableService,
    RestartService,
    /// 仅由 systemd 在 ExecCondition 中以 root 调用：判断本次开机是否需要
    /// 启动客户端。有线路径已可用（网关会话仍保持）或网线未连接时以退出码 1
    /// 让 systemd 跳过本次启动（ExecCondition 语义：1-254 = skip，单元不算
    /// 失败、不触发 Restart），避免在会话已可用时反复停 NM 甚至反复崩溃。
    ShouldAuthenticate(String),
    /// 仅由 systemd 在 ExecStartPost 中以 root 调用：恢复被官方客户端
    /// 主动停掉的 NetworkManager，防止开机认证后无线网络连接受损。
    RestoreNetwork,
}

impl HelperRequest {
    pub fn parse(args: &[String]) -> Result<Self> {
        let Some(command) = args.first().map(String::as_str) else {
            anyhow::bail!("缺少 helper 子命令");
        };
        match command {
            "install-client" if args.len() == 2 => {
                let path = PathBuf::from(&args[1]);
                if !path.is_absolute() {
                    anyhow::bail!("客户端 ZIP 必须使用绝对路径");
                }
                Ok(Self::InstallClient(path))
            }
            "install-wrapper" if args.len() == 1 => Ok(Self::InstallWrapper),
            "authenticate" if args.len() == 5 => {
                let options = parse_options(&args[1..], None)?;
                Ok(Self::Authenticate(options))
            }
            "disconnect" if args.len() == 1 => Ok(Self::Disconnect),
            "enable-service" if args.len() == 5 => {
                let options = parse_options(&args[1..], None)?;
                Ok(Self::EnableService(options))
            }
            "disable-service" if args.len() == 1 => Ok(Self::DisableService),
            "restart-service" if args.len() == 1 => Ok(Self::RestartService),
            "should-authenticate" if args.len() == 2 => {
                let nic = args[1].trim().to_string();
                validate_nic(&nic)?;
                Ok(Self::ShouldAuthenticate(nic))
            }
            "restore-network" if args.len() == 1 => Ok(Self::RestoreNetwork),
            _ => anyhow::bail!("不支持的 helper 子命令或参数数量：{command}"),
        }
    }

    pub fn arguments(&self) -> Vec<String> {
        match self {
            Self::InstallClient(path) => vec![
                "install-client".to_string(),
                path.to_string_lossy().into_owned(),
            ],
            Self::InstallWrapper => vec!["install-wrapper".to_string()],
            Self::Authenticate(options) => options_arguments("authenticate", options),
            Self::Disconnect => vec!["disconnect".to_string()],
            Self::EnableService(options) => options_arguments("enable-service", options),
            Self::DisableService => vec!["disable-service".to_string()],
            Self::RestartService => vec!["restart-service".to_string()],
            Self::ShouldAuthenticate(nic) => {
                vec!["should-authenticate".to_string(), nic.clone()]
            }
            Self::RestoreNetwork => vec!["restore-network".to_string()],
        }
    }
}

pub fn client_binary_path() -> PathBuf {
    Path::new(CLIENT_DIR)
        .join(current_arch_dir())
        .join("rjsupplicant")
}

pub fn client_log_path() -> PathBuf {
    Path::new(CLIENT_DIR)
        .join(current_arch_dir())
        .join("log/run.log")
}

/// 生成开机认证服务单元。
///
/// 关键设计（都有实机依据）：
///   * `Type=simple`：官方客户端保持前台运行、持会话；后台化假设会立刻失败；
///   * `ExecCondition` 先探测有线路径：已联网（网关会话仍保持）或网线未连接时
///     跳过本次启动，避免在会话已可用时反复停 NM、反复触发客户端崩溃；
///   * `ExecStartPost` 在启动约 8 秒后恢复被客户端停掉的 NM（DHCP 注入）；
///   * 重试上限放宽到 5 次 / 300 秒：客户端崩溃是间歇性竞态，给开机阶段留出
///     更多重试机会，而不是 3 次 / 60 秒就把单元打进 failed。
pub fn service_file(options: &AuthOptions) -> String {
    let dhcp = bool_flag(options.dhcp);
    let save = bool_flag(options.save_password);
    let client = systemd_quote(CLIENT_WRAPPER_PATH);
    let nic = systemd_quote(&options.nic);
    let username = systemd_quote(&options.username);
    // systemd 的 WorkingDirectory= 不接受引号包裹（与 ExecStart 不同），必须裸绝对路径
    let workdir = Path::new(CLIENT_DIR)
        .join(current_arch_dir())
        .to_string_lossy()
        .into_owned();

    format!(
        "         [Unit]\n\
         Description=Ruijie RG-SU wired authentication client\n\
         Documentation=https://etr.gdufs.edu.cn/info/1303/5137.htm\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         StartLimitIntervalSec=300\n\
         StartLimitBurst=5\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecCondition=\"{HELPER_PATH}\" should-authenticate {nic}\n\
         ExecStart={client} -a 1 -d {dhcp} -n {nic} -u {username} -S {save}\n\
         ExecStop={client} -q\n\
         ExecStartPost=\"{HELPER_PATH}\" restore-network\n\
         Restart=on-failure\n\
         RestartSec=15\n\
         TimeoutStartSec=60\n\
         TimeoutStopSec=15\n\
         WorkingDirectory={workdir}\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n"
    )
}

/// 本项目模板会写入的 unit 指令。校验器只接受这些键，其余（`User=`、
/// `BindPaths=`、`RootDirectory=`、`StandardOutput=file=` 等）一律判为不安全：
/// 旧实现只看 `Exec*`/`WorkingDirectory=`/`Environment*` 三类行，其它指令原样
/// 放行，导致"服务文件安全"这个结论弱于它的字面含义。
const SAFE_SERVICE_KEYS: &[&str] = &[
    "Description",
    "Documentation",
    "After",
    "Wants",
    "StartLimitIntervalSec",
    "StartLimitBurst",
    "Type",
    "ExecStart",
    "ExecStop",
    "ExecStartPost",
    "Restart",
    "RestartSec",
    "TimeoutStartSec",
    "TimeoutStopSec",
    "WorkingDirectory",
    "WantedBy",
];

/// 安全校验：单元只引用固定 root-owned 路径、不夹带可提权指令。
///
/// 注意这是"安全"而不是"模板当前版本"：旧模板（无 ExecCondition）仍然只引用
/// 固定路径，因此**可以被安全地禁用/重启**（否则用户无法关闭一个待迁移的服务）；
/// 是否需要迁移由 [`service_is_current_template`] 单独判断。
pub fn service_content_uses_owned_paths(content: &str) -> bool {
    let expected_program = format!("ExecStart=\"{CLIENT_WRAPPER_PATH}\"");
    let expected_stop = format!("ExecStop=\"{CLIENT_WRAPPER_PATH}\" -q");
    let expected_post = format!("ExecStartPost=\"{HELPER_PATH}\" restore-network");
    let expected_workdir = format!(
        "WorkingDirectory={}",
        Path::new(CLIENT_DIR).join(current_arch_dir()).display()
    );
    let mut start_count = 0;
    let mut stop_count = 0;
    let mut post_count = 0;
    let mut condition_count = 0;
    let mut workdir_count = 0;
    let mut start_nic = None;
    let mut condition_nic = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(['#', ';']) || trimmed.starts_with('[') {
            continue;
        }
        if trimmed.starts_with("Exec") {
            if trimmed.starts_with(&expected_program)
                && let Some(nic) = service_start_nic(trimmed)
            {
                start_count += 1;
                start_nic = Some(nic);
            } else if trimmed == expected_stop {
                stop_count += 1;
            } else if trimmed == expected_post {
                post_count += 1;
            } else if let Some(nic) = service_condition_nic(trimmed) {
                condition_count += 1;
                condition_nic = Some(nic);
            } else {
                return false;
            }
        } else if trimmed.starts_with("WorkingDirectory=") {
            if trimmed != expected_workdir {
                return false;
            }
            workdir_count += 1;
        } else if trimmed.starts_with("Environment") {
            return false;
        } else {
            let key = trimmed.split('=').next().unwrap_or_default().trim();
            if !SAFE_SERVICE_KEYS.contains(&key) {
                return false;
            }
        }
    }

    start_count == 1
        && stop_count == 1
        && post_count == 1
        && workdir_count == 1
        && condition_count <= 1
        && (condition_count == 0 || condition_nic == start_nic)
}

/// 停止/禁用场景的安全校验：只需要 ExecStop 指向固定 root-owned wrapper，
/// ExecStart 也必须是固定路径（证明这是本项目生成的单元），但不要求当前模板
/// 一定带 ExecStartPost/ExecCondition——否则旧模板会陷入"想禁用却被告知先
/// 重新启用"的迁移死锁。`systemctl disable --now` 实际只会执行 ExecStop。
pub fn service_content_safe_to_stop(content: &str) -> bool {
    let expected_start = format!("ExecStart=\"{CLIENT_WRAPPER_PATH}\"");
    let expected_stop = format!("ExecStop=\"{CLIENT_WRAPPER_PATH}\" -q");
    let expected_post = format!("ExecStartPost=\"{HELPER_PATH}\" restore-network");
    let expected_workdir = format!(
        "WorkingDirectory={}",
        Path::new(CLIENT_DIR).join(current_arch_dir()).display()
    );
    let mut start_count = 0;
    let mut stop_count = 0;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(['#', ';']) || trimmed.starts_with('[') {
            continue;
        }
        if trimmed.starts_with("Exec") {
            if trimmed == expected_stop {
                stop_count += 1;
            } else if trimmed.starts_with(&expected_start) && service_start_nic(trimmed).is_some() {
                start_count += 1;
            } else if trimmed == expected_post || service_condition_nic(trimmed).is_some() {
                // 停止不会执行这两类行；只要求它们也引用固定路径。
            } else {
                return false;
            }
        } else if trimmed.starts_with("WorkingDirectory=") {
            if trimmed != expected_workdir {
                return false;
            }
        } else if trimmed.starts_with("Environment") {
            return false;
        } else {
            let key = trimmed.split('=').next().unwrap_or_default().trim();
            if !SAFE_SERVICE_KEYS.contains(&key) {
                return false;
            }
        }
    }

    start_count == 1 && stop_count == 1
}

/// 单元模板是否为当前版本（带"已联网/无网线则跳过"的 ExecCondition）。
/// 仅用于提示用户重新开启一次开机认证完成迁移；它不影响安全判断。
pub fn service_is_current_template(content: &str) -> bool {
    content
        .lines()
        .any(|line| service_condition_nic(line).is_some())
}

/// 解析 ExecStart 行并返回其中的网卡名；任何不合规都返回 None。
fn service_start_nic(line: &str) -> Option<String> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 11
        || fields[0] != format!("ExecStart=\"{CLIENT_WRAPPER_PATH}\"")
        || fields[1] != "-a"
        || fields[2] != "1"
        || fields[3] != "-d"
        || fields[5] != "-n"
        || fields[7] != "-u"
        || fields[9] != "-S"
    {
        return None;
    }
    let nic = fields[6].strip_prefix('"')?.strip_suffix('"')?;
    let username = fields[8].strip_prefix('"')?.strip_suffix('"')?;
    HelperRequest::parse(&[
        "enable-service".to_string(),
        fields[4].to_string(),
        nic.to_string(),
        username.to_string(),
        fields[10].to_string(),
    ])
    .ok()?;
    Some(nic.to_string())
}

/// 解析 ExecCondition 行并返回其中的网卡名；任何不合规都返回 None。
fn service_condition_nic(line: &str) -> Option<String> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 3
        || fields[0] != format!("ExecCondition=\"{HELPER_PATH}\"")
        || fields[1] != "should-authenticate"
    {
        return None;
    }
    let nic = fields[2].strip_prefix('"')?.strip_suffix('"')?;
    HelperRequest::parse(&["should-authenticate".to_string(), nic.to_string()]).ok()?;
    Some(nic.to_string())
}

fn parse_options(args: &[String], password: Option<String>) -> Result<AuthOptions> {
    if args.len() < 4 {
        anyhow::bail!("认证参数不完整");
    }
    let dhcp = parse_bool_flag(&args[0], "DHCP")?;
    let nic = args[1].trim().to_string();
    let username = args[2].trim().to_string();
    let save_password = parse_bool_flag(&args[3], "保存密码")?;
    validate_username(&username)?;
    validate_nic(&nic)?;
    Ok(AuthOptions {
        username,
        nic,
        dhcp,
        save_password,
        password: password.filter(|value| !value.is_empty()),
    })
}

fn options_arguments(command: &str, options: &AuthOptions) -> Vec<String> {
    vec![
        command.to_string(),
        bool_flag(options.dhcp).to_string(),
        options.nic.clone(),
        options.username.clone(),
        bool_flag(options.save_password).to_string(),
    ]
}

fn parse_bool_flag(value: &str, label: &str) -> Result<bool> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => anyhow::bail!("{label} 参数必须是 0 或 1"),
    }
}

fn bool_flag(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

fn validate_username(username: &str) -> Result<()> {
    if username.is_empty() {
        anyhow::bail!("校园网账号不能为空");
    }
    if username.len() > 128
        || !username
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '@' | '.' | '_' | '+' | '-'))
    {
        anyhow::bail!("校园网账号包含不支持的字符");
    }
    Ok(())
}

fn validate_nic(nic: &str) -> Result<()> {
    if nic.is_empty()
        || nic.len() >= libc::IFNAMSIZ
        || nic == "."
        || nic == ".."
        || !nic
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-'))
    {
        anyhow::bail!("网卡名称无效");
    }
    Ok(())
}

/// 官方客户端只提供 x86 的 `x64`/`x86` 两种目录。用指针宽度判定会在 aarch64 上
/// 得到 64 位并错误指向不存在的 `x64`，因此按实际编译架构判定；三处
/// （helper、GUI 配置路径、客户端安装）必须共用这一个来源，避免各算各的。
pub fn client_arch_dir() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("x64"),
        "x86" => Some("x86"),
        _ => None,
    }
}

/// 无法识别架构时返回一个必然不存在的目录名，让"客户端已安装"之类的检查统一
/// 判为假，而不是猜一个 x64 去访问别人的目录。
fn current_arch_dir() -> &'static str {
    match client_arch_dir() {
        Some(dir) => dir,
        None => "unsupported-arch",
    }
}

fn systemd_quote(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> AuthOptions {
        AuthOptions {
            username: "20260001@gdufs".to_string(),
            nic: "enp4s0.20".to_string(),
            dhcp: true,
            save_password: false,
            password: Some("secret".to_string()),
        }
    }

    #[test]
    fn authenticate_arguments_never_contain_password() {
        let request = HelperRequest::Authenticate(options());
        let arguments = request.arguments();
        assert!(!arguments.iter().any(|argument| argument == "secret"));
        assert_eq!(
            HelperRequest::parse(&arguments).expect("parse request"),
            HelperRequest::Authenticate(AuthOptions {
                password: None,
                ..options()
            })
        );
    }

    #[test]
    fn rejects_unknown_extra_or_unsafe_arguments() {
        assert!(HelperRequest::parse(&["unknown".to_string()]).is_err());
        assert!(HelperRequest::parse(&["disconnect".to_string(), "extra".to_string()]).is_err());
        assert!(
            HelperRequest::parse(&[
                "authenticate".to_string(),
                "1".to_string(),
                "eno1 --help".to_string(),
                "student".to_string(),
                "1".to_string(),
            ])
            .is_err()
        );
        assert!(
            HelperRequest::parse(&["install-client".to_string(), "relative.zip".to_string(),])
                .is_err()
        );
        assert!(
            HelperRequest::parse(&[
                "authenticate".to_string(),
                "1".to_string(),
                "eno1".to_string(),
                "student".to_string(),
                "1".to_string(),
                "password-must-use-stdin".to_string(),
            ])
            .is_err()
        );
    }

    #[test]
    fn root_service_uses_only_fixed_client_paths() {
        let content = service_file(&options());
        assert!(service_content_uses_owned_paths(&content));
        assert!(service_is_current_template(&content));
        assert!(content.contains("Type=simple"));
        assert!(content.contains(CLIENT_WRAPPER_PATH));
        assert!(content.contains("-n \"enp4s0.20\" -u \"20260001@gdufs\""));
        assert!(content.contains(
            "ExecCondition=\"/usr/lib/rjsupplicant-gui/rjsupplicant-helper\" \
             should-authenticate \"enp4s0.20\""
        ));
        assert!(content.contains(
            "ExecStartPost=\"/usr/lib/rjsupplicant-gui/rjsupplicant-helper\" restore-network"
        ));
        assert!(!content.contains("/home/"));
    }

    #[test]
    fn service_without_exec_condition_is_safe_but_not_current() {
        // 旧模板（无 ExecCondition）必须仍然通过安全校验：否则用户无法关闭一个
        // 等待迁移的服务（disable/restart 都走同一校验），会形成死锁。
        let legacy = service_file(&options()).replace(
            "ExecCondition=\"/usr/lib/rjsupplicant-gui/rjsupplicant-helper\" \
             should-authenticate \"enp4s0.20\"\n",
            "",
        );
        assert!(!legacy.contains("ExecCondition="), "测试替换未生效");
        assert!(service_content_uses_owned_paths(&legacy));
        assert!(!service_is_current_template(&legacy));
    }

    #[test]
    fn rejects_exec_condition_with_foreign_targets() {
        let content = service_file(&options());
        let foreign_path = content.replace(
            "ExecCondition=\"/usr/lib/rjsupplicant-gui/rjsupplicant-helper\" should-authenticate",
            "ExecCondition=\"/tmp/evil-helper\" should-authenticate",
        );
        assert!(!service_content_uses_owned_paths(&foreign_path));

        let wrong_action = content.replace(
            "should-authenticate \"enp4s0.20\"",
            "restore-network \"enp4s0.20\"",
        );
        assert!(!service_content_uses_owned_paths(&wrong_action));

        // 条件与 ExecStart 的网卡不一致：必须拒绝，否则"跳过"判断会落在别的网卡上。
        let other_nic = content.replace(
            "should-authenticate \"enp4s0.20\"",
            "should-authenticate \"eno1\"",
        );
        assert!(!service_content_uses_owned_paths(&other_nic));
    }

    #[test]
    fn parses_should_authenticate_action() {
        let request = HelperRequest::ShouldAuthenticate("enp4s0.20".to_string());
        assert_eq!(
            HelperRequest::parse(&request.arguments()).expect("round-trip"),
            request
        );
        assert!(HelperRequest::parse(&["should-authenticate".to_string()]).is_err());
        assert!(
            HelperRequest::parse(&[
                "should-authenticate".to_string(),
                "enp4s0.20".to_string(),
                "extra".to_string(),
            ])
            .is_err()
        );
        assert!(
            HelperRequest::parse(&["should-authenticate".to_string(), "eno1 --help".to_string(),])
                .is_err()
        );
    }

    #[test]
    fn rejects_service_units_carrying_unexpected_directives() {
        // 校验器过去只看 Exec* / WorkingDirectory= / Environment* 三类行，
        // 其余指令原样放行，于是 User= 这类提权外指令会被判成"安全"。
        let base = service_file(&options());
        for injected in [
            "Type=simple\nUser=nobody\n",
            "Type=simple\nBindPaths=/home/student:/mnt\n",
            "Type=simple\nRootDirectory=/home/student\n",
            "Type=simple\nStandardOutput=file:/home/student/out\n",
            "Type=simple\nOnFailure=evil.target\n",
        ] {
            let content = base.replace("Type=simple\n", injected);
            assert!(content != base, "注入未生效，测试本身失效：{injected:?}");
            assert!(
                !service_content_uses_owned_paths(&content),
                "未知 systemd 指令被当成安全：{injected:?}"
            );
        }
    }

    #[test]
    fn accepts_service_comments_and_section_headers() {
        // 白名单不能把注释与 [Section] 行误判成危险指令。
        let content = service_file(&options())
            .replace("[Service]\n", "# 维护说明\n; 第二行注释\n[Service]\n");
        assert!(service_content_uses_owned_paths(&content));
    }

    #[test]
    fn parses_restore_network_action() {
        assert_eq!(
            HelperRequest::parse(&["restore-network".to_string()]).expect("parse"),
            HelperRequest::RestoreNetwork
        );
        assert!(
            HelperRequest::parse(&["restore-network".to_string(), "extra".to_string()]).is_err()
        );
    }

    #[test]
    fn parses_install_wrapper_action() {
        let request = HelperRequest::InstallWrapper;
        assert_eq!(
            HelperRequest::parse(&request.arguments()).expect("round-trip"),
            request
        );
        assert_eq!(request.arguments(), ["install-wrapper"]);
        assert!(
            HelperRequest::parse(&["install-wrapper".to_string(), "extra".to_string()]).is_err()
        );
    }

    #[test]
    fn classifies_english_and_chinese_auth_lines() {
        for line in ["Success", "认证成功", "Supplicate success", " success "] {
            assert_eq!(
                classify_auth_line(line),
                Some(AuthLineKind::Success),
                "{line}"
            );
        }
        for line in [
            "Supplicate failure",
            "网线没有连接上，请检查网卡连接。",
            "Adapter links down, Please check it.",
            "IP has chaged,please re-auth.",
        ] {
            assert_eq!(
                classify_auth_line(line),
                Some(AuthLineKind::TransientFailure),
                "{line}"
            );
        }
        for line in [
            "Authentication failure,Reason： ...",
            "Can't connect radius server.",
            "Unable to get dynamic IP address.",
            "认证失败，可能导致本次认证失败的原因如下",
            "无法获取动态IP地址",
            "Supplicate close",
        ] {
            assert_eq!(
                classify_auth_line(line),
                Some(AuthLineKind::TerminalFailure),
                "{line}"
            );
        }
        for line in [
            "Lookup the switch...",
            "Connecting radius server...",
            "quit rjsupplicant success.",
        ] {
            assert_eq!(classify_auth_line(line), None, "{line}");
        }
    }

    #[test]
    fn classify_auth_text_prioritizes_success_over_retries() {
        let text = "2026-10-24 09:50:55 Supplicate failure\n\
            2026-10-24 09:51:01 Success\n";
        assert_eq!(classify_auth_text(text), Some(AuthLineKind::Success));

        let text = "2026-10-24 09:50:55 Supplicate failure\n\
            2026-10-24 09:50:56 Lookup the switch...\n";
        assert_eq!(
            classify_auth_text(text),
            Some(AuthLineKind::TransientFailure)
        );

        assert_eq!(
            classify_auth_text("Adapter links up, Speed:1000.0 Mbps"),
            None
        );

        // 无时间戳的原始行也要能判定（单测与手工粘贴的日志片段不带前缀）。
        assert_eq!(
            classify_auth_text("Success\nSupplicate failure"),
            Some(AuthLineKind::Success)
        );
        assert_eq!(
            classify_auth_text("Supplicate failure"),
            Some(AuthLineKind::TransientFailure)
        );
    }

    #[test]
    fn old_service_without_restore_post_is_unsafe() {
        let content = service_file(&options());
        let legacy = content.replace(
            "ExecStartPost=\"/usr/lib/rjsupplicant-gui/rjsupplicant-helper\" restore-network\n",
            "",
        );
        assert!(!service_content_uses_owned_paths(&legacy));
    }

    #[test]
    fn debug_never_prints_the_password() {
        let text = format!("{:?}", options());
        assert!(text.contains("<redacted>"));
        assert!(!text.contains("secret"));
    }

    #[test]
    fn rejects_impossible_interface_names() {
        for bad in [".", "..", &"a".repeat(16)] {
            assert!(validate_nic(bad).is_err(), "应拒绝网卡名 {bad:?}");
        }
        assert!(validate_nic("eno1").is_ok());
        assert!(validate_nic("enp4s0.20").is_ok());
    }

    #[test]
    fn rejects_exec_directive_hidden_behind_leading_whitespace() {
        let content = service_file(&options());
        let injected = content.replace("ExecStop=", " ExecStop=/tmp/evil\nExecStop=");
        assert!(
            !service_content_uses_owned_paths(&injected),
            "前导空格的 ExecStop 不能被当成安全单元"
        );
        assert!(!service_content_safe_to_stop(&injected));
    }

    #[test]
    fn old_template_without_condition_can_still_be_stopped() {
        let content = service_file(&options());
        let legacy = content
            .lines()
            .filter(|line| {
                !line.starts_with("ExecStartPost=") && !line.starts_with("ExecCondition=")
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !service_content_uses_owned_paths(&legacy),
            "缺少 ExecStartPost 的旧模板不应被当成当前模板"
        );
        assert!(
            service_content_safe_to_stop(&legacy),
            "旧模板仍必须可以安全停止/禁用"
        );
    }

    #[test]
    fn rejects_service_with_user_controlled_paths_or_environment() {
        let content = service_file(&options());
        let legacy = content.replace(CLIENT_WRAPPER_PATH, "/home/student/.local/bin/rjsupplicant");
        assert!(!service_content_uses_owned_paths(&legacy));

        let injected = content.replace(
            "Type=simple",
            "Type=simple\nEnvironment=LD_PRELOAD=/home/student/lib.so",
        );
        assert!(!service_content_uses_owned_paths(&injected));
    }
}
