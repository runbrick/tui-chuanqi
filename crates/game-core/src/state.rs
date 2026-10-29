//! 玩家状态。派生属性带缓存，换装和升级时失效。

use crate::event::{GameEvent, OfflineReport};
use game_data::{
    Class, ClassBaseline, EquipSlot, GameData, Gender, ItemId, MapId, MonsterId, Range, SkillId,
};
use rand::SeedableRng;
use rand_pcg::Pcg64;
use serde::{Deserialize, Serialize};
use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};

/// 装备槽数量。
pub const SLOT_COUNT: usize = EquipSlot::COUNT;

/// 保底计数的键。存档里写成 slug，内存里用 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PityKey {
    /// 怪物。
    pub monster: MonsterId,
    /// 物品。
    pub item: ItemId,
}

/// 背包或装备槽里的一叠物品。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStack {
    /// 物品。
    pub item: ItemId,
    /// 数量。装备恒为 1。
    pub count: u32,
    /// 当前耐久。非装备为 0。
    pub durability: u32,
    /// 武器强化等级。
    pub upgrade: u8,
    /// 高风险死亡时锁定则不掉。
    pub locked: bool,
    /// 黑铁纯度。仅矿石。
    pub purity: Option<i32>,
}

impl ItemStack {
    /// 按定义生成一叠新物品。矿石纯度在这里 roll。
    pub fn fresh(item: ItemId, count: u32, data: &GameData, rng: &mut Pcg64) -> Self {
        let def = data.item(item);
        let (durability, purity) = match &def.kind {
            game_data::ItemKind::Weapon(g)
            | game_data::ItemKind::Armor(g)
            | game_data::ItemKind::Helmet(g)
            | game_data::ItemKind::Accessory(g) => (g.durability, None),
            game_data::ItemKind::Material(material) => (0, Some(material.purity.roll(rng))),
            _ => (0, None),
        };
        Self {
            item,
            count,
            durability,
            upgrade: 0,
            locked: false,
            purity,
        }
    }

    /// 耐久归零的装备不提供属性。
    pub fn is_broken(&self, data: &GameData) -> bool {
        let Some(gear) = data.item(self.item).kind.gear() else {
            return false;
        };
        gear.durability > 0 && self.durability == 0
    }
}

/// 已学技能。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillProgress {
    /// 当前等级，从 1 起。
    pub level: u8,
    /// 熟练度。
    pub exp: u32,
    /// 挂机时是否自动放。
    pub auto: bool,
}

/// 累计统计，给统计面板。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifetimeStats {
    /// 总击杀。
    pub kills: u64,
    /// 总获得金币（不含花费）。
    pub gold_gained: u64,
    /// 总获得经验。
    pub exp_gained: u64,
    /// 死亡次数。
    pub deaths: u32,
    /// 掉落次数（按条目，不是数量）。
    pub drop_events: u64,
    /// 各怪物击杀。
    pub monster_kills: BTreeMap<MonsterId, u64>,
}

/// 自动化档位。阈值存在角色上（ADR-007）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Automation {
    /// 自动喝药。
    pub auto_potion: bool,
    /// 生命阈值 0..1。
    pub hp_ratio: f64,
    /// 魔法阈值 0..1。
    pub mp_ratio: f64,
    /// 自动换图。未解锁时指令会拒绝打开。
    pub auto_advance: bool,
    /// 自动卖掉白色垃圾装备。
    pub auto_sell_junk: bool,
}

impl Default for Automation {
    fn default() -> Self {
        Self {
            auto_potion: true,
            hp_ratio: 0.5,
            mp_ratio: 0.3,
            auto_advance: false,
            auto_sell_junk: false,
        }
    }
}

/// 成就 id。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Achievement {
    /// 第一次击杀。
    FirstKill,
    /// 击杀 100。
    Kills100,
    /// 击杀 1000。
    Kills1000,
    /// 到达 10 级。
    Level10,
    /// 到达 30 级。
    Level30,
    /// 第一次拿到绿色或更好的装备。
    FirstGreen,
    /// 第一次击杀 BOSS。
    FirstBoss,
}

impl Achievement {
    /// 中文名。
    pub fn label(self) -> &'static str {
        match self {
            Self::FirstKill => "初战",
            Self::Kills100 => "百人斩",
            Self::Kills1000 => "千人斩",
            Self::Level10 => "小有所成",
            Self::Level30 => "驰名玛法",
            Self::FirstGreen => "第一件绿装",
            Self::FirstBoss => "屠龙预备役",
        }
    }
}

