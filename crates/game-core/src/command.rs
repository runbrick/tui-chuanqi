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
    /// 卖掉一件。
    Sell {
        /// 背包下标。
        index: usize,
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
    /// 按预算把红蓝药补到目标瓶数。
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
            Self::Equip { index } => equip(state, data, index)?,
            Self::Unequip { slot } => unequip(state, data, slot)?,
            Self::Use { index } => use_item(state, data, index, &mut events)?,
            Self::Drop { index } => {
                let _ = take_bag(state, index)?;
            }
            Self::Sell { index } => sell_one(state, data, index)?,
            Self::SellJunk => sell_junk(state, data),
            Self::Buy {
                npc,
                stock_index,
                count,
            } => buy(state, data, npc, stock_index, count)?,
            Self::Repair => repair_all(state, data)?,
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
            Self::Restock { hp, mp } => restock(state, data, hp, mp)?,
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

fn equip(state: &mut GameState, data: &GameData, index: usize) -> Result<(), CoreError> {
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
    if let Some(old) = state.equipment[slot.index()].take() {
        state
            .inventory
            .insert(index.min(state.inventory.len()), old);
    }
    state.equipment[slot.index()] = Some(stack);
    state.invalidate_derived();
    state.clamp_vitals(data);
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

fn unequip(state: &mut GameState, data: &GameData, slot: EquipSlot) -> Result<(), CoreError> {
    if state.inventory.len() >= data.balance.inventory_slots as usize {
        return Err(CoreError::BagFull);
    }
    let stack = state.equipment[slot.index()]
        .take()
        .ok_or(CoreError::EmptySlot)?;
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
            equip(state, data, index)?;
        }
        _ => return Err(CoreError::NotUsable),
    }
    Ok(())
}

fn sell_one(state: &mut GameState, data: &GameData, index: usize) -> Result<(), CoreError> {
    let stack = take_bag(state, index)?;
    let rate = sell_rate(data);
    let gold =
        (f64::from(data.item(stack.item).price) * rate * f64::from(stack.count)).floor() as u64;
    state.gold = state.gold.saturating_add(gold);
    Ok(())
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

fn repair_all(state: &mut GameState, data: &GameData) -> Result<(), CoreError> {
    let cost_per = data
        .npcs
        .iter()
        .find_map(|npc| {
            npc.services.iter().find_map(|service| match service {
                game_data::NpcService::Repair(repair) => Some(repair.cost_per_durability),
                _ => None,
            })
        })
        .unwrap_or(1);
    let mut bill = 0u64;
    let mut any = false;
    let consider = |stack: &ItemStack, bill: &mut u64, any: &mut bool| {
        let Some(gear) = data.item(stack.item).kind.gear() else {
            return;
        };
        if gear.durability > stack.durability {
            *any = true;
            *bill += u64::from(gear.durability - stack.durability) * u64::from(cost_per.max(1));
        }
    };
    for stack in state.equipment.iter().flatten() {
        consider(stack, &mut bill, &mut any);
    }
    for stack in &state.inventory {
        consider(stack, &mut bill, &mut any);
    }
    if !any {
        return Err(CoreError::NothingToRepair);
    }
    if state.gold < bill {
        return Err(CoreError::NotEnoughGold);
    }
    state.gold -= bill;
    let fix = |stack: &mut ItemStack| {
        if let Some(gear) = data.item(stack.item).kind.gear() {
            if stack.durability == 0 && gear.durability > 0 {
                // 修好后属性回来，需要失效缓存。
            }
            stack.durability = gear.durability;
        }
    };
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
    let def = data.map(map);
    if state.level < def.req_level {
        return false;
    }
    if let Some((prev, need)) = def
        .req_map_id
        .zip(def.req_map_kills.as_ref().map(|(_, n)| *n))
    {
        return state.map_kills.get(&prev).copied().unwrap_or(0) >= need;
    }
    true
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
    state.automation.auto_potion = auto;
    state.automation.hp_ratio = hp;
    state.automation.mp_ratio = mp;
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
    let pick = |prefer_large: bool, hp: bool| -> Option<usize> {
        let mut found = None;
        for (index, id) in shop.stock_ids.iter().enumerate() {
            let ItemKind::Consumable(potion) = &data.item(*id).kind else {
                continue;
            };
            let matches = if hp {
                potion.heal_hp > 0 && potion.heal_mp == 0
            } else {
                potion.heal_mp > 0 && potion.heal_hp == 0
            };
            if !matches {
                continue;
            }
            let large = data.item(*id).id.contains("large");
            if prefer_large == large || found.is_none() {
                found = Some(index);
                if prefer_large == large {
                    break;
                }
            }
        }
        found
    };
    let prefer_large = state.level >= 20;
    let hp_owned: u32 = state
        .inventory
        .iter()
        .filter(
            |stack| matches!(&data.item(stack.item).kind, ItemKind::Consumable(p) if p.heal_hp > 0),
        )
        .map(|stack| stack.count)
        .sum();
    let mp_owned: u32 = state
        .inventory
        .iter()
        .filter(
            |stack| matches!(&data.item(stack.item).kind, ItemKind::Consumable(p) if p.heal_mp > 0),
        )
        .map(|stack| stack.count)
        .sum();
    if let Some(index) = pick(prefer_large, true) {
        if hp_target > hp_owned {
            let _ = buy(state, data, potion_npc, index, hp_target - hp_owned);
        }
    }
    if let Some(index) = pick(prefer_large, false) {
        if mp_target > mp_owned {
            let _ = buy(state, data, potion_npc, index, mp_target - mp_owned);
        }
    }
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

/// 毫秒间隔，给界面显示攻速。
pub fn attack_interval_ms(state: &GameState) -> u32 {
    u32::try_from(ms_to_ticks(state.base.attack_interval_ms) * 100).unwrap_or(u32::MAX)
}

/// 给界面用的职业限制文案。
pub fn class_label(req: ClassReq) -> &'static str {
    req.label()
}
