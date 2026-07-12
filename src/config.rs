use std::fs;
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
    fs::write(path, content)?;
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
