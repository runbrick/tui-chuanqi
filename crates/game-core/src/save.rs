//! 存档 DTO。磁盘上记 slug，不记会随数据表插入而漂移的下标。

use crate::state::{
    Achievement, Automation, BaseStats, Combat, GameState, ItemStack, LifetimeStats, Pet,
    SkillProgress, SLOT_COUNT,
};
use game_data::{GameData, MapId, MonsterId, SkillId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// 当前存档格式。迁移链从 1 开始，目前没有旧档。
pub const SAVE_VERSION: u32 = 1;

/// 可序列化的完整角色。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedGame {
    /// 格式版本。
    pub version: u32,
    /// 名字。
    pub name: String,
    /// 职业 slug：warrior / mage / taoist。
    pub class: String,
    /// male / female。
    pub gender: String,
    /// 高风险。
    pub high_risk: bool,
    /// 等级。
    pub level: u16,
    /// 经验。
    pub exp: u64,
    /// 生命。
    pub hp: i32,
    /// 魔法。
    pub mp: i32,
    /// 金币。
    pub gold: u64,
    /// 裸属性。
    pub base: BaseStats,
    /// 装备 slug，空槽为 None。
    pub equipment: Vec<Option<SavedStack>>,
    /// 背包。
    pub inventory: Vec<SavedStack>,
    /// 仓库。
    pub storage: Vec<SavedStack>,
    /// 技能进度，键是 slug。
    pub skills: BTreeMap<String, SkillProgress>,
    /// 当前地图 slug。
    pub current_map: String,
    /// 地图击杀。
    pub map_kills: BTreeMap<String, u64>,
    /// 战斗。
    pub combat: Option<SavedCombat>,
    /// 宝宝。
    pub pet: Option<SavedPet>,
    /// 统计。怪物键是 slug。
    pub kills: u64,
    /// 累计金币。
    pub gold_gained: u64,
    /// 累计经验。
    pub exp_gained: u64,
    /// 死亡。
    pub deaths: u32,
    /// 掉落次数。
    pub drop_events: u64,
    /// 分怪击杀。
    pub monster_kills: BTreeMap<String, u64>,
    /// 保底，键是 `monster|item`。
    pub pity: BTreeMap<String, u32>,
    /// tick。
    pub tick_count: u64,
    /// 休息到。
    pub resting_until: u64,
    /// 持续红。
    pub hot_hp: i32,
    /// 每 tick 红。
    pub hot_hp_per: i32,
    /// 持续蓝。
    pub hot_mp: i32,
    /// 每 tick 蓝。
    pub hot_mp_per: i32,
    /// 魔法盾。
    pub shield_left: u32,
    /// 减伤。
    pub shield_reduction: f32,
    /// 敏捷增益。
    pub buff_agility: i32,
    /// 增益剩余。
    pub buff_left: u32,
    /// BOSS 连跪。
    pub boss_wipes: BTreeMap<String, u8>,
    /// BOSS 可再次出现。
    pub boss_ready_at: BTreeMap<String, u64>,
    /// 自动化。
    pub automation: Automation,
    /// 成就名。
    pub achievements: BTreeSet<String>,
    /// 下次刷怪。
    pub next_spawn_at: u64,
    /// 休息完要回的地图 slug。旧档没有这个字段。
    #[serde(default)]
    pub return_map: Option<String>,
    /// 连续死亡次数。
    #[serde(default)]
    pub death_streak: u8,
    /// PCG64 的 RON 文本。JSON 数字装不下内部 128 位状态。
    pub rng: String,
}

/// 存档里的一叠物品。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedStack {
    /// 物品 slug。
    pub item: String,
    /// 数量。
    pub count: u32,
    /// 耐久。
    pub durability: u32,
    /// 强化。
    pub upgrade: u8,
    /// 锁定。
    pub locked: bool,
    /// 纯度。
    pub purity: Option<i32>,
}

/// 存档里的战斗。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCombat {
    /// 怪物 slug。
    pub monster: String,
    /// 是否 BOSS。
    pub boss: bool,
    /// 剩余生命。
    pub hp: i32,
    /// 玩家下次出手。
    pub next_player_attack_at: u64,
    /// 怪物下次出手。
    pub next_monster_attack_at: u64,
    /// 技能冷却，键是 slug。
    pub skill_ready_at: BTreeMap<String, u64>,
    /// 毒剩余。
    pub poison_left: u32,
    /// 毒伤。
    pub poison_dps: i32,
    /// 降防。
    pub defense_down: f32,
    /// 降防剩余。
    pub defense_down_left: u32,
}