/// 当前战斗。
#[derive(Debug, Clone, PartialEq)]
pub struct Combat {
    /// 怪物种类。
    pub monster: MonsterId,
    /// 是否 BOSS。
    pub boss: bool,
    /// 剩余生命。
    pub hp: i32,
    /// 玩家下次普攻 tick。
    pub next_player_attack_at: u64,
    /// 怪物下次出手 tick。
    pub next_monster_attack_at: u64,
    /// 技能冷却到期 tick。
    pub skill_ready_at: BTreeMap<SkillId, u64>,
    /// 烈火等“下一击”加成，打完清掉。
    pub next_attack_power: Option<(game_data::DamageSource, f32)>,
    /// 怪物中毒剩余 tick。
    pub poison_left: u32,
    /// 每跳（10 tick）毒伤。
    pub poison_dps: i32,
    /// 怪物防御降低比例，0..1。
    pub defense_down: f32,
    /// 降防剩余 tick。
    pub defense_down_left: u32,
}

/// 道士宝宝。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pet {
    /// 来源技能，重召时替换。
    pub skill: SkillId,
    /// 剩余生命。
    pub hp: i32,
    /// 最大生命。
    pub max_hp: i32,
    /// 攻击。
    pub attack: Range,
    /// 攻击间隔 tick。
    pub interval: u64,
    /// 下次出手。
    pub next_attack_at: u64,
    /// 消失 tick。
    pub expires_at: u64,
}

/// 换装后汇总的面板属性。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedStats {
    /// 最大生命。
    pub max_hp: i32,
    /// 最大魔法。
    pub max_mp: i32,
    /// 攻击。
    pub attack: Range,
    /// 魔法。
    pub magic: Range,
    /// 道术。
    pub taoism: Range,
    /// 防御。
    pub defense: Range,
    /// 魔防。
    pub magic_defense: Range,
    /// 准确。
    pub accuracy: i32,
    /// 敏捷。
    pub agility: i32,
    /// 普攻间隔 tick。
    pub attack_interval: u64,
}

/// 全部可变状态。
#[derive(Debug, Clone)]
pub struct GameState {
    /// 存档版本。
    pub version: u32,
    /// 角色名。允许重名，身份是槽位而不是名字。
    pub name: String,
    /// 职业。创建后不可改。
    pub class: Class,
    /// 性别。创建后不可改。
    pub gender: Gender,
    /// 高风险模式。创建后不可改。
    pub high_risk: bool,
    /// 等级。
    pub level: u16,
    /// 当前等级经验。
    pub exp: u64,
    /// 当前生命。
    pub hp: i32,
    /// 当前魔法。
    pub mp: i32,
    /// 金币。
    pub gold: u64,
    /// 等级裸属性，不含装备。
    pub base: BaseStats,
    /// 装备。下标见 [`EquipSlot::index`]。
    pub equipment: [Option<ItemStack>; SLOT_COUNT],
    /// 背包。
    pub inventory: Vec<ItemStack>,
    /// 仓库。
    pub storage: Vec<ItemStack>,
    /// 已学技能。
    pub skills: BTreeMap<SkillId, SkillProgress>,
    /// 当前地图。
    pub current_map: MapId,
    /// 各地图击杀数，用于解锁。
    pub map_kills: BTreeMap<MapId, u64>,
    /// 进行中的战斗。
    pub combat: Option<Combat>,
    /// 战斗间隙保留的召唤物。击杀后不立刻消失。
    pub pet: Option<Pet>,
    /// 终身统计。
    pub stats: LifetimeStats,
    /// 稀有掉落保底。
    pub pity: BTreeMap<PityKey, u32>,
    /// 已经过的逻辑 tick。
    pub tick_count: u64,
    /// 休息到这个 tick（含）之前不战斗。
    pub resting_until: u64,
    /// 持续回复的剩余生命。
    pub hot_hp: i32,
    /// 每 tick 回血。
    pub hot_hp_per: i32,
    /// 持续回复的剩余魔法。
    pub hot_mp: i32,
    /// 每 tick 回蓝。
    pub hot_mp_per: i32,
    /// 魔法盾剩余 tick。
    pub shield_left: u32,
    /// 魔法盾减伤。
    pub shield_reduction: f32,
    /// 幽灵盾等额外敏捷剩余。
    pub buff_agility: i32,
    /// 额外敏捷剩余 tick。
    pub buff_left: u32,
    /// 同一 BOSS 连续被打死次数。达到 2 次就暂时不招它。
    pub boss_wipes: BTreeMap<MonsterId, u8>,
    /// BOSS 下次允许出现的 tick。
    pub boss_ready_at: BTreeMap<MonsterId, u64>,
    /// 自动化设置。
    pub automation: Automation,
    /// 成就。
    pub achievements: BTreeSet<Achievement>,
    /// 下次刷怪 tick。
    pub next_spawn_at: u64,
    /// 死亡或没药回城前所在的地图。休息完自动回去；玩家手动换图就清掉。
    pub return_map: Option<MapId>,
    /// 升级前连续死了几次。连死太多就退回上一张图，升级或手动换图清零。
    pub death_streak: u8,
    /// 确定性随机数。
    pub rng: Pcg64,
    pub(crate) derived: OnceCell<DerivedStats>,
}

