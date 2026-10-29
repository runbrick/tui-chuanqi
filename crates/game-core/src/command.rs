//! 玩家指令。界面只发指令，不直接改状态。

use crate::event::GameEvent;
use crate::state::{ms_to_ticks, GameState, ItemStack, SkillProgress};
use crate::tick::{apply_potion, grant_item};
use game_data::{ClassReq, EquipSlot, GameData, ItemId, ItemKind, MapId, Quality};
use rand::Rng;
use thiserror::Error;

/// 规则层拒绝指令的原因。界面应把这些原文显示出来。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    /// 下标越界。
    #[error("没有这个格子")]
    BadIndex,
    /// 背包满了。
    #[error("背包满了")]
    BagFull,
    /// 仓库满了。
    #[error("仓库满了")]
    StorageFull,
    /// 空槽。
    #[error("这个槽是空的")]
    EmptySlot,
    /// 不是装备。
    #[error("这件东西不能穿")]
    CannotEquip,
    /// 等级不够。
    #[error("等级不够，需要 {0} 级")]
    LevelTooLow(u16),
    /// 职业不符。
    #[error("职业不符，这件是{0}装备")]
    ClassMismatch(String),
    /// 性别不符。
    #[error("性别不符，这是{0}款衣服")]
    GenderMismatch(String),
    /// 不是消耗品或技能书。
    #[error("这件东西现在用不了")]
    NotUsable,
    /// 已经会了。
    #[error("已经学会这个技能")]
    AlreadyKnown,
    /// 金币不够。
    #[error("金币不够")]
    NotEnoughGold,
    /// 地图还没解锁。
    #[error("这张地图还没解锁")]
    MapLocked,
    /// 没有需要修理的装备。
    #[error("没有需要修理的装备")]
    NothingToRepair,
    /// 这把武器不能升级。
    #[error("这把武器不能升级")]
    NotUpgradable,
    /// 材料不够。
    #[error("缺少黑铁矿或首饰")]
    MissingMaterials,
    /// 自动化还没解锁。
    #[error("还没解锁这项自动化")]
    LockedAutomation,
    /// 阈值超出当前精度。
    #[error("当前等级只能用固定阈值或 25% 一档")]
    ThresholdLocked,
    /// 锁定的东西不能卖。
    #[error("这件东西锁住了，先按 L 解锁")]
    ItemLocked,
    /// 强化过的武器不能直接卖。
    #[error("强化过的武器不能卖")]
    Upgraded,
    /// 数量不够。
    #[error("数量不够，这叠只有 {0} 个")]
    NotEnoughCount(u32),
    /// 药已经够了。
    #[error("药已经够了，不用补")]
    StockFull,
}

/// 换装前后的属性差，正数是变强。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatDelta {
    /// 攻击下。
    pub attack_min: i32,
    /// 攻击上。
    pub attack_max: i32,
    /// 魔法下。
    pub magic_min: i32,
    /// 魔法上。
    pub magic_max: i32,
    /// 道术下。
    pub taoism_min: i32,
    /// 道术上。
    pub taoism_max: i32,
    /// 防御下。
    pub defense_min: i32,
    /// 防御上。
    pub defense_max: i32,
    /// 魔防下。
    pub magic_defense_min: i32,
    /// 魔防上。
    pub magic_defense_max: i32,
    /// 准确。
    pub accuracy: i32,
    /// 敏捷。
    pub agility: i32,
    /// 生命。
    pub hp: i32,
    /// 魔法。
    pub mp: i32,
}

