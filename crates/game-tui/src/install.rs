//! 把游戏装进用户目录，并让终端能直接运行 `r-chuanqi`。
//!
//! Windows 装到 `%LOCALAPPDATA%\tui-game`，把这个目录加进用户 PATH。
//! 其它系统装到 `~/.local/share/tui-game`，命令放在 `~/.local/bin/r-chuanqi`。
//! 存档不在这里，安装和更新都不会动它。

use crate::update::UpdateError;
use std::path::{Path, PathBuf};

const SHELL_MARK: &str = "# r-chuanqi";

/// 安装结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    /// 程序和 `data/` 所在目录。
    pub dir: PathBuf,
    /// 给玩家看的下一句。
    pub note: String,
}

/// 安装根目录。
pub fn install_dir() -> Result<PathBuf, UpdateError> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| {
                directories::BaseDirs::new().map(|dirs| dirs.data_local_dir().to_path_buf())
            })
            .map(|dir| dir.join("tui-game"))
            .ok_or_else(|| UpdateError("找不到本地应用目录".to_string()))
    } else {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()))
            .map(|home| home.join(".local").join("share").join("tui-game"))
            .ok_or_else(|| UpdateError("找不到用户主目录".to_string()))
    }
}

/// 把解压出来的目录装到安装根，并接上终端命令。
pub fn install_from(source: &Path) -> Result<InstallReport, UpdateError> {
    let new_exe = find_binary(source)?;
    let payload = new_exe
        .parent()
        .ok_or_else(|| UpdateError("安装包里的程序没有目录".to_string()))?;
    let dest = install_dir()?;
    std::fs::create_dir_all(&dest).map_err(|err| UpdateError(format!("建不了安装目录：{err}")))?;
    copy_tree(payload, &dest, &new_exe)?;
    let installed = dest.join(binary_name());
    make_executable(&installed)?;
    let note = link_command(&dest, &installed)?;
    Ok(InstallReport { dir: dest, note })
}

/// 把发布包字节装进去。发布构建才会带上内容。
pub fn install_embedded(bytes: &[u8]) -> Result<InstallReport, UpdateError> {
    if bytes.is_empty() {
        return Err(UpdateError(
            "这个程序没有带上游戏文件。请下载 GitHub Release 里的安装程序。".to_string(),
        ));
    }
    let staging = std::env::temp_dir().join(format!("r-chuanqi-setup-{}", std::process::id()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .map_err(|err| UpdateError(format!("清不了临时目录：{err}")))?;
    }
    std::fs::create_dir_all(&staging)
        .map_err(|err| UpdateError(format!("建不了临时目录：{err}")))?;
    let archive_name = if bytes.starts_with(&[0x1f, 0x8b]) {
        "payload.tar.gz"
    } else {
        "payload.zip"
    };
    let archive = staging.join(archive_name);
    std::fs::write(&archive, bytes).map_err(|err| UpdateError(format!("写不了安装包：{err}")))?;
    let extracted = staging.join("files");
    std::fs::create_dir_all(&extracted)
        .map_err(|err| UpdateError(format!("建不了解压目录：{err}")))?;
    let result = (|| {
        extract_archive(&archive, &extracted)?;
        install_from(&extracted)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

pub(crate) fn copy_tree(source: &Path, dest: &Path, skip: &Path) -> Result<(), UpdateError> {
    let entries =
        std::fs::read_dir(source).map_err(|err| UpdateError(format!("读不了安装包：{err}")))?;
    for entry in entries {
        let entry = entry.map_err(|err| UpdateError(format!("读不了安装包：{err}")))?;
        let path = entry.path();
        if path == skip || path.file_name().and_then(|name| name.to_str()) == Some(".update") {
            continue;
        }
        let target = dest.join(entry.file_name());
        if path.is_dir() {
            std::fs::create_dir_all(&target)
                .map_err(|err| UpdateError(format!("建不了安装目录：{err}")))?;
            copy_tree(&path, &target, skip)?;
        } else {
            std::fs::copy(&path, &target).map_err(|err| {
                UpdateError(format!(
                    "复制 {} 失败：{err}",
                    entry.file_name().to_string_lossy()
                ))
            })?;
        }
    }
    Ok(())
}

pub(crate) fn extract_archive(archive: &Path, dest: &Path) -> Result<(), UpdateError> {
    let name = archive
        .file_name()
        .and_then(|item| item.to_str())
        .unwrap_or("");
    if name.ends_with(".zip") {
        extract_zip(archive, dest)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(archive, dest)
    } else {
        Err(UpdateError(format!("不认识的安装包：{name}")))
    }
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), UpdateError> {
    let file =
        std::fs::File::open(archive).map_err(|err| UpdateError(format!("打不开安装包：{err}")))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|err| UpdateError(format!("安装包损坏：{err}")))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|err| UpdateError(format!("读不了压缩项：{err}")))?;
        let rel = safe_relative(entry.name())?;
        let path = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&path)
                .map_err(|err| UpdateError(format!("建不了解压目录：{err}")))?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| UpdateError(format!("建不了解压目录：{err}")))?;
        }
        let mut output = std::fs::File::create(&path)
            .map_err(|err| UpdateError(format!("写不了解压文件：{err}")))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|err| UpdateError(format!("解压失败：{err}")))?;
    }
    Ok(())
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), UpdateError> {
    let file =
        std::fs::File::open(archive).map_err(|err| UpdateError(format!("打不开安装包：{err}")))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    let entries = tar
        .entries()
        .map_err(|err| UpdateError(format!("安装包损坏：{err}")))?;
    for entry in entries {
        let mut entry = entry.map_err(|err| UpdateError(format!("读不了压缩项：{err}")))?;
        let rel = entry
            .path()
            .map_err(|err| UpdateError(format!("压缩项路径坏了：{err}")))?;
        let rel = safe_relative(&rel.to_string_lossy())?;
        let path = dest.join(rel);
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&path)
                .map_err(|err| UpdateError(format!("建不了解压目录：{err}")))?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| UpdateError(format!("建不了解压目录：{err}")))?;
        }
        let mut output = std::fs::File::create(&path)
            .map_err(|err| UpdateError(format!("写不了解压文件：{err}")))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|err| UpdateError(format!("解压失败：{err}")))?;
    }
    Ok(())
}

