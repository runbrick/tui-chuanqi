//! 无界面的数值推演。平衡调参用它，不画终端。
//!
//! 默认是纯挂机：建号后什么都不做。`--bot` 模拟一个会回城补给、买书、换装的玩家，
//! 只通过 `Command` 操作，和真人在界面里能做的事一样。

use game_core::{Command, GameEvent, GameState};
use game_data::{Class, ClassReq, EquipSlot, GameData, Gender, ItemKind, MapId, NpcService};
use r_chuanqi::find_data_dir;
use std::collections::BTreeMap;
use std::env;
use std::time::Instant;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut class = Class::Warrior;
    let mut ticks: u64 = 36_000;
    let mut seed: u64 = 42;
    let mut use_bot = false;
    let mut trace = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--class" => {
                class = parse_class(args.get(index + 1).ok_or("缺少 --class")?);
                index += 2;
            }
            "--ticks" => {
                ticks = args
                    .get(index + 1)
                    .ok_or("缺少 --ticks")?
                    .parse()
                    .map_err(|_| "tick 数不是整数")?;
                index += 2;
            }
            "--seed" => {
                seed = args
                    .get(index + 1)
                    .ok_or("缺少 --seed")?
                    .parse()
                    .map_err(|_| "种子不是整数")?;
                index += 2;
            }
            "--bot" => {
                use_bot = true;
                index += 1;
            }
            "--trace" => {
                trace = true;
                index += 1;
            }
            other => return Err(format!("未知参数 {other}")),
        }
    }
    let data_dir = find_data_dir().ok_or("找不到 data 目录")?;
    let data = GameData::load(&data_dir).map_err(|err| err.to_string())?;
    let mut state = GameState::new("模拟", class, Gender::Male, false, &data, seed);
    let mut bot = Bot::default();
    let mut track = Track::default();
    let started = Instant::now();
    for _ in 0..ticks {
        let map = state.current_map;
        let deaths = state.stats.deaths;
        let level = state.level;
        let in_combat = state.combat.is_some();
        let resting = state.tick_count < state.resting_until;
        let events = state.tick(&data);
        if trace {
            track.tally(&events, &data, in_combat, resting);
        }
        *track.map_ticks.entry(map).or_insert(0) += 1;
        if state.stats.deaths > deaths {
            *track.map_deaths.entry(map).or_insert(0) += 1;
        }
        if state.level > level {
            track
                .levels
                .push((state.level, state.tick_count, state.current_map));
        }
        if use_bot && state.tick_count % 10 == 0 {
            bot.manage(&mut state, &data);
        }
    }
    let elapsed = started.elapsed();
    let hours = ticks as f64 / 36_000.0;
    let exp_per_hour = if hours == 0.0 {
        0.0
    } else {
        state.stats.exp_gained as f64 / hours
    };
    println!("{}", state.summary(&data));
    println!("ticks={ticks} 耗时={elapsed:?} 每小时经验={exp_per_hour:.0}");
    println!(
        "击杀={} 金币={} 累计金币={} 死亡={}",
        state.stats.kills, state.gold, state.stats.gold_gained, state.stats.deaths
    );
    let mut kills: Vec<_> = state.stats.monster_kills.iter().collect();
    kills.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (id, count) in kills.iter().take(8) {
        println!("  {} ×{count}", data.monster(**id).name);
    }
    track.print(&data);
    if trace {
        track.print_tally();
    }
    if use_bot {
        let gear: Vec<String> = state
            .equipment
            .iter()
            .flatten()
            .map(|stack| data.item(stack.item).name.clone())
            .collect();
        println!("装备：{}", gear.join(" "));
        let skills: Vec<String> = state
            .skills
            .iter()
            .map(|(id, p)| format!("{}{}", data.skill(*id).name, p.level))
            .collect();
        println!("技能：{}", skills.join(" "));
    }
    Ok(())
}

fn parse_class(text: &str) -> Class {
    match text {
        "mage" | "法师" => Class::Mage,
        "taoist" | "道士" => Class::Taoist,
        _ => Class::Warrior,
    }
}

