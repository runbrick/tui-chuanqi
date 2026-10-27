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
}

/// 格式化一条事件。
pub fn format_event(event: &GameEvent, data: &GameData) -> LogLine {
    let monster = |id| data.monster(id).name.clone();
    let item = |id| data.item(id).name.clone();
    let skill = |id| data.skill(id).name.clone();
    let map_name = |id| data.map(id).name.clone();
    let mut line = LogLine {
        text: String::new(),
        rare: false,
        level: false,
        bad: false,
    };
    line.text = match event {
        GameEvent::Attacked { target, damage, .. } => {
            format!("命中 {}，{} 点", monster(*target), damage)
        }
        GameEvent::Blocked { target } => format!("{} 抵挡了你的攻击", monster(*target)),
        GameEvent::Missed { target } => format!("攻击 {} 落空", monster(*target)),
        GameEvent::MonsterHit { source, damage } => {
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
    };
    if let GameEvent::ItemDropped { id, count, rare } = event {
        line.rare = *rare;
        let star = if *rare { " ★" } else { "" };
        line.text = format!("掉落 {}×{count}{star}", item(*id));
    }
    line
}