fn safe_relative(name: &str) -> Result<PathBuf, UpdateError> {
    let path = Path::new(name);
    if path.is_absolute() || name.contains(':') {
        return Err(UpdateError("安装包里有不安全的路径".to_string()));
    }
    if path.components().any(|part| {
        matches!(
            part,
            std::path::Component::ParentDir | std::path::Component::RootDir
        )
    }) {
        return Err(UpdateError("安装包里有不安全的路径".to_string()));
    }
    Ok(path.to_path_buf())
}

pub(crate) fn find_binary(root: &Path) -> Result<PathBuf, UpdateError> {
    let mut found = Vec::new();
    collect_named(root, binary_name(), &mut found)?;
    found.sort();
    found
        .into_iter()
        .next()
        .ok_or_else(|| UpdateError(format!("安装包里没有 {}", binary_name())))
}

fn collect_named(dir: &Path, name: &str, found: &mut Vec<PathBuf>) -> Result<(), UpdateError> {
    let entries =
        std::fs::read_dir(dir).map_err(|err| UpdateError(format!("读不了解压目录：{err}")))?;
    for entry in entries {
        let entry = entry.map_err(|err| UpdateError(format!("读不了解压目录：{err}")))?;
        let path = entry.path();
        if path.is_dir() {
            collect_named(&path, name, found)?;
        } else if path.file_name().and_then(|item| item.to_str()) == Some(name) {
            found.push(path);
        }
    }
    Ok(())
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "r-chuanqi.exe"
    } else {
        "r-chuanqi"
    }
}

fn make_executable(path: &Path) -> Result<(), UpdateError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)
            .map_err(|err| UpdateError(format!("读不了程序权限：{err}")))?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions)
            .map_err(|err| UpdateError(format!("设不了程序权限：{err}")))?;
    }
    let _ = path;
    Ok(())
}

#[cfg(windows)]
fn link_command(dir: &Path, _installed: &Path) -> Result<String, UpdateError> {
    use winreg::enums::{RegType, HKEY_CURRENT_USER};
    use winreg::RegKey;
    let root = RegKey::predef(HKEY_CURRENT_USER);
    let (env, _) = root
        .create_subkey("Environment")
        .map_err(|err| UpdateError(format!("打不开用户环境变量：{err}")))?;
    let (kind, bytes) = match env.get_raw_value("Path") {
        Ok(value) => (value.vtype, value.bytes),
        Err(_) => (RegType::REG_EXPAND_SZ, Vec::new()),
    };
    let current = decode_reg_text(&bytes);
    let (next, added) = merge_path(&current, &dir.display().to_string());
    if added {
        let kind = if kind == RegType::REG_SZ {
            RegType::REG_SZ
        } else {
            RegType::REG_EXPAND_SZ
        };
        env.set_raw_value(
            "Path",
            &winreg::RegValue {
                vtype: kind,
                bytes: encode_reg_text(&next),
            },
        )
        .map_err(|err| UpdateError(format!("写不了用户 PATH：{err}")))?;
    }
    Ok("已安装。关掉并重新打开终端后，运行 r-chuanqi。".to_string())
}