/// 存档里的宝宝。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPet {
    /// 技能 slug。
    pub skill: String,
    /// 生命。
    pub hp: i32,
    /// 上限。
    pub max_hp: i32,
    /// 攻击下。
    pub attack_min: i32,
    /// 攻击上。
    pub attack_max: i32,
    /// 间隔。
    pub interval: u64,
    /// 下次出手。
    pub next_attack_at: u64,
    /// 到期。
    pub expires_at: u64,
}

/// 把内存状态导出成可写入 JSON 的结构。
pub fn export_state(state: &GameState, data: &GameData) -> SavedGame {
    let stack = |s: &ItemStack| SavedStack {
        item: data.item(s.item).id.clone(),
        count: s.count,
        durability: s.durability,
        upgrade: s.upgrade,
        locked: s.locked,
        purity: s.purity,
    };
    SavedGame {
        version: SAVE_VERSION,
        name: state.name.clone(),
        class: match state.class {
            game_data::Class::Warrior => "warrior",
            game_data::Class::Mage => "mage",
            game_data::Class::Taoist => "taoist",
        }
        .to_string(),
        gender: match state.gender {
            game_data::Gender::Male => "male",
            game_data::Gender::Female => "female",
        }
        .to_string(),
        high_risk: state.high_risk,
        level: state.level,
        exp: state.exp,
        hp: state.hp,
        mp: state.mp,
        gold: state.gold,
        base: state.base,
        equipment: state
            .equipment
            .iter()
            .map(|slot| slot.as_ref().map(stack))
            .collect(),
        inventory: state.inventory.iter().map(stack).collect(),
        storage: state.storage.iter().map(stack).collect(),
        skills: state
            .skills
            .iter()
            .map(|(id, progress)| (data.skill(*id).id.clone(), progress.clone()))
            .collect(),
        current_map: data.map(state.current_map).id.clone(),
        map_kills: state
            .map_kills
            .iter()
            .map(|(id, n)| (data.map(*id).id.clone(), *n))
            .collect(),
        combat: state.combat.as_ref().map(|combat| SavedCombat {
            monster: data.monster(combat.monster).id.clone(),
            boss: combat.boss,
            hp: combat.hp,
            next_player_attack_at: combat.next_player_attack_at,
            next_monster_attack_at: combat.next_monster_attack_at,
            skill_ready_at: combat
                .skill_ready_at
                .iter()
                .map(|(id, tick)| (data.skill(*id).id.clone(), *tick))
                .collect(),
            poison_left: combat.poison_left,
            poison_dps: combat.poison_dps,
            defense_down: combat.defense_down,
            defense_down_left: combat.defense_down_left,
        }),
        pet: state.pet.as_ref().map(|pet| SavedPet {
            skill: data.skill(pet.skill).id.clone(),
            hp: pet.hp,
            max_hp: pet.max_hp,
            attack_min: pet.attack.min,
            attack_max: pet.attack.max,
            interval: pet.interval,
            next_attack_at: pet.next_attack_at,
            expires_at: pet.expires_at,
        }),
        kills: state.stats.kills,
        gold_gained: state.stats.gold_gained,
        exp_gained: state.stats.exp_gained,
        deaths: state.stats.deaths,
        drop_events: state.stats.drop_events,
        monster_kills: state
            .stats
            .monster_kills
            .iter()
            .map(|(id, n)| (data.monster(*id).id.clone(), *n))
            .collect(),
        pity: state
            .pity
            .iter()
            .map(|(key, n)| {
                (
                    format!(
                        "{}|{}",
                        data.monster(key.monster).id,
                        data.item(key.item).id
                    ),
                    *n,
                )
            })
            .collect(),
        tick_count: state.tick_count,
        resting_until: state.resting_until,
        hot_hp: state.hot_hp,
        hot_hp_per: state.hot_hp_per,
        hot_mp: state.hot_mp,
        hot_mp_per: state.hot_mp_per,
        shield_left: state.shield_left,
        shield_reduction: state.shield_reduction,
        buff_agility: state.buff_agility,
        buff_left: state.buff_left,
        boss_wipes: state
            .boss_wipes
            .iter()
            .map(|(id, n)| (data.monster(*id).id.clone(), *n))
            .collect(),
        boss_ready_at: state
            .boss_ready_at
            .iter()
            .map(|(id, n)| (data.monster(*id).id.clone(), *n))
            .collect(),
        automation: state.automation.clone(),
        achievements: state
            .achievements
            .iter()
            .map(|id| format!("{id:?}"))
            .collect(),
        next_spawn_at: state.next_spawn_at,
        return_map: state.return_map.map(|id| data.map(id).id.clone()),
        death_streak: state.death_streak,
        rng: hex_encode(&bincode::serialize(&state.rng).unwrap_or_default()),
    }
}

