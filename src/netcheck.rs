//! 有线路径可达性探测（GUI 与 root helper 共用）。
//!
//! 2026-10-01 实机事故背景：官方客户端在认证已成功、网关会话仍保持时也会
//! SIGSEGV（崩溃点在 `get_nics_info`，约在启动后 3 秒的控制线程节拍上，与接口表
//! 变动竞争；12 次崩溃中 9 次 eno1 持有有效地址且 NM 正在运行）。客户端不在
//! 运行时，进程状态无法说明会话是否还在；实测该场景下 eno1 仍可直连外网。
//! 本模块回答"这条有线路径现在能不能用"，供两处消费：
//!   * GUI 展示真实连接状态（避免把"客户端没在运行"误报成"未连接"）；
//!   * systemd `ExecCondition` 决定是否需要启动客户端（已联网则跳过）。
//!
//! 探测方式的硬约束（本机实测）：
//!   * 目标必须是字面 IPv4：本机运行 FlClash（TUN + fake-IP DNS）时任何域名都
//!     会解析到 198.18.0.0/15 并被代理接管，测不出有线路径本身；
//!   * 必须绑定到指定网卡（SO_BINDTODEVICE）：否则流量走 TUN/代理，同样测不出。
//!
//! 环境不可信时的加固（探测结论必须与"直连、环境干净"时一致）：
//!   * 子进程环境里清除大小写两套代理变量（GUI 会话被代理客户端写入是常态），
//!     并把 `NO_PROXY` 钉成 `*`，任何残留代理逻辑都不得参与探测；
//!   * curl 首参固定 `--disable`：`~/.curlrc` 里的 `proxy = ...` 能改写目标，
//!     只有放在第一个参数位置才会被 curl 忽略；
//!   * 工具只从固定绝对目录查找，不信任 PATH（桌面会话的 PATH 可被包装脚本改写）；
//!   * curl/ping 失败后始终用 libc 直接做绑定网卡的 TCP 兜底，不因缺少某一个
//!     工具而放弃；兜底同样只读，不改任何系统状态。
//!
//! 判定语义：只有"明确失败"才可能得到 [`ProbeOutcome::Unreachable`]。carrier
//! 读不到（[`CarrierState::Unknown`]）仍继续探测；超时、权限不足、工具缺失都
//! 只能得到 [`ProbeOutcome::Unknown`]，避免把一次测不到误报成"未连接"。

use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// 阿里公共 DNS 223.5.5.5：国内稳定可达，既有 HTTP 服务也响应 ICMP，且是字面 IP。
const PROBE_IPV4: &str = "223.5.5.5";
/// [`PROBE_IPV4`] 的八位组形式，供 libc 兜底路径直接构造 `sockaddr_in`。
const PROBE_IPV4_OCTETS: [u8; 4] = [223, 5, 5, 5];
/// 第二个 TCP 目标：1.1.1.1:443，用于排除单一目标/单一端口被校园网策略拦截。
const SECONDARY_IPV4_OCTETS: [u8; 4] = [1, 1, 1, 1];
const PROBE_URL: &str = "http://223.5.5.5/";
/// HTTP 探测与 libc TCP/80 兜底使用同一端口。
const PROBE_PORT: u16 = 80;

#[derive(Clone, Copy)]
struct TcpTarget {
    octets: [u8; 4],
    port: u16,
}

/// 多目标 TCP 探测：任一成功即可达。223.5.5.5:80 与 HTTP 探测同目标，
/// 但走独立的内核实现，用于在 curl 缺失/失败时验证同一条链路。
const TCP_TARGETS: [TcpTarget; 3] = [
    TcpTarget {
        octets: PROBE_IPV4_OCTETS,
        port: PROBE_PORT,
    },
    TcpTarget {
        octets: PROBE_IPV4_OCTETS,
        port: 443,
    },
    TcpTarget {
        octets: SECONDARY_IPV4_OCTETS,
        port: 443,
    },
];

/// 探测工具只允许从这些绝对目录里找；顺序固定，永不走 PATH。
const TOOL_DIRS: [&str; 3] = ["/usr/bin", "/bin", "/usr/local/bin"];
const CURL_NAME: &str = "curl";
const PING_NAME: &str = "ping";
/// 必须从探测子进程环境中清除的代理变量（大小写两套：curl 两种写法都认）。
const PROXY_ENV_VARS: [&str; 6] = [
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
];
const CARRIER_DIR: &str = "/sys/class/net";

