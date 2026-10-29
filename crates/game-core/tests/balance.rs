//! 数值和挂机流程的回归测试。每条都对应一个曾经让挂机卡死或白白浪费资源的问题。

use game_core::{Command, CoreError, GameEvent, GameState, SkillProgress};
use game_data::{Class, EquipSlot, GameData, Gender, ItemKind, Range};
use std::time::Duration;

fn data() -> GameData {
    GameData::load(game_core::repo_data_dir()).unwrap()
}

/// 去掉新手装的角色，方便精确控制背包和属性。
fn bare(class: Class, gender: Gender) -> (GameState, GameData) {
    let data = data();
    let mut state = GameState::new("测", class, gender, false, &data, 5);
    state.inventory.clear();
    for slot in state.equipment.iter_mut() {
        *slot = None;
    }
    state.invalidate_derived();
    state.clamp_vitals(&data);
    (state, data)
}

fn learn(state: &mut GameState, data: &GameData, skill: &str, level: u8) {
    state.skills.insert(
        data.skill_id(skill).unwrap(),
        SkillProgress {
            level,
            exp: 0,
            auto: true,
        },
    );
    state.invalidate_derived();
}

fn put(state: &mut GameState, data: &GameData, item: &str, count: u32) {
    let id = data.item_id(item).unwrap();
    let stack = game_core::ItemStack::fresh(id, count, data, &mut state.rng);
    state.inventory.push(stack);
}

fn count_of(state: &GameState, data: &GameData, item: &str) -> u32 {
    let id = data.item_id(item).unwrap();
    state
        .inventory
        .iter()
        .filter(|stack| stack.item == id)
        .map(|stack| stack.count)
        .sum()
}

fn tough(state: &mut GameState) {
    state.base.max_hp = 5000;
    state.base.defense = Range::new(500, 500);
    state.base.magic_defense = Range::new(500, 500);
    state.invalidate_derived();
    state.hp = 5000;
}

#[test]
fn new_character_wears_matching_starter_kit() {
    let data = data();
    let man = GameState::new("男", Class::Warrior, Gender::Male, false, &data, 1);
    let woman = GameState::new("女", Class::Mage, Gender::Female, false, &data, 1);
    let worn = |state: &GameState, slot: EquipSlot| {
        state.equipment[slot.index()]
            .as_ref()
            .map(|stack| data.item(stack.item).id.clone())
    };
    assert_eq!(
        worn(&man, EquipSlot::Weapon).as_deref(),
        Some("wooden_sword")
    );
    assert_eq!(worn(&man, EquipSlot::Armor).as_deref(), Some("cloth_m"));
    assert_eq!(worn(&woman, EquipSlot::Armor).as_deref(), Some("cloth_f"));
    assert!(count_of(&woman, &data, "potion_hp_small") > 0);
    assert!(count_of(&woman, &data, "potion_mp_small") > 0);
    assert_eq!(
        count_of(&woman, &data, "cloth_m"),
        0,
        "穿不上的那件不该塞进背包"
    );
    assert_eq!(man.hp, man.derived_stats(&data).max_hp);
}

#[test]
fn retreat_without_potions_returns_to_the_same_map() {
    let (mut state, data) = bare(Class::Warrior, Gender::Male);
    let field = data.map_id("bichen_field").unwrap();
    state.current_map = field;
    for _ in 0..200 {
        let _ = state.tick(&data);
        if state.combat.is_some() {
            break;
        }
    }
    assert!(state.combat.is_some());
    state.hp = 1;
    let mut back = false;
    for _ in 0..400 {
        for event in state.tick(&data) {
            if matches!(event, GameEvent::ReturnedToHunt { map } if map == field) {
                back = true;
            }
        }
        if back {
            break;
        }
    }
    assert!(back, "休息完应该回原来的图，而不是一直待在城里");
    assert_eq!(state.current_map, field);
}

#[test]
fn repeated_deaths_fall_back_to_the_previous_map() {
    let (mut state, data) = bare(Class::Warrior, Gender::Male);
    state.level = 11;
    state.base.max_hp = 1;
    state.base.defense = Range::new(0, 0);
    state.invalidate_derived();
    state.hp = 1;
    state.automation.auto_potion = false;
    put(&mut state, &data, "potion_hp_small", 5);
    state.current_map = data.map_id("viper_valley").unwrap();
    state.death_streak = 2;
    let mut fell_back = None;
    for _ in 0..3000 {
        for event in state.tick(&data) {
            if let GameEvent::FellBack { map, .. } = event {
                fell_back = Some(map);
            }
        }
        if state.stats.deaths > 0 {
            break;
        }
    }
    assert_eq!(state.stats.deaths, 1);
    assert_eq!(fell_back, data.map_id("bichen_deep"));
}

