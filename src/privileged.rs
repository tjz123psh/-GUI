use anyhow::Result;
use std::path::{Path, PathBuf};

pub const HELPER_PATH: &str = "/usr/lib/rjsupplicant-gui/rjsupplicant-helper";
pub const CLIENT_DIR: &str = "/usr/lib/rjsupplicant";
pub const CLIENT_WRAPPER_PATH: &str = "/usr/lib/rjsupplicant-gui/rjsupplicant";
pub const SERVICE_PATH: &str = "/etc/systemd/system/rjsupplicant.service";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthOptions {
    pub username: String,
    pub nic: String,
    pub dhcp: bool,
    pub save_password: bool,
    pub password: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HelperRequest {
    InstallClient(PathBuf),
    Authenticate(AuthOptions),
    Disconnect,
    EnableService(AuthOptions),
    DisableService,
    RestartService,
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
            _ => anyhow::bail!("不支持的 helper 子命令或参数数量：{command}"),
        }
    }

    pub fn arguments(&self) -> Vec<String> {
        match self {
            Self::InstallClient(path) => vec![
                "install-client".to_string(),
                path.to_string_lossy().into_owned(),
            ],
            Self::Authenticate(options) => options_arguments("authenticate", options),
            Self::Disconnect => vec!["disconnect".to_string()],
            Self::EnableService(options) => options_arguments("enable-service", options),
            Self::DisableService => vec!["disable-service".to_string()],
            Self::RestartService => vec!["restart-service".to_string()],
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

pub fn service_file(options: &AuthOptions) -> String {
    let dhcp = bool_flag(options.dhcp);
    let save = bool_flag(options.save_password);
    let client = systemd_quote(CLIENT_WRAPPER_PATH);
    let nic = systemd_quote(&options.nic);
    let username = systemd_quote(&options.username);
    let workdir = systemd_quote(
        &Path::new(CLIENT_DIR)
            .join(current_arch_dir())
            .to_string_lossy(),
    );

    format!(
        "[Unit]\n\
         Description=Ruijie RG-SU wired authentication client\n\
         Documentation=https://etr.gdufs.edu.cn/info/1303/5137.htm\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=forking\n\
         GuessMainPID=yes\n\
         ExecStart={client} -a 1 -d {dhcp} -n {nic} -u {username} -S {save}\n\
         ExecStop={client} -q\n\
         Restart=on-failure\n\
         RestartSec=10\n\
         TimeoutStartSec=30\n\
         TimeoutStopSec=15\n\
         WorkingDirectory={workdir}\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n"
    )
}

pub fn service_content_uses_owned_paths(content: &str) -> bool {
    let expected_program = format!("ExecStart=\"{CLIENT_WRAPPER_PATH}\"");
    let expected_stop = format!("ExecStop=\"{CLIENT_WRAPPER_PATH}\" -q");
    let expected_workdir = format!(
        "WorkingDirectory=\"{}\"",
        Path::new(CLIENT_DIR).join(current_arch_dir()).display()
    );
    let mut start_count = 0;
    let mut stop_count = 0;
    let mut workdir_count = 0;

    for line in content.lines() {
        if line.starts_with("Exec") {
            if line.starts_with(&expected_program) && valid_service_start(line) {
                start_count += 1;
            } else if line == expected_stop {
                stop_count += 1;
            } else {
                return false;
            }
        } else if line.starts_with("WorkingDirectory=") {
            if line != expected_workdir {
                return false;
            }
            workdir_count += 1;
        } else if line.starts_with("Environment") {
            return false;
        }
    }

    start_count == 1 && stop_count == 1 && workdir_count == 1
}

fn valid_service_start(line: &str) -> bool {
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
        return false;
    }
    let Some(nic) = fields[6]
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return false;
    };
    let Some(username) = fields[8]
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return false;
    };
    HelperRequest::parse(&[
        "enable-service".to_string(),
        fields[4].to_string(),
        nic.to_string(),
        username.to_string(),
        fields[10].to_string(),
    ])
    .is_ok()
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
        || nic.len() > 32
        || !nic
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-'))
    {
        anyhow::bail!("网卡名称无效");
    }
    Ok(())
}

fn current_arch_dir() -> &'static str {
    if cfg!(target_pointer_width = "64") {
        "x64"
    } else {
        "x86"
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
        assert!(content.contains("Type=forking"));
        assert!(content.contains(CLIENT_WRAPPER_PATH));
        assert!(content.contains("-n \"enp4s0.20\" -u \"20260001@gdufs\""));
        assert!(!content.contains("/home/"));
    }

    #[test]
    fn rejects_service_with_user_controlled_paths_or_environment() {
        let content = service_file(&options());
        let legacy = content.replace(CLIENT_WRAPPER_PATH, "/home/student/.local/bin/rjsupplicant");
        assert!(!service_content_uses_owned_paths(&legacy));

        let injected = content.replace(
            "Type=forking",
            "Type=forking\nEnvironment=LD_PRELOAD=/home/student/lib.so",
        );
        assert!(!service_content_uses_owned_paths(&injected));
    }
}
