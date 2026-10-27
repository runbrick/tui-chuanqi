//! 终端前端、存档和文本工具。规则都在 `game-core`。

pub mod app;
pub mod logfmt;
pub mod paths;
pub mod saveio;
pub mod theme;
pub mod ui;
pub mod update;
pub mod util;

pub use paths::{default_save_root, find_data_dir};
pub use saveio::SaveStore;