#[test]
fn restock_splits_a_small_budget_instead_of_buying_nothing() {
    let (mut state, data) = bare(Class::Warrior, Gender::Male);
    state.current_map = data.safe_map().unwrap();
    state.gold = 100;
    let events = Command::Restock { hp: 5, mp: 5 }
        .apply(&mut state, &data)
        .unwrap();
    assert_eq!(count_of(&state, &data, "potion_hp_small"), 2);
    assert_eq!(count_of(&state, &data, "potion_mp_small"), 2);
    assert_eq!(state.gold, 10);
    assert!(events.iter().any(|event| matches!(
        event,
        GameEvent::Restocked {
            hp: 2,
            mp: 2,
            gold: 90
        }
    )));
    state.gold = 0;
    let err = Command::Restock { hp: 5, mp: 5 }
        .apply(&mut state, &data)
        .unwrap_err();
    assert_eq!(err, CoreError::NotEnoughGold);
}

#[test]
fn auto_potion_waits_for_the_slow_heal_to_land() {
    let (mut state, data) = bare(Class::Warrior, Gender::Male);
    state.current_map = data.safe_map().unwrap();
    put(&mut state, &data, "potion_hp_small", 5);
    let max = state.derived_stats(&data).max_hp;
    state.hp = max * 2 / 5;
    for _ in 0..5 {
        let _ = state.tick(&data);
    }
    assert_eq!(
        count_of(&state, &data, "potion_hp_small"),
        4,
        "回血药还在生效时不该连着喝"
    );
}

#[test]
fn over_trained_skill_still_casts_at_the_tier_the_level_allows() {
    let (mut state, data) = bare(Class::Mage, Gender::Male);
    state.level = 7;
    state.exp = 0;
    tough(&mut state);
    state.base.max_mp = 500;
    state.base.magic = Range::new(10, 20);
    state.invalidate_derived();
    state.mp = 500;
    learn(&mut state, &data, "fire_ball", 2);
    state.current_map = data.map_id("bichen_field").unwrap();
    let fire = data.skill_id("fire_ball").unwrap();
    let mut casts = 0;
    for _ in 0..300 {
        casts += state
            .tick(&data)
            .iter()
            .filter(|event| matches!(event, GameEvent::SkillCast { id } if *id == fire))
            .count();
    }
    assert!(
        casts > 0,
        "熟练度先升到 2 级、角色才 7 级时，火球术应该按 1 级放"
    );
    let progress = &state.skills[&fire];
    assert_eq!(progress.level, 2, "熟练度不该再往上冲");
}

#[test]
fn heal_is_not_wasted_at_full_health() {
    let (mut state, data) = bare(Class::Taoist, Gender::Male);
    state.level = 7;
    tough(&mut state);
    learn(&mut state, &data, "heal", 1);
    state.current_map = data.map_id("bichen_field").unwrap();
    let heal = data.skill_id("heal").unwrap();
    for _ in 0..300 {
        for event in state.tick(&data) {
            assert!(
                !matches!(event, GameEvent::SkillCast { id } if id == heal),
                "满血时不该放治愈术"
            );
        }
    }
}

#[test]
fn fire_wall_uses_magic_and_pulses_once_a_second() {
    let (mut state, data) = bare(Class::Mage, Gender::Male);
    state.level = 22;
    tough(&mut state);
    state.base.max_mp = 500;
    state.base.magic = Range::new(30, 40);
    state.invalidate_derived();
    state.mp = 500;
    learn(&mut state, &data, "fire_wall", 1);
    state.current_map = data.map_id("bichen_deep").unwrap();
    let mut pulses = Vec::new();
    for tick in 0..400u32 {
        for event in state.tick(&data) {
            if let GameEvent::DotDamage { damage, .. } = event {
                pulses.push((tick, damage));
            }
        }
    }
    assert!(pulses.len() >= 3, "{pulses:?}");
    assert!(
        pulses.iter().all(|(_, damage)| *damage >= 10),
        "火墙按魔法算，不该只有 1 点：{pulses:?}"
    );
    assert!(
        pulses.windows(2).all(|pair| pair[1].0 - pair[0].0 >= 10),
        "持续伤害一秒一跳：{pulses:?}"
    );
}

#[test]
fn auto_advance_skips_dead_end_boss_maps() {
    let (mut state, data) = bare(Class::Warrior, Gender::Male);
    state.level = 32;
    state.automation.auto_advance = true;
    state.current_map = data.map_id("woma_3").unwrap();
    state.map_kills.insert(data.map_id("woma_3").unwrap(), 500);
    state.next_spawn_at = u64::MAX / 2;
    let _ = state.tick(&data);
    assert_eq!(
        state.current_map,
        data.map_id("pig_cave").unwrap(),
        "沃玛七层是支线尽头，主线下一张是猪洞"
    );
}

