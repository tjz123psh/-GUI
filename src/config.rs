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
            "dhcp" => settings.dhcp = value.trim() != "false",
            "save_password" => settings.save_password = value.trim() != "false",
            _ => {}
        }
    }

    settings
}

pub fn save(settings: &Settings) -> anyhow::Result<()> {
    validate(settings)?;

    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = format!(
        "username={}\nnic={}\ndhcp={}\nsave_password={}\n",
        clean_value(&settings.username),
        clean_value(&settings.nic),
        settings.dhcp,
        settings.save_password
    );
    let mut options = fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    use std::io::Write;
    options.open(&path)?.write_all(content.as_bytes())?;
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    Ok(())
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
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
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
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(data_home);
    }

    home_dir().join(".local/share")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn is_64_bit() -> bool {
    std::mem::size_of::<usize>() == 8
}

fn arch_dir() -> &'static str {
    if is_64_bit() { "x64" } else { "x86" }
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
    fn accepts_common_account_and_interface_names() {
        assert!(validate(&settings("20260001@gdufs", "enp4s0.20")).is_ok());
    }

    #[test]
    fn rejects_values_that_could_change_service_arguments() {
        assert!(validate(&settings("student --help", "eno1")).is_err());
        assert!(validate(&settings("student", "eno1 --help")).is_err());
    }
}