/// 一次状态刷新最多花在探测上的时间。单项预算都受这个总 deadline 约束，
/// 即使 curl/ping/libc 全部超时也不会超过约 4 秒。
const PROBE_TOTAL_TIMEOUT: Duration = Duration::from_secs(4);
const HTTP_PROBE_TIMEOUT: Duration = Duration::from_millis(1500);
const ICMP_PROBE_TIMEOUT: Duration = Duration::from_millis(1000);
const LIBC_TARGET_TIMEOUT: Duration = Duration::from_millis(1000);

/// 网卡物理链路状态。`Missing` 表示网卡不存在；`Unknown` 表示网卡存在但
/// carrier 因权限/驱动原因读不到——后者不能当作"没有网线"。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CarrierState {
    Up,
    Down,
    Missing,
    Unknown,
}

/// 有线路径探测结论。只有多个独立方法都明确失败时才给出 `Unreachable`。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbeOutcome {
    Reachable,
    Unreachable,
    Unknown,
}

/// 读取网卡载波状态；名字非法或网卡不存在为 `Missing`，读不到为 `Unknown`。
pub fn carrier_state(nic: &str) -> CarrierState {
    let nic = nic.trim();
    if !valid_nic_name(nic) {
        return CarrierState::Missing;
    }
    let path = Path::new(CARRIER_DIR).join(nic);
    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return CarrierState::Missing,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return CarrierState::Missing,
        Err(_) => return CarrierState::Unknown,
    }
    match std::fs::read_to_string(path.join("carrier")) {
        Ok(value) => match value.trim() {
            "1" => CarrierState::Up,
            "0" => CarrierState::Down,
            _ => CarrierState::Unknown,
        },
        // 网卡目录存在但 carrier 文件缺失/无权限：无法判断，不是明确断开。
        Err(_) => CarrierState::Unknown,
    }
}

/// 兼容旧调用方：`Some(true)` 载波已连接，`Some(false)` 明确断开，
/// `None` 表示网卡不存在或无法判断（旧语义下调用方按"未知"处理）。
pub fn carrier_up(nic: &str) -> Option<bool> {
    match carrier_state(nic) {
        CarrierState::Up => Some(true),
        CarrierState::Down => Some(false),
        CarrierState::Missing | CarrierState::Unknown => None,
    }
}

/// 判断 `nic` 的有线路径当前是否可用。
///
/// `Reachable` 可用；`Unreachable` 是多个独立方法都给出的明确失败；
/// `Unknown` 表示测不到（超时/权限/工具缺失），调用方不得据此谎报未连接。
pub fn probe_wired_path(nic: &str) -> ProbeOutcome {
    let nic = nic.trim();
    let carrier = carrier_state(nic);
    if carrier_short_circuits(carrier) {
        return ProbeOutcome::Unreachable;
    }

    let deadline = Instant::now() + PROBE_TOTAL_TIMEOUT;
    let mut explicit_methods = BTreeSet::new();

    // HTTP（保留原路径）：绑定网卡、字面 IP，按状态码判断是否被门户劫持。
    if let Some(curl) = find_tool(CURL_NAME) {
        let budget = remaining(deadline).min(HTTP_PROBE_TIMEOUT);
        if !budget.is_zero() {
            match curl_http_probe(&curl, nic, budget) {
                AttemptResult::Success => return ProbeOutcome::Reachable,
                AttemptResult::ExplicitFailure => {
                    explicit_methods.insert("http");
                }
                AttemptResult::Indeterminate | AttemptResult::Unavailable => {}
            }
        }
    }

    // ICMP：少数校园网放行 ICMP 但拦截 80/443。ping 的退出码 1 是"没收到
    // 回复"（超时），不能当作明确失败。
    if let Some(ping) = find_tool(PING_NAME) {
        let budget = remaining(deadline).min(ICMP_PROBE_TIMEOUT);
        if !budget.is_zero() {
            match icmp_probe_attempt(&ping, nic, budget) {
                AttemptResult::Success => return ProbeOutcome::Reachable,
                AttemptResult::ExplicitFailure => {
                    explicit_methods.insert("icmp");
                }
                AttemptResult::Indeterminate | AttemptResult::Unavailable => {}
            }
        }
    }

    // libc TCP 兜底总是尝试（不再等 curl/ping 都缺失），三个目标共享剩余预算。
    let mut tcp_results = Vec::new();
    for (index, target) in TCP_TARGETS.iter().enumerate() {
        let left = (TCP_TARGETS.len() - index) as u32;
        let budget = (remaining(deadline) / left).min(LIBC_TARGET_TIMEOUT);
        if budget.is_zero() {
            break;
        }
        let result = bind_device_tcp_probe_target(nic, target.octets, target.port, budget);
        if result == AttemptResult::Success {
            return ProbeOutcome::Reachable;
        }
        tcp_results.push(result);
    }
    let tcp_result = combine_tcp_results(&tcp_results);
    if tcp_result == AttemptResult::ExplicitFailure {
        // 三个目标同属一个 TCP 探测方法；在集合里只记一次。
        explicit_methods.insert("tcp");
    }

    // 至少两个独立方法明确失败才算不可达；否则宁可报告未知。
    if explicit_methods.len() >= 2 {
        ProbeOutcome::Unreachable
    } else {
        ProbeOutcome::Unknown
    }
}

