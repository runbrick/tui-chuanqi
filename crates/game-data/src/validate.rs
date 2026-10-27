//! 启动期一次性校验。任何问题直接返回，核心层之后按 ID 索引。

use crate::error::DataError;
use crate::load::{shop_stock, skill_book_ref};
use crate::model::{
    BalanceConfig, ClassReq, GameData, ItemDef, ItemKind, MapDef, MonsterDef, NpcDef, NpcService,
    SkillDef,
};
use std::collections::{HashMap, HashSet};

pub(crate) fn validate(
    monsters: &[MonsterDef],
    items: &[ItemDef],
    skills: &[SkillDef],
    maps: &[MapDef],
    npcs: &[NpcDef],
    balance: &BalanceConfig,
) -> Result<(), DataError> {
    let monster_ids = unique_ids("monsters.ron", monsters.iter().map(|m| m.id.as_str()))?;
    let item_ids = unique_ids("items.ron", items.iter().map(|m| m.id.as_str()))?;
    let skill_ids = unique_ids("skills.ron", skills.iter().map(|m| m.id.as_str()))?;
    let map_ids = unique_ids("maps.ron", maps.iter().map(|m| m.id.as_str()))?;
    let _npc_ids = unique_ids("npcs.ron", npcs.iter().map(|m| m.id.as_str()))?;

    for monster in monsters {
        check_range(
            "monsters.ron",
            &monster.id,
            "攻击",
            monster.attack.is_ordered(),
        )?;
        check_range(
            "monsters.ron",
            &monster.id,
            "防御",
            monster.defense.is_ordered(),
        )?;
        check_range(
            "monsters.ron",
            &monster.id,
            "魔防",
            monster.magic_defense.is_ordered(),
        )?;
        if monster.hp <= 0 {
            return Err(fail("monsters.ron", &monster.id, "生命必须大于 0", ""));
        }
        if monster.attack_interval_ms == 0 {
            return Err(fail("monsters.ron", &monster.id, "攻击间隔必须大于 0", ""));
        }
        for drop in &monster.drops {
            check_chance("monsters.ron", &monster.id, drop.chance)?;
            if drop.min > drop.max {
                return Err(fail(
                    "monsters.ron",
                    &monster.id,
                    format!("掉落 '{}' 的数量 min > max", drop.item),
                    "",
                ));
            }
            if !item_ids.contains(drop.item.as_str()) {
                return Err(fail(
                    "monsters.ron",
                    &monster.id,
                    format!("的掉落表引用了不存在的物品 '{}'", drop.item),
                    suggest(&drop.item, &item_ids),
                ));
            }
        }
    }

    for item in items {
        if let Some(gear) = item.kind.gear() {
            for (label, range) in [
                ("攻击", gear.attack),
                ("魔法", gear.magic),
                ("道术", gear.taoism),
                ("防御", gear.defense),
                ("魔防", gear.magic_defense),
            ] {
                check_range("items.ron", &item.id, label, range.is_ordered())?;
            }
        }
        if let ItemKind::Material(material) = &item.kind {
            check_range("items.ron", &item.id, "纯度", material.purity.is_ordered())?;
        }
        if let Some((skill, req_class)) = skill_book_ref(&item.kind) {
            let Some(skill_def) = skills.iter().find(|s| s.id == skill) else {
                return Err(fail(
                    "items.ron",
                    &item.id,
                    format!("引用了不存在的技能 '{skill}'"),
                    suggest(skill, &skill_ids),
                ));
            };
            let expected = match skill_def.class {
                crate::model::Class::Warrior => ClassReq::Warrior,
                crate::model::Class::Mage => ClassReq::Mage,
                crate::model::Class::Taoist => ClassReq::Taoist,
            };
            if req_class != expected {
                return Err(fail(
                    "items.ron",
                    &item.id,
                    format!(
                        "的职业限制 {:?} 与技能 '{}' 的职业 {:?} 不一致",
                        req_class, skill, skill_def.class
                    ),
                    "把 req_class 改成和技能相同的职业",
                ));
            }
        }
    }

    for skill in skills {
        if skill.levels.len() != skill.max_level as usize {
            return Err(fail(
                "skills.ron",
                &skill.id,
                format!(
                    "levels 长度 {} 不等于 max_level {}",
                    skill.levels.len(),
                    skill.max_level
                ),
                "补齐或删掉多余等级",
            ));
        }
        if skill.max_level == 0 {
            return Err(fail("skills.ron", &skill.id, "max_level 必须大于 0", ""));
        }
        let mut prev = 0u16;
        for (i, level) in skill.levels.iter().enumerate() {
            if !level.power.is_ordered() {
                return Err(fail(
                    "skills.ron",
                    &skill.id,
                    format!("第 {} 级 power 的 min > max", i + 1),
                    "",
                ));
            }
            if i > 0 && level.req_level <= prev {
                return Err(fail("skills.ron", &skill.id, "req_level 必须严格递增", ""));
            }
            prev = level.req_level;
        }
    }

    for map in maps {
        if let Some((prev, _kills)) = &map.req_map_kills {
            if !map_ids.contains(prev.as_str()) {
                return Err(fail(
                    "maps.ron",
                    &map.id,
                    format!("的前置地图 '{}' 不存在", prev),
                    suggest(prev, &map_ids),
                ));
            }
        }
        for spawn in &map.spawns {
            if spawn.weight == 0 {
                return Err(fail(
                    "maps.ron",
                    &map.id,
                    format!("刷怪 '{}' 的权重必须大于 0", spawn.monster),
                    "",
                ));
            }
            if !monster_ids.contains(spawn.monster.as_str()) {
                return Err(fail(
                    "maps.ron",
                    &map.id,
                    format!("引用了不存在的怪物 '{}'", spawn.monster),
                    suggest(&spawn.monster, &monster_ids),
                ));
            }
        }
        if let Some(boss) = &map.boss {
            if !monster_ids.contains(boss.monster.as_str()) {
                return Err(fail(
                    "maps.ron",
                    &map.id,
                    format!("的 BOSS '{}' 不存在", boss.monster),
                    suggest(&boss.monster, &monster_ids),
                ));
            }
        }
    }
    detect_cycle(maps)?;
    if !maps
        .iter()
        .any(|map| map.req_level <= 1 && map.req_map_kills.is_none())
    {
        return Err(fail(
            "maps.ron",
            "*",
            "没有 req_level <= 1 且无前置的起始地图",
            "加一张新手安全区或野外",
        ));
    }

    for npc in npcs {
        if !map_ids.contains(npc.map.as_str()) {
            return Err(fail(
                "npcs.ron",
                &npc.id,
                format!("所在地图 '{}' 不存在", npc.map),
                suggest(&npc.map, &map_ids),
            ));
        }
        for service in &npc.services {
            if let Some(stock) = shop_stock(service) {
                for slug in stock {
                    if !item_ids.contains(slug.as_str()) {
                        return Err(fail(
                            "npcs.ron",
                            &npc.id,
                            format!("的商店引用了不存在的物品 '{slug}'"),
                            suggest(slug, &item_ids),
                        ));
                    }
                }
            }
            if let NpcService::Shop(shop) = service {
                if !(0.0..=10.0).contains(&shop.buy_markup)
                    || !(0.0..=1.0).contains(&shop.sell_rate)
                {
                    return Err(fail(
                        "npcs.ron",
                        &npc.id,
                        "商店倍率超出合理范围",
                        "buy_markup 0..10，sell_rate 0..1",
                    ));
                }
            }
        }
    }

    if balance.upgrade_success_rates.is_empty() {
        return Err(fail("balance.ron", "upgrade_success_rates", "不能为空", ""));
    }
    for rate in &balance.upgrade_success_rates {
        if !(0.0..=1.0).contains(rate) {
            return Err(fail(
                "balance.ron",
                "upgrade_success_rates",
                format!("概率 {rate} 不在 [0, 1]"),
                "",
            ));
        }
    }
    let hit = &balance.hit_chance;
    if !(0.0..=1.0).contains(&hit.min) || !(0.0..=1.0).contains(&hit.max) || hit.min > hit.max {
        return Err(fail(
            "balance.ron",
            "hit_chance",
            "上下限必须落在 [0, 1] 且 min <= max",
            "",
        ));
    }
    if !(0.0..=1.0).contains(&balance.death_penalty_exp_pct) {
        return Err(fail(
            "balance.ron",
            "death_penalty_exp_pct",
            "必须在 [0, 1]",
            "",
        ));
    }
    if balance.high_risk_drop_multiplier < 1.0 {
        return Err(fail(
            "balance.ron",
            "high_risk_drop_multiplier",
            "不能小于 1",
            "",
        ));
    }
    if !items
        .iter()
        .any(|item| matches!(item.kind, ItemKind::Currency))
    {
        return Err(fail(
            "items.ron",
            "*",
            "缺少 kind 为 Currency 的金币物品",
            "加一条 id 为 gold 的物品",
        ));
    }
    Ok(())
}

