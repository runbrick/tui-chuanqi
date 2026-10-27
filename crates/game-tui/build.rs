fn main() {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let dest = out.join("payload.bin");
    println!("cargo:rerun-if-env-changed=TUI_GAME_PAYLOAD");
    match std::env::var_os("TUI_GAME_PAYLOAD") {
        Some(source) => {
            println!("cargo:rerun-if-changed={}", source.to_string_lossy());
            std::fs::copy(source, dest).expect("复制安装包失败");
        }
        None => {
            std::fs::write(dest, []).expect("写空安装包失败");
        }
    }
}
