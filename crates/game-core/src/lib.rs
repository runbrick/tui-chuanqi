//! 纯游戏规则。不读文件、不画终端。
//!
//! `(状态, 静态数据, 指令) → 新状态 + 事件`。RNG 在状态里，同一存档可复现。

mod command;
mod event;
mod save;
mod state;
mod tick;

pub use command::{
    attack_interval_ms, class_label, compact_inventory, compare_equip, map_block, repair_cost,
    repair_service, restock_targets, sale_gold, upgrade_preview, validate_name, Command, CoreError,
    MapBlock, StatDelta,
};
pub use event::{GameEvent, OfflineReport};
pub use save::{
    export_state, import_state, migrate_json, state_from_json, state_to_json, SavedGame,
    SAVE_VERSION,
};
pub use state::{
    ms_to_ticks, skill_rank, Achievement, Automation, GameState, ItemStack, LifetimeStats, PityKey,
    SkillProgress, SLOT_COUNT,
};
pub use tick::main_line;

/// 逻辑 tick 长度。
pub const TICK_MS: u64 = 100;

/// 仓库数据目录（相对本 crate）。
pub fn repo_data_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
}