/// 玩家能发出的指令。
#[derive(Debug, Clone)]
pub enum Command {
    /// 穿上背包第 `index` 件。左右手镯 / 戒指会找空槽。
    Equip {
        /// 背包下标。
        index: usize,
    },
    /// 脱下。
    Unequip {
        /// 槽位。
        slot: EquipSlot,
    },
    /// 使用消耗品或技能书。
    Use {
        /// 背包下标。
        index: usize,
    },
    /// 丢掉。
    Drop {
        /// 背包下标。
        index: usize,
    },
    /// 卖掉背包里的若干个。数量不能超过这一格。
    Sell {
        /// 背包下标。
        index: usize,
        /// 卖出数量。
        count: u32,
    },
    /// 卖掉白色未强化装备。
    SellJunk,
    /// 从某 NPC 商店买。
    Buy {
        /// NPC 下标，按数据表顺序。
        npc: usize,
        /// 该商店库存下标。
        stock_index: usize,
        /// 数量。
        count: u32,
    },
    /// 修理身上和背包里的装备。
    Repair,
    /// 存仓。
    Deposit {
        /// 背包下标。
        index: usize,
    },
    /// 取仓。
    Withdraw {
        /// 仓库下标。
        index: usize,
    },
    /// 切换技能自动施放。
    ToggleSkill {
        /// 技能表下标。
        index: usize,
    },
    /// 换图。
    ChangeMap {
        /// 地图。
        map: MapId,
    },
    /// 自动推进开关。
    SetAutoAdvance {
        /// 是否开启。
        enabled: bool,
    },
    /// 自动喝药开关和阈值。
    SetThresholds {
        /// 自动喝药。
        auto: bool,
        /// 生命比例。
        hp: f64,
        /// 魔法比例。
        mp: f64,
    },
    /// 自动卖白装。
    SetAutoSell {
        /// 是否开启。
        enabled: bool,
    },
    /// 锁定背包物品，高风险死亡不掉。
    LockInventory {
        /// 背包下标。
        index: usize,
    },
    /// 锁定已穿装备。
    LockEquipped {
        /// 槽位。
        slot: EquipSlot,
    },
    /// 武器升级。调用前界面必须已经二次确认。
    UpgradeWeapon {
        /// 用作“首饰”的背包下标，决定加成方向。
        accessory_index: usize,
    },
    /// 按预算把红蓝药补到目标瓶数。钱不够时两边轮流买，能买多少买多少。
    Restock {
        /// 金创药目标数量。
        hp: u32,
        /// 魔法药目标数量。
        mp: u32,
    },
}

impl Command {
    /// 执行指令。
    pub fn apply(
        self,
        state: &mut GameState,
        data: &GameData,
    ) -> Result<Vec<GameEvent>, CoreError> {
        let mut events = Vec::new();
        match self {
            Self::Equip { index } => equip(state, data, index, &mut events)?,
            Self::Unequip { slot } => unequip(state, data, slot, &mut events)?,
            Self::Use { index } => use_item(state, data, index, &mut events)?,
            Self::Drop { index } => {
                let _ = take_bag(state, index)?;
            }
            Self::Sell { index, count } => sell_one(state, data, index, count)?,
            Self::SellJunk => sell_junk(state, data),
            Self::Buy {
                npc,
                stock_index,
                count,
            } => buy(state, data, npc, stock_index, count)?,
            Self::Repair => repair_all(state, data, &mut events)?,
            Self::Deposit { index } => deposit(state, data, index)?,
            Self::Withdraw { index } => withdraw(state, data, index)?,
            Self::ToggleSkill { index } => toggle_skill(state, data, index)?,
            Self::ChangeMap { map } => change_map(state, data, map, &mut events)?,
            Self::SetAutoAdvance { enabled } => {
                if enabled && state.level < data.balance.auto_advance_level {
                    return Err(CoreError::LockedAutomation);
                }
                state.automation.auto_advance = enabled;
            }
            Self::SetThresholds { auto, hp, mp } => set_thresholds(state, data, auto, hp, mp)?,
            Self::SetAutoSell { enabled } => state.automation.auto_sell_junk = enabled,
            Self::LockInventory { index } => {
                let stack = state.inventory.get_mut(index).ok_or(CoreError::BadIndex)?;
                stack.locked = !stack.locked;
            }
            Self::LockEquipped { slot } => {
                let stack = state.equipment[slot.index()]
                    .as_mut()
                    .ok_or(CoreError::EmptySlot)?;
                stack.locked = !stack.locked;
            }
            Self::UpgradeWeapon { accessory_index } => {
                upgrade(state, data, accessory_index, &mut events)?
            }
            Self::Restock { hp, mp } => restock(state, data, hp, mp, &mut events)?,
        }
        Ok(events)
    }
}

/// 把背包和仓库里可堆叠的东西并成一格，技能书和药一样。
pub fn compact_inventory(state: &mut GameState, data: &GameData) {
    compact(&mut state.inventory, data);
    compact(&mut state.storage, data);
}

fn compact(slots: &mut Vec<ItemStack>, data: &GameData) {
    let mut index = 0;
    while index < slots.len() {
        if !data.item(slots[index].item).stackable {
            index += 1;
            continue;
        }
        let item = slots[index].item;
        let mut next = index + 1;
        while next < slots.len() {
            if slots[next].item == item {
                let extra = slots[next].count;
                slots[index].count = slots[index].count.saturating_add(extra);
                slots.remove(next);
            } else {
                next += 1;
            }
        }
        index += 1;
    }
}

fn take_bag(state: &mut GameState, index: usize) -> Result<ItemStack, CoreError> {
    if index >= state.inventory.len() {
        return Err(CoreError::BadIndex);
    }
    Ok(state.inventory.remove(index))
}

