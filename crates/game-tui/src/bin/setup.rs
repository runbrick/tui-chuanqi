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
            let message = if uninstalling {
                report.note
            } else {
                format!("安装目录：{}\n\n{}", report.dir.display(), report.note)
            };
            println!("{message}");
            r_chuanqi::install::notify(
                if uninstalling {
                    "已卸载"
                } else {
                    "安装完成"
                },
                &message,
                false,
            );
        }
        Err(err) => {
            let message = err.to_string();
            eprintln!("{message}");
            r_chuanqi::install::notify(
                if uninstalling {
                    "卸载失败"
                } else {
                    "安装失败"
                },
                &message,
                true,
            );
            std::process::exit(1);
        }
    }
}