#[derive(Default)]
struct Track {
    levels: Vec<(u16, u64, MapId)>,
    map_ticks: BTreeMap<MapId, u64>,
    map_deaths: BTreeMap<MapId, u32>,
    events: BTreeMap<String, u64>,
    combat_ticks: u64,
    rest_ticks: u64,
    idle_ticks: u64,
}

impl Track {
    fn tally(&mut self, events: &[GameEvent], data: &GameData, in_combat: bool, resting: bool) {
        if resting {
            self.rest_ticks += 1;
        } else if in_combat {
            self.combat_ticks += 1;
        } else {
            self.idle_ticks += 1;
        }
        for event in events {
            let key = match event {
                GameEvent::SkillCast { id } => format!("施放 {}", data.skill(*id).name),
                GameEvent::PotionUsed { id } => format!("喝 {}", data.item(*id).name),
                GameEvent::Attacked { .. } => "命中".to_string(),
                GameEvent::Missed { .. } => "落空".to_string(),
                GameEvent::Blocked { .. } => "被挡".to_string(),
                GameEvent::MonsterHit { .. } => "受击".to_string(),
                GameEvent::DotDamage { .. } => "持续伤害".to_string(),
                GameEvent::PetAttacked { .. } => "宝宝出手".to_string(),
                GameEvent::OutOfPotions => "没药回城".to_string(),
                GameEvent::PlayerDied { .. } => "死亡".to_string(),
                _ => continue,
            };
            *self.events.entry(key).or_insert(0) += 1;
        }
    }

    fn print_tally(&self) {
        let total = (self.combat_ticks + self.rest_ticks + self.idle_ticks).max(1) as f64;
        println!(
            "战斗 {:.0}%  休息 {:.0}%  空闲 {:.0}%",
            self.combat_ticks as f64 * 100.0 / total,
            self.rest_ticks as f64 * 100.0 / total,
            self.idle_ticks as f64 * 100.0 / total
        );
        let mut rows: Vec<_> = self.events.iter().collect();
        rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        for (key, n) in rows {
            println!("  {key} {n}");
        }
    }
    fn print(&self, data: &GameData) {
        println!("等级  到达(分钟)  本级用时  地图");
        let mut last = 0u64;
        for (level, tick, map) in &self.levels {
            println!(
                "{level:>4}  {:>9.1}  {:>8.1}  {}",
                *tick as f64 / 600.0,
                (*tick - last) as f64 / 600.0,
                data.map(*map).name
            );
            last = *tick;
        }
        println!("地图  停留(分钟)  死亡");
        for (map, ticks) in &self.map_ticks {
            println!(
                "  {}  {:.1}  {}",
                data.map(*map).name,
                *ticks as f64 / 600.0,
                self.map_deaths.get(map).copied().unwrap_or(0)
            );
        }
    }
}

/// 一个“正常玩家”：药少了回城补，钱够就买书、买更好的装备，打完补给再回原来的图。
#[derive(Default)]
struct Bot {
    hunt: Option<MapId>,
    next_shop_at: u64,
}

impl Bot {
    fn manage(&mut self, state: &mut GameState, data: &GameData) {
        if state.level >= data.balance.auto_advance_level && !state.automation.auto_advance {
            let _ = Command::SetAutoAdvance { enabled: true }.apply(state, data);
        }
        learn_books(state, data);
        equip_upgrades(state, data);
        let here = data.map(state.current_map);
        if !here.safe_zone {
            self.hunt = Some(state.current_map);
            if state.combat.is_none() && self.needs_town(state, data) {
                if let Some(city) = data.safe_map() {
                    let _ = Command::ChangeMap { map: city }.apply(state, data);
                }
            }
            return;
        }
        self.shop(state, data);
        if state.tick_count >= state.resting_until {
            if let Some(map) = self.hunt.or_else(|| data.starting_map()) {
                let _ = Command::ChangeMap { map }.apply(state, data);
            }
        }
    }

    fn needs_town(&self, state: &GameState, data: &GameData) -> bool {
        let (hp_target, mp_target) = potion_targets(state.class);
        let (hp, mp) = potion_counts(state, data);
        if hp_target > 0 && hp < 3 {
            return true;
        }
        if mp_target > 5 && mp < 3 {
            return true;
        }
        if state.inventory.len() + 2 >= data.balance.inventory_slots as usize {
            return true;
        }
        if let Some(weapon) = &state.equipment[EquipSlot::Weapon.index()] {
            if let Some(gear) = data.item(weapon.item).kind.gear() {
                if gear.durability > 0 && weapon.durability * 10 < gear.durability {
                    return true;
                }
            }
        }
        state.tick_count >= self.next_shop_at
    }