/// 只有明确的 Down/Missing 才能不经探测直接判不可达；Unknown 必须继续探测。
fn carrier_short_circuits(state: CarrierState) -> bool {
    matches!(state, CarrierState::Down | CarrierState::Missing)
}

/// 兼容 helper 的旧入口：Reachable=>Some(true), Unreachable=>Some(false),
/// Unknown=>None。
pub fn wired_path_reachable(nic: &str) -> Option<bool> {
    outcome_to_option(probe_wired_path(nic))
}

fn outcome_to_option(outcome: ProbeOutcome) -> Option<bool> {
    match outcome {
        ProbeOutcome::Reachable => Some(true),
        ProbeOutcome::Unreachable => Some(false),
        ProbeOutcome::Unknown => None,
    }
}

/// 网卡名必须是单个合法路径分量，同时满足内核 `IFNAMSIZ` 限制。
///
/// 名字来自配置/命令行的可控输入，带 `/` 或 `..` 时 `CARRIER_DIR` 的拼接会
/// 逃出 `/sys`；带 NUL 或超长则会让 `SO_BINDTODEVICE` 静默失败。
fn valid_nic_name(nic: &str) -> bool {
    !nic.is_empty()
        && nic.len() < libc::IFNAMSIZ
        && !nic.contains('/')
        && !nic.contains('\0')
        && nic != "."
        && nic != ".."
}

