//! 游戏入口。终端的开关和 panic 恢复都在这里。

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use game_data::GameData;
use game_tui::{app::App, default_save_root, find_data_dir};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, stdout};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let data_dir = find_data_dir()
        .ok_or("找不到 data 目录。请在仓库根目录运行，或把 data 放在可执行文件旁边。")?;
    let data = GameData::load(&data_dir).map_err(|err| err.to_string())?;
    init_tracing();
    let store = game_tui::SaveStore::open(default_save_root())?;
    let app = App::open(data, store)?;
    let mut terminal = setup_terminal().map_err(|err| err.to_string())?;
    install_panic_hook();
    let result = app.run(&mut terminal);
    restore_terminal(&mut terminal);
    result
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    Terminal::new(backend)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) {
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();
}

fn install_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
        hook(info);
    }));
}

fn init_tracing() {
    let dir = default_save_root().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let file = tracing_appender::rolling::never(dir, "game.log");
    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(file)
        .with_max_level(tracing::Level::INFO)
        .try_init();
}