/// 裸属性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseStats {
    /// 最大生命。
    pub max_hp: i32,
    /// 最大魔法。
    pub max_mp: i32,
    /// 攻击。
    pub attack: Range,
    /// 魔法。
    pub magic: Range,
    /// 道术。
    pub taoism: Range,
    /// 防御。
    pub defense: Range,
    /// 魔防。
    pub magic_defense: Range,
    /// 准确。
    pub accuracy: i32,
    /// 敏捷。
    pub agility: i32,
    /// 普攻间隔毫秒。
    pub attack_interval_ms: u32,
}

impl GameState {
    /// 新建角色。职业、性别、高风险之后不能改。
    pub fn new(
        name: impl Into<String>,
        class: Class,
        gender: Gender,
        high_risk: bool,
        data: &GameData,
        seed: u64,
    ) -> Self {
        let base = base_at_level(data.balance.classes.get(class), 1);
        let map = data
            .starting_map()
            .or_else(|| data.safe_map())
            .unwrap_or(game_data::MapId(0));
        let automation = Automation {
            hp_ratio: data.balance.potion_hp_ratio,
            mp_ratio: data.balance.potion_mp_ratio,
            ..Automation::default()
        };
        let mut state = Self {
            version: crate::SAVE_VERSION,
            name: name.into(),
            class,
            gender,
            high_risk,
            level: 1,
            exp: 0,
            hp: base.max_hp,
            mp: base.max_mp,
            gold: data.balance.starting_gold,
            base,
            equipment: std::array::from_fn(|_| None),
            inventory: Vec::new(),
            storage: Vec::new(),
            skills: BTreeMap::new(),
            current_map: map,
            map_kills: BTreeMap::new(),
            combat: None,
            pet: None,
            stats: LifetimeStats::default(),
            pity: BTreeMap::new(),
            tick_count: 0,
            resting_until: 0,
            hot_hp: 0,
            hot_hp_per: 0,
            hot_mp: 0,
            hot_mp_per: 0,
            shield_left: 0,
            shield_reduction: 0.0,
            buff_agility: 0,
            buff_left: 0,
            boss_wipes: BTreeMap::new(),
            boss_ready_at: BTreeMap::new(),
            automation,
            achievements: BTreeSet::new(),
            next_spawn_at: 0,
            return_map: None,
            death_streak: 0,
            rng: Pcg64::seed_from_u64(seed),
            derived: OnceCell::new(),
        };
        state.give_starting_items(data);
        state.clamp_vitals(data);
        state.hp = state.derived_stats(data).max_hp;
        state.mp = state.derived_stats(data).max_mp;
        state
    }

    /// 新手装。装备能穿就穿，性别或职业不符的直接跳过，所以男女款可以都写上。
    fn give_starting_items(&mut self, data: &GameData) {
        for start in &data.balance.starting_items {
            let Some(item) = data.item_id(&start.item) else {
                continue;
            };
            let def = data.item(item);
            if let Some(gear) = def.kind.gear() {
                let fits = self.level >= gear.req_level
                    && gear.req_class.allows(self.class)
                    && (!def.kind.is_armor() || gear.gender.allows(self.gender));
                let slot = gear.slot.index();
                if fits && self.equipment[slot].is_none() {
                    self.equipment[slot] = Some(ItemStack::fresh(item, 1, data, &mut self.rng));
                    self.invalidate_derived();
                }
                continue;
            }
            if start.count == 0 {
                continue;
            }
            if let Some(stack) = self
                .inventory
                .iter_mut()
                .find(|stack| def.stackable && stack.item == item)
            {
                stack.count = stack.count.saturating_add(start.count);
            } else {
                let stack = ItemStack::fresh(item, start.count, data, &mut self.rng);
                self.inventory.push(stack);
            }
        }
    }

