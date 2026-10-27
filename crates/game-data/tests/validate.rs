use game_data::{ClassReq, DataError, GameData, NamedSource};

fn base() -> Vec<NamedSource> {
    vec![
        NamedSource::new(
            "monsters.ron",
            r#"[
                (id: "chicken", name: "鸡", version_origin: r#1.67, level: 1, hp: 8,
                 attack: (0, 1), defense: (0, 0), magic_defense: (0, 0), accuracy: 1, agility: 2,
                 attack_interval_ms: 1800, exp: 3, undead: false, attack_type: Melee,
                 drops: [(item: "gold", chance: 0.6, min: 1, max: 5)]),
                (id: "deer", name: "鹿", version_origin: r#1.67, level: 3, hp: 20,
                 attack: (1, 3), defense: (0, 1), magic_defense: (0, 0), accuracy: 2, agility: 3,
                 attack_interval_ms: 1600, exp: 8, undead: false, attack_type: Melee,
                 drops: [(item: "gold", chance: 0.8, min: 2, max: 8)]),
                (id: "hook_cat", name: "多钩猫", version_origin: r#1.67, level: 6, hp: 40,
                 attack: (2, 5), defense: (0, 2), magic_defense: (0, 1), accuracy: 3, agility: 4,
                 attack_interval_ms: 1500, exp: 16, undead: false, attack_type: Melee,
                 drops: [(item: "gold", chance: 1.0, min: 4, max: 12)]),
            ]"#,
        ),
        NamedSource::new(
            "items.ron",
            r#"[
                (id: "gold", name: "金币", version_origin: r#1.67, quality: White, price: 1, stackable: true, kind: Currency),
                (id: "potion_hp_small", name: "金创药(小)", version_origin: r#1.67, quality: White, price: 20, stackable: true,
                 kind: Consumable((heal_hp: 20, heal_mp: 0, instant: true, duration_ticks: 0))),
                (id: "wooden_sword", name: "木剑", version_origin: r#1.67, quality: White, price: 50, stackable: false,
                 kind: Weapon((slot: Weapon, req_level: 1, req_class: Any, attack: (2, 5), durability: 2000))),
                (id: "cloth_m", name: "布衣(男)", version_origin: r#1.67, quality: White, price: 40, stackable: false,
                 kind: Armor((slot: Armor, req_level: 1, req_class: Any, gender: Male, defense: (1, 2), durability: 2000))),
                (id: "book_basic_sword", name: "基本剑术(书)", version_origin: r#1.67, quality: Blue, price: 100, stackable: false,
                 kind: SkillBook((skill: "basic_sword", req_level: 7, req_class: Warrior))),
            ]"#,
        ),
        NamedSource::new(
            "skills.ron",
            r#"[
                (id: "basic_sword", name: "基本剑术", version_origin: r#1.67, class: Warrior, max_level: 1,
                 levels: [(req_level: 7, mp_cost: 0, cooldown_ms: 0, power: (1.0, 1.0))],
                 effect: Passive((accuracy: 3))),
            ]"#,
        ),
        NamedSource::new(
            "maps.ron",
            r#"[
                (id: "bichen_city", name: "比奇城", version_origin: r#1.67, req_level: 1, req_map_kills: None,
                 spawn_interval_ms: 0, safe_zone: true, spawns: [], boss: None),
                (id: "bichen_field", name: "比奇省", version_origin: r#1.67, req_level: 1, req_map_kills: None,
                 spawn_interval_ms: 1500, safe_zone: false,
                 spawns: [(monster: "chicken", weight: 50), (monster: "deer", weight: 30), (monster: "hook_cat", weight: 20)],
                 boss: None),
            ]"#,
        ),
        NamedSource::new(
            "npcs.ron",
            r#"[
                (id: "bichen_potion", name: "药店老板", version_origin: r#1.67, map: "bichen_city",
                 services: [Shop((stock: ["potion_hp_small"], buy_markup: 1.0, sell_rate: 0.3))],
                 greeting: "药都在这儿。"),
            ]"#,
        ),
        NamedSource::new(
            "balance.ron",
            r#"(
                exp_curve: (base: 40.0, exponent: 2.2),
                hit_chance: (base: 0.75, per_point: 0.05, min: 0.15, max: 0.98),
                death_penalty_exp_pct: 0.05,
                death_rest_ticks: 300,
                offline_cap_hours: 12,
                upgrade_success_rates: [0.95, 0.85, 0.70, 0.50, 0.30],
                upgrade_gold_base: 1000,
                potion_hp_ratio: 0.5,
                potion_mp_ratio: 0.3,
                retreat_hp_ratio: 0.2,
                high_risk_drop_multiplier: 1.5,
                inventory_slots: 40,
                storage_slots: 50,
                starting_gold: 200,
                skill_exp_base: 20,
                auto_coarse_level: 5,
                auto_fine_level: 15,
                auto_advance_level: 4,
                max_level: 60,
                classes: (
                    warrior: (
                        hp: 40, mp: 15, attack: (2, 6), magic: (0, 0), taoism: (0, 0),
                        defense: (0, 1), magic_defense: (0, 0), accuracy: 8, agility: 5,
                        attack_interval_ms: 1200,
                        growth: (hp: 8, mp: 1, attack: (1, 1), magic: (0, 0), taoism: (0, 0), defense: (0, 1), magic_defense: (0, 0), accuracy: 1, agility: 0),
                    ),
                    mage: (
                        hp: 20, mp: 40, attack: (1, 2), magic: (2, 6), taoism: (0, 0),
                        defense: (0, 0), magic_defense: (0, 1), accuracy: 4, agility: 6,
                        attack_interval_ms: 1400,
                        growth: (hp: 3, mp: 6, attack: (0, 0), magic: (1, 1), taoism: (0, 0), defense: (0, 0), magic_defense: (0, 1), accuracy: 0, agility: 0),
                    ),
                    taoist: (
                        hp: 28, mp: 28, attack: (1, 3), magic: (0, 1), taoism: (2, 5),
                        defense: (0, 1), magic_defense: (0, 1), accuracy: 6, agility: 5,
                        attack_interval_ms: 1300,
                        growth: (hp: 5, mp: 4, attack: (0, 0), magic: (0, 0), taoism: (1, 1), defense: (0, 0), magic_defense: (0, 0), accuracy: 0, agility: 0),
                    ),
                ),
            )"#,
        ),
    ]
}

