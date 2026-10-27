use std::path::{Path, PathBuf};

fn main() {
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
