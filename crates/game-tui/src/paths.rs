//! 数据目录和存档目录。

use directories::ProjectDirs;
use std::path::PathBuf;

/// 按可执行文件、当前目录、编译期仓库路径找 `data/`。
pub fn find_data_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("data"));
        }
    }
    candidates.push(PathBuf::from("data"));
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data"));
    candidates
        .into_iter()
        .find(|path| path.join("balance.ron").is_file())
}

/// `%APPDATA%/tui-mir`，拿不到时退回 `./saves`。
pub fn default_save_root() -> PathBuf {
    ProjectDirs::from("com", "tui", "tui-mir")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("saves"))
}
