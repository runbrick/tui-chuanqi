//! 安装入口。把旁边打进去的游戏文件装到用户目录，并接上 `r-chuanqi` 命令。

fn main() {
    match r_chuanqi::install::install_embedded(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/payload.bin"
    ))) {
        Ok(report) => {
            println!("装到 {}", report.dir.display());
            println!("{}", report.note);
        }
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