/// 从存档恢复。引用了已删除的 slug 时返回错误文本。
pub fn import_state(saved: SavedGame, data: &GameData) -> Result<GameState, String> {
    import_owned(saved, data)
}

pub(crate) fn import_owned(saved: SavedGame, data: &GameData) -> Result<GameState, String> {
    let class = match saved.class.as_str() {
        "warrior" => game_data::Class::Warrior,
        "mage" => game_data::Class::Mage,
        "taoist" => game_data::Class::Taoist,
        other => return Err(format!("未知职业 {other}")),
    };
    let gender = match saved.gender.as_str() {
        "male" => game_data::Gender::Male,
        "female" => game_data::Gender::Female,
        other => return Err(format!("未知性别 {other}")),
    };
    let stack = |s: SavedStack| -> Result<ItemStack, String> {
        let item = data
            .item_id(&s.item)
            .ok_or_else(|| format!("存档引用了不存在的物品 {}", s.item))?;
        Ok(ItemStack {
            item,
            count: s.count,
            durability: s.durability,
            upgrade: s.upgrade,
            locked: s.locked,
            purity: s.purity,
        })
    };
    let mut equipment: [Option<ItemStack>; SLOT_COUNT] = std::array::from_fn(|_| None);
    for (index, slot) in saved.equipment.into_iter().take(SLOT_COUNT).enumerate() {
        equipment[index] = slot.map(stack).transpose()?;
    }
    let monster = |slug: &str| {
        data.monster_id(slug)
            .ok_or_else(|| format!("存档引用了不存在的怪物 {slug}"))
    };
    let map = |slug: &str| {
        data.map_id(slug)
            .ok_or_else(|| format!("存档引用了不存在的地图 {slug}"))
    };
    let skill = |slug: &str| {
        data.skill_id(slug)
            .ok_or_else(|| format!("存档引用了不存在的技能 {slug}"))
    };
    let mut skills = BTreeMap::<SkillId, SkillProgress>::new();
    for (slug, progress) in saved.skills {
        skills.insert(skill(&slug)?, progress);
    }
    let mut map_kills = BTreeMap::<MapId, u64>::new();
    for (slug, n) in saved.map_kills {
        map_kills.insert(map(&slug)?, n);
    }
    let mut monster_kills = BTreeMap::<MonsterId, u64>::new();
    for (slug, n) in saved.monster_kills {
        monster_kills.insert(monster(&slug)?, n);
    }
    let mut pity = BTreeMap::new();
    for (key, n) in saved.pity {
        let Some((monster_slug, item_slug)) = key.split_once('|') else {
            continue;
        };
        pity.insert(
            crate::state::PityKey {
                monster: monster(monster_slug)?,
                item: data
                    .item_id(item_slug)
                    .ok_or_else(|| format!("存档引用了不存在的物品 {item_slug}"))?,
            },
            n,
        );
    }
    let mut boss_wipes = BTreeMap::<MonsterId, u8>::new();
    for (slug, n) in saved.boss_wipes {
        boss_wipes.insert(monster(&slug)?, n);
    }
    let mut boss_ready_at = BTreeMap::<MonsterId, u64>::new();
    for (slug, n) in saved.boss_ready_at {
        boss_ready_at.insert(monster(&slug)?, n);
    }
    let combat = saved
        .combat
        .map(|combat| {
            let mut skill_ready_at = BTreeMap::<SkillId, u64>::new();
            for (slug, tick) in combat.skill_ready_at {
                skill_ready_at.insert(skill(&slug)?, tick);
            }
            Ok::<Combat, String>(Combat {
                monster: monster(&combat.monster)?,
                boss: combat.boss,
                hp: combat.hp,
                next_player_attack_at: combat.next_player_attack_at,
                next_monster_attack_at: combat.next_monster_attack_at,
                skill_ready_at,
                next_attack_power: None,
                poison_left: combat.poison_left,
                poison_dps: combat.poison_dps,
                defense_down: combat.defense_down,
                defense_down_left: combat.defense_down_left,
            })
        })
        .transpose()?;
    let pet = saved
        .pet
        .map(|pet| {
            Ok::<Pet, String>(Pet {
                skill: skill(&pet.skill)?,
                hp: pet.hp,
                max_hp: pet.max_hp,
                attack: game_data::Range::new(pet.attack_min, pet.attack_max),
                interval: pet.interval,
                next_attack_at: pet.next_attack_at,
                expires_at: pet.expires_at,
            })
        })
        .transpose()?;
    let mut achievements = BTreeSet::new();
    for name in saved.achievements {
        if let Some(id) = parse_achievement(&name) {
            achievements.insert(id);
        }
    }
    let mut state = GameState {
        version: SAVE_VERSION,
        name: saved.name,
        class,
        gender,
        high_risk: saved.high_risk,
        level: saved.level,
        exp: saved.exp,
        hp: saved.hp,
        mp: saved.mp,
        gold: saved.gold,
        base: saved.base,
        equipment,
        inventory: saved
            .inventory
            .into_iter()
            .map(stack)
            .collect::<Result<_, _>>()?,
        storage: saved
            .storage
            .into_iter()
            .map(stack)
            .collect::<Result<_, _>>()?,
        skills,
        current_map: map(&saved.current_map)?,
        map_kills,
        combat,
        pet,
        stats: LifetimeStats {
            kills: saved.kills,
            gold_gained: saved.gold_gained,
            exp_gained: saved.exp_gained,
            deaths: saved.deaths,
            drop_events: saved.drop_events,
            monster_kills,
        },
        pity,
        tick_count: saved.tick_count,
        resting_until: saved.resting_until,
        hot_hp: saved.hot_hp,
        hot_hp_per: saved.hot_hp_per,
        hot_mp: saved.hot_mp,
        hot_mp_per: saved.hot_mp_per,
        shield_left: saved.shield_left,
        shield_reduction: saved.shield_reduction,
        buff_agility: saved.buff_agility,
        buff_left: saved.buff_left,
        boss_wipes,
        boss_ready_at,
        automation: saved.automation,
        achievements,
        next_spawn_at: saved.next_spawn_at,
        return_map: saved
            .return_map
            .as_deref()
            .and_then(|slug| data.map_id(slug)),
        death_streak: saved.death_streak,
        rng: bincode::deserialize(&hex_decode(&saved.rng)?)
            .map_err(|err| format!("RNG 无法恢复：{err}"))?,
        derived: std::cell::OnceCell::new(),
    };
    state.clamp_vitals(data);
    Ok(state)
}