/// 在固定的绝对目录里查找可执行文件；找不到或不可执行时返回 `None`。
fn find_tool(name: &str) -> Option<PathBuf> {
    TOOL_DIRS
        .iter()
        .map(|dir| Path::new(dir).join(name))
        .find(|path| is_executable(path))
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn remaining(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

/// 探测子进程的公共外壳：清代理变量、钉子进程环境、丢掉全部输出。
///
/// stdin 设为 null 以免子进程停在交互提示上耗光超时预算；需要读取 curl
/// 状态码时调用方会把 stdout 改成 piped。
fn sanitized_probe_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // NO_PROXY=`*` 表示"任何目标都不走代理"，与 `--noproxy '*'` 同向；
        // 大小写两套都设，避免调用方或 libcurl 只认其中一种写法。
        .env("NO_PROXY", "*")
        .env("no_proxy", "*");
    for name in PROXY_ENV_VARS {
        command.env_remove(name);
    }
    command
}

/// curl HTTP 探测命令：绑定网卡、绕过代理、字面 IP，禁用 `~/.curlrc`，
/// 用 `-w '%{http_code}'` 读取响应状态码（不能只看退出码：captive portal
/// 的 3xx/407/511 也会让 curl 成功退出）。
fn curl_http_probe_command(curl: &Path, nic: &str, budget: Duration) -> Command {
    let max_secs = budget.as_secs_f64().clamp(0.2, 4.0);
    let connect_secs = max_secs.min(2.0);
    let max_time = format!("{max_secs:.1}");
    let connect_timeout = format!("{connect_secs:.1}");

    let mut command = sanitized_probe_command(curl);
    command
        // 必须是首参：只有第一个参数位置的 --disable 才会让 curl 完全不读
        // ~/.curlrc / $CURL_HOME/curlrc，否则配置里的 `proxy = ...` 会生效。
        .arg("--disable")
        // 字面 IPv4 已排除域名，-4 只是防止网络栈做无谓的家族选择。
        .arg("-4")
        .arg("--noproxy")
        // 命令行优先于任何代理环境变量；配合上面的 env_remove 双保险。
        .arg("*")
        .arg("--interface")
        // 非 IP 字符串会让 curl 用 SO_BINDTODEVICE 绑定设备，流量不经过 TUN。
        .arg(nic)
        .args(["-sS", "-o", "/dev/null"])
        .arg("--connect-timeout")
        .arg(connect_timeout)
        .arg("--max-time")
        .arg(max_time)
        .arg("-w")
        .arg("%{http_code}")
        .arg(PROBE_URL)
        .stdout(Stdio::piped());
    command
}

fn curl_http_probe(curl: &Path, nic: &str, budget: Duration) -> AttemptResult {
    let command = curl_http_probe_command(curl, nic, budget);
    let outcome = run_command_with_timeout(command, budget);
    match outcome {
        CommandOutcome::Exited(output) => {
            if output.status.success() {
                match parse_http_code(&output.stdout) {
                    Some(code) => classify_http_code(code),
                    None => AttemptResult::Indeterminate,
                }
            } else {
                classify_curl_failure(output.status.code())
            }
        }
        CommandOutcome::TimedOut => AttemptResult::Indeterminate,
        CommandOutcome::Failed => AttemptResult::Unavailable,
    }
}

fn parse_http_code(stdout: &[u8]) -> Option<u16> {
    String::from_utf8_lossy(stdout)
        .split_whitespace()
        .last()?
        .parse()
        .ok()
}

/// 2xx/404 才是正常可达；3xx/407/511 是 captive portal/代理认证页。
fn classify_http_code(code: u16) -> AttemptResult {
    match code {
        200..=299 | 404 => AttemptResult::Success,
        0 => AttemptResult::Indeterminate,
        _ => AttemptResult::ExplicitFailure,
    }
}

/// curl 自身失败退出码：超时是未知，工具/参数/写输出等本地问题也是未知；
/// 其余连接错误（拒绝、不可达、TLS 等）视为明确失败。
fn classify_curl_failure(code: Option<i32>) -> AttemptResult {
    match code {
        Some(28) => AttemptResult::Indeterminate,
        Some(1..=5 | 23) | None => AttemptResult::Unavailable,
        Some(_) => AttemptResult::ExplicitFailure,
    }
}

/// ping 探测命令：数字输出、单包、2 秒等待、绑定网卡；只看退出码。
fn icmp_probe_command(ping: &Path, nic: &str) -> Command {
    let mut command = sanitized_probe_command(ping);
    command.args([
        "-n", // 数字输出：不做反向解析，避免把 DNS 污染引入判定。
        "-c", "1", "-W", "2", "-I", nic, PROBE_IPV4,
    ]);
    command
}

fn icmp_probe_attempt(ping: &Path, nic: &str, budget: Duration) -> AttemptResult {
    let command = icmp_probe_command(ping, nic);
    match run_command_with_timeout(command, budget) {
        CommandOutcome::Exited(output) if output.status.success() => AttemptResult::Success,
        // ping 的退出码 1 通常就是"超时未收到回复"，不是明确不可达。
        CommandOutcome::Exited(_) | CommandOutcome::TimedOut => AttemptResult::Indeterminate,
        CommandOutcome::Failed => AttemptResult::Unavailable,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttemptResult {
    Success,
    ExplicitFailure,
    Indeterminate,
    Unavailable,
}

enum CommandOutcome {
    Exited(std::process::Output),
    TimedOut,
    Failed,
}

/// 有界运行探测子进程：到期立即 kill，绝不让单次状态刷新被某个工具拖住。
fn run_command_with_timeout(mut command: Command, timeout: Duration) -> CommandOutcome {
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return CommandOutcome::Failed,
    };
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return match child.wait_with_output() {
                    Ok(output) => CommandOutcome::Exited(output),
                    Err(_) => CommandOutcome::Failed,
                };
            }
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return CommandOutcome::Failed;
            }
        }
        let now = Instant::now();
        if now >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return CommandOutcome::TimedOut;
        }
        std::thread::sleep((deadline - now).min(Duration::from_millis(20)));
    }
}

