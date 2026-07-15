use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

const UNZIP_PATH: &str = "/usr/bin/unzip";

pub fn install_official_client(zip_path: &Path, app_dir: &Path, client_path: &Path) -> Result<()> {
    if !is_executable_file(Path::new(UNZIP_PATH)) {
        anyhow::bail!("找不到 {UNZIP_PATH}，请先安装 unzip");
    }
    let data_home = app_dir.parent().context("无法确定客户端数据目录")?;
    fs::create_dir_all(data_home).context("无法创建客户端数据目录")?;
    let staging = create_staging_dir(data_home)?;
    let staged_archive = staging.join("selected-client.zip");

    let result = (|| {
        snapshot_archive(zip_path, &staged_archive)?;
        validate_archive_entries(&staged_archive)?;
        install_official_client_inner(&staged_archive, &staging, app_dir, client_path)
    })();

    let _ = fs::remove_dir_all(&staging);
    result
}

fn snapshot_archive(source_path: &Path, destination: &Path) -> Result<()> {
    let mut source_options = fs::OpenOptions::new();
    source_options.read(true);
    #[cfg(target_os = "linux")]
    source_options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let mut source = source_options
        .open(source_path)
        .context("无法打开选择的安装包")?;
    if !source.metadata()?.is_file() {
        anyhow::bail!("选择的安装包不是普通文件");
    }
    let mut destination = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .context("无法暂存选择的安装包")?;
    std::io::copy(&mut source, &mut destination).context("无法复制选择的安装包")?;
    destination.sync_all()?;
    Ok(())
}

fn install_official_client_inner(
    zip_path: &Path,
    staging: &Path,
    app_dir: &Path,
    client_path: &Path,
) -> Result<()> {
    let output = Command::new(UNZIP_PATH)
        .arg("-q")
        .arg(zip_path)
        .arg("-d")
        .arg(staging)
        .output()
        .context("无法启动 unzip")?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "解压官方客户端失败{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!("：{detail}")
            }
        );
    }

    let extracted = staging.join("rjsupplicant");
    if !extracted.is_dir() {
        anyhow::bail!("ZIP 内未找到 rjsupplicant 目录，请选择学校提供的 Linux 客户端安装包");
    }
    let required_binary = extracted
        .join(if cfg!(target_pointer_width = "64") {
            "x64"
        } else {
            "x86"
        })
        .join("rjsupplicant");
    if !required_binary
        .symlink_metadata()
        .map(|metadata| metadata.file_type().is_file())
        .unwrap_or(false)
    {
        anyhow::bail!("ZIP 内缺少当前系统架构的 rjsupplicant 可执行文件");
    }
    harden_extracted_tree(&extracted)?;
    make_client_binaries_executable(&extracted)?;

    let bin_dir = client_path.parent().context("无法确定用户程序目录")?;
    fs::create_dir_all(bin_dir).context("无法创建用户程序目录")?;
    let app_parent = app_dir.parent().context("无法确定客户端安装目录")?;
    fs::create_dir_all(app_parent).context("无法创建客户端安装目录")?;
    let wrapper_temp = write_wrapper_temp(bin_dir, &wrapper_script(app_dir))?;

    let previous = staging.join("previous-installation");
    let had_previous = app_dir.exists();
    if had_previous && let Err(err) = fs::rename(app_dir, &previous) {
        let _ = fs::remove_file(&wrapper_temp);
        return Err(err).context("无法暂存旧客户端目录");
    }
    if let Err(err) = fs::rename(&extracted, app_dir) {
        if had_previous {
            let _ = fs::rename(&previous, app_dir);
        }
        let _ = fs::remove_file(&wrapper_temp);
        return Err(err).context("无法安装新的客户端目录");
    }

    if let Err(err) = fs::rename(&wrapper_temp, client_path) {
        let _ = fs::remove_dir_all(app_dir);
        if had_previous {
            let _ = fs::rename(&previous, app_dir);
        }
        let _ = fs::remove_file(&wrapper_temp);
        return Err(err).context("无法安装官方客户端 wrapper");
    }

    Ok(())
}

