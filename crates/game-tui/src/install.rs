//! 把游戏装进用户目录，并让终端能直接运行 `r-chuanqi`。
//!
//! Windows 装到 `%LOCALAPPDATA%\tui-game`，把这个目录加进用户 PATH，
//! 再通知系统刷新环境。其它系统装到 `~/.local/share/tui-game`，
//! 命令放在 `~/.local/bin/r-chuanqi`。存档不在这里，安装、更新和卸载都不动它。

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
    // 首次安装没有正在运行的游戏进程。跳过包里的 exe 会让 PATH 指向一个没有命令的目录。
    copy_tree(payload, &dest, Path::new(""))?;
    let installed = dest.join(binary_name());
    if !installed.is_file() {
        return Err(UpdateError(format!("安装后找不到 {}", binary_name())));
    }
    make_executable(&installed)?;
    let note = link_command(&dest, &installed)?;
    Ok(InstallReport { dir: dest, note })
}

/// 从用户 PATH 拿掉命令，并删掉安装目录。存档留下。
pub fn uninstall() -> Result<InstallReport, UpdateError> {
    let dir = install_dir()?;
    let refreshed = unlink_command(&dir)?;
    let removed_now = remove_install_dir(&dir)?;
    Ok(InstallReport {
        dir,
        note: uninstall_note(removed_now, &refreshed),
    })
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
        if !skip.as_os_str().is_empty() && same_file(&path, skip)
            || path.file_name().and_then(|name| name.to_str()) == Some(".update")
        {
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

struct PathUpdate {
    changed: bool,
    refreshed: bool,
}

#[cfg(windows)]
fn link_command(dir: &Path, _installed: &Path) -> Result<String, UpdateError> {
    let update = update_user_path(dir, merge_path)?;
    Ok(command_note(update.refreshed))
}

#[cfg(not(windows))]
fn link_command(_dir: &Path, installed: &Path) -> Result<String, UpdateError> {
    let home = home_dir()?;
    let bin = home.join(".local").join("bin");
    std::fs::create_dir_all(&bin).map_err(|err| UpdateError(format!("建不了命令目录：{err}")))?;
    let link = bin.join("r-chuanqi");
    replace_link(&link, installed)?;
    let bin_text = bin.display().to_string();
    let already = std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|entry| entry == bin));
    if !already {
        let line = format!("export PATH=\"{bin_text}:$PATH\" {SHELL_MARK}");
        for name in shell_rc_names() {
            let path = home.join(name);
            let existing = std::fs::read_to_string(&path).unwrap_or_default();
            if let Some(next) = append_shell_line(&existing, &line) {
                std::fs::write(&path, next)
                    .map_err(|err| UpdateError(format!("写不了 {name}：{err}")))?;
            }
        }
    }
    Ok(command_note(true))
}

fn command_note(refreshed: bool) -> String {
    let mut note =
        "已加入用户 PATH。请新开一个终端，运行 r-chuanqi。卸载：r-chuanqi uninstall。存档会留下。"
            .to_string();
    if !refreshed {
        note.push_str(" 这次没能通知系统刷新环境，注销一次后再开终端。");
    }
    note
}

fn uninstall_note(removed_now: bool, update: &PathUpdate) -> String {
    let save = crate::default_save_root();
    let mut note = if update.changed {
        "命令已从 PATH 移除。".to_string()
    } else {
        "PATH 里本来就没有这条命令。".to_string()
    };
    note.push_str(&format!("存档还在 {}。", save.display()));
    if removed_now {
        note.push_str(" 程序文件已删除。");
    } else {
        note.push_str(" 关掉这个窗口后，程序文件会删掉。");
    }
    if !update.refreshed {
        note.push_str(" 这次没能通知系统刷新环境，注销一次后命令才会从已打开的终端里消失。");
    }
    note
}

#[cfg(windows)]
fn unlink_command(dir: &Path) -> Result<PathUpdate, UpdateError> {
    update_user_path(dir, remove_path)
}

