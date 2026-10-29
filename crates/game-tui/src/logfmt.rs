//! 把核心事件写成一行中文日志。

use game_core::GameEvent;
use game_data::GameData;

/// 一行日志。`rare` 给界面上色，不代表保底。
pub struct LogLine {
    /// 文本。
    pub text: String,
    /// 稀有掉落。
    pub rare: bool,
    /// 升级。
    pub level: bool,
    /// 死亡或碎裂。
    pub bad: bool,
    /// 自己挂彩。颜色比 `bad` 淡，方便一眼分清谁在打谁。
    pub hurt: bool,
}

/// 格式化一条事件。
pub fn format_event(event: &GameEvent, data: &GameData) -> LogLine {
    let monster = |id| data.monster(id).name.clone();
    let item = |id| item_label(data, id);
    let skill = |id| data.skill(id).name.clone();
    let map_name = |id| data.map(id).name.clone();
    let mut line = LogLine {
        text: String::new(),
        rare: false,
        level: false,
        bad: false,
        hurt: false,
    };
    line.text = match event {
        GameEvent::Attacked { target, damage, .. } => {
            format!("命中 {}，{} 点", monster(*target), damage)
        }
        GameEvent::Blocked { target } => format!("{} 抵挡了你的攻击", monster(*target)),
        GameEvent::Missed { target } => format!("攻击 {} 落空", monster(*target)),
        GameEvent::MonsterHit { source, damage } => {
            line.hurt = true;
            format!("{} 击中你，{} 点", monster(*source), damage)
        }
        GameEvent::MonsterBlocked { source } => format!("你抵挡了 {} 的攻击", monster(*source)),
        GameEvent::MonsterMissed { source } => format!("你躲开了 {}", monster(*source)),
        GameEvent::MonsterSlain { id, exp } => format!("击杀 {}，获得 {} 经验", monster(*id), exp),
        GameEvent::ItemDropped { id, count, rare } => {
            line.rare = *rare;
            let mark = if *rare { " ★" } else { "" };
            format!("{} 掉落 {}×{count}{mark}", "", item(*id))
                .trim()
                .to_string()
                .replacen(" 掉落", "掉落", 1)
        }
        GameEvent::GoldGained { amount } => format!("获得金币 {amount}"),
        GameEvent::LevelUp { level } => {
            line.level = true;
            format!("升级到 {level} 级")
        }
        GameEvent::SkillLevelUp { id, level } => {
            line.level = true;
            format!("{} 升到 {level} 级", skill(*id))
        }
        GameEvent::SkillCast { id } => format!("施放 {}", skill(*id)),
        GameEvent::PlayerDied {
            lost_exp,
            dropped_item,
        } => {
            line.bad = true;
            match dropped_item {
                Some(id) => format!("你死了，损失 {lost_exp} 经验，掉落 {}", item(*id)),
                None => format!("你死了，损失 {lost_exp} 经验"),
            }
        }
        GameEvent::PotionUsed { id } => format!("喝下 {}", item(*id)),
        GameEvent::OutOfPotions => {
            line.bad = true;
            "没有药了，先回城".to_string()
        }
        GameEvent::Resting { ticks } => format!("休息 {} 秒", ticks / 10),
        GameEvent::MapChanged { map } => format!("进入 {}", map_name(*map)),
        GameEvent::BossSkipped { id } => format!("{} 现在打不过，先刷小怪", monster(*id)),
        GameEvent::PetAttacked { damage } => format!("宝宝造成 {damage} 点伤害"),
        GameEvent::PetGone => "宝宝消失了".to_string(),
        GameEvent::AchievementUnlocked { id } => {
            line.level = true;
            format!("成就：{id}")
        }
        GameEvent::SoldOverflow { id, gold } => format!("背包满了，{} 折价 {gold} 金币", item(*id)),
        GameEvent::UpgradeSucceeded { plus } => {
            line.level = true;
            format!("武器升级成功，现在 +{plus}")
        }
        GameEvent::UpgradeProtected => "幸运符碎裂，武器保住了".to_string(),
        GameEvent::UpgradeDestroyed => {
            line.bad = true;
            "武器碎裂了".to_string()
        }
        GameEvent::SkillLearned { id } => {
            line.level = true;
            format!("学会了 {}", skill(*id))
        }
        GameEvent::DotDamage { target, damage } => {
            format!("{} 持续受伤 {} 点", monster(*target), damage)
        }
        GameEvent::MapUnlocked { map } => {
            line.level = true;
            format!("解锁新地图：{}（按 4 去地图，A 自动推进）", map_name(*map))
        }
        GameEvent::ReturnedToHunt { map } => format!("休息好了，回到 {}", map_name(*map)),
        GameEvent::FellBack { map, deaths } => {
            line.bad = true;
            format!("连死 {deaths} 次，先回 {} 练级", map_name(*map))
        }
        GameEvent::Restocked { hp, mp, gold } => match (*hp, *mp) {
            (0, mp) => format!("补了蓝药 {mp} 瓶，花费 {gold} 金币"),
            (hp, 0) => format!("补了红药 {hp} 瓶，花费 {gold} 金币"),
            (hp, mp) => format!("补了红药 {hp} 瓶、蓝药 {mp} 瓶，花费 {gold} 金币"),
        },
        GameEvent::Repaired { pieces, gold } => format!("修好 {pieces} 件装备，花费 {gold} 金币"),
        GameEvent::Equipped { id } => format!("穿上 {}", item(*id)),
        GameEvent::Unequipped { id } => format!("脱下 {}", item(*id)),
    };
    if let GameEvent::ItemDropped { id, count, rare } = event {
        line.rare = *rare;
        let star = if *rare { " ★" } else { "" };
        line.text = format!("掉落 {}×{count}{star}", item(*id));
    }
    line
}

/// 掉落和喝药日志里也带上职业、回复量，和背包用同一套说法。
fn item_label(data: &GameData, id: game_data::ItemId) -> String {
    let def = data.item(id);
    match &def.kind {
        game_data::ItemKind::SkillBook(book) => {
            format!("{}[{}]", def.name, book.req_class.label())
        }
        game_data::ItemKind::Consumable(potion) => {
            format!("{}{}", def.name, potion_label(potion))
        }
        _ => def.name.clone(),
    }
}

fn potion_label(potion: &game_data::ConsumableDef) -> String {
    let mut parts = Vec::new();
    if potion.heal_hp > 0 {
        parts.push(format!("血+{}", potion.heal_hp));
    }
    if potion.heal_mp > 0 {
        parts.push(format!("蓝+{}", potion.heal_mp));
    }
    if parts.is_empty() {
        return String::new();
    }
    format!(" {}", parts.join(" "))
}