#[test]
fn auto_advance_never_pulls_the_player_out_of_town() {
    let (mut state, data) = bare(Class::Warrior, Gender::Male);
    state.level = 20;
    state.automation.auto_advance = true;
    let city = data.safe_map().unwrap();
    state.current_map = city;
    for map in &data.maps {
        state.map_kills.insert(map.runtime_id, 500);
    }
    for _ in 0..50 {
        let _ = state.tick(&data);
    }
    assert_eq!(state.current_map, city);
}

#[test]
fn repair_costs_a_share_of_the_price() {
    let (mut state, data) = bare(Class::Warrior, Gender::Male);
    put(&mut state, &data, "wooden_sword", 1);
    Command::Equip { index: 0 }
        .apply(&mut state, &data)
        .unwrap();
    if let Some(weapon) = state.equipment[EquipSlot::Weapon.index()].as_mut() {
        weapon.durability = 0;
    }
    state.gold = 1000;
    let events = Command::Repair.apply(&mut state, &data).unwrap();
    let price = data.item(data.item_id("wooden_sword").unwrap()).price;
    let spent = 1000 - state.gold;
    assert!(
        spent <= u64::from(price),
        "修木剑花了 {spent}，比买新的还贵"
    );
    assert!(events
        .iter()
        .any(|event| matches!(event, GameEvent::Repaired { pieces: 1, .. })));
}

#[test]
fn locked_gear_is_never_used_as_upgrade_material() {
    let (mut state, data) = bare(Class::Warrior, Gender::Male);
    state.gold = 1_000_000;
    put(&mut state, &data, "wooden_sword", 1);
    Command::Equip { index: 0 }
        .apply(&mut state, &data)
        .unwrap();
    put(&mut state, &data, "black_iron_ore", 1);
    put(&mut state, &data, "iron_ring", 1);
    let ring = state.inventory.len() - 1;
    state.inventory[ring].locked = true;
    let err = Command::UpgradeWeapon {
        accessory_index: ring,
    }
    .apply(&mut state, &data)
    .unwrap_err();
    assert_eq!(err, CoreError::ItemLocked);
    assert_eq!(count_of(&state, &data, "iron_ring"), 1);
}

#[test]
fn manual_map_change_cancels_the_auto_return() {
    let (mut state, data) = bare(Class::Warrior, Gender::Male);
    let city = data.safe_map().unwrap();
    state.current_map = city;
    state.return_map = data.map_id("bichen_field");
    state.death_streak = 2;
    Command::ChangeMap { map: city }
        .apply(&mut state, &data)
        .unwrap();
    assert_eq!(state.return_map, None);
    assert_eq!(state.death_streak, 0);
}

#[test]
fn mage_keeps_progressing_offline() {
    let data = data();
    let mut state = GameState::new("法", Class::Mage, Gender::Male, false, &data, 3);
    let report = state.simulate_offline(&data, Duration::from_secs(4 * 3600));
    assert!(state.level >= 6, "lv={}", state.level);
    assert!(report.kills > 300, "kills={}", report.kills);
}

#[test]
fn old_saves_without_new_fields_still_load() {
    let data = data();
    let state = GameState::new("旧", Class::Taoist, Gender::Female, false, &data, 8);
    let json = game_core::state_to_json(&state, &data).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("return_map");
    object.remove("death_streak");
    let loaded = game_core::state_from_json(&value.to_string(), &data).unwrap();
    assert_eq!(loaded.return_map, None);
    assert_eq!(loaded.death_streak, 0);
}

#[test]
fn bigger_potions_cost_a_modest_premium() {
    let data = data();
    // 大瓶回得快，可以贵一点，但不能贵到反而不如多买小瓶。
    for (small, large, heal) in [
        ("potion_hp_small", "potion_hp_large", true),
        ("potion_mp_small", "potion_mp_large", false),
    ] {
        let per = |id: &str| {
            let def = data.item(data.item_id(id).unwrap());
            let ItemKind::Consumable(p) = &def.kind else {
                panic!("{id} 不是药");
            };
            let amount = if heal { p.heal_hp } else { p.heal_mp };
            f64::from(def.price) / f64::from(amount)
        };
        assert!(per(large) <= 1.0, "{large} 每点 {:.2} 金", per(large));
        assert!(per(large) <= per(small) * 1.6, "{large} 比 {small} 贵太多");
    }
}