fn validate_archive_entries(zip_path: &Path) -> Result<()> {
    let output = Command::new(UNZIP_PATH)
        .arg("-Z1")
        .arg(zip_path)
        .output()
        .context("无法读取 ZIP 文件列表")?;
    if !output.status.success() {
        anyhow::bail!("无法读取 ZIP 文件列表，请确认安装包完整");
    }

    let listing = String::from_utf8_lossy(&output.stdout);
    if listing.trim().is_empty() {
        anyhow::bail!("ZIP 安装包为空");
    }
    for entry in listing.lines() {
        if !archive_entry_is_safe(entry) {
            anyhow::bail!("ZIP 包含不安全路径：{entry}");
        }
    }

    let output = Command::new(UNZIP_PATH)
        .args(["-Z", "-l"])
        .arg(zip_path)
        .output()
        .context("无法读取 ZIP 文件类型")?;
    if !output.status.success() {
        anyhow::bail!("无法读取 ZIP 文件类型，请确认安装包完整");
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if !archive_type_line_is_safe(line) {
            anyhow::bail!("ZIP 包含不允许的链接或特殊文件");
        }
    }
    Ok(())
}

fn archive_type_line_is_safe(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.len() < 10
        || !bytes[1..10]
            .iter()
            .all(|byte| matches!(byte, b'-' | b'r' | b'w' | b'x' | b's' | b'S' | b't' | b'T'))
    {
        return true;
    }
    !matches!(bytes[0], b'l' | b'p' | b'c' | b'b' | b's')
}

fn archive_entry_is_safe(entry: &str) -> bool {
    if entry.is_empty() || entry.starts_with(['/', '\\']) {
        return false;
    }
    let normalized = entry.replace('\\', "/");
    Path::new(&normalized)
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn create_staging_dir(parent: &Path) -> Result<PathBuf> {
    for attempt in 0..100_u32 {
        let path = parent.join(format!(
            ".rjsupplicant-install-{}-{attempt}",
            std::process::id()
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err).context("无法创建客户端安装临时目录"),
        }
    }
    anyhow::bail!("无法分配客户端安装临时目录")
}

fn write_wrapper_temp(bin_dir: &Path, content: &str) -> Result<PathBuf> {
    for attempt in 0..100_u32 {
        let path = bin_dir.join(format!(
            ".rjsupplicant-wrapper-{}-{attempt}",
            std::process::id()
        ));
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o755);
        match options.open(&path) {
            Ok(mut file) => {
                file.write_all(content.as_bytes())?;
                file.sync_all()?;
                return Ok(path);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err).context("无法创建官方客户端 wrapper"),
        }
    }
    anyhow::bail!("无法分配官方客户端 wrapper 临时文件")
}

fn harden_extracted_tree(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("无法检查解压文件 {}", path.display()))?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        anyhow::bail!("ZIP 包含不允许的符号链接：{}", path.display());
    }
    if file_type.is_dir() {
        #[cfg(unix)]
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        for entry in fs::read_dir(path)? {
            harden_extracted_tree(&entry?.path())?;
        }
        return Ok(());
    }
    if file_type.is_file() {
        #[cfg(unix)]
        {
            let executable = metadata.permissions().mode() & 0o111 != 0;
            let mode = if executable { 0o755 } else { 0o644 };
            fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
        }
        return Ok(());
    }
    anyhow::bail!("ZIP 包含不支持的特殊文件：{}", path.display())
}