pub(crate) fn resolve_refs(data: &mut GameData) -> Result<(), DataError> {
    let monster_of = data.monster_index.clone();
    let item_of = data.item_index.clone();
    let skill_of = data.skill_index.clone();
    let map_of = data.map_index.clone();

    for monster in &mut data.monsters {
        for drop in &mut monster.drops {
            drop.item_id = item_of[&drop.item];
        }
    }
    for item in &mut data.items {
        if let ItemKind::SkillBook(book) = &mut item.kind {
            book.skill_id = skill_of[&book.skill];
        }
    }
    for map in &mut data.maps {
        map.req_map_id = map.req_map_kills.as_ref().map(|(slug, _)| map_of[slug]);
        for spawn in &mut map.spawns {
            spawn.monster_id = monster_of[&spawn.monster];
        }
        if let Some(boss) = &mut map.boss {
            boss.monster_id = monster_of[&boss.monster];
        }
    }
    for npc in &mut data.npcs {
        npc.map_id = map_of[&npc.map];
        for service in &mut npc.services {
            if let NpcService::Shop(shop) = service {
                shop.stock_ids = shop.stock.iter().map(|slug| item_of[slug]).collect();
            }
        }
    }
    Ok(())
}

fn unique_ids<'a>(
    file: &str,
    ids: impl Iterator<Item = &'a str>,
) -> Result<HashSet<&'a str>, DataError> {
    let mut set = HashSet::new();
    for id in ids {
        if id.is_empty() {
            return Err(fail(file, "*", "存在空 ID", ""));
        }
        if !set.insert(id) {
            return Err(fail(file, id, "ID 重复", "换一个没被占用的 slug"));
        }
    }
    Ok(set)
}