    fn shop(&mut self, state: &mut GameState, data: &GameData) {
        self.next_shop_at = state.tick_count + 18_000;
        sell_unwanted(state, data);
        let _ = Command::SellJunk.apply(state, data);
        let _ = Command::Repair.apply(state, data);
        let (hp, mp) = potion_targets(state.class);
        let _ = Command::Restock { hp, mp }.apply(state, data);
        let reserve = u64::from(state.level) * 60;
        buy_books(state, data, reserve);
        buy_gear(state, data, reserve);
        equip_upgrades(state, data);
    }
}

fn potion_targets(class: Class) -> (u32, u32) {
    game_core::restock_targets(class)
}

fn potion_counts(state: &GameState, data: &GameData) -> (u32, u32) {
    let mut hp = 0;
    let mut mp = 0;
    for stack in &state.inventory {
        if let ItemKind::Consumable(potion) = &data.item(stack.item).kind {
            if potion.heal_hp > 0 {
                hp += stack.count;
            }
            if potion.heal_mp > 0 {
                mp += stack.count;
            }
        }
    }
    (hp, mp)
}

fn usable(state: &GameState, data: &GameData, item: game_data::ItemId) -> bool {
    let def = data.item(item);
    let Some(gear) = def.kind.gear() else {
        return false;
    };
    state.level >= gear.req_level
        && gear.req_class.allows(state.class)
        && (!def.kind.is_armor() || gear.gender.allows(state.gender))
}

fn score(state: &GameState, data: &GameData, item: game_data::ItemId, upgrade: u8) -> f64 {
    let Some(gear) = data.item(item).kind.gear() else {
        return 0.0;
    };
    let mid = |r: game_data::Range| f64::from(r.min + r.max) / 2.0;
    let attack = mid(gear.attack) + f64::from(upgrade) / 2.0;
    let (w_atk, w_mag, w_tao, w_acc) = match state.class {
        Class::Warrior => (2.0, 0.0, 0.0, 1.2),
        Class::Mage => (0.2, 2.5, 0.0, 0.2),
        Class::Taoist => (0.8, 0.0, 2.2, 0.4),
    };
    attack * w_atk
        + mid(gear.magic) * w_mag
        + mid(gear.taoism) * w_tao
        + mid(gear.defense) * 1.4
        + mid(gear.magic_defense) * 0.9
        + f64::from(gear.hp) * 0.2
        + f64::from(gear.mp) * 0.05
        + f64::from(gear.accuracy) * w_acc
        + f64::from(gear.agility) * 1.0
}

/// 穿上会落到哪个槽。和核心的规则一致：手镯戒指先找空槽，都满了换左边。
fn target_slot(state: &GameState, slot: EquipSlot) -> EquipSlot {
    let pair = match slot {
        EquipSlot::BraceletL | EquipSlot::BraceletR => [EquipSlot::BraceletL, EquipSlot::BraceletR],
        EquipSlot::RingL | EquipSlot::RingR => [EquipSlot::RingL, EquipSlot::RingR],
        other => [other, other],
    };
    pair.into_iter()
        .find(|slot| state.equipment[slot.index()].is_none())
        .unwrap_or(pair[0])
}

fn gain(state: &GameState, data: &GameData, item: game_data::ItemId, upgrade: u8) -> f64 {
    let Some(gear) = data.item(item).kind.gear() else {
        return 0.0;
    };
    let slot = target_slot(state, gear.slot);
    let current = state.equipment[slot.index()]
        .as_ref()
        .filter(|stack| !stack.is_broken(data))
        .map(|stack| score(state, data, stack.item, stack.upgrade))
        .unwrap_or(0.0);
    score(state, data, item, upgrade) - current
}

fn equip_upgrades(state: &mut GameState, data: &GameData) {
    for _ in 0..20 {
        let pick = state.inventory.iter().enumerate().find(|(_, stack)| {
            usable(state, data, stack.item) && gain(state, data, stack.item, stack.upgrade) > 0.01
        });
        let Some((index, _)) = pick else {
            return;
        };
        if (Command::Equip { index }).apply(state, data).is_err() {
            return;
        }
    }
}