#[test]
fn minimal_dataset_loads() {
    let data = GameData::load_sources(&base()).expect("base dataset");
    assert_eq!(data.monsters.len(), 3);
    let chicken = data.monster(data.monster_id("chicken").unwrap());
    assert_eq!(chicken.drops[0].item_id, data.item_id("gold").unwrap());
    assert!(data.starting_map().is_some());
}

#[test]
fn unknown_drop_names_file_entry_and_suggestion() {
    let mut sources = base();
    sources[0].text = sources[0].text.replace("item: \"gold\"", "item: \"gld\"");
    let err = GameData::load_sources(&sources).unwrap_err();
    let text = err.to_string();
    assert!(text.contains("monsters.ron"), "{text}");
    assert!(text.contains("chicken"), "{text}");
    assert!(text.contains("gld"), "{text}");
    assert!(text.contains("gold"), "{text}");
    match err {
        DataError::Validation { suggestion, .. } => {
            assert!(suggestion.contains("gold"), "{suggestion}")
        }
        other => panic!("{other}"),
    }
}

#[test]
fn map_cycle_reports_path() {
    let mut sources = base();
    sources[3].text = r#"[
        (id: "a", name: "甲", version_origin: r#1.67, req_level: 1, req_map_kills: Some(("b", 1)),
         spawn_interval_ms: 1000, safe_zone: false, spawns: [(monster: "chicken", weight: 1)], boss: None),
        (id: "b", name: "乙", version_origin: r#1.67, req_level: 1, req_map_kills: Some(("a", 1)),
         spawn_interval_ms: 1000, safe_zone: false, spawns: [(monster: "chicken", weight: 1)], boss: None),
    ]"#
    .into();
    let text = GameData::load_sources(&sources).unwrap_err().to_string();
    assert!(
        text.contains("a -> b -> a") || text.contains("b -> a -> b"),
        "{text}"
    );
}

#[test]
fn duplicate_id_rejected() {
    let mut sources = base();
    sources[0].text = sources[0]
        .text
        .replacen("id: \"deer\"", "id: \"chicken\"", 1);
    let text = GameData::load_sources(&sources).unwrap_err().to_string();
    assert!(text.contains("重复"), "{text}");
    assert!(text.contains("chicken"), "{text}");
}

#[test]
fn chance_out_of_range_rejected() {
    let mut sources = base();
    sources[0].text = sources[0].text.replacen("chance: 0.6", "chance: 1.5", 1);
    let text = GameData::load_sources(&sources).unwrap_err().to_string();
    assert!(text.contains("概率"), "{text}");
    assert!(text.contains("chicken"), "{text}");
}

#[test]
fn inverted_range_rejected() {
    let mut sources = base();
    sources[0].text = sources[0]
        .text
        .replacen("attack: (0, 1)", "attack: (5, 1)", 1);
    let text = GameData::load_sources(&sources).unwrap_err().to_string();
    assert!(text.contains("min > max"), "{text}");
}

#[test]
fn skillbook_class_mismatch_rejected() {
    let mut sources = base();
    sources[1].text = sources[1]
        .text
        .replace("req_class: Warrior", "req_class: Mage");
    let text = GameData::load_sources(&sources).unwrap_err().to_string();
    assert!(text.contains("不一致"), "{text}");
    assert!(text.contains("book_basic_sword"), "{text}");
}

#[test]
fn missing_start_map_rejected() {
    let mut sources = base();
    sources[3].text = r#"[
        (id: "deep", name: "深处", version_origin: r#1.67, req_level: 10, req_map_kills: None,
         spawn_interval_ms: 1000, safe_zone: false, spawns: [(monster: "chicken", weight: 1)], boss: None),
    ]"#
    .into();
    let text = GameData::load_sources(&sources).unwrap_err().to_string();
    assert!(text.contains("起始地图"), "{text}");
}

#[test]
fn skill_level_length_rejected() {
    let mut sources = base();
    sources[2].text = sources[2].text.replace("max_level: 1", "max_level: 3");
    let text = GameData::load_sources(&sources).unwrap_err().to_string();
    assert!(text.contains("max_level"), "{text}");
}

#[test]
fn class_req_allows() {
    assert!(ClassReq::Any.allows(game_data::Class::Mage));
    assert!(!ClassReq::Warrior.allows(game_data::Class::Mage));
}

#[test]
fn repo_data_directory_loads() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let data = GameData::load(&root).expect("repo data");
    assert!(data.monsters.len() >= 3);
    assert!(data.starting_map().is_some());
    assert!(data.safe_map().is_some());
}
