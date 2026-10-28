//! 安装入口。把旁边打进去的游戏文件装到用户目录，并接上 `r-chuanqi` 命令。

fn main() {
    let uninstalling = std::env::args()
        .skip(1)
        .any(|arg| arg == "uninstall" || arg == "--uninstall");
    let result = if uninstalling {
        r_chuanqi::install::uninstall()
    } else {
        r_chuanqi::install::install_embedded(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/payload.bin"
        )))
    };
    match result {
        Ok(report) => {
            if !uninstalling {
                println!("装到 {}", report.dir.display());
            }
            println!("{}", report.note);
        }
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
