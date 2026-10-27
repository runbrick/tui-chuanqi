use game_core::{Command, CoreError, GameEvent, GameState, PityKey};
use game_data::{BalanceConfig, Class, EquipSlot, GameData, Gender, ItemKind, Range};
use std::time::Duration;

fn data() -> GameData {
    GameData::load(game_core::repo_data_dir()).unwrap()
}

fn warrior(seed: u64) -> (GameState, GameData) {
    let data = data();
    let state = GameState::new("战士", Class::Warrior, Gender::Male, false, &data, seed);
    (state, data)
}

#[test]
fn thousand_ticks_stay_sane() {
    let (mut state, data) = warrior(7);
    let mut level = state.level;
    for _ in 0..1000 {
        let _ = state.tick(&data);
        assert!(state.hp >= 0);
        assert!(state.level >= level);
        level = state.level;
    }
}

#[test]
fn same_seed_is_deterministic() {
    let (mut a, data) = warrior(42);
    let (mut b, _) = warrior(42);
    let ea: Vec<_> = (0..300).flat_map(|_| a.tick(&data)).collect();
    let eb: Vec<_> = (0..300).flat_map(|_| b.tick(&data)).collect();
    assert_eq!(ea, eb);
    assert_eq!(a.summary(&data), b.summary(&data));
}

#[test]
fn full_block_when_attack_cannot_pierce() {
    let (mut state, data) = warrior(1);
    state.base.attack = Range::new(0, 0);
    state.invalidate_derived();
    state.current_map = data.map_id("bichen_deep").unwrap();
    state.next_spawn_at = 0;
    let mut blocked = false;
    let mut dealt = false;
    for _ in 0..80 {
        for event in state.tick(&data) {
            match event {
                GameEvent::Blocked { .. } => blocked = true,
                GameEvent::Attacked { damage, .. } if damage > 0 => dealt = true,
                _ => {}
            }
        }
    }
    assert!(blocked);
    assert!(!dealt);
}

#[test]
fn pity_forces_the_configured_rare_drop() {
    let (mut state, data) = warrior(1);
    let corpse = data.monster_id("corpse").unwrap();
    let book = data.item_id("book_fire_sword").unwrap();
    let drop = data
        .monster(corpse)
        .drops
        .iter()
        .find(|drop| drop.item_id == book)
        .unwrap();
    let threshold = BalanceConfig::pity_threshold(drop.chance).unwrap();
    state.pity.insert(
        PityKey {
            monster: corpse,
            item: book,
        },
        threshold - 1,
    );
    state.base.attack = Range::new(500, 500);
    state.invalidate_derived();
    state.current_map = data.map_id("death_coffin").unwrap();
    // 地图未解锁也没关系，刷怪只看当前地图的表。直接跳到能刷行尸的图。
    state
        .map_kills
        .insert(data.map_id("skeleton_cave").unwrap(), 9999);
    state.level = 30;
    state.next_spawn_at = 0;
    let mut got = false;
    for _ in 0..400 {
        for event in state.tick(&data) {
            if matches!(event, GameEvent::ItemDropped { id, .. } if id == book) {
                got = true;
            }
        }
        if got {
            break;
        }
    }
    assert!(got, "pity counter should force the book");
}

#[test]
fn gender_mismatch_explains_why() {
    let (mut state, data) = warrior(1);
    let dress = data.item_id("cloth_f").unwrap();
    state
        .inventory
        .push(game_core::ItemStack::fresh(dress, 1, &data, &mut state.rng));
    let err = Command::Equip { index: 0 }
        .apply(&mut state, &data)
        .unwrap_err();
    assert!(matches!(err, CoreError::GenderMismatch(_)));
    assert!(err.to_string().contains("女"), "{err}");
}

#[test]
fn offline_two_hours_advances_ticks() {
    let (mut state, data) = warrior(3);
    let before = state.tick_count;
    let report = state.simulate_offline(&data, Duration::from_secs(2 * 3600));
    assert_eq!(state.tick_count - before, 72_000);
    assert_eq!(report.ticks, 72_000);
    assert!(state.stats.kills > 0);
}

#[test]
fn snapshot_warrior_seed_42() {
    let (mut state, data) = warrior(42);
    for _ in 0..5_000 {
        let _ = state.tick(&data);
    }
    insta::assert_snapshot!(state.summary(&data));
}