fn parse_achievement(name: &str) -> Option<Achievement> {
    Some(match name {
        "FirstKill" => Achievement::FirstKill,
        "Kills100" => Achievement::Kills100,
        "Kills1000" => Achievement::Kills1000,
        "Level10" => Achievement::Level10,
        "Level30" => Achievement::Level30,
        "FirstGreen" => Achievement::FirstGreen,
        "FirstBoss" => Achievement::FirstBoss,
        _ => return None,
    })
}

/// 版本迁移壳。现在只有 v1，以后在这里加 `1 => 2`。
pub fn migrate_json(value: serde_json::Value) -> Result<serde_json::Value, String> {
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(1);
    if version > u64::from(SAVE_VERSION) {
        return Err(format!("存档版本 {version} 比程序新，请升级游戏"));
    }
    let mut current = value;
    let mut ver = version;
    while ver < u64::from(SAVE_VERSION) {
        current = migrate_one(current, ver)?;
        ver += 1;
    }
    Ok(current)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

fn hex_decode(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 {
        return Err("RNG 十六进制长度不对".to_string());
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    let val = |c: u8| -> Result<u8, String> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            _ => Err("RNG 十六进制里有非法字符".to_string()),
        }
    };
    for chunk in bytes.chunks(2) {
        out.push((val(chunk[0])? << 4) | val(chunk[1])?);
    }
    Ok(out)
}

fn migrate_one(value: serde_json::Value, from: u64) -> Result<serde_json::Value, String> {
    match from {
        1 => Ok(value),
        other => Err(format!("没有从版本 {other} 出发的迁移")),
    }
}

/// JSON 文本进出。损坏时由外层回退 `.bak`。
pub fn state_to_json(state: &GameState, data: &GameData) -> Result<String, String> {
    serde_json::to_string_pretty(&export_state(state, data)).map_err(|err| err.to_string())
}

/// 从 JSON 恢复，先走迁移链。
pub fn state_from_json(text: &str, data: &GameData) -> Result<GameState, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
    let migrated = migrate_json(value)?;
    let saved: SavedGame = serde_json::from_value(migrated).map_err(|err| err.to_string())?;
    import_state(saved, data)
}