fn equip(
    state: &mut GameState,
    data: &GameData,
    index: usize,
    events: &mut Vec<GameEvent>,
) -> Result<(), CoreError> {
    let stack = state.inventory.get(index).ok_or(CoreError::BadIndex)?;
    let def = data.item(stack.item);
    let gear = def.kind.gear().ok_or(CoreError::CannotEquip)?;
    if state.level < gear.req_level {
        return Err(CoreError::LevelTooLow(gear.req_level));
    }
    if !gear.req_class.allows(state.class) {
        return Err(CoreError::ClassMismatch(gear.req_class.label().to_string()));
    }
    if def.kind.is_armor() && !gear.gender.allows(state.gender) {
        return Err(CoreError::GenderMismatch(gear.gender.label().to_string()));
    }
    let slot = find_slot(state, gear.slot).ok_or(CoreError::CannotEquip)?;
    let stack = take_bag(state, index)?;
    let id = stack.item;
    if let Some(old) = state.equipment[slot.index()].take() {
        state
            .inventory
            .insert(index.min(state.inventory.len()), old);
    }
    state.equipment[slot.index()] = Some(stack);
    state.invalidate_derived();
    state.clamp_vitals(data);
    events.push(GameEvent::Equipped { id });
    Ok(())
}

fn find_slot(state: &GameState, item_slot: EquipSlot) -> Option<EquipSlot> {
    let preferred = match item_slot {
        EquipSlot::BraceletL | EquipSlot::BraceletR => [EquipSlot::BraceletL, EquipSlot::BraceletR],
        EquipSlot::RingL | EquipSlot::RingR => [EquipSlot::RingL, EquipSlot::RingR],
        other => [other, other],
    };
    preferred
        .into_iter()
        .find(|slot| state.equipment[slot.index()].is_none())
        .or(Some(preferred[0]))
}

fn unequip(
    state: &mut GameState,
    data: &GameData,
    slot: EquipSlot,
    events: &mut Vec<GameEvent>,
) -> Result<(), CoreError> {
    if state.inventory.len() >= data.balance.inventory_slots as usize {
        return Err(CoreError::BagFull);
    }
    let stack = state.equipment[slot.index()]
        .take()
        .ok_or(CoreError::EmptySlot)?;
    events.push(GameEvent::Unequipped { id: stack.item });
    state.inventory.push(stack);
    state.invalidate_derived();
    state.clamp_vitals(data);
    Ok(())
}

fn use_item(
    state: &mut GameState,
    data: &GameData,
    index: usize,
    events: &mut Vec<GameEvent>,
) -> Result<(), CoreError> {
    let stack = state.inventory.get(index).ok_or(CoreError::BadIndex)?;
    match &data.item(stack.item).kind {
        ItemKind::Consumable(_) => {
            let id = stack.item;
            if state.inventory[index].count > 1 {
                state.inventory[index].count -= 1;
            } else {
                state.inventory.remove(index);
            }
            apply_potion(state, data, id);
            events.push(GameEvent::PotionUsed { id });
        }
        ItemKind::SkillBook(book) => {
            if state.level < book.req_level {
                return Err(CoreError::LevelTooLow(book.req_level));
            }
            if !book.req_class.allows(state.class) {
                return Err(CoreError::ClassMismatch(book.req_class.label().to_string()));
            }
            if state.skills.contains_key(&book.skill_id) {
                return Err(CoreError::AlreadyKnown);
            }
            let skill = book.skill_id;
            if state.inventory[index].count > 1 {
                state.inventory[index].count -= 1;
            } else {
                state.inventory.remove(index);
            }
            state.skills.insert(
                skill,
                SkillProgress {
                    level: 1,
                    exp: 0,
                    auto: true,
                },
            );
            state.invalidate_derived();
            events.push(GameEvent::SkillLearned { id: skill });
        }
        ItemKind::Weapon(_) | ItemKind::Armor(_) | ItemKind::Helmet(_) | ItemKind::Accessory(_) => {
            equip(state, data, index, events)?;
        }
        _ => return Err(CoreError::NotUsable),
    }
    Ok(())
}

fn sell_one(
    state: &mut GameState,
    data: &GameData,
    index: usize,
    count: u32,
) -> Result<(), CoreError> {
    if count == 0 {
        return Ok(());
    }
    let stack = state.inventory.get(index).ok_or(CoreError::BadIndex)?;
    if stack.locked {
        return Err(CoreError::ItemLocked);
    }
    if stack.upgrade > 0 {
        return Err(CoreError::Upgraded);
    }
    if count > stack.count {
        return Err(CoreError::NotEnoughCount(stack.count));
    }
    let item = stack.item;
    if count == stack.count {
        let _ = take_bag(state, index)?;
    } else {
        state.inventory[index].count -= count;
    }
    let gold = sale_gold(data, item, count);
    state.gold = state.gold.saturating_add(gold);
    Ok(())
}

