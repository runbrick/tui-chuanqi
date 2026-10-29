use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rustc-env=TUI_GAME_VERSION={}", game_version());
    println!("cargo:rerun-if-env-changed=TUI_GAME_VERSION");

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let dest = out.join("payload.bin");
    println!("cargo:rerun-if-env-changed=TUI_GAME_PAYLOAD");
    match std::env::var_os("TUI_GAME_PAYLOAD") {
        Some(source) => {
            let source = payload_source(Path::new(&source));
            println!("cargo:rerun-if-changed={}", source.display());
            std::fs::copy(&source, dest)
                .unwrap_or_else(|err| panic!("复制安装包失败 {}: {err}", source.display()));
        }
        None => {
            std::fs::write(dest, []).expect("写空安装包失败");
        }
    }
}

/// 发布版本跟 git 标签走。优先用 `TUI_GAME_VERSION`（发布流程传入），
/// 其次是当前提交上的标签。都没有才用 Cargo.toml 里的版本。
fn game_version() -> String {
    if let Ok(given) = std::env::var("TUI_GAME_VERSION") {
        let given = normalize_version(&given);
        if !given.is_empty() {
            return given;
        }
    }
    if let Some(tag) = git_tag() {
        return tag;
    }
    std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string())
}

fn normalize_version(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string()
}

/// 只有 HEAD 正好在标签上才算这个版本。标签之后的本地提交仍然是上一个发布版。
fn git_tag() -> Option<String> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").ok()?);
    let output = std::process::Command::new("git")
        .args(["describe", "--tags", "--exact-match", "HEAD"])
        .current_dir(&manifest)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let tag = String::from_utf8(output.stdout).ok()?;
    let tag = normalize_version(&tag);
    if tag.is_empty() {
        None
    } else {
        Some(tag)
    }
}

/// 构建脚本的工作目录是 crate 目录。工作流里的路径相对仓库根，那里才有 `dist/`。
fn payload_source(source: &Path) -> PathBuf {
    if source.is_absolute() || source.exists() {
        return source.to_path_buf();
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|dir| dir.parent())
        .map(|root| root.join(source))
        .filter(|candidate| candidate.exists())
        .unwrap_or_else(|| source.to_path_buf())
}
