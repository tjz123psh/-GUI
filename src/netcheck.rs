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
//! 探测方式的两个硬约束（本机实测）：
//!   * 目标必须是字面 IPv4：本机运行 FlClash（TUN + fake-IP DNS）时任何域名都
//!     会解析到 198.18.0.0/15 并被代理接管，测不出有线路径本身；
//!   * 必须绑定到指定网卡（SO_BINDTODEVICE）：否则流量走 TUN/代理，同样测不出。

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

/// 阿里公共 DNS：国内稳定可达，既有 HTTP 服务也响应 ICMP，且是字面 IP。
const PROBE_IPV4: &str = "223.5.5.5";
const PROBE_URL: &str = "http://223.5.5.5/";
const CURL_PATH: &str = "/usr/bin/curl";
const PING_PATH: &str = "/usr/bin/ping";
const CARRIER_DIR: &str = "/sys/class/net";

/// 网卡物理链路是否已连接；网卡不存在或 carrier 读不到时返回 `None`。
pub fn carrier_up(nic: &str) -> Option<bool> {
    if nic.trim().is_empty() {
        return None;
    }
    let path = Path::new(CARRIER_DIR).join(nic.trim());
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
/// `None` 表示探测工具缺失、无法判断——调用方按"需要认证 / 未连接"处理。
pub fn wired_path_reachable(nic: &str) -> Option<bool> {
    if carrier_up(nic) != Some(true) {
        // 网卡不存在或无网线：有线路径必然不可用，连探测都不必发。
        return Some(false);
    }
    let has_curl = is_executable(CURL_PATH);
    let has_ping = is_executable(PING_PATH);
    if !has_curl && !has_ping {
        return None;
    }
    if has_curl && tcp_probe(nic) {
        return Some(true);
    }
    if has_ping && icmp_probe(nic) {
        return Some(true);
    }
    Some(false)
}

/// HTTP 探测：绑定网卡、绕过代理、字面 IP，任何 HTTP 响应（含 404）都算路径可用。
fn tcp_probe(nic: &str) -> bool {
    Command::new(CURL_PATH)
        .args([
            "-4",
            "--noproxy",
            "*",
            "--interface",
            nic,
            "-sS",
            "-o",
            "/dev/null",
            "--connect-timeout",
            "2",
            "--max-time",
            "4",
            PROBE_URL,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// ICMP 兜底：少数校园网放行 ICMP 但拦截 80/443。
fn icmp_probe(nic: &str) -> bool {
    Command::new(PING_PATH)
        .args(["-c", "1", "-W", "2", "-I", nic, PROBE_IPV4])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn is_executable(path: &str) -> bool {
    Path::new(path)
        .metadata()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_interfaces_are_not_probed() {
        // 空网卡名与不存在的网卡都必须直接判为不可用，不能去访问 /sys 之外的路径。
        assert_eq!(wired_path_reachable(""), Some(false));
        assert_eq!(wired_path_reachable("rj-nonexistent0"), Some(false));
        assert_eq!(carrier_up(""), None);
        assert_eq!(carrier_up("rj-nonexistent0"), None);
    }
}
