use game_data::{Class, GameData, Gender};
use r_chuanqi::SaveStore;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn data() -> GameData {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    GameData::load(dir).unwrap()
}

fn temp_store() -> (PathBuf, SaveStore) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("tui-mir-{nanos}-{}", std::process::id()));
    let store = SaveStore::open(&root).unwrap();
    (root, store)
}

#[test]
fn same_name_characters_are_independent() {
    let data = data();
    let (root, store) = temp_store();
    let a = store
        .create(&data, "同名", Class::Warrior, Gender::Male, false)
        .unwrap();
    let b = store
        .create(&data, "同名", Class::Warrior, Gender::Female, true)
        .unwrap();
    store.delete(&a).unwrap();
    let list = store.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, b);
    assert!(list[0].high_risk);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn loading_one_slot_does_not_touch_the_other() {
    let data = data();
    let (root, store) = temp_store();
    let a = store
        .create(&data, "甲", Class::Warrior, Gender::Male, false)
        .unwrap();
    let b = store
        .create(&data, "乙", Class::Mage, Gender::Female, false)
        .unwrap();
    rewrite_last(&root, &a, unix() - 7_200);
    rewrite_last(&root, &b, unix() - 7_200);
    let before = fs::read(root.join("slots").join(&b).join("save.json")).unwrap();
    let (_state, report) = store.load(&a, &data).unwrap();
    let after = fs::read(root.join("slots").join(&b).join("save.json")).unwrap();
    assert_eq!(before, after);
    assert!(report.ticks >= 7_000, "ticks={}", report.ticks);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn truncated_save_falls_back_to_bak() {
    let data = data();
    let (root, store) = temp_store();
    let id = store
        .create(&data, "备份", Class::Taoist, Gender::Male, false)
        .unwrap();
    let path = root.join("slots").join(&id).join("save.json");
    fs::copy(&path, root.join("slots").join(&id).join("save.json.bak")).unwrap();
    fs::write(&path, "{").unwrap();
    let (state, _) = store.load(&id, &data).unwrap();
    assert_eq!(state.name, "备份");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn missing_index_is_rebuilt_from_slots() {
    let data = data();
    let (root, store) = temp_store();
    store
        .create(&data, "重建", Class::Warrior, Gender::Female, true)
        .unwrap();
    fs::remove_file(root.join("characters.json")).unwrap();
    let list = store.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "重建");
    assert!(root.join("characters.json").is_file());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn future_timestamp_does_not_grant_offline() {
    let data = data();
    let (root, store) = temp_store();
    let id = store
        .create(&data, "时光", Class::Warrior, Gender::Male, false)
        .unwrap();
    rewrite_last(&root, &id, unix() + 10_000);
    let (state, report) = store.load(&id, &data).unwrap();
    assert_eq!(report.ticks, 0);
    assert_eq!(state.level, 1);
    let _ = fs::remove_dir_all(root);
}

fn rewrite_last(root: &std::path::Path, id: &str, last: u64) {
    let path = root.join("slots").join(id).join("save.json");
    let mut value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    value["last_saved_at"] = Value::from(last);
    fs::write(path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
}

fn unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