#[cfg(not(windows))]
fn link_command(_dir: &Path, installed: &Path) -> Result<String, UpdateError> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| UpdateError("找不到用户主目录".to_string()))?;
    let bin = home.join(".local").join("bin");
    std::fs::create_dir_all(&bin).map_err(|err| UpdateError(format!("建不了命令目录：{err}")))?;
    let link = bin.join("r-chuanqi");
    replace_link(&link, installed)?;
    let bin_text = bin.display().to_string();
    let already = std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|entry| entry == bin));
    if !already {
        let line = format!("export PATH=\"{bin_text}:$PATH\" {SHELL_MARK}");
        for name in [
            ".profile",
            ".zprofile",
            ".bash_profile",
            ".bashrc",
            ".zshrc",
        ] {
            let path = home.join(name);
            let existing = std::fs::read_to_string(&path).unwrap_or_default();
            if let Some(next) = append_shell_line(&existing, &line) {
                std::fs::write(&path, next)
                    .map_err(|err| UpdateError(format!("写不了 {name}：{err}")))?;
            }
        }
    }
    Ok("已安装。新开一个终端后，运行 r-chuanqi。".to_string())
}

#[cfg(not(windows))]
fn replace_link(link: &Path, target: &Path) -> Result<(), UpdateError> {
    if std::fs::symlink_metadata(link).is_ok() {
        std::fs::remove_file(link).map_err(|err| UpdateError(format!("换不了命令链接：{err}")))?;
    }
    std::os::unix::fs::symlink(target, link)
        .map_err(|err| UpdateError(format!("建不了命令链接：{err}")))
}

/// 没有就加到末尾。大小写不同也算同一条。
pub fn merge_path(current: &str, dir: &str) -> (String, bool) {
    let separator = if cfg!(windows) { ';' } else { ':' };
    let present = current
        .split(separator)
        .any(|entry| !entry.is_empty() && entry.trim_matches('"').eq_ignore_ascii_case(dir));
    if present {
        return (current.to_string(), false);
    }
    let trimmed = current.trim().trim_end_matches(separator);
    if trimmed.is_empty() {
        (dir.to_string(), true)
    } else {
        (format!("{trimmed}{separator}{dir}"), true)
    }
}

/// 配置里已经有标记就不动。
pub fn append_shell_line(existing: &str, line: &str) -> Option<String> {
    if existing.lines().any(|item| item.contains(SHELL_MARK)) {
        return None;
    }
    let mut next = existing.to_string();
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(line);
    next.push('\n');
    Some(next)
}

fn decode_reg_text(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

fn encode_reg_text(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for unit in text.encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    bytes.extend(0_u16.to_le_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_entry_is_added_once() {
        let (first, added) = merge_path("", r"C:\Users\a\AppData\Local\tui-game");
        assert!(added);
        assert_eq!(first, r"C:\Users\a\AppData\Local\tui-game");
        let (second, again) = merge_path(&first, r"c:\users\a\appdata\local\tui-game");
        assert!(!again);
        assert_eq!(second, first);
        let (third, extra) = merge_path(r"C:\Windows\system32;", r"D:\games");
        assert!(extra);
        assert_eq!(third, r"C:\Windows\system32;D:\games");
    }

    #[test]
    fn shell_line_is_not_repeated() {
        let line = "export PATH=\"$HOME/.local/bin:$PATH\" # r-chuanqi";
        let once = append_shell_line("export A=1\n", line).unwrap();
        assert!(once.contains(line));
        assert!(append_shell_line(&once, line).is_none());
    }

    #[test]
    fn registry_text_round_trips() {
        let bytes = encode_reg_text(r"%USERPROFILE%\bin;C:\Tools");
        assert_eq!(decode_reg_text(&bytes), r"%USERPROFILE%\bin;C:\Tools");
    }
}