/// 按商店卖出倍率算一堆能换多少金币。界面提示和实际入账用同一公式。
pub fn sale_gold(data: &GameData, item: ItemId, count: u32) -> u64 {
    (f64::from(data.item(item).price) * sell_rate(data) * f64::from(count)).floor() as u64
}

fn sell_junk(state: &mut GameState, data: &GameData) {
    let rate = sell_rate(data);
    let mut gold = 0u64;
    state.inventory.retain(|stack| {
        let def = data.item(stack.item);
        let junk = def.quality == Quality::White
            && def.kind.gear().is_some()
            && !stack.locked
            && stack.upgrade == 0;
        if junk {
            gold = gold.saturating_add(
                (f64::from(def.price) * rate * f64::from(stack.count)).floor() as u64,
            );
            false
        } else {
            true
        }
    });
    state.gold = state.gold.saturating_add(gold);
}

fn sell_rate(data: &GameData) -> f64 {
    data.npcs
        .iter()
        .find_map(|npc| {
            npc.services.iter().find_map(|service| match service {
                game_data::NpcService::Shop(shop) => Some(f64::from(shop.sell_rate)),
                _ => None,
            })
        })
        .unwrap_or(0.3)
}

fn buy(
    state: &mut GameState,
    data: &GameData,
    npc: usize,
    stock_index: usize,
    count: u32,
) -> Result<(), CoreError> {
    if count == 0 {
        return Ok(());
    }
    let npc = data.npcs.get(npc).ok_or(CoreError::BadIndex)?;
    let shop = npc
        .services
        .iter()
        .find_map(|service| match service {
            game_data::NpcService::Shop(shop) => Some(shop),
            _ => None,
        })
        .ok_or(CoreError::BadIndex)?;
    let item = *shop.stock_ids.get(stock_index).ok_or(CoreError::BadIndex)?;
    let unit = (f64::from(data.item(item).price) * f64::from(shop.buy_markup)).ceil() as u64;
    let cost = unit.saturating_mul(u64::from(count));
    if state.gold < cost {
        return Err(CoreError::NotEnoughGold);
    }
    if !can_fit(state, data, item, count) {
        return Err(CoreError::BagFull);
    }
    state.gold -= cost;
    let mut discarded = Vec::new();
    grant_item(state, data, &mut discarded, item, count, false);
    Ok(())
}

fn can_fit(state: &GameState, data: &GameData, item: ItemId, count: u32) -> bool {
    if matches!(data.item(item).kind, ItemKind::Currency) {
        return true;
    }
    if data.item(item).stackable && state.inventory.iter().any(|stack| stack.item == item) {
        return true;
    }
    let need = if data.item(item).stackable {
        1
    } else {
        count as usize
    };
    state.inventory.len() + need <= data.balance.inventory_slots as usize
}

/// 第一家提供修理的 NPC。
pub fn repair_service(data: &GameData) -> Option<&game_data::RepairService> {
    data.npcs.iter().find_map(|npc| {
        npc.services.iter().find_map(|service| match service {
            game_data::NpcService::Repair(repair) => Some(repair),
            _ => None,
        })
    })
}

/// 修满这一件要多少钱。不用修返回 `None`。
pub fn repair_cost(
    data: &GameData,
    service: &game_data::RepairService,
    stack: &ItemStack,
) -> Option<u64> {
    let def = data.item(stack.item);
    let gear = def.kind.gear()?;
    if gear.durability == 0 || stack.durability >= gear.durability {
        return None;
    }
    let missing = gear.durability - stack.durability;
    Some(match service.price_ratio {
        Some(ratio) => {
            let share = f64::from(missing) / f64::from(gear.durability);
            (f64::from(def.price) * f64::from(ratio) * share)
                .ceil()
                .max(1.0) as u64
        }
        None => u64::from(missing) * u64::from(service.cost_per_durability.max(1)),
    })
}

