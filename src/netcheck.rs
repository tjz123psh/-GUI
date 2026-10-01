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
//!   * curl、ping 都缺失时用 libc 直接做绑定网卡的 TCP 兜底，不再直接返回
//!     `None`；兜底同样只读，不改任何系统状态。

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// 阿里公共 DNS 223.5.5.5：国内稳定可达，既有 HTTP 服务也响应 ICMP，且是字面 IP。
const PROBE_IPV4: &str = "223.5.5.5";
/// [`PROBE_IPV4`] 的八位组形式，供 libc 兜底路径直接构造 `sockaddr_in`。
const PROBE_IPV4_OCTETS: [u8; 4] = [223, 5, 5, 5];
const PROBE_URL: &str = "http://223.5.5.5/";
/// TCP 兜底只连 80 端口，语义与 HTTP 探测一致：能完成三次握手就算路径可用。
const PROBE_PORT: u16 = 80;
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
/// curl/ping 都缺失时的 libc 兜底预算，与 curl 的 `--connect-timeout 2` 对齐。
const LIBC_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// 网卡物理链路是否已连接；网卡不存在或 carrier 读不到时返回 `None`。
pub fn carrier_up(nic: &str) -> Option<bool> {
    let nic = nic.trim();
    if !valid_nic_name(nic) {
        return None;
    }
    let path = Path::new(CARRIER_DIR).join(nic);
    if !path.is_dir() {
        return None;
    }
    std::fs::read_to_string(path.join("carrier"))
        .ok()
        .map(|value| value.trim() == "1")
}