fn check_range(file: &str, entry: &str, label: &str, ok: bool) -> Result<(), DataError> {
    if ok {
        Ok(())
    } else {
        Err(fail(
            file,
            entry,
            format!("{label} 区间 min > max"),
            "把上下界对调",
        ))
    }
}

fn check_chance(file: &str, entry: &str, chance: f32) -> Result<(), DataError> {
    if (0.0..=1.0).contains(&chance) {
        Ok(())
    } else {
        Err(fail(
            file,
            entry,
            format!("概率 {chance} 不在 [0, 1]"),
            "改成 0 到 1 之间",
        ))
    }
}

fn detect_cycle(maps: &[MapDef]) -> Result<(), DataError> {
    let index: HashMap<&str, usize> = maps
        .iter()
        .enumerate()
        .map(|(i, map)| (map.id.as_str(), i))
        .collect();
    let mut color = vec![0u8; maps.len()];
    let mut stack = Vec::new();
    for start in 0..maps.len() {
        if color[start] != 0 {
            continue;
        }
        let mut cursor = start;
        loop {
            if color[cursor] == 1 {
                let slug = maps[cursor].id.as_str();
                let at = stack.iter().position(|id| *id == slug).unwrap_or(0);
                let mut path: Vec<&str> = stack[at..].to_vec();
                path.push(slug);
                return Err(fail(
                    "maps.ron",
                    slug,
                    format!("地图依赖成环：{}", path.join(" -> ")),
                    "去掉其中一条前置",
                ));
            }
            if color[cursor] == 2 {
                break;
            }
            color[cursor] = 1;
            stack.push(maps[cursor].id.as_str());
            if let Some((prev, _)) = &maps[cursor].req_map_kills {
                cursor = index[prev.as_str()];
            } else {
                break;
            }
        }
        for seen in stack.drain(..) {
            color[index[seen]] = 2;
        }
    }
    Ok(())
}

fn suggest(needle: &str, pool: &HashSet<&str>) -> String {
    let mut best: Option<(&str, usize)> = None;
    for candidate in pool {
        let dist = levenshtein(needle, candidate);
        let limit = if needle.len() <= 4 { 1 } else { 2 };
        if dist <= limit && best.is_none_or(|(_, best_dist)| dist < best_dist) {
            best = Some((*candidate, dist));
        }
    }
    match best {
        Some((name, _)) => format!("是否想写 '{name}'？"),
        None => String::new(),
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn fail(
    file: &str,
    entry: &str,
    message: impl Into<String>,
    suggestion: impl Into<String>,
) -> DataError {
    DataError::validation(file, entry, message, suggestion)
}