fn make_client_binaries_executable(extracted: &Path) -> Result<()> {
    for arch in ["x64", "x86"] {
        let path = extracted.join(arch).join("rjsupplicant");
        if !path.exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("无法检查客户端文件 {}", path.display()))?;
        if !metadata.file_type().is_file() {
            anyhow::bail!("客户端文件不是普通文件：{}", path.display());
        }
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn wrapper_script(app_dir: &Path) -> String {
    format!(
        "#!/usr/bin/bash\n\
         set -euo pipefail\n\
         app_dir={}\n\
         arch_dir=\"${{app_dir}}/x64\"\n\
         if [[ \"$(/usr/bin/getconf LONG_BIT)\" != \"64\" ]]; then\n\
           arch_dir=\"${{app_dir}}/x86\"\n\
         fi\n\
         cd \"${{arch_dir}}\"\n\
         export LD_LIBRARY_PATH=\"${{arch_dir}}/lib\"\n\
         exec \"${{arch_dir}}/rjsupplicant\" \"$@\"\n",
        shell_quote(&app_dir.to_string_lossy())
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn is_executable_file(path: &Path) -> bool {
    path.is_file()
        && path
            .metadata()
            .map(|metadata| {
                #[cfg(unix)]
                {
                    metadata.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    true
                }
            })
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_u16(buffer: &mut Vec<u8>, value: u16) {
        buffer.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(buffer: &mut Vec<u8>, value: u32) {
        buffer.extend_from_slice(&value.to_le_bytes());
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = u32::MAX;
        for byte in data {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
            }
        }
        !crc
    }

    fn write_stored_zip(path: &Path, entries: &[(&str, &[u8])]) {
        struct CentralEntry {
            name: Vec<u8>,
            crc: u32,
            size: u32,
            offset: u32,
        }

        let mut archive = Vec::new();
        let mut central_entries = Vec::new();
        for (name, data) in entries {
            let name = name.as_bytes();
            let crc = crc32(data);
            let offset = archive.len() as u32;
            push_u32(&mut archive, 0x0403_4b50);
            push_u16(&mut archive, 20);
            push_u16(&mut archive, 0);
            push_u16(&mut archive, 0);
            push_u16(&mut archive, 0);
            push_u16(&mut archive, 0);
            push_u32(&mut archive, crc);
            push_u32(&mut archive, data.len() as u32);
            push_u32(&mut archive, data.len() as u32);
            push_u16(&mut archive, name.len() as u16);
            push_u16(&mut archive, 0);
            archive.extend_from_slice(name);
            archive.extend_from_slice(data);
            central_entries.push(CentralEntry {
                name: name.to_vec(),
                crc,
                size: data.len() as u32,
                offset,
            });
        }

        let central_offset = archive.len() as u32;
        for entry in &central_entries {
            push_u32(&mut archive, 0x0201_4b50);
            push_u16(&mut archive, 20);
            push_u16(&mut archive, 20);
            push_u16(&mut archive, 0);
            push_u16(&mut archive, 0);
            push_u16(&mut archive, 0);
            push_u16(&mut archive, 0);
            push_u32(&mut archive, entry.crc);
            push_u32(&mut archive, entry.size);
            push_u32(&mut archive, entry.size);
            push_u16(&mut archive, entry.name.len() as u16);
            push_u16(&mut archive, 0);
            push_u16(&mut archive, 0);
            push_u16(&mut archive, 0);
            push_u16(&mut archive, 0);
            push_u32(&mut archive, 0);
            push_u32(&mut archive, entry.offset);
            archive.extend_from_slice(&entry.name);
        }
        let central_size = archive.len() as u32 - central_offset;
        push_u32(&mut archive, 0x0605_4b50);
        push_u16(&mut archive, 0);
        push_u16(&mut archive, 0);
        push_u16(&mut archive, central_entries.len() as u16);
        push_u16(&mut archive, central_entries.len() as u16);
        push_u32(&mut archive, central_size);
        push_u32(&mut archive, central_offset);
        push_u16(&mut archive, 0);
        fs::write(path, archive).expect("write ZIP fixture");
    }

    fn client_zip(path: &Path) {
        write_stored_zip(
            path,
            &[
                ("rjsupplicant/x64/rjsupplicant", b"#!/bin/sh\n"),
                ("rjsupplicant/x86/rjsupplicant", b"#!/bin/sh\n"),
                ("rjsupplicant/x64/lib/placeholder", b"fixture\n"),
            ],
        );
    }

    #[test]
    fn rejects_unsafe_archive_entries() {
        assert!(archive_entry_is_safe("rjsupplicant/x64/rjsupplicant"));
        assert!(archive_entry_is_safe("rjsupplicant/x64/lib/libpcap.so"));
        assert!(!archive_entry_is_safe("../outside"));
        assert!(!archive_entry_is_safe("rjsupplicant/../../outside"));
        assert!(!archive_entry_is_safe("/etc/systemd/system/unit"));
        assert!(!archive_entry_is_safe("..\\outside"));
        assert!(archive_type_line_is_safe(
            "-rwxr-xr-x  3.0 unx 123 b- 100 stor file"
        ));
        assert!(archive_type_line_is_safe(
            "drwxr-xr-x  3.0 unx   0 bx   0 stor directory/"
        ));
        assert!(!archive_type_line_is_safe(
            "lrwxrwxrwx  3.0 unx  12 bx  12 stor link"
        ));
        assert!(!archive_type_line_is_safe(
            "prw-------  3.0 unx   0 bx   0 stor pipe"
        ));
    }

    #[test]
    fn quotes_wrapper_data_path() {
        let script = wrapper_script(Path::new("/home/student's data/rjsupplicant"));

        assert!(script.starts_with("#!/usr/bin/bash\n"));
        assert!(script.contains("app_dir='/home/student'\"'\"'s data/rjsupplicant'"));
        assert!(script.contains("$(/usr/bin/getconf LONG_BIT)"));
        assert!(script.contains("export LD_LIBRARY_PATH=\"${arch_dir}/lib\""));
        assert!(!script.contains("${LD_LIBRARY_PATH"));
        assert!(script.contains("exec \"${arch_dir}/rjsupplicant\" \"$@\""));
    }

    #[test]
    fn installs_client_archive_and_wrapper() {
        let root = create_staging_dir(&std::env::temp_dir()).expect("create fixture root");
        let zip = root.join("client.zip");
        client_zip(&zip);
        validate_archive_entries(&zip).expect("validate fixture ZIP");

        let staging = root.join("staging");
        fs::create_dir(&staging).expect("create extraction staging");
        let app_dir = root.join("data/rjsupplicant");
        let client_path = root.join("bin/rjsupplicant");
        install_official_client_inner(&zip, &staging, &app_dir, &client_path)
            .expect("install fixture client");

        assert!(app_dir.join("x64/rjsupplicant").is_file());
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(app_dir.join("x64/rjsupplicant"))
                .expect("read client mode")
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert!(client_path.is_file());
        let wrapper = fs::read_to_string(&client_path).expect("read installed wrapper");
        assert!(wrapper.contains(&shell_quote(&app_dir.to_string_lossy())));
        fs::remove_dir_all(root).expect("clean fixture root");
    }

    #[test]
    fn restores_previous_client_when_wrapper_install_fails() {
        let root = create_staging_dir(&std::env::temp_dir()).expect("create fixture root");
        let zip = root.join("client.zip");
        client_zip(&zip);
        let staging = root.join("staging");
        fs::create_dir(&staging).expect("create extraction staging");

        let app_dir = root.join("data/rjsupplicant");
        fs::create_dir_all(&app_dir).expect("create previous client");
        fs::write(app_dir.join("old-marker"), b"old").expect("write previous marker");
        let client_path = root.join("bin/rjsupplicant");
        fs::create_dir_all(&client_path).expect("create wrapper collision");

        let result = install_official_client_inner(&zip, &staging, &app_dir, &client_path);

        assert!(result.is_err());
        assert!(app_dir.join("old-marker").is_file());
        assert!(!app_dir.join("x64/rjsupplicant").exists());
        fs::remove_dir_all(root).expect("clean fixture root");
    }

    #[test]
    fn rejects_zip_with_parent_directory_entry() {
        let root = create_staging_dir(&std::env::temp_dir()).expect("create fixture root");
        let zip = root.join("unsafe.zip");
        write_stored_zip(&zip, &[("../outside", b"unsafe")]);

        assert!(validate_archive_entries(&zip).is_err());
        fs::remove_dir_all(root).expect("clean fixture root");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn rejects_symlink_archive_source() {
        use std::os::unix::fs::symlink;

        let root = create_staging_dir(&std::env::temp_dir()).expect("create fixture root");
        let zip = root.join("client.zip");
        client_zip(&zip);
        let link = root.join("selected.zip");
        symlink(&zip, &link).expect("create archive symlink");

        let result = snapshot_archive(&link, &root.join("snapshot.zip"));

        assert!(result.is_err());
        fs::remove_dir_all(root).expect("clean fixture root");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_and_removes_group_write_permissions() {
        use std::os::unix::fs::symlink;

        let root = create_staging_dir(&std::env::temp_dir()).expect("create fixture root");
        let regular = root.join("regular");
        fs::write(&regular, b"data").expect("write fixture file");
        fs::set_permissions(&regular, fs::Permissions::from_mode(0o666)).expect("set fixture mode");
        harden_extracted_tree(&regular).expect("harden regular file");
        assert_eq!(
            fs::metadata(&regular)
                .expect("read mode")
                .permissions()
                .mode()
                & 0o777,
            0o644
        );

        let link = root.join("link");
        symlink(&regular, &link).expect("create fixture symlink");
        assert!(harden_extracted_tree(&link).is_err());
        fs::remove_dir_all(root).expect("clean fixture root");
    }
}