fn repair_all(
    state: &mut GameState,
    data: &GameData,
    events: &mut Vec<GameEvent>,
) -> Result<(), CoreError> {
    let fallback = game_data::RepairService {
        cost_per_durability: 1,
        price_ratio: None,
    };
    let service = repair_service(data).unwrap_or(&fallback);
    let mut bill = 0u64;
    let mut pieces = 0u32;
    for stack in state
        .equipment
        .iter()
        .flatten()
        .chain(state.inventory.iter())
    {
        if let Some(cost) = repair_cost(data, service, stack) {
            bill = bill.saturating_add(cost);
            pieces += 1;
        }
    }
    if pieces == 0 {
        return Err(CoreError::NothingToRepair);
    }
    if state.gold < bill {
        return Err(CoreError::NotEnoughGold);
    }
    state.gold -= bill;
    let fix = |stack: &mut ItemStack| {
        if let Some(gear) = data.item(stack.item).kind.gear() {
            stack.durability = gear.durability;
        }
    };
    // 修好后损坏的装备属性回来，需要失效缓存。
    let was_broken = state
        .equipment
        .iter()
        .flatten()
        .any(|stack| stack.is_broken(data));
    for stack in state.equipment.iter_mut().flatten() {
        fix(stack);
    }
    for stack in &mut state.inventory {
        fix(stack);
    }
    if was_broken {
        state.invalidate_derived();
    }
    events.push(GameEvent::Repaired { pieces, gold: bill });
    Ok(())
}

fn storage_limit(data: &GameData) -> usize {
    data.npcs
        .iter()
        .filter_map(|npc| {
            npc.services.iter().find_map(|service| match service {
                game_data::NpcService::Storage(storage) => Some(storage.slots as usize),
                _ => None,
            })
        })
        .max()
        .unwrap_or(data.balance.storage_slots as usize)
}

fn deposit(state: &mut GameState, data: &GameData, index: usize) -> Result<(), CoreError> {
    if state.storage.len() >= storage_limit(data)
        && !data
            .item(state.inventory.get(index).ok_or(CoreError::BadIndex)?.item)
            .stackable
    {
        return Err(CoreError::StorageFull);
    }
    let stack = take_bag(state, index)?;
    if data.item(stack.item).stackable {
        if let Some(existing) = state.storage.iter_mut().find(|s| s.item == stack.item) {
            existing.count = existing.count.saturating_add(stack.count);
            return Ok(());
        }
    }
    if state.storage.len() >= storage_limit(data) {
        state
            .inventory
            .insert(index.min(state.inventory.len()), stack);
        return Err(CoreError::StorageFull);
    }
    state.storage.push(stack);
    Ok(())
}

fn withdraw(state: &mut GameState, data: &GameData, index: usize) -> Result<(), CoreError> {
    if index >= state.storage.len() {
        return Err(CoreError::BadIndex);
    }
    if state.inventory.len() >= data.balance.inventory_slots as usize {
        return Err(CoreError::BagFull);
    }
    let stack = state.storage.remove(index);
    state.inventory.push(stack);
    Ok(())
}

fn toggle_skill(state: &mut GameState, data: &GameData, index: usize) -> Result<(), CoreError> {
    let Some(skill) = data.skills.get(index) else {
        return Err(CoreError::BadIndex);
    };
    let Some(progress) = state.skills.get_mut(&skill.runtime_id) else {
        return Err(CoreError::BadIndex);
    };
    progress.auto = !progress.auto;
    Ok(())
}

pub(crate) fn map_unlocked(state: &GameState, data: &GameData, map: MapId) -> bool {
    map_block(state, data, map).is_none()
}

/// 地图为什么进不去。界面用它写「需 15 级」「毒蛇山谷 12/60」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapBlock {
    /// 等级不够。
    Level(u16),
    /// 前置地图击杀不够：地图、已有、需要。
    Kills(MapId, u64, u64),
}

/// 返回第一条没满足的条件。`None` 表示能进。
pub fn map_block(state: &GameState, data: &GameData, map: MapId) -> Option<MapBlock> {
    let def = data.map(map);
    if state.level < def.req_level {
        return Some(MapBlock::Level(def.req_level));
    }
    if let Some((prev, need)) = def
        .req_map_id
        .zip(def.req_map_kills.as_ref().map(|(_, n)| *n))
    {
        let have = state.map_kills.get(&prev).copied().unwrap_or(0);
        if have < need {
            return Some(MapBlock::Kills(prev, have, need));
        }
    }
    None
}

fn change_map(
    state: &mut GameState,
    data: &GameData,
    map: MapId,
    events: &mut Vec<GameEvent>,
) -> Result<(), CoreError> {
    if data.maps.get(map.0 as usize).is_none() {
        return Err(CoreError::BadIndex);
    }
    if !map_unlocked(state, data, map) {
        return Err(CoreError::MapLocked);
    }
    state.combat = None;
    state.current_map = map;
    state.next_spawn_at = state.tick_count;
    // 手动换图就是玩家自己做主，不再自动回原图，连死也重新计。
    state.return_map = None;
    state.death_streak = 0;
    events.push(GameEvent::MapChanged { map });
    Ok(())
}