    /// 从已迁移的存档结构恢复。未知 slug 会返回错误。
    pub fn from_save(saved: crate::save::SavedGame, data: &GameData) -> Result<Self, String> {
        crate::save::import_owned(saved, data)
    }

    /// 派生属性。缓存命中则不重算。
    pub fn derived_stats(&self, data: &GameData) -> &DerivedStats {
        self.derived.get_or_init(|| compute_derived(self, data))
    }

    /// 换装、升级、增益变化后调用。漏调会导致面板读到旧属性。
    pub fn invalidate_derived(&mut self) {
        self.derived.take();
    }

    /// 把生命魔法夹进当前上限。
    pub fn clamp_vitals(&mut self, data: &GameData) {
        let derived = *self.derived_stats(data);
        self.hp = self.hp.clamp(0, derived.max_hp.max(1));
        self.mp = self.mp.clamp(0, derived.max_mp.max(0));
    }

    /// 推进一个逻辑 tick。
    pub fn tick(&mut self, data: &GameData) -> Vec<GameEvent> {
        let mut events = Vec::new();
        self.tick_into(data, &mut events);
        events
    }

    /// 把事件写进任意容器。离线推演传 [`OfflineReport`]，避免攒日志。
    pub fn tick_into(&mut self, data: &GameData, sink: &mut impl EventSink) {
        crate::tick::step(self, data, sink);
    }

    /// 离线推演。和在线是同一套 tick，只是不保留逐条日志。
    pub fn simulate_offline(
        &mut self,
        data: &GameData,
        elapsed: std::time::Duration,
    ) -> OfflineReport {
        let cap = u64::from(data.balance.offline_cap_hours) * 3600 * 1000;
        let ms = elapsed.as_millis().min(u128::from(cap)) as u64;
        let ticks = ms / crate::TICK_MS;
        let start_level = self.level;
        let start_exp_total = self.total_exp_snapshot(data);
        let start_gold = self.gold;
        let start_deaths = self.stats.deaths;
        let start_kills = self.stats.kills;
        let mut report = OfflineReport {
            ticks,
            ..OfflineReport::default()
        };
        for _ in 0..ticks {
            self.tick_into(data, &mut report);
        }
        report.exp = self
            .total_exp_snapshot(data)
            .saturating_sub(start_exp_total);
        report.gold = self.gold.saturating_sub(start_gold);
        report.levels_gained = self.level.saturating_sub(start_level);
        report.deaths = self.stats.deaths.saturating_sub(start_deaths);
        report.kills = self.stats.kills.saturating_sub(start_kills);
        report
    }

    fn total_exp_snapshot(&self, data: &GameData) -> u64 {
        let mut total = self.exp;
        for level in 1..self.level {
            total = total.saturating_add(data.balance.exp_to_next(level));
        }
        total
    }

    /// 固定种子的调试摘要，给快照测试。
    pub fn summary(&self, data: &GameData) -> String {
        let d = self.derived_stats(data);
        format!(
            "name={} class={:?} lv={} exp={} hp={}/{} mp={}/{} atk={}-{} def={}-{} gold={} kills={} map={}",
            self.name,
            self.class,
            self.level,
            self.exp,
            self.hp,
            d.max_hp,
            self.mp,
            d.max_mp,
            d.attack.min,
            d.attack.max,
            d.defense.min,
            d.defense.max,
            self.gold,
            self.stats.kills,
            data.map(self.current_map).id,
        )
    }
}

/// 接收 tick 事件。
pub trait EventSink {
    /// 收下一条事件。
    fn push_event(&mut self, event: GameEvent);
}

impl EventSink for Vec<GameEvent> {
    fn push_event(&mut self, event: GameEvent) {
        self.push(event);
    }
}

impl EventSink for OfflineReport {
    fn push_event(&mut self, event: GameEvent) {
        self.absorb(&event);
    }
}

/// 毫秒换成 tick，至少 1。
pub fn ms_to_ticks(ms: u32) -> u64 {
    (u64::from(ms) / crate::TICK_MS).max(1)
}