fn combine_tcp_results(results: &[AttemptResult]) -> AttemptResult {
    if results.contains(&AttemptResult::Success) {
        return AttemptResult::Success;
    }
    if !results.is_empty()
        && results
            .iter()
            .all(|result| *result == AttemptResult::ExplicitFailure)
    {
        AttemptResult::ExplicitFailure
    } else if results.contains(&AttemptResult::Unavailable) || results.is_empty() {
        AttemptResult::Unavailable
    } else {
        AttemptResult::Indeterminate
    }
}

/// 关闭裸 fd 的最小 RAII；探测路径上任何提前返回都不能泄漏描述符。
struct CloseOnDrop(libc::c_int);

impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        // SAFETY: fd 由本模块的 socket() 创建，且只在这里关闭一次。
        unsafe { libc::close(self.0) };
    }
}

/// 兼容旧调用方的单目标 libc TCP 探测（固定 223.5.5.5:80）。
#[cfg(test)]
fn bind_device_tcp_probe(nic: &str, timeout: Duration) -> Option<bool> {
    match bind_device_tcp_probe_target(nic, PROBE_IPV4_OCTETS, PROBE_PORT, timeout) {
        AttemptResult::Success => Some(true),
        AttemptResult::ExplicitFailure => Some(false),
        AttemptResult::Indeterminate | AttemptResult::Unavailable => None,
    }
}

/// 单目标内核兜底：`SO_BINDTODEVICE` + 非阻塞 connect + poll。
///
/// 不依赖 curl/ping，也不解析任何输出；除了一个 TCP 连接尝试外不改系统状态。
fn bind_device_tcp_probe_target(
    nic: &str,
    octets: [u8; 4],
    port: u16,
    timeout: Duration,
) -> AttemptResult {
    if !valid_nic_name(nic) {
        return AttemptResult::Unavailable;
    }
    // SAFETY: 所有 libc 调用使用的都是本地初始化的、类型正确的缓冲区与结构体；
    // socket 成功后由 CloseOnDrop 在函数任意返回路径上关闭，不泄漏描述符。
    unsafe {
        let fd = libc::socket(
            libc::AF_INET,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        );
        if fd < 0 {
            return AttemptResult::Unavailable;
        }
        let _fd = CloseOnDrop(fd);

        // 设备名要 NUL 结尾：SO_BINDTODEVICE 会按 optlen 复制并按字符串解释。
        let mut device = [0_u8; libc::IFNAMSIZ];
        device[..nic.len()].copy_from_slice(nic.as_bytes());
        // 部分内核仍要求 CAP_NET_RAW（返回 EPERM）：此时只能返回"无法判断"，
        // 而不是谎报"路径不可用"。
        if libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_BINDTODEVICE,
            device.as_ptr().cast(),
            device.len() as libc::socklen_t,
        ) != 0
        {
            return classify_socket_errno(std::io::Error::last_os_error().raw_os_error());
        }

        let address = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            // 端口与地址都必须是网络字节序：to_be / from_ne_bytes 让内存布局
            // 与大端表示一致（from_ne_bytes 保证字节按原样落盘，不做数值交换）。
            sin_port: port.to_be(),
            sin_addr: libc::in_addr {
                s_addr: u32::from_ne_bytes(octets),
            },
            sin_zero: [0; 8],
        };
        let connected = libc::connect(
            fd,
            std::ptr::addr_of!(address).cast(),
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        );
        if connected == 0 {
            return AttemptResult::Success;
        }
        // 非阻塞套接字上"正在连接"是可继续等待的状态，其余错误码按语义分类。
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINPROGRESS) {
            return classify_socket_errno(std::io::Error::last_os_error().raw_os_error());
        }

        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return AttemptResult::Indeterminate;
            }
            let mut pollfd = libc::pollfd {
                fd,
                events: libc::POLLOUT,
                revents: 0,
            };
            let millis = remaining.as_millis().clamp(1, i32::MAX as u128);
            let ready = libc::poll(&mut pollfd, 1, millis as libc::c_int);
            if ready < 0 {
                // 信号打断是正常情况，重新计算剩余预算后继续等。
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                return AttemptResult::Unavailable;
            }
            if ready == 0 {
                return AttemptResult::Indeterminate;
            }
            break;
        }

        // 可写只说明有结果，必须用 SO_ERROR 区分"连上"和"被拒/超时"。
        let mut so_error: libc::c_int = 0;
        let mut length = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        if libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_ERROR,
            std::ptr::addr_of_mut!(so_error).cast(),
            &mut length,
        ) != 0
        {
            return AttemptResult::Unavailable;
        }
        if so_error == 0 {
            AttemptResult::Success
        } else {
            classify_socket_errno(Some(so_error))
        }
    }
}