fn set_thresholds(
    state: &mut GameState,
    data: &GameData,
    auto: bool,
    hp: f64,
    mp: f64,
) -> Result<(), CoreError> {
    let hp = hp.clamp(0.05, 0.95);
    let mp = mp.clamp(0.05, 0.95);
    let changed = (hp - state.automation.hp_ratio).abs() > 0.001
        || (mp - state.automation.mp_ratio).abs() > 0.001;
    if changed {
        if state.level < data.balance.auto_coarse_level {
            let fixed_hp = data.balance.potion_hp_ratio;
            let fixed_mp = data.balance.potion_mp_ratio;
            if (hp - fixed_hp).abs() > 0.02 || (mp - fixed_mp).abs() > 0.02 {
                return Err(CoreError::ThresholdLocked);
            }
        } else if state.level < data.balance.auto_fine_level {
            let coarse = |v: f64| {
                [0.25, 0.5, 0.75]
                    .into_iter()
                    .any(|step| (v - step).abs() < 0.02)
            };
            if !coarse(hp) || !coarse(mp) {
                return Err(CoreError::ThresholdLocked);
            }
        }
        state.automation.hp_ratio = hp;
        state.automation.mp_ratio = mp;
    }
    state.automation.auto_potion = auto;
    Ok(())
}

/// 升级预览，给二次确认用。不改状态。
pub fn upgrade_preview(state: &GameState, data: &GameData) -> Result<(f64, u64, u8), CoreError> {
    let weapon = state.equipment[EquipSlot::Weapon.index()]
        .as_ref()
        .ok_or(CoreError::EmptySlot)?;
    let gear = data
        .item(weapon.item)
        .kind
        .gear()
        .ok_or(CoreError::NotUpgradable)?;
    if !gear.upgradable {
        return Err(CoreError::NotUpgradable);
    }
    let rate = data.balance.upgrade_rate(weapon.upgrade);
    let gold = u64::from(data.balance.upgrade_gold_base) * u64::from(weapon.upgrade + 1);
    Ok((rate, gold, weapon.upgrade.saturating_add(1)))
}

fn upgrade(
    state: &mut GameState,
    data: &GameData,
    accessory_index: usize,
    events: &mut Vec<GameEvent>,
) -> Result<(), CoreError> {
    let (rate, gold, _) = upgrade_preview(state, data)?;
    let accessory = state
        .inventory
        .get(accessory_index)
        .ok_or(CoreError::BadIndex)?;
    if data.item(accessory.item).kind.gear().is_none() {
        return Err(CoreError::MissingMaterials);
    }
    if accessory.locked {
        return Err(CoreError::ItemLocked);
    }
    let ore_index = state.inventory.iter().position(|stack| {
        matches!(data.item(stack.item).kind, ItemKind::Material(_)) && stack.item != accessory.item
    });
    let Some(ore_index) = ore_index else {
        return Err(CoreError::MissingMaterials);
    };
    if state.gold < gold {
        return Err(CoreError::NotEnoughGold);
    }
    state.gold -= gold;
    // 先消耗矿和首饰。下标可能因为删除移位。
    let (first, second) = if accessory_index > ore_index {
        (accessory_index, ore_index)
    } else {
        (ore_index, accessory_index)
    };
    remove_one(state, first);
    if first != second {
        remove_one(state, second);
    }
    let success = state.rng.gen_bool(rate.clamp(0.0, 1.0));
    if success {
        if let Some(weapon) = state.equipment[EquipSlot::Weapon.index()].as_mut() {
            weapon.upgrade = weapon.upgrade.saturating_add(1);
            let plus = weapon.upgrade;
            state.invalidate_derived();
            events.push(GameEvent::UpgradeSucceeded { plus });
        }
        return Ok(());
    }
    if let Some(charm) = state
        .inventory
        .iter()
        .position(|stack| matches!(data.item(stack.item).kind, ItemKind::LuckCharm))
    {
        remove_one(state, charm);
        events.push(GameEvent::UpgradeProtected);
        return Ok(());
    }
    state.equipment[EquipSlot::Weapon.index()] = None;
    state.invalidate_derived();
    events.push(GameEvent::UpgradeDestroyed);
    Ok(())
}

fn remove_one(state: &mut GameState, index: usize) {
    if state.inventory[index].count > 1 {
        state.inventory[index].count -= 1;
    } else {
        state.inventory.remove(index);
    }
}