#[cfg(not(windows))]
fn unlink_command(dir: &Path) -> Result<PathUpdate, UpdateError> {
    let home = home_dir()?;
    let changed = remove_command_link(&home.join(".local").join("bin").join("r-chuanqi"), dir)?;
    if strip_shell_configs(&home)? {
        return Ok(PathUpdate {
            changed: true,
            refreshed: true,
        });
    }
    Ok(PathUpdate {
        changed,
        refreshed: true,
    })
}

#[cfg(not(windows))]
fn home_dir() -> Result<PathBuf, UpdateError> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| UpdateError("找不到用户主目录".to_string()))
}

#[cfg(not(windows))]
fn shell_rc_names() -> [&'static str; 5] {
    [
        ".profile",
        ".zprofile",
        ".bash_profile",
        ".bashrc",
        ".zshrc",
    ]
}

#[cfg(not(windows))]
fn replace_link(link: &Path, target: &Path) -> Result<(), UpdateError> {
    if std::fs::symlink_metadata(link).is_ok() {
        std::fs::remove_file(link).map_err(|err| UpdateError(format!("换不了命令链接：{err}")))?;
    }
    std::os::unix::fs::symlink(target, link)
        .map_err(|err| UpdateError(format!("建不了命令链接：{err}")))
}

#[cfg(not(windows))]
fn remove_command_link(link: &Path, install: &Path) -> Result<bool, UpdateError> {
    let Ok(meta) = std::fs::symlink_metadata(link) else {
        return Ok(false);
    };
    let owned = if meta.file_type().is_symlink() {
        match std::fs::read_link(link) {
            Ok(target) if target.is_absolute() => same_file(&target, &install.join(binary_name())),
            Ok(target) => link
                .parent()
                .is_some_and(|dir| same_file(&dir.join(target), &install.join(binary_name()))),
            Err(_) => true,
        }
    } else {
        same_file(link, &install.join(binary_name()))
    };
    if owned {
        std::fs::remove_file(link).map_err(|err| UpdateError(format!("删不了命令：{err}")))?;
    }
    Ok(owned)
}

#[cfg(not(windows))]
fn strip_shell_configs(home: &Path) -> Result<bool, UpdateError> {
    let mut changed = false;
    for name in shell_rc_names() {
        let path = home.join(name);
        let Ok(existing) = std::fs::read_to_string(&path) else {
            continue;
        };
        let next = strip_shell_mark(&existing);
        if next != existing {
            std::fs::write(&path, next)
                .map_err(|err| UpdateError(format!("写不了 {name}：{err}")))?;
            changed = true;
        }
    }
    Ok(changed)
}

