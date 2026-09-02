use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub const APP_ID: &str = "io.github.pang.RjSupplicantGui";
pub const SERVICE: &str = "rjsupplicant.service";

#[derive(Clone, Debug)]
pub struct Settings {
    pub username: String,
    pub nic: String,
    pub dhcp: bool,
    pub save_password: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            username: String::new(),
            nic: "eno1".to_string(),
            dhcp: true,
            save_password: true,
        }
    }
}

pub fn load() -> Settings {
    let mut settings = Settings::default();

    let Ok(content) = fs::read_to_string(settings_path()) else {
        return settings;
    };

    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        match key.trim() {
            "username" => settings.username = value.trim().to_string(),
            "nic" => settings.nic = value.trim().to_string(),
            "dhcp" => settings.dhcp = parse_flag(value, settings.dhcp),
            "save_password" => {
                settings.save_password = parse_flag(value, settings.save_password);
            }
            _ => {}
        }
    }

    settings
}

/// 只认明确字面量。旧写法 `value != "false"` 会把 `0`/`no`/`off`/空值/拼错
/// 全部读成 true，手改配置文件的人会得到与所见相反的行为。
fn parse_flag(value: &str, fallback: bool) -> bool {
    match value.trim() {
        "true" | "1" => true,
        "false" | "0" => false,
        _ => fallback,
    }
}

pub fn save(settings: &Settings) -> anyhow::Result<()> {
    validate(settings)?;

    let path = settings_path();
    let parent = path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&parent)?;

    let content = format!(
        "username={}\nnic={}\ndhcp={}\nsave_password={}\n",
        clean_value(&settings.username),
        clean_value(&settings.nic),
        settings.dhcp,
        settings.save_password
    );
    // 先写临时文件再 rename：原来的 create+truncate 就地改写，进程在两步之间
    // 被杀就留下半截配置，而下一次 load 会静默回落默认值（等于账号丢失）。
    let temporary = parent.join(format!(".settings.conf.{}.tmp", std::process::id()));
    let result = (|| -> anyhow::Result<()> {
        use std::io::Write;
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        #[cfg(unix)]
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
        fs::rename(&temporary, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn validate(settings: &Settings) -> anyhow::Result<()> {
    let username = settings.username.trim();
    if username.is_empty() {
        anyhow::bail!("校园网账号不能为空");
    }
    if username.len() > 128
        || !username
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '@' | '.' | '_' | '+' | '-'))
    {
        anyhow::bail!("校园网账号只能包含字母、数字和 @ . _ + -");
    }

    let nic = settings.nic.trim();
    if nic.is_empty() || nic.len() > 32 {
        anyhow::bail!("网卡名称无效");
    }
    if !nic
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-'))
    {
        anyhow::bail!("网卡名称包含不支持的字符");
    }

    Ok(())
}

pub fn settings_path() -> PathBuf {
    // 空字符串按“未设置”处理：否则 XDG_CONFIG_HOME="" 会拼出依赖当前目录的
    // 相对路径，配置可能被写进任意目录。
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(config_home).join("rjsupplicant-gui/settings.conf");
    }

    home_dir().join(".config/rjsupplicant-gui/settings.conf")
}

pub fn client_path() -> PathBuf {
    bin_dir().join("rjsupplicant")
}

pub fn client_binary_path() -> PathBuf {
    data_dir().join(arch_dir()).join("rjsupplicant")
}

pub fn data_dir() -> PathBuf {
    data_home().join("rjsupplicant")
}

pub fn log_path() -> PathBuf {
    data_dir().join(arch_dir()).join("log/run.log")
}

pub fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn bin_dir() -> PathBuf {
    home_dir().join(".local/bin")
}

fn data_home() -> PathBuf {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(data_home);
    }

    home_dir().join(".local/share")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 架构目录名以 `privileged::client_arch_dir` 为唯一来源；此前这里按指针宽度
/// 判定，aarch64 会得到 64 位并错误指向不存在的 `x64`。
fn arch_dir() -> &'static str {
    match rjsupplicant_gui::privileged::client_arch_dir() {
        Some(dir) => dir,
        None => "unsupported-arch",
    }
}

fn clean_value(value: &str) -> String {
    value
        .chars()
        .filter(|ch| *ch != '\n' && *ch != '\r')
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(username: &str, nic: &str) -> Settings {
        Settings {
            username: username.to_string(),
            nic: nic.to_string(),
            dhcp: true,
            save_password: true,
        }
    }

    #[test]
    fn flags_only_accept_explicit_literals() {
        // 旧写法 `value != "false"` 会把 0/no/off/空值/拼错一律读成 true。
        for truthy in ["true", "1", " true "] {
            assert!(parse_flag(truthy, false), "{truthy} 应解析为 true");
        }
        for falsy in ["false", "0", " false "] {
            assert!(!parse_flag(falsy, true), "{falsy} 应解析为 false");
        }
        // 认不出的值保持各自默认，而不是悄悄统一成某个方向。
        assert!(parse_flag("yes", true));
        assert!(!parse_flag("no", false));
        assert!(parse_flag("", true));
        assert!(!parse_flag("", false));
    }

    #[test]
    fn accepts_common_account_and_interface_names() {
        assert!(validate(&settings("20260001@gdufs", "enp4s0.20")).is_ok());
    }

    #[test]
    fn rejects_values_that_could_change_service_arguments() {
        assert!(validate(&settings("student --help", "eno1")).is_err());
        assert!(validate(&settings("student", "eno1 --help")).is_err());
    }
}