/// 技能实际生效的等级下标。熟练度升上去但角色等级还没到那一档的要求时，
/// 按能用的最高一档算，而不是整个技能都放不出来。连第一档都不够就返回 `None`。
pub fn skill_rank(
    def: &game_data::SkillDef,
    progress: &SkillProgress,
    level: u16,
) -> Option<usize> {
    let trained = (progress.level.max(1) as usize).min(def.levels.len());
    def.levels[..trained]
        .iter()
        .rposition(|tier| tier.req_level <= level)
}

fn base_at_level(row: &ClassBaseline, level: u16) -> BaseStats {
    let steps = i32::from(level.saturating_sub(1));
    let grow = |base: Range, per: Range| {
        Range::new(base.min + per.min * steps, base.max + per.max * steps)
    };
    BaseStats {
        max_hp: row.hp + row.growth.hp * steps,
        max_mp: row.mp + row.growth.mp * steps,
        attack: grow(row.attack, row.growth.attack),
        magic: grow(row.magic, row.growth.magic),
        taoism: grow(row.taoism, row.growth.taoism),
        defense: grow(row.defense, row.growth.defense),
        magic_defense: grow(row.magic_defense, row.growth.magic_defense),
        accuracy: row.accuracy + row.growth.accuracy * steps,
        agility: row.agility + row.growth.agility * steps,
        attack_interval_ms: row.attack_interval_ms,
    }
}

pub(crate) fn recompute_base(state: &mut GameState, data: &GameData) {
    state.base = base_at_level(data.balance.classes.get(state.class), state.level);
    state.invalidate_derived();
}

fn compute_derived(state: &GameState, data: &GameData) -> DerivedStats {
    let mut attack = state.base.attack;
    let mut magic = state.base.magic;
    let mut taoism = state.base.taoism;
    let mut defense = state.base.defense;
    let mut magic_defense = state.base.magic_defense;
    let mut accuracy = state.base.accuracy;
    let mut agility = state.base.agility;
    let mut max_hp = state.base.max_hp;
    let mut max_mp = state.base.max_mp;
    for stack in state.equipment.iter().flatten() {
        if stack.is_broken(data) {
            continue;
        }
        let Some(gear) = data.item(stack.item).kind.gear() else {
            continue;
        };
        attack.min += gear.attack.min;
        attack.max += gear.attack.max + i32::from(stack.upgrade);
        magic.min += gear.magic.min;
        magic.max += gear.magic.max;
        taoism.min += gear.taoism.min;
        taoism.max += gear.taoism.max;
        defense.min += gear.defense.min;
        defense.max += gear.defense.max;
        magic_defense.min += gear.magic_defense.min;
        magic_defense.max += gear.magic_defense.max;
        accuracy += gear.accuracy;
        agility += gear.agility;
        max_hp += gear.hp;
        max_mp += gear.mp;
    }
    for (id, progress) in &state.skills {
        let def = data.skill(*id);
        if let game_data::SkillEffect::Passive(passive) = &def.effect {
            if let Some(rank) = skill_rank(def, progress, state.level) {
                accuracy += passive.accuracy * (rank as i32 + 1);
            }
        }
    }
    if state.buff_left > 0 {
        agility += state.buff_agility;
    }
    DerivedStats {
        max_hp: max_hp.max(1),
        max_mp: max_mp.max(0),
        attack,
        magic,
        taoism,
        defense,
        magic_defense,
        accuracy,
        agility,
        attack_interval: ms_to_ticks(state.base.attack_interval_ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_data::GameData;

    fn data() -> GameData {
        GameData::load(crate::repo_data_dir()).expect("data")
    }

    #[test]
    fn weapon_changes_attack_and_unequip_reverts() {
        let data = data();
        let mut state = GameState::new("测", Class::Warrior, Gender::Male, false, &data, 1);
        state.inventory.clear();
        state.equipment = std::array::from_fn(|_| None);
        state.invalidate_derived();
        let before = state.derived_stats(&data).attack;
        let sword = data.item_id("wooden_sword").unwrap();
        state
            .inventory
            .push(ItemStack::fresh(sword, 1, &data, &mut state.rng));
        state.derived_stats(&data);
        crate::Command::Equip { index: 0 }
            .apply(&mut state, &data)
            .unwrap();
        let worn = state.derived_stats(&data).attack;
        assert_eq!(worn.min, before.min + 2);
        assert_eq!(worn.max, before.max + 5);
        crate::Command::Unequip {
            slot: EquipSlot::Weapon,
        }
        .apply(&mut state, &data)
        .unwrap();
        let after = state.derived_stats(&data).attack;
        assert_eq!(after, before);
    }
}
