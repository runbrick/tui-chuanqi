//! 内容空洞检查。加载失败直接非 0；空洞只打印警告。

use game_data::{EquipSlot, GameData, ItemKind};
use game_tui::find_data_dir;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let data_dir = find_data_dir().ok_or("找不到 data 目录")?;
    let data = GameData::load(&data_dir).map_err(|err| err.to_string())?;
    let mut warnings = 0;
    for band in (1..60).step_by(5) {
        for slot in [
            EquipSlot::Weapon,
            EquipSlot::Armor,
            EquipSlot::Helmet,
            EquipSlot::Necklace,
            EquipSlot::BraceletL,
            EquipSlot::RingL,
        ] {
            let covered = data.items.iter().any(|item| {
                let Some(gear) = item.kind.gear() else {
                    return false;
                };
                gear.slot == slot && gear.req_level >= band && gear.req_level < band + 5
            });
            if !covered {
                println!("Lv.{band}-{} 没有 {} 可穿", band + 4, slot.label());
                warnings += 1;
            }
        }
    }
    for monster in &data.monsters {
        if monster.drops.is_empty() {
            println!("怪物 {} 没有掉落", monster.name);
            warnings += 1;
        }
    }
    for skill in &data.skills {
        let book = data
            .items
            .iter()
            .any(|item| matches!(&item.kind, ItemKind::SkillBook(book) if book.skill == skill.id));
        if !book {
            println!("技能 {} 没有技能书", skill.name);
            warnings += 1;
        }
    }
    println!(
        "检查完成，警告 {warnings} 条。怪物 {}，地图 {}，物品 {}，技能 {}。",
        data.monsters.len(),
        data.maps.len(),
        data.items.len(),
        data.skills.len()
    );
    Ok(())
}