fn restock(
    state: &mut GameState,
    data: &GameData,
    hp_target: u32,
    mp_target: u32,
    events: &mut Vec<GameEvent>,
) -> Result<(), CoreError> {
    let potion_npc = data
        .npcs
        .iter()
        .position(|npc| npc.services.iter().any(|service| matches!(service, game_data::NpcService::Shop(shop) if shop.stock.iter().any(|id| id.contains("potion")))))
        .ok_or(CoreError::BadIndex)?;
    let shop = data.npcs[potion_npc]
        .services
        .iter()
        .find_map(|service| match service {
            game_data::NpcService::Shop(shop) => Some(shop),
            _ => None,
        });
    let Some(shop) = shop else {
        return Err(CoreError::BadIndex);
    };
    let derived = *state.derived_stats(data);
    // 挑一瓶不超过上限一半的最大规格。喝药线在一半附近，再大就是白花钱；
    // 上限太低连小瓶都超的时候就买小瓶。
    let pick = |hp: bool| -> Option<(usize, u64)> {
        let cap = if hp { derived.max_hp } else { derived.max_mp } / 2;
        let mut fits: Option<(usize, i32)> = None;
        let mut smallest: Option<(usize, i32)> = None;
        for (index, id) in shop.stock_ids.iter().enumerate() {
            let ItemKind::Consumable(potion) = &data.item(*id).kind else {
                continue;
            };
            let amount = if hp && potion.heal_mp == 0 {
                potion.heal_hp
            } else if !hp && potion.heal_hp == 0 {
                potion.heal_mp
            } else {
                0
            };
            if amount <= 0 {
                continue;
            }
            if amount <= cap && fits.is_none_or(|(_, best)| amount > best) {
                fits = Some((index, amount));
            }
            if smallest.is_none_or(|(_, best)| amount < best) {
                smallest = Some((index, amount));
            }
        }
        let (index, _) = fits.or(smallest)?;
        let unit = (f64::from(data.item(shop.stock_ids[index]).price) * f64::from(shop.buy_markup))
            .ceil() as u64;
        Some((index, unit.max(1)))
    };
    let owned = |hp: bool| -> u32 {
        state
            .inventory
            .iter()
            .filter(|stack| {
                matches!(&data.item(stack.item).kind, ItemKind::Consumable(p)
                    if if hp { p.heal_hp > 0 } else { p.heal_mp > 0 })
            })
            .map(|stack| stack.count)
            .sum()
    };
    let hp_pick = pick(true);
    let mp_pick = if derived.max_mp > 0 {
        pick(false)
    } else {
        None
    };
    let hp_need = if hp_pick.is_some() {
        hp_target.saturating_sub(owned(true))
    } else {
        0
    };
    let mp_need = if mp_pick.is_some() {
        mp_target.saturating_sub(owned(false))
    } else {
        0
    };
    if hp_need == 0 && mp_need == 0 {
        return Err(CoreError::StockFull);
    }
    // 钱不够时一瓶一瓶轮着买，哪边缺得多先补哪边，整单买不起也不至于一瓶都没有。
    let mut budget = state.gold;
    let (mut hp_buy, mut mp_buy) = (0u32, 0u32);
    loop {
        let hp_left = hp_need - hp_buy;
        let mp_left = mp_need - mp_buy;
        let hp_unit = hp_pick.map(|(_, unit)| unit).unwrap_or(u64::MAX);
        let mp_unit = mp_pick.map(|(_, unit)| unit).unwrap_or(u64::MAX);
        let hp_ok = hp_left > 0 && budget >= hp_unit;
        let mp_ok = mp_left > 0 && budget >= mp_unit;
        if !hp_ok && !mp_ok {
            break;
        }
        let hp_first = if hp_ok && mp_ok {
            u64::from(hp_buy) * u64::from(mp_need.max(1))
                <= u64::from(mp_buy) * u64::from(hp_need.max(1))
        } else {
            hp_ok
        };
        if hp_first {
            hp_buy += 1;
            budget -= hp_unit;
        } else {
            mp_buy += 1;
            budget -= mp_unit;
        }
    }
    if hp_buy == 0 && mp_buy == 0 {
        return Err(CoreError::NotEnoughGold);
    }
    let before = state.gold;
    let mut bought_hp = 0;
    let mut bought_mp = 0;
    if let Some((index, _)) = hp_pick.filter(|_| hp_buy > 0) {
        if buy(state, data, potion_npc, index, hp_buy).is_ok() {
            bought_hp = hp_buy;
        }
    }
    if let Some((index, _)) = mp_pick.filter(|_| mp_buy > 0) {
        if buy(state, data, potion_npc, index, mp_buy).is_ok() {
            bought_mp = mp_buy;
        }
    }
    if bought_hp == 0 && bought_mp == 0 {
        return Err(CoreError::BagFull);
    }
    events.push(GameEvent::Restocked {
        hp: bought_hp,
        mp: bought_mp,
        gold: before - state.gold,
    });
    Ok(())
}

