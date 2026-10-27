//! tick 产出的事件。UI 负责格式化，核心不拼中文句子以外的展示。

use game_data::{ItemId, MapId, MonsterId, SkillId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 一次逻辑步进的可观察结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GameEvent {
    /// 玩家打中。`rare` 不用在攻击上，保留 crit 字段与架构草图一致，当前公式无暴击所以恒为 false。
    Attacked {
        /// 目标。
        target: MonsterId,
        /// 伤害。
        damage: i32,
        /// 是否暴击。当前恒为 false。
        crit: bool,
    },
    /// 伤害被防御吃光。
    Blocked {
        /// 目标。
        target: MonsterId,
    },
    /// 没命中。
    Missed {
        /// 目标。
        target: MonsterId,
    },
    /// 怪物打中玩家。
    MonsterHit {
        /// 来源。
        source: MonsterId,
        /// 伤害。
        damage: i32,
    },
    /// 玩家抵挡了怪物。
    MonsterBlocked {
        /// 来源。
        source: MonsterId,
    },
    /// 玩家闪避。
    MonsterMissed {
        /// 来源。
        source: MonsterId,
    },
    /// 击杀。
    MonsterSlain {
        /// 怪物。
        id: MonsterId,
        /// 经验。
        exp: u64,
    },
    /// 掉落。保底触发也不额外标记。
    ItemDropped {
        /// 物品。
        id: ItemId,
        /// 数量。
        count: u32,
        /// `chance < 0.01` 时为 true，给日志高亮，不是保底标记。
        rare: bool,
    },
    /// 金币进钱包。
    GoldGained {
        /// 数量。
        amount: u64,
    },
    /// 升级。
    LevelUp {
        /// 新等级。
        level: u16,
    },
    /// 技能升级。
    SkillLevelUp {
        /// 技能。
        id: SkillId,
        /// 新等级。
        level: u8,
    },
    /// 施放技能。
    SkillCast {
        /// 技能。
        id: SkillId,
    },
    /// 玩家死亡。
    PlayerDied {
        /// 损失经验。
        lost_exp: u64,
        /// 高风险模式下掉落的装备。
        dropped_item: Option<ItemId>,
    },
    /// 喝药。
    PotionUsed {
        /// 药水。
        id: ItemId,
    },
    /// 没药且血量过低，收工回城。
    OutOfPotions,
    /// 回到安全区休息。
    Resting {
        /// 休息 tick。
        ticks: u64,
    },
    /// 换图。
    MapChanged {
        /// 新地图。
        map: MapId,
    },
    /// 打不过的 BOSS 被跳过，避免挂机送死。
    BossSkipped {
        /// BOSS。
        id: MonsterId,
    },
    /// 宝宝出手。
    PetAttacked {
        /// 伤害。
        damage: i32,
    },
    /// 宝宝倒下或到期。
    PetGone,
    /// 成就。
    AchievementUnlocked {
        /// 成就 id。
        id: String,
    },
    /// 背包满，物品按卖店价折现。
    SoldOverflow {
        /// 物品。
        id: ItemId,
        /// 金币。
        gold: u64,
    },
    /// 武器升级成功。
    UpgradeSucceeded {
        /// 新的强化等级。
        plus: u8,
    },
    /// 幸运符碎了，武器还在。
    UpgradeProtected,
    /// 武器碎了。
    UpgradeDestroyed,
    /// 学会技能。
    SkillLearned {
        /// 技能。
        id: SkillId,
    },
}

/// 离线推演的聚合，不保留逐条战斗日志。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OfflineReport {
    /// 推进的 tick。
    pub ticks: u64,
    /// 获得经验。
    pub exp: u64,
    /// 钱包增量（不含离线期间不可能发生的购物）。
    pub gold: u64,
    /// 升了几级。
    pub levels_gained: u16,
    /// 死亡次数。
    pub deaths: u32,
    /// 击杀数。
    pub kills: u64,
    /// 掉落计数。
    pub drops: BTreeMap<ItemId, u64>,
}

impl OfflineReport {
    /// 把一条事件折进统计。离线路径用它代替日志缓冲。
    pub fn absorb(&mut self, event: &GameEvent) {
        match event {
            GameEvent::MonsterSlain { .. } => self.kills = self.kills.saturating_add(1),
            GameEvent::ItemDropped { id, count, .. } => {
                *self.drops.entry(*id).or_insert(0) += u64::from(*count);
            }
            GameEvent::GoldGained { amount } => self.gold = self.gold.saturating_add(*amount),
            GameEvent::LevelUp { .. } => self.levels_gained = self.levels_gained.saturating_add(1),
            GameEvent::PlayerDied { .. } => self.deaths = self.deaths.saturating_add(1),
            _ => {}
        }
    }
}