/// 判断 `nic` 的有线路径当前是否可用。
///
/// `Some(true)` 可用；`Some(false)` 不可用（含网卡不存在、无网线、探测失败）；
/// `None` 表示探测工具缺失且内核兜底也无权限执行、无法判断——调用方按
/// "需要认证 / 未连接"处理。
pub fn wired_path_reachable(nic: &str) -> Option<bool> {
    let nic = nic.trim();
    if carrier_up(nic) != Some(true) {
        // 网卡不存在或无网线：有线路径必然不可用，连探测都不必发。
        return Some(false);
    }
    let curl = find_tool(CURL_NAME);
    let ping = find_tool(PING_NAME);
    if curl.is_none() && ping.is_none() {
        // 两个探测工具都缺失时不再直接放弃：内核态 TCP 兜底不依赖任何外部程序。
        return bind_device_tcp_probe(nic, LIBC_PROBE_TIMEOUT);
    }
    if curl.is_some_and(|path| curl_probe(&path, nic)) {
        return Some(true);
    }
    if ping.is_some_and(|path| icmp_probe(&path, nic)) {
        return Some(true);
    }
    Some(false)
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

/// 探测子进程的公共外壳：清代理变量、钉子进程环境、丢掉全部输出。
///
/// 只看退出码判定结果，因此必须屏蔽 stdout/stderr（curl 的进度表和 ping 的
/// 本地化输出在不同 locale 下不可解析）。stdin 设为 null 以免子进程停在
/// 交互提示上耗光超时预算。
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

/// curl 探测命令：绑定网卡、绕过代理、字面 IP，禁用 `~/.curlrc`。
fn curl_probe_command(curl: &Path, nic: &str) -> Command {
    let mut command = sanitized_probe_command(curl);
    command.args([
        // 必须是首参：只有第一个参数位置的 --disable 才会让 curl 完全不读
        // ~/.curlrc / $CURL_HOME/curlrc，否则配置里的 `proxy = ...` 会生效。
        "--disable",
        // 字面 IPv4 已排除域名，-4 只是防止网络栈做无谓的家族选择。
        "-4",
        "--noproxy",
        // 命令行优先于任何代理环境变量；配合上面的 env_remove 双保险。
        "*",
        "--interface",
        // 非 IP 字符串会让 curl 用 SO_BINDTODEVICE 绑定设备，流量不经过 TUN。
        nic,
        "-sS",
        "-o",
        "/dev/null",
        "--connect-timeout",
        "2",
        "--max-time",
        "4",
        PROBE_URL,
    ]);
    command
}

/// curl 探测：任何 HTTP 响应（含 404）都算路径可用，只看退出码。
fn curl_probe(curl: &Path, nic: &str) -> bool {
    curl_probe_command(curl, nic)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// ping 探测命令：数字输出、单包、2 秒超时、绑定网卡；只看退出码。
fn icmp_probe_command(ping: &Path, nic: &str) -> Command {
    let mut command = sanitized_probe_command(ping);
    command.args([
        "-n", // 数字输出：不做反向解析，避免把 DNS 污染引入判定。
        "-c", "1", "-W", "2", "-I", nic, PROBE_IPV4,
    ]);
    command
}

/// ICMP 兜底：少数校园网放行 ICMP 但拦截 80/443。
fn icmp_probe(ping: &Path, nic: &str) -> bool {
    icmp_probe_command(ping, nic)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// 关闭裸 fd 的最小 RAII；探测路径上任何提前返回都不能泄漏描述符。
struct CloseOnDrop(libc::c_int);

impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        // SAFETY: fd 由本模块的 socket() 创建，且只在这里关闭一次。
        unsafe { libc::close(self.0) };
    }
}

/// curl、ping 都不可用时的内核兜底：`SO_BINDTODEVICE` + 非阻塞 connect + poll。
///
/// 返回 `None` 表示本进程无权绑定设备（内核要求 CAP_NET_RAW 的 EPERM）或参数
/// 不可用，保留"无法判断"语义；`Some(false)` 才是绑定了设备后的明确探测失败。
/// 不依赖 curl/ping，也不解析任何输出；除了一个 TCP 连接尝试外不改系统状态。
fn bind_device_tcp_probe(nic: &str, timeout: Duration) -> Option<bool> {
    if !valid_nic_name(nic) {
        return None;
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
            return None;
        }
        let _fd = CloseOnDrop(fd);

        // 设备名要 NUL 结尾：SO_BINDTODEVICE 会按 optlen 复制并按字符串解释。
        let mut device = [0_u8; libc::IFNAMSIZ];
        device[..nic.len()].copy_from_slice(nic.as_bytes());
        // 部分内核仍要求 CAP_NET_RAW（返回 EPERM）：此时只能返回 None（无法
        // 判断），而不是谎报"路径不可用"。本机内核 7.2.7 已允许普通用户绑定。
        if libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_BINDTODEVICE,
            device.as_ptr().cast(),
            device.len() as libc::socklen_t,
        ) != 0
        {
            return None;
        }

        let address = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            // 端口与地址都必须是网络字节序：to_be / from_ne_bytes 让内存布局
            // 与大端表示一致（from_ne_bytes 保证字节按原样落盘，不做数值交换）。
            sin_port: PROBE_PORT.to_be(),
            sin_addr: libc::in_addr {
                s_addr: u32::from_ne_bytes(PROBE_IPV4_OCTETS),
            },
            sin_zero: [0; 8],
        };
        let connected = libc::connect(
            fd,
            std::ptr::addr_of!(address).cast(),
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        );
        if connected == 0 {
            return Some(true);
        }
        // 非阻塞套接字上"正在连接"是可继续等待的状态，其余错误码直接判失败。
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINPROGRESS) {
            return Some(false);
        }

        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Some(false);
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
                return Some(false);
            }
            if ready == 0 {
                return Some(false);
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
            return Some(false);
        }
        Some(so_error == 0)
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
        }
        assert!(valid_nic_name("eno1"));
    }

    #[test]
    fn probe_target_is_a_literal_ipv4_address() {
        // 目标一旦含域名，本机 TUN 的 fake-IP DNS 会把它劫持到 198.18.0.0/15。
        assert!(PROBE_URL.contains(PROBE_IPV4));
        assert_eq!(
            PROBE_IPV4.parse::<std::net::Ipv4Addr>().unwrap().octets(),
            PROBE_IPV4_OCTETS
        );
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

        let command = curl_probe_command(Path::new("/usr/bin/curl"), "eno1");
        assert_eq!(command.get_program(), Path::new("/usr/bin/curl"));
        let args = args_of(&command);
        // 首参 --disable：放在其它位置时 curl 仍会读 ~/.curlrc 里的 proxy 配置。
        assert_eq!(args.first().map(String::as_str), Some("--disable"));
        assert!(args.windows(2).any(|pair| pair == ["--noproxy", "*"]));
        assert!(args.windows(2).any(|pair| pair == ["--interface", "eno1"]));
        assert!(args.contains(&"-4".to_string()));
        assert!(args.contains(&PROBE_URL.to_string()));

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
            println!("probe smoke: curl_probe={}", curl_probe(curl, &nic));
        }
        if let Some(ping) = &ping {
            println!("probe smoke: icmp_probe={}", icmp_probe(ping, &nic));
        }
        // 内核兜底与 curl 是同一探测（TCP/80、绑定网卡）的两套实现，结论必须一致；
        // 内核不允许绑定设备时返回 None，不影响 curl/ping 路径的结论。
        let fallback = bind_device_tcp_probe(&nic, LIBC_PROBE_TIMEOUT);
        println!("probe smoke: libc_tcp_probe={fallback:?}");
        if let (Some(curl), Some(fallback)) = (&curl, fallback) {
            assert_eq!(
                curl_probe(curl, &nic),
                fallback,
                "内核兜底与 curl 对同一 TCP/80 路径的结论必须一致"
            );
        }
        let reachable = wired_path_reachable(&nic);
        println!("probe smoke: wired_path_reachable({nic}) = {reachable:?}");
        assert_eq!(
            reachable,
            Some(true),
            "有线路径探测应报告可用（直连/代理污染/恶意 curlrc 下都必须一致）"
        );
    }
}