/// 如果穿上背包里这件，属性会怎么变。不修改状态。
pub fn compare_equip(state: &GameState, data: &GameData, index: usize) -> Option<StatDelta> {
    let stack = state.inventory.get(index)?;
    let gear = data.item(stack.item).kind.gear()?;
    let slot = find_slot(state, gear.slot)?;
    let current = state.derived_stats(data);
    let mut attack = current.attack;
    let mut magic = current.magic;
    let mut taoism = current.taoism;
    let mut defense = current.defense;
    let mut magic_defense = current.magic_defense;
    let mut accuracy = current.accuracy;
    let mut agility = current.agility;
    let mut hp = current.max_hp;
    let mut mp = current.max_mp;
    if let Some(old) = &state.equipment[slot.index()] {
        if let Some(old_gear) = data.item(old.item).kind.gear() {
            if !old.is_broken(data) {
                attack.min -= old_gear.attack.min;
                attack.max -= old_gear.attack.max + i32::from(old.upgrade);
                magic.min -= old_gear.magic.min;
                magic.max -= old_gear.magic.max;
                taoism.min -= old_gear.taoism.min;
                taoism.max -= old_gear.taoism.max;
                defense.min -= old_gear.defense.min;
                defense.max -= old_gear.defense.max;
                magic_defense.min -= old_gear.magic_defense.min;
                magic_defense.max -= old_gear.magic_defense.max;
                accuracy -= old_gear.accuracy;
                agility -= old_gear.agility;
                hp -= old_gear.hp;
                mp -= old_gear.mp;
            }
        }
    }
    let broken = stack.is_broken(data);
    let add = |base: i32, value: i32| if broken { base } else { base + value };
    Some(StatDelta {
        attack_min: add(attack.min, gear.attack.min) - current.attack.min,
        attack_max: add(attack.max, gear.attack.max + i32::from(stack.upgrade))
            - current.attack.max,
        magic_min: add(magic.min, gear.magic.min) - current.magic.min,
        magic_max: add(magic.max, gear.magic.max) - current.magic.max,
        taoism_min: add(taoism.min, gear.taoism.min) - current.taoism.min,
        taoism_max: add(taoism.max, gear.taoism.max) - current.taoism.max,
        defense_min: add(defense.min, gear.defense.min) - current.defense.min,
        defense_max: add(defense.max, gear.defense.max) - current.defense.max,
        magic_defense_min: add(magic_defense.min, gear.magic_defense.min)
            - current.magic_defense.min,
        magic_defense_max: add(magic_defense.max, gear.magic_defense.max)
            - current.magic_defense.max,
        accuracy: add(accuracy, gear.accuracy) - current.accuracy,
        agility: add(agility, gear.agility) - current.agility,
        hp: add(hp, gear.hp) - current.max_hp,
        mp: add(mp, gear.mp) - current.max_mp,
    })
}

/// 名字规则：1 到 12 个字符，允许中文、字母、数字、下划线，禁止路径字符。允许重名。
pub fn validate_name(name: &str) -> Result<(), String> {
    let count = name.chars().count();
    if !(1..=12).contains(&count) {
        return Err("名字长度要在 1 到 12 个字".to_string());
    }
    if name.chars().any(|ch| {
        matches!(
            ch,
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\n' | '\r'
        )
    }) {
        return Err("名字里不能有 \\ / : * ? \" < > |".to_string());
    }
    Ok(())
}

/// 按 F 补药的目标瓶数：(红, 蓝)。战士几乎不耗蓝，法师反过来。
pub fn restock_targets(class: game_data::Class) -> (u32, u32) {
    match class {
        game_data::Class::Warrior => (30, 5),
        game_data::Class::Mage => (25, 30),
        game_data::Class::Taoist => (20, 20),
    }
}

/// 毫秒间隔，给界面显示攻速。
pub fn attack_interval_ms(state: &GameState) -> u32 {
    u32::try_from(ms_to_ticks(state.base.attack_interval_ms) * 100).unwrap_or(u32::MAX)
}

/// 给界面用的职业限制文案。
pub fn class_label(req: ClassReq) -> &'static str {
    req.label()
}