fn learn_books(state: &mut GameState, data: &GameData) {
    for _ in 0..10 {
        let pick = state.inventory.iter().position(|stack| {
            matches!(&data.item(stack.item).kind, ItemKind::SkillBook(book)
                if book.req_class.allows(state.class)
                    && state.level >= book.req_level
                    && !state.skills.contains_key(&book.skill_id))
        });
        let Some(index) = pick else {
            return;
        };
        if (Command::Use { index }).apply(state, data).is_err() {
            return;
        }
    }
}

/// 卖掉别的职业的书、已经会的书、穿不上也不比身上好的装备。
fn sell_unwanted(state: &mut GameState, data: &GameData) {
    for _ in 0..60 {
        let pick = state.inventory.iter().position(|stack| {
            if stack.locked || stack.upgrade > 0 {
                return false;
            }
            match &data.item(stack.item).kind {
                ItemKind::SkillBook(book) => {
                    !book.req_class.allows(state.class) || state.skills.contains_key(&book.skill_id)
                }
                kind if kind.gear().is_some() => {
                    let gear = kind.gear().unwrap_or_else(|| unreachable!());
                    let wrong = !gear.req_class.allows(state.class)
                        || (kind.is_armor() && !gear.gender.allows(state.gender));
                    let later = state.level < gear.req_level;
                    wrong || (!later && gain(state, data, stack.item, stack.upgrade) <= 0.01)
                }
                _ => false,
            }
        });
        let Some(index) = pick else {
            return;
        };
        let count = state.inventory[index].count;
        if (Command::Sell { index, count }).apply(state, data).is_err() {
            return;
        }
    }
}

fn shops(data: &GameData) -> Vec<(usize, &game_data::ShopService)> {
    data.npcs
        .iter()
        .enumerate()
        .filter_map(|(index, npc)| {
            npc.services.iter().find_map(|service| match service {
                NpcService::Shop(shop) => Some((index, shop)),
                _ => None,
            })
        })
        .collect()
}

fn unit_price(data: &GameData, shop: &game_data::ShopService, item: game_data::ItemId) -> u64 {
    (f64::from(data.item(item).price) * f64::from(shop.buy_markup)).ceil() as u64
}

fn buy_books(state: &mut GameState, data: &GameData, reserve: u64) {
    for (npc, shop) in shops(data) {
        for (stock_index, item) in shop.stock_ids.iter().enumerate() {
            let ItemKind::SkillBook(book) = &data.item(*item).kind else {
                continue;
            };
            if book.req_class != class_req(state.class)
                || state.level < book.req_level
                || state.skills.contains_key(&book.skill_id)
            {
                continue;
            }
            if state.gold < unit_price(data, shop, *item) + reserve {
                continue;
            }
            let _ = Command::Buy {
                npc,
                stock_index,
                count: 1,
            }
            .apply(state, data);
            learn_books(state, data);
        }
    }
}

fn buy_gear(state: &mut GameState, data: &GameData, reserve: u64) {
    for _ in 0..EquipSlot::COUNT {
        let mut best: Option<(f64, usize, usize)> = None;
        for (npc, shop) in shops(data) {
            for (stock_index, item) in shop.stock_ids.iter().enumerate() {
                if !usable(state, data, *item) {
                    continue;
                }
                if state.gold < unit_price(data, shop, *item) + reserve {
                    continue;
                }
                let value = gain(state, data, *item, 0);
                if value > 0.01 && best.is_none_or(|(b, _, _)| value > b) {
                    best = Some((value, npc, stock_index));
                }
            }
        }
        let Some((_, npc, stock_index)) = best else {
            return;
        };
        if (Command::Buy {
            npc,
            stock_index,
            count: 1,
        })
        .apply(state, data)
        .is_err()
        {
            return;
        }
        equip_upgrades(state, data);
        sell_unwanted(state, data);
    }
}

fn class_req(class: Class) -> ClassReq {
    match class {
        Class::Warrior => ClassReq::Warrior,
        Class::Mage => ClassReq::Mage,
        Class::Taoist => ClassReq::Taoist,
    }
}