#[cfg(windows)]
fn update_user_path(
    dir: &Path,
    edit: fn(&str, &str) -> (String, bool),
) -> Result<PathUpdate, UpdateError> {
    use winreg::enums::{RegType, HKEY_CURRENT_USER};
    use winreg::RegKey;
    let root = RegKey::predef(HKEY_CURRENT_USER);
    let (env, _) = root
        .create_subkey("Environment")
        .map_err(|err| UpdateError(format!("打不开用户环境变量：{err}")))?;
    let current = match env.get_raw_value("Path") {
        Ok(value) => value,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => winreg::RegValue {
            vtype: RegType::REG_EXPAND_SZ,
            bytes: Vec::new(),
        },
        Err(err) => return Err(UpdateError(format!("读不了用户 PATH：{err}"))),
    };
    let text = decode_reg_text(&current.bytes);
    let (next, changed) = edit(&text, &dir.display().to_string());
    if changed {
        let kind = if current.vtype == RegType::REG_SZ {
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
    Ok(PathUpdate {
        changed,
        refreshed: broadcast_environment(),
    })
}

/// 告诉资源管理器用户环境变了。不发的话，新开的终端还是登录时那份 PATH。
#[cfg(windows)]
fn broadcast_environment() -> bool {
    // 仓库禁止 unsafe。用 PowerShell 代发 WM_SETTINGCHANGE，和安装器自己写注册表是同一条用户权限。
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Add-Type -Namespace Native -Name Env -MemberDefinition '[DllImport(\"user32.dll\", CharSet=CharSet.Unicode, SetLastError=true)] public static extern System.IntPtr SendMessageTimeout(System.IntPtr hWnd, uint msg, System.UIntPtr wParam, string lParam, uint flags, uint timeout, out System.UIntPtr result);'; $result = [System.UIntPtr]::Zero; $sent = [Native.Env]::SendMessageTimeout([System.IntPtr]0xffff, 0x001A, [System.UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result); if ($sent -eq [System.IntPtr]::Zero) { exit 1 }",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// `true` 表示目录已经不在了。正在运行的程序删不掉自己，等退出后再删。
fn remove_install_dir(dir: &Path) -> Result<bool, UpdateError> {
    if !dir.exists() {
        return Ok(true);
    }
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(true),
        Err(_) if exe_is_inside(dir) => {
            let _ = remove_children_except_self(dir);
            schedule_delete(dir)?;
            Ok(false)
        }
        Err(err) => Err(UpdateError(format!("删不了安装目录：{err}"))),
    }
}

fn exe_is_inside(dir: &Path) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let (Ok(exe), Ok(dir)) = (std::fs::canonicalize(&exe), std::fs::canonicalize(dir)) else {
        return false;
    };
    exe.starts_with(dir)
}

fn remove_children_except_self(dir: &Path) -> Result<(), UpdateError> {
    let exe = std::env::current_exe().ok();
    let entries =
        std::fs::read_dir(dir).map_err(|err| UpdateError(format!("读不了安装目录：{err}")))?;
    for entry in entries {
        let entry = entry.map_err(|err| UpdateError(format!("读不了安装目录：{err}")))?;
        let path = entry.path();
        if exe.as_ref().is_some_and(|exe| same_file(&path, exe)) {
            continue;
        }
        let result = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if let Err(err) = result {
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(UpdateError(format!(
                    "删不了 {}：{err}",
                    entry.file_name().to_string_lossy()
                )));
            }
        }
    }
    Ok(())
}

fn same_file(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(windows)]
fn schedule_delete(dir: &Path) -> Result<(), UpdateError> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script_path =
        std::env::temp_dir().join(format!("r-chuanqi-uninstall-{}.cmd", std::process::id()));
    let script = format!(
        "@echo off\r\nfor /L %%i in (1,1,20) do (\r\n  if not exist \"{dir}\" goto done\r\n  ping 127.0.0.1 -n 2 >nul\r\n  rmdir /s /q \"{dir}\"\r\n)\r\n:done\r\ndel \"%~f0\"\r\n",
        dir = dir.display()
    );
    std::fs::write(&script_path, script)
        .map_err(|err| UpdateError(format!("写不了清理脚本：{err}")))?;
    Command::new("cmd")
        .args([
            "/D",
            "/C",
            "start",
            "",
            "/min",
            "cmd",
            "/D",
            "/C",
            &script_path.display().to_string(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| UpdateError(format!("排不了退出后删除：{err}")))?;
    Ok(())
}

#[cfg(not(windows))]
fn schedule_delete(dir: &Path) -> Result<(), UpdateError> {
    use std::process::{Command, Stdio};
    let quoted = shell_quote(&dir.display().to_string());
    let script = format!(
        "i=0; while [ \"$i\" -lt 20 ]; do if [ ! -e {quoted} ]; then exit 0; fi; sleep 0.3; rm -rf {quoted}; i=$((i+1)); done"
    );
    Command::new("sh")
        .args(["-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| UpdateError(format!("排不了退出后删除：{err}")))?;
    Ok(())
}

#[cfg(not(windows))]
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// 没有就加到末尾。引号、末尾斜杠和大小写不同也算同一条。
pub fn merge_path(current: &str, dir: &str) -> (String, bool) {
    let separator = path_separator();
    let present = current.split(separator).any(|entry| same_dir(entry, dir));
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

/// 只拿掉安装目录。其它条目原样留下。
pub fn remove_path(current: &str, dir: &str) -> (String, bool) {
    let separator = path_separator();
    let mut kept = Vec::new();
    let mut removed = false;
    for entry in current.split(separator) {
        if entry.is_empty() {
            continue;
        }
        if same_dir(entry, dir) {
            removed = true;
        } else {
            kept.push(entry);
        }
    }
    (kept.join(&separator.to_string()), removed)
}

fn path_separator() -> char {
    if cfg!(windows) {
        ';'
    } else {
        ':'
    }
}

fn same_dir(entry: &str, dir: &str) -> bool {
    let left = normalize_dir(entry);
    let right = normalize_dir(dir);
    !left.is_empty() && left.eq_ignore_ascii_case(&right)
}

fn normalize_dir(entry: &str) -> String {
    entry
        .trim()
        .trim_matches('"')
        .trim_end_matches(['/', '\\'])
        .to_string()
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

/// 卸掉带标记的那一行。其它行不动。
pub fn strip_shell_mark(existing: &str) -> String {
    let mut next = String::new();
    for line in existing.lines() {
        if line.contains(SHELL_MARK) {
            continue;
        }
        next.push_str(line);
        next.push('\n');
    }
    if existing.ends_with('\n') || next.is_empty() {
        next
    } else {
        next.pop();
        next
    }
}

#[cfg(any(windows, test))]
fn decode_reg_text(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg(any(windows, test))]
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
        let sep = if cfg!(windows) { ';' } else { ':' };
        let (dir, existing, extra_dir) = if cfg!(windows) {
            (
                r"C:\Users\a\AppData\Local\tui-game",
                r"C:\Windows\system32",
                r"D:\games",
            )
        } else {
            ("/home/a/.local/share/tui-game", "/usr/bin", "/opt/games")
        };
        let (first, added) = merge_path("", dir);
        assert!(added);
        assert_eq!(first, dir);
        let (second, again) = merge_path(&first, &dir.to_ascii_lowercase());
        assert!(!again);
        assert_eq!(second, first);
        let (third, extra) = merge_path(&format!("{existing}{sep}"), extra_dir);
        assert!(extra);
        assert_eq!(third, format!("{existing}{sep}{extra_dir}"));
    }

    #[test]
    fn path_entry_is_removed_once() {
        let sep = if cfg!(windows) { ';' } else { ':' };
        let (dir, other) = if cfg!(windows) {
            (r"C:\Users\a\AppData\Local\tui-game", r"C:\Windows\system32")
        } else {
            ("/home/a/.local/share/tui-game", "/usr/bin")
        };
        let quoted = format!("\"{dir}\\\"");
        let current = format!("{other}{sep}{dir}{sep}{quoted}");
        let (once, removed) = remove_path(&current, dir);
        assert!(removed);
        assert_eq!(once, other);
        let (again, extra) = remove_path(&once, dir);
        assert!(!extra);
        assert_eq!(again, once);
    }

    #[test]
    fn shell_line_is_not_repeated() {
        let line = "export PATH=\"$HOME/.local/bin:$PATH\" # r-chuanqi";
        let once = append_shell_line("export A=1\n", line).unwrap();
        assert!(once.contains(line));
        assert!(append_shell_line(&once, line).is_none());
        let stripped = strip_shell_mark(&once);
        assert_eq!(stripped, "export A=1\n");
        assert!(!stripped.contains(SHELL_MARK));
    }

    #[test]
    fn registry_text_round_trips() {
        let bytes = encode_reg_text(r"%USERPROFILE%\bin;C:\Tools");
        assert_eq!(decode_reg_text(&bytes), r"%USERPROFILE%\bin;C:\Tools");
    }

    #[test]
    fn install_copies_the_game_binary() {
        let root = std::env::temp_dir().join(format!("r-chuanqi-copy-{}", std::process::id()));
        let source = root.join("src");
        let dest = root.join("dst");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(source.join("data")).unwrap();
        let binary = source.join(binary_name());
        std::fs::write(&binary, b"game").unwrap();
        std::fs::write(source.join("data").join("maps.ron"), b"[]").unwrap();
        std::fs::create_dir_all(&dest).unwrap();

        copy_tree(&source, &dest, Path::new("")).unwrap();
        assert_eq!(std::fs::read(dest.join(binary_name())).unwrap(), b"game");
        assert_eq!(
            std::fs::read(dest.join("data").join("maps.ron")).unwrap(),
            b"[]"
        );

        std::fs::write(&binary, b"next").unwrap();
        copy_tree(&source, &dest, &binary).unwrap();
        assert_eq!(std::fs::read(dest.join(binary_name())).unwrap(), b"game");
        let _ = std::fs::remove_dir_all(&root);
    }
}