#[test]
fn upgrade_rate_near_configured() {
    let (mut state, data) = warrior(9);
    let sword = data.item_id("wooden_sword").unwrap();
    let ore = data.item_id("black_iron_ore").unwrap();
    let ring = data.item_id("iron_ring").unwrap();
    state.gold = 100_000_000;
    state
        .inventory
        .push(game_core::ItemStack::fresh(sword, 1, &data, &mut state.rng));
    Command::Equip { index: 0 }
        .apply(&mut state, &data)
        .unwrap();
    let mut success = 0u32;
    for _ in 0..2_000 {
        state.gold = 100_000_000;
        if state.equipment[EquipSlot::Weapon.index()].is_none() {
            state
                .inventory
                .push(game_core::ItemStack::fresh(sword, 1, &data, &mut state.rng));
            let index = state.inventory.len() - 1;
            Command::Equip { index }.apply(&mut state, &data).unwrap();
        }
        if let Some(weapon) = state.equipment[EquipSlot::Weapon.index()].as_mut() {
            weapon.upgrade = 0;
        }
        state
            .inventory
            .push(game_core::ItemStack::fresh(ore, 1, &data, &mut state.rng));
        state
            .inventory
            .push(game_core::ItemStack::fresh(ring, 1, &data, &mut state.rng));
        let accessory_index = state
            .inventory
            .iter()
            .rposition(|stack| stack.item == ring)
            .unwrap();
        let events = Command::UpgradeWeapon { accessory_index }
            .apply(&mut state, &data)
            .unwrap();
        if events
            .iter()
            .any(|event| matches!(event, GameEvent::UpgradeSucceeded { .. }))
        {
            success += 1;
        }
    }
    let rate = success as f64 / 2000.0;
    assert!((rate - 0.95).abs() < 0.03, "rate={rate}");
}

#[test]
fn luck_charm_prevents_shatter() {
    let (mut state, data) = warrior(1);
    let sword = data.item_id("wooden_sword").unwrap();
    let ore = data.item_id("black_iron_ore").unwrap();
    let ring = data.item_id("iron_ring").unwrap();
    let charm = data.item_id("luck_charm").unwrap();
    state.gold = 10_000_000;
    state
        .inventory
        .push(game_core::ItemStack::fresh(sword, 1, &data, &mut state.rng));
    Command::Equip { index: 0 }
        .apply(&mut state, &data)
        .unwrap();
    let mut protected = false;
    for _ in 0..40 {
        state
            .inventory
            .push(game_core::ItemStack::fresh(charm, 1, &data, &mut state.rng));
        state
            .inventory
            .push(game_core::ItemStack::fresh(ore, 1, &data, &mut state.rng));
        state
            .inventory
            .push(game_core::ItemStack::fresh(ring, 1, &data, &mut state.rng));
        if let Some(weapon) = state.equipment[EquipSlot::Weapon.index()].as_mut() {
            weapon.upgrade = 4;
        }
        let accessory = state
            .inventory
            .iter()
            .rposition(|stack| stack.item == ring)
            .unwrap();
        let events = Command::UpgradeWeapon {
            accessory_index: accessory,
        }
        .apply(&mut state, &data)
        .unwrap();
        assert!(
            state.equipment[EquipSlot::Weapon.index()].is_some(),
            "charm must keep the weapon"
        );
        if events
            .iter()
            .any(|event| matches!(event, GameEvent::UpgradeProtected))
        {
            protected = true;
            break;
        }
    }
    assert!(protected);
    let _ = ItemKind::LuckCharm;
}

#[test]
fn negative_elapsed_is_zero_gain() {
    let (mut state, data) = warrior(1);
    let before = state.summary(&data);
    let report = state.simulate_offline(&data, Duration::ZERO);
    assert_eq!(report.ticks, 0);
    assert_eq!(state.summary(&data), before);
}

#[test]
fn offline_twelve_hours_is_fast_enough_in_release() {
    if cfg!(debug_assertions) {
        return;
    }
    let (mut state, data) = warrior(42);
    let started = std::time::Instant::now();
    let report = state.simulate_offline(&data, Duration::from_secs(12 * 3600));
    assert_eq!(report.ticks, 432_000);
    assert!(
        started.elapsed().as_millis() < 500,
        "elapsed={:?}",
        started.elapsed()
    );
}

#[test]
fn save_roundtrip_keeps_rng() {
    let (mut state, data) = warrior(11);
    for _ in 0..50 {
        let _ = state.tick(&data);
    }
    let json = game_core::state_to_json(&state, &data).unwrap();
    let loaded = game_core::state_from_json(&json, &data).unwrap();
    let mut a = state;
    let mut b = loaded;
    let ea: Vec<_> = (0..30).flat_map(|_| a.tick(&data)).collect();
    let eb: Vec<_> = (0..30).flat_map(|_| b.tick(&data)).collect();
    assert_eq!(ea, eb);
}