/// errno 语义：超时与权限问题都不能算"明确不可达"。
fn classify_socket_errno(errno: Option<i32>) -> AttemptResult {
    match errno {
        Some(0) => AttemptResult::Success,
        Some(libc::ETIMEDOUT | libc::EINPROGRESS | libc::EALREADY | libc::EAGAIN) => {
            AttemptResult::Indeterminate
        }
        Some(libc::EPERM | libc::EACCES) => AttemptResult::Unavailable,
        _ => AttemptResult::ExplicitFailure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::OsString;

    /// 进程环境是全局可变状态，所有会写它的测试都必须串行化。
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 把六种代理变量污染成"指向死端口"，退出作用域时原样恢复。
    struct PendingProxyEnv {
        saved: Vec<(&'static str, Option<OsString>)>,
    }

    impl PendingProxyEnv {
        fn poison() -> Self {
            let saved = PROXY_ENV_VARS
                .iter()
                .map(|name| (*name, std::env::var_os(name)))
                .collect();
            for name in PROXY_ENV_VARS {
                // SAFETY: 写进程环境的测试都由 ENV_LOCK 串行化，且这里的值与
                // 其它线程读取的目的无关；Rust 2024 起 set_var 因 getenv/setenv
                // 竞争而标记为 unsafe。
                unsafe { std::env::set_var(name, "http://127.0.0.1:1") };
            }
            Self { saved }
        }
    }

    impl Drop for PendingProxyEnv {
        fn drop(&mut self) {
            for (name, value) in self.saved.drain(..) {
                // SAFETY: 同上，仍在 ENV_LOCK 保护范围内。
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(name, value),
                        None => std::env::remove_var(name),
                    }
                }
            }
        }
    }

    fn args_of(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    fn envs_of(command: &Command) -> HashMap<String, Option<String>> {
        command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect()
    }

    #[test]
    fn unknown_interfaces_are_not_probed() {
        // 空网卡名与不存在的网卡都必须直接判为不可用，不能去访问 /sys 之外的路径。
        assert_eq!(wired_path_reachable(""), Some(false));
        assert_eq!(wired_path_reachable("rj-nonexistent0"), Some(false));
        assert_eq!(carrier_up(""), None);
        assert_eq!(carrier_up("rj-nonexistent0"), None);
        assert_eq!(carrier_state(""), CarrierState::Missing);
        assert_eq!(carrier_state("rj-nonexistent0"), CarrierState::Missing);
    }

    #[test]
    fn nic_names_cannot_escape_sysfs() {
        // `..`、含 `/` 或超长/带 NUL 的名字都会被 valid_nic_name 拒绝，
        // `/sys/class/net/../..` 这类拼接不会再被当成网卡目录。
        for name in [
            "..",
            ".",
            "../etc",
            "eno1/../..",
            "eno\0 1",
            "aaaaaaaaaaaaaaaa",
        ] {
            assert!(!valid_nic_name(name), "{name:?} 不应通过校验");
            assert_eq!(carrier_up(name), None, "{name:?} 不应被当作网卡");
            assert_eq!(carrier_state(name), CarrierState::Missing);
        }
        assert!(valid_nic_name("eno1"));
    }

    #[test]
    fn unknown_carrier_does_not_short_circuit_to_unreachable() {
        assert!(carrier_short_circuits(CarrierState::Down));
        assert!(carrier_short_circuits(CarrierState::Missing));
        assert!(!carrier_short_circuits(CarrierState::Up));
        assert!(
            !carrier_short_circuits(CarrierState::Unknown),
            "carrier 读不到时必须继续探测，不能直接判不可达"
        );
    }

    #[test]
    fn probe_outcome_maps_to_option() {
        // helper 旧入口的映射必须稳定；这里不触发任何真实网络请求。
        assert_eq!(outcome_to_option(ProbeOutcome::Reachable), Some(true));
        assert_eq!(outcome_to_option(ProbeOutcome::Unreachable), Some(false));
        assert_eq!(outcome_to_option(ProbeOutcome::Unknown), None);
    }

    #[test]
    fn probe_targets_include_required_literal_ipv4_endpoints() {
        // 目标一旦含域名，本机 TUN 的 fake-IP DNS 会把它劫持到 198.18.0.0/15。
        assert!(PROBE_URL.contains(PROBE_IPV4));
        assert_eq!(
            PROBE_IPV4.parse::<std::net::Ipv4Addr>().unwrap().octets(),
            PROBE_IPV4_OCTETS
        );
        assert_eq!(
            "1.1.1.1".parse::<std::net::Ipv4Addr>().unwrap().octets(),
            SECONDARY_IPV4_OCTETS
        );
        assert!(
            TCP_TARGETS
                .iter()
                .any(|target| { target.octets == PROBE_IPV4_OCTETS && target.port == PROBE_PORT })
        );
        assert!(
            TCP_TARGETS
                .iter()
                .any(|target| { target.octets == PROBE_IPV4_OCTETS && target.port == 443 })
        );
        assert!(
            TCP_TARGETS
                .iter()
                .any(|target| { target.octets == SECONDARY_IPV4_OCTETS && target.port == 443 })
        );
    }

    #[test]
    fn http_status_codes_are_classified() {
        assert_eq!(classify_http_code(200), AttemptResult::Success);
        assert_eq!(classify_http_code(204), AttemptResult::Success);
        assert_eq!(classify_http_code(404), AttemptResult::Success);
        for code in [301, 302, 307, 407, 511, 500] {
            assert_eq!(
                classify_http_code(code),
                AttemptResult::ExplicitFailure,
                "HTTP {code} 不应被当作可达"
            );
        }
        assert_eq!(classify_http_code(0), AttemptResult::Indeterminate);
        assert_eq!(parse_http_code(b"200\n"), Some(200));
        assert_eq!(parse_http_code(b"\n404"), Some(404));
        assert_eq!(parse_http_code(b""), None);
    }

    #[test]
    fn tcp_failure_aggregation_requires_all_targets_to_fail_explicitly() {
        assert_eq!(
            combine_tcp_results(&[
                AttemptResult::ExplicitFailure,
                AttemptResult::ExplicitFailure,
                AttemptResult::ExplicitFailure,
            ]),
            AttemptResult::ExplicitFailure
        );
        assert_eq!(
            combine_tcp_results(&[AttemptResult::ExplicitFailure, AttemptResult::Indeterminate]),
            AttemptResult::Indeterminate
        );
        assert_eq!(
            combine_tcp_results(&[AttemptResult::ExplicitFailure, AttemptResult::Unavailable]),
            AttemptResult::Unavailable
        );
        assert_eq!(
            combine_tcp_results(&[AttemptResult::Success, AttemptResult::ExplicitFailure]),
            AttemptResult::Success
        );
        assert_eq!(combine_tcp_results(&[]), AttemptResult::Unavailable);
    }

    #[test]
    fn tools_are_found_by_absolute_candidates_only() {
        assert_eq!(find_tool("rj-definitely-not-a-real-tool"), None);
        for dir in TOOL_DIRS {
            assert!(dir.starts_with('/'), "候选目录必须是绝对路径：{dir}");
        }
        if Path::new("/usr/bin/curl").is_file() {
            assert_eq!(
                find_tool(CURL_NAME).as_deref(),
                Some(Path::new("/usr/bin/curl"))
            );
        }
    }

    #[test]
    fn curl_probe_ignores_proxy_environment_and_curlrc() {
        let _lock = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _poisoned_env = PendingProxyEnv::poison();
        // 污染确实写进了进程环境：这样下面的断言才说明子进程"不继承"它。
        assert_eq!(
            std::env::var("HTTP_PROXY").as_deref(),
            Ok("http://127.0.0.1:1")
        );

        let command = curl_http_probe_command(
            Path::new("/usr/bin/curl"),
            "eno1",
            Duration::from_millis(1500),
        );
        assert_eq!(command.get_program(), Path::new("/usr/bin/curl"));
        let args = args_of(&command);
        // 首参 --disable：放在其它位置时 curl 仍会读 ~/.curlrc 里的 proxy 配置。
        assert_eq!(args.first().map(String::as_str), Some("--disable"));
        assert!(args.windows(2).any(|pair| pair == ["--noproxy", "*"]));
        assert!(args.windows(2).any(|pair| pair == ["--interface", "eno1"]));
        assert!(args.contains(&"-4".to_string()));
        assert!(args.contains(&PROBE_URL.to_string()));
        // 必须读取状态码，不能只看 curl 的退出码。
        assert!(args.windows(2).any(|pair| pair == ["-w", "%{http_code}"]));

        let envs = envs_of(&command);
        for name in PROXY_ENV_VARS {
            assert_eq!(envs.get(name), Some(&None), "{name} 必须被显式清除");
        }
        assert_eq!(envs.get("NO_PROXY"), Some(&Some("*".to_string())));
    }

    #[test]
    fn ping_probe_ignores_proxy_environment_too() {
        let _lock = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _poisoned_env = PendingProxyEnv::poison();

        let command = icmp_probe_command(Path::new("/usr/bin/ping"), "eno1");
        assert_eq!(
            args_of(&command),
            ["-n", "-c", "1", "-W", "2", "-I", "eno1", PROBE_IPV4]
        );

        let envs = envs_of(&command);
        for name in PROXY_ENV_VARS {
            assert_eq!(envs.get(name), Some(&None), "{name} 必须被显式清除");
        }
        assert_eq!(envs.get("NO_PROXY"), Some(&Some("*".to_string())));
    }

    #[test]
    fn bounded_command_runner_captures_output_and_kills_on_timeout() {
        let mut echo = Command::new("/bin/sh");
        echo.args(["-c", "printf 200"]).stdout(Stdio::piped());
        match run_command_with_timeout(echo, Duration::from_secs(1)) {
            CommandOutcome::Exited(output) => {
                assert!(output.status.success());
                assert_eq!(output.stdout, b"200");
            }
            _ => panic!("短命令应正常结束"),
        }

        let mut slow = Command::new("/bin/sleep");
        slow.arg("5");
        let started = Instant::now();
        let outcome = run_command_with_timeout(slow, Duration::from_millis(50));
        assert!(matches!(outcome, CommandOutcome::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn libc_fallback_rejects_bogus_interface_names() {
        // 兜底路径同样要拒绝逃逸名与超长名，不会把参数透传给内核。
        assert_eq!(bind_device_tcp_probe("", Duration::from_secs(1)), None);
        assert_eq!(
            bind_device_tcp_probe("../etc", Duration::from_secs(1)),
            None
        );
        assert_eq!(
            bind_device_tcp_probe("aaaaaaaaaaaaaaaa", Duration::from_secs(1)),
            None
        );
    }

    /// 真实探测冒烟测试：需要有线会话可用，默认忽略。
    ///
    /// 运行方式：`cargo test --lib -- --ignored --nocapture probe`
    #[test]
    #[ignore = "需要真实有线网络环境（发往 223.5.5.5 的 TCP/ICMP 探测）"]
    fn probe_real_wired_path_smoke() {
        let nic = std::env::var("RJ_NETCHECK_NIC").unwrap_or_else(|_| "eno1".to_string());
        let carrier = carrier_up(&nic);
        println!("probe smoke: nic={nic} carrier_up={carrier:?}");
        assert_eq!(carrier, Some(true), "{nic} 没有载波，无法做真实探测冒烟");

        let curl = find_tool(CURL_NAME);
        let ping = find_tool(PING_NAME);
        println!("probe smoke: curl={curl:?} ping={ping:?}");
        if let Some(curl) = &curl {
            println!(
                "probe smoke: curl_http_probe={:?}",
                curl_http_probe(curl, &nic, Duration::from_secs(4))
            );
        }
        if let Some(ping) = &ping {
            println!(
                "probe smoke: icmp_probe={:?}",
                icmp_probe_attempt(ping, &nic, Duration::from_secs(2))
            );
        }
        // 内核兜底与 HTTP 是同一目标（TCP/80、绑定网卡）的两套实现；
        // 内核不允许绑定设备时返回"无法判断"，不影响 curl/ping 路径的结论。
        let fallback = bind_device_tcp_probe(&nic, Duration::from_secs(2));
        println!("probe smoke: libc_tcp_probe={fallback:?}");
        let reachable = wired_path_reachable(&nic);
        println!("probe smoke: wired_path_reachable({nic}) = {reachable:?}");
        assert_eq!(
            reachable,
            Some(true),
            "有线路径探测应报告可用（直连/代理污染/恶意 curlrc 下都必须一致）"
        );
    }
}
