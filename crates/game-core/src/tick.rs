//! 固定顺序的逻辑 tick。改顺序会影响快照和存档复现，不要随意调整。

use crate::event::GameEvent;
use crate::state::{ms_to_ticks, skill_rank, Achievement, Combat, EventSink, GameState, Pet};
use game_data::{
    DamageSource, GameData, ItemId, ItemKind, MapId, MonsterDef, MonsterId, Quality, Range,
    SkillEffect, SkillId,
};
use rand::Rng;
use std::collections::BTreeMap;

/// 持续伤害每隔这么多 tick 跳一次。
const DOT_PULSE: u32 = 10;

pub(crate) fn step(state: &mut GameState, data: &GameData, sink: &mut impl EventSink) {
    apply_dots(state, data, sink);
    if monster_down(state) {
        finish_kill(state, data, sink);
    }
    let resting = state.tick_count < state.resting_until;
    if resting {
        recover_rest(state, data);
    } else {
        regen(state, data);
        return_to_hunt(state, data, sink);
        auto_potions(state, data, sink);
        if should_retreat(state, data) {
            retreat(state, data, sink);
        } else {
            if state.combat.is_none() {
                try_spawn(state, data, sink);
            }
            if monster_alive(state) {
                player_act(state, data, sink);
            }
            if monster_alive(state) {
                pet_act(state, data, sink);
            }
            if monster_alive(state) {
                monster_act(state, data, sink);
            }
            if monster_down(state) {
                finish_kill(state, data, sink);
            }
        }
    }
    while_level(state, data, sink);
    if !resting {
        auto_advance(state, data, sink);
    }
    check_achievements(state, sink);
    state.tick_count = state.tick_count.saturating_add(1);
}

fn monster_alive(state: &GameState) -> bool {
    state.combat.as_ref().is_some_and(|combat| combat.hp > 0)
}

fn monster_down(state: &GameState) -> bool {
    state.combat.as_ref().is_some_and(|combat| combat.hp <= 0)
}

fn apply_dots(state: &mut GameState, data: &GameData, sink: &mut impl EventSink) {
    if state.hot_hp > 0 && state.hot_hp_per > 0 {
        let gain = state.hot_hp_per.min(state.hot_hp);
        let max_hp = state.derived_stats(data).max_hp;
        state.hp = (state.hp + gain).min(max_hp);
        state.hot_hp -= gain;
    }
    if state.hot_mp > 0 && state.hot_mp_per > 0 {
        let gain = state.hot_mp_per.min(state.hot_mp);
        let max_mp = state.derived_stats(data).max_mp;
        state.mp = (state.mp + gain).min(max_mp);
        state.hot_mp -= gain;
    }
    if state.shield_left > 0 {
        state.shield_left -= 1;
    }
    if state.buff_left > 0 {
        state.buff_left -= 1;
        if state.buff_left == 0 {
            state.invalidate_derived();
        }
    }
    if let Some(combat) = state.combat.as_mut() {
        if combat.defense_down_left > 0 {
            combat.defense_down_left -= 1;
            if combat.defense_down_left == 0 {
                combat.defense_down = 0.0;
            }
        }
        if combat.poison_left > 0 && combat.poison_dps > 0 && combat.hp > 0 {
            combat.poison_left -= 1;
            if combat.poison_left % DOT_PULSE == 0 {
                combat.hp -= combat.poison_dps;
                sink.push_event(GameEvent::DotDamage {
                    target: combat.monster,
                    damage: combat.poison_dps,
                });
            }
        }
    }
}

fn recover_rest(state: &mut GameState, data: &GameData) {
    let max_hp = state.derived_stats(data).max_hp;
    let step = (max_hp / 40).max(1);
    state.hp = (state.hp + step).min(max_hp);
    let max_mp = state.derived_stats(data).max_mp;
    state.mp = (state.mp + (max_mp / 40).max(1)).min(max_mp);
}

/// 平时的缓慢回复。休息时有 `recover_rest`，这里不叠加。
fn regen(state: &mut GameState, data: &GameData) {
    let cfg = &data.balance.regen;
    if cfg.interval_ms == 0 || state.tick_count % ms_to_ticks(cfg.interval_ms) != 0 {
        return;
    }
    let derived = *state.derived_stats(data);
    let amount = |max: i32, ratio: f64| ((f64::from(max) * ratio).round() as i32).max(1);
    if cfg.hp_ratio > 0.0 && state.hp > 0 && state.hp < derived.max_hp {
        state.hp = (state.hp + amount(derived.max_hp, cfg.hp_ratio)).min(derived.max_hp);
    }
    if cfg.mp_ratio > 0.0 && state.mp < derived.max_mp {
        state.mp = (state.mp + amount(derived.max_mp, cfg.mp_ratio)).min(derived.max_mp);
    }
}

/// 死亡或没药回城后，休息完回到原来的图。图被锁了（比如等级掉了）就留在城里。
fn return_to_hunt(state: &mut GameState, data: &GameData, sink: &mut impl EventSink) {
    let Some(map) = state.return_map else {
        return;
    };
    if !data.map(state.current_map).safe_zone {
        state.return_map = None;
        return;
    }
    state.return_map = None;
    if !crate::command::map_unlocked(state, data, map) {
        return;
    }
    state.current_map = map;
    state.next_spawn_at = state.tick_count;
    sink.push_event(GameEvent::ReturnedToHunt { map });
}

fn auto_potions(state: &mut GameState, data: &GameData, sink: &mut impl EventSink) {
    if !state.automation.auto_potion {
        return;
    }
    // 慢回的药还没回完就不再喝，否则一口气连灌好几瓶。
    let derived = *state.derived_stats(data);
    if derived.max_hp > 0
        && f64::from(state.hp + state.hot_hp) / f64::from(derived.max_hp)
            < state.automation.hp_ratio
    {
        drink(state, data, sink, true);
    }
    let derived = *state.derived_stats(data);
    if derived.max_mp > 0
        && f64::from(state.mp + state.hot_mp) / f64::from(derived.max_mp)
            < state.automation.mp_ratio
    {
        drink(state, data, sink, false);
    }
}

fn drink(state: &mut GameState, data: &GameData, sink: &mut impl EventSink, hp: bool) {
    let derived = *state.derived_stats(data);
    let missing = if hp {
        derived.max_hp - state.hp - state.hot_hp
    } else {
        derived.max_mp - state.mp - state.hot_mp
    };
    if missing <= 0 {
        return;
    }
    // 取能补上缺口的最小规格；都不够就取最大的。
    let mut covers: Option<(usize, i32)> = None;
    let mut largest: Option<(usize, i32)> = None;
    for (index, stack) in state.inventory.iter().enumerate() {
        let ItemKind::Consumable(potion) = &data.item(stack.item).kind else {
            continue;
        };
        let amount = if hp { potion.heal_hp } else { potion.heal_mp };
        if amount <= 0 {
            continue;
        }
        if amount >= missing && covers.is_none_or(|(_, best)| amount < best) {
            covers = Some((index, amount));
        }
        if largest.is_none_or(|(_, best)| amount > best) {
            largest = Some((index, amount));
        }
    }
    if let Some((index, _)) = covers.or(largest) {
        let id = state.inventory[index].item;
        consume_one(state, index);
        apply_potion(state, data, id);
        sink.push_event(GameEvent::PotionUsed { id });
    }
}

pub(crate) fn apply_potion(state: &mut GameState, data: &GameData, id: ItemId) {
    let ItemKind::Consumable(potion) = &data.item(id).kind else {
        return;
    };
    let derived = *state.derived_stats(data);
    if potion.instant || potion.duration_ticks == 0 {
        state.hp = (state.hp + potion.heal_hp).min(derived.max_hp);
        state.mp = (state.mp + potion.heal_mp).min(derived.max_mp);
        return;
    }
    let ticks = potion.duration_ticks.max(1) as i32;
    if potion.heal_hp > 0 {
        state.hot_hp += potion.heal_hp;
        state.hot_hp_per = state.hot_hp_per.max((potion.heal_hp / ticks).max(1));
    }
    if potion.heal_mp > 0 {
        state.hot_mp += potion.heal_mp;
        state.hot_mp_per = state.hot_mp_per.max((potion.heal_mp / ticks).max(1));
    }
}

fn consume_one(state: &mut GameState, index: usize) {
    if state.inventory[index].count > 1 {
        state.inventory[index].count -= 1;
    } else {
        state.inventory.remove(index);
    }
}

fn should_retreat(state: &GameState, data: &GameData) -> bool {
    let derived = state.derived_stats(data);
    if derived.max_hp == 0 {
        return false;
    }
    let low = f64::from(state.hp + state.hot_hp) / f64::from(derived.max_hp)
        < data.balance.retreat_hp_ratio;
    low && !has_hp_potion(state, data) && state.combat.is_some()
}

fn has_hp_potion(state: &GameState, data: &GameData) -> bool {
    state.inventory.iter().any(
        |stack| matches!(&data.item(stack.item).kind, ItemKind::Consumable(p) if p.heal_hp > 0),
    )
}

fn retreat(state: &mut GameState, data: &GameData, sink: &mut impl EventSink) {
    state.combat = None;
    if !data.map(state.current_map).safe_zone {
        state.return_map = Some(state.current_map);
    }
    if let Some(safe) = data.safe_map() {
        state.current_map = safe;
    }
    state.resting_until = state
        .tick_count
        .saturating_add(data.balance.death_rest_ticks);
    sink.push_event(GameEvent::OutOfPotions);
    sink.push_event(GameEvent::Resting {
        ticks: data.balance.death_rest_ticks,
    });
}

fn try_spawn(state: &mut GameState, data: &GameData, sink: &mut impl EventSink) {
    if state.tick_count < state.next_spawn_at {
        return;
    }
    let map = data.map(state.current_map);
    if map.safe_zone || map.spawns.is_empty() {
        return;
    }
    if let Some(boss) = map.boss.as_ref() {
        let kills = state
            .map_kills
            .get(&state.current_map)
            .copied()
            .unwrap_or(0);
        let ready = state
            .boss_ready_at
            .get(&boss.monster_id)
            .copied()
            .unwrap_or(0);
        let wipes = state.boss_wipes.get(&boss.monster_id).copied().unwrap_or(0);
        if kills >= boss.kills_to_unlock && state.tick_count >= ready && wipes < 2 {
            if can_beat(state, data, boss.monster_id) {
                spawn_monster(state, data, boss.monster_id, true);
                return;
            }
            sink.push_event(GameEvent::BossSkipped {
                id: boss.monster_id,
            });
            state
                .boss_ready_at
                .insert(boss.monster_id, state.tick_count.saturating_add(3000));
        }
    }
    let total: u32 = map.spawns.iter().map(|spawn| spawn.weight).sum();
    if total == 0 {
        return;
    }
    let mut roll = state.rng.gen_range(0..total);
    let mut picked = map.spawns[0].monster_id;
    for spawn in &map.spawns {
        if roll < spawn.weight {
            picked = spawn.monster_id;
            break;
        }
        roll -= spawn.weight;
    }
    spawn_monster(state, data, picked, false);
}

fn spawn_monster(state: &mut GameState, data: &GameData, id: MonsterId, boss: bool) {
    let monster = data.monster(id);
    let interval = ms_to_ticks(monster.attack_interval_ms);
    let player_interval = state.derived_stats(data).attack_interval;
    state.combat = Some(Combat {
        monster: id,
        boss,
        hp: monster.hp,
        next_player_attack_at: state.tick_count.saturating_add(player_interval),
        next_monster_attack_at: state.tick_count.saturating_add(interval),
        skill_ready_at: BTreeMap::new(),
        next_attack_power: None,
        poison_left: 0,
        poison_dps: 0,
        defense_down: 0.0,
        defense_down_left: 0,
    });
}

fn can_beat(state: &GameState, data: &GameData, id: MonsterId) -> bool {
    let monster = data.monster(id);
    let derived = state.derived_stats(data);
    let player_hit = match state.class {
        game_data::Class::Mage => damage_avg(derived.magic, monster.magic_defense)
            .max(damage_avg(derived.attack, monster.defense)),
        game_data::Class::Taoist => damage_avg(derived.taoism, monster.magic_defense)
            .max(damage_avg(derived.attack, monster.defense)),
        game_data::Class::Warrior => damage_avg(derived.attack, monster.defense),
    };
    if player_hit <= 0 {
        return false;
    }
    let mitigation = if monster.attack_type.uses_magic_defense() {
        derived.magic_defense
    } else {
        derived.defense
    };
    let incoming = damage_avg(monster.attack, mitigation);
    if incoming <= 0 {
        return true;
    }
    let ticks_to_kill = (i64::from(monster.hp)
        * i64::try_from(derived.attack_interval).unwrap_or(1)
        / i64::from(player_hit))
    .max(1);
    let player_hp = i64::from(derived.max_hp) * 3 / 2;
    let ticks_alive = player_hp * i64::from(ms_to_ticks(monster.attack_interval_ms) as u32)
        / i64::from(incoming).max(1);
    ticks_alive * 4 / 5 >= ticks_to_kill
}

fn damage_avg(attack: Range, defense: Range) -> i32 {
    (attack.midpoint() - defense.midpoint()).max(0)
}

fn player_act(state: &mut GameState, data: &GameData, sink: &mut impl EventSink) {
    let now = state.tick_count;
    let ready = state
        .combat
        .as_ref()
        .is_some_and(|combat| now >= combat.next_player_attack_at);
    if !ready {
        return;
    }
    if let Some(skill) = pick_skill(state, data) {
        cast_skill(state, data, sink, skill);
        return;
    }
    basic_attack(state, data, sink);
}

fn pick_skill(state: &GameState, data: &GameData) -> Option<SkillId> {
    let combat = state.combat.as_ref()?;
    let monster = data.monster(combat.monster);
    let derived = state.derived_stats(data);
    // 蓝多时挑最猛的，蓝快见底时挑最省的，免得几下就把蓝打空。
    let thrifty = derived.max_mp > 0 && state.mp * 3 < derived.max_mp;
    let mut best: Option<(u8, f32, SkillId)> = None;
    for (id, progress) in &state.skills {
        if !progress.auto {
            continue;
        }
        let def = data.skill(*id);
        if matches!(def.effect, SkillEffect::Passive(_)) {
            continue;
        }
        let Some(rank) = skill_rank(def, progress, state.level) else {
            continue;
        };
        let level = &def.levels[rank];
        if state.mp < level.mp_cost {
            continue;
        }
        let ready_at = combat.skill_ready_at.get(id).copied().unwrap_or(0);
        if state.tick_count < ready_at {
            continue;
        }
        if !worth_casting(state, data, combat, monster, &def.effect, level.power) {
            continue;
        }
        let priority = skill_priority(&def.effect);
        let mut power = level.power.min + level.power.max;
        if let SkillEffect::Damage(effect) | SkillEffect::AoeDamage(effect) = &def.effect {
            if monster.undead {
                power *= effect.undead_bonus;
            }
        }
        let value = if thrifty {
            power / level.mp_cost.max(1) as f32
        } else {
            power
        };
        let better = match best {
            None => true,
            Some((score, best_value, _)) => {
                priority > score || (priority == score && value > best_value)
            }
        };
        if better {
            best = Some((priority, value, *id));
        }
    }
    best.map(|(_, _, id)| id)
}

/// 放了也白放的就不放：满血不加血、盾还在不重开、毒还没完不重上、怪快死了不上 debuff。
fn worth_casting(
    state: &GameState,
    data: &GameData,
    combat: &Combat,
    monster: &MonsterDef,
    effect: &SkillEffect,
    power: game_data::FRange,
) -> bool {
    match effect {
        SkillEffect::Heal(heal) => {
            let derived = state.derived_stats(data);
            let base = source_range(derived, heal.source).midpoint().max(1) as f32;
            let expected = (base * (power.min + power.max) / 2.0).round() as i32;
            derived.max_hp - state.hp - state.hot_hp >= expected.max(1)
        }
        SkillEffect::BuffStats(_) => state.shield_left == 0,
        SkillEffect::DebuffDefense(_) => {
            combat.defense_down_left == 0 && combat.hp * 3 > monster.hp
        }
        SkillEffect::Poison(_) => combat.poison_left == 0 && combat.hp * 4 > monster.hp,
        SkillEffect::Summon(_) => state
            .pet
            .as_ref()
            .is_none_or(|pet| pet.hp <= 0 || pet.expires_at <= state.tick_count),
        SkillEffect::BuffNextAttack(_) => combat.next_attack_power.is_none(),
        SkillEffect::Paralyze(_) => {
            let derived = state.derived_stats(data);
            let soon = combat.next_monster_attack_at <= state.tick_count.saturating_add(4);
            soon && state.hp * 10 < derived.max_hp * 7
        }
        SkillEffect::Damage(_) | SkillEffect::AoeDamage(_) => true,
        SkillEffect::Passive(_) => false,
    }
}

fn skill_priority(effect: &SkillEffect) -> u8 {
    match effect {
        SkillEffect::Heal(_) => 8,
        SkillEffect::BuffStats(_) | SkillEffect::DebuffDefense(_) => 7,
        SkillEffect::Summon(_) => 6,
        SkillEffect::BuffNextAttack(_) => 5,
        SkillEffect::Poison(_) => 4,
        SkillEffect::Paralyze(_) => 3,
        // 场上只有一只怪，群攻和单体按威力比，不再让群攻天然优先。
        SkillEffect::AoeDamage(_) | SkillEffect::Damage(_) => 2,
        SkillEffect::Passive(_) => 0,
    }
}

fn source_range(derived: &crate::state::DerivedStats, source: DamageSource) -> Range {
    match source {
        DamageSource::Attack => derived.attack,
        DamageSource::Magic => derived.magic,
        DamageSource::Taoism => derived.taoism,
    }
}

fn cast_skill(state: &mut GameState, data: &GameData, sink: &mut impl EventSink, id: SkillId) {
    let def = data.skill(id).clone();
    let Some(rank) = state
        .skills
        .get(&id)
        .and_then(|progress| skill_rank(&def, progress, state.level))
    else {
        return;
    };
    let level = &def.levels[rank];
    let mp_cost = level.mp_cost;
    let cooldown = ms_to_ticks(level.cooldown_ms);
    let power = level.power.roll(&mut state.rng);
    state.mp -= mp_cost;
    let char_level = state.level;
    if let Some(progress) = state.skills.get_mut(&id) {
        let need = data
            .balance
            .skill_exp_base
            .saturating_mul(u32::from(progress.level));
        // 下一档要求的角色等级还没到，熟练度就停在满值，等升级后下一发再升。
        let next_ok = def
            .levels
            .get(progress.level as usize)
            .is_some_and(|next| next.req_level <= char_level);
        progress.exp = progress.exp.saturating_add(1).min(need);
        if progress.level < def.max_level && progress.exp >= need && next_ok {
            progress.exp = 0;
            progress.level += 1;
            let new_level = progress.level;
            sink.push_event(GameEvent::SkillLevelUp {
                id,
                level: new_level,
            });
        }
    }
    sink.push_event(GameEvent::SkillCast { id });
    let interval = state.derived_stats(data).attack_interval;
    if let Some(combat) = state.combat.as_mut() {
        if level.cooldown_ms > 0 {
            combat
                .skill_ready_at
                .insert(id, state.tick_count.saturating_add(cooldown));
        }
        // 施法占掉这一拍。烈火只是给下一击蓄力，不占。
        if !matches!(def.effect, SkillEffect::BuffNextAttack(_)) {
            combat.next_player_attack_at = state.tick_count.saturating_add(interval);
        }
    }
    match &def.effect {
        SkillEffect::Damage(effect) | SkillEffect::AoeDamage(effect) => {
            let mut mult = if effect.multiplier_from_power {
                power
            } else {
                1.0
            };
            if state
                .combat
                .as_ref()
                .is_some_and(|c| data.monster(c.monster).undead)
            {
                mult *= effect.undead_bonus;
            }
            spell_hit(state, data, sink, effect.source, mult);
        }
        SkillEffect::Heal(heal) => {
            let derived = *state.derived_stats(data);
            let base = source_range(&derived, heal.source)
                .roll(&mut state.rng)
                .max(1);
            let amount = ((base as f32) * power).round() as i32;
            state.hp = (state.hp + amount.max(1)).min(derived.max_hp);
        }
        SkillEffect::BuffNextAttack(_) => {
            if let Some(combat) = state.combat.as_mut() {
                combat.next_attack_power = Some((DamageSource::Attack, power));
            }
        }
        SkillEffect::BuffStats(buff) => {
            state.shield_left = buff.duration_ticks;
            state.shield_reduction = buff.damage_reduction;
            if buff.agility != 0 {
                state.buff_agility = buff.agility;
                state.buff_left = buff.duration_ticks;
                state.invalidate_derived();
            }
        }
        SkillEffect::DebuffDefense(debuff) => {
            if let Some(combat) = state.combat.as_mut() {
                combat.defense_down = debuff.defense_pct;
                combat.defense_down_left = debuff.duration_ticks;
            }
        }
        SkillEffect::Poison(poison) => {
            let derived = *state.derived_stats(data);
            let base = source_range(&derived, poison.source).midpoint().max(1);
            let undead = state
                .combat
                .as_ref()
                .is_some_and(|c| data.monster(c.monster).undead);
            let bonus = if undead { poison.undead_bonus } else { 1.0 };
            let per_pulse = ((base as f32) * poison.tick_ratio * power * bonus)
                .round()
                .max(1.0) as i32;
            if let Some(combat) = state.combat.as_mut() {
                combat.poison_dps = per_pulse;
                combat.poison_left = poison.duration_ticks;
            }
        }
        SkillEffect::Summon(summon) => {
            let derived = *state.derived_stats(data);
            let scale = if summon.scale_from_taoism {
                (derived.taoism.midpoint() / 4).max(0)
            } else {
                0
            };
            let hp = ((summon.base_hp + scale * 8) as f32 * power).round() as i32;
            let attack = Range::new(
                summon.base_attack.min + scale,
                summon.base_attack.max + scale * 2,
            );
            state.pet = Some(Pet {
                skill: id,
                hp: hp.max(1),
                max_hp: hp.max(1),
                attack,
                interval: ms_to_ticks(summon.attack_interval_ms),
                next_attack_at: state
                    .tick_count
                    .saturating_add(ms_to_ticks(summon.attack_interval_ms)),
                expires_at: state.tick_count.saturating_add(summon.duration_ticks),
            });
        }
        SkillEffect::Paralyze(paralyze) => {
            if let Some(combat) = state.combat.as_mut() {
                combat.next_monster_attack_at = combat
                    .next_monster_attack_at
                    .saturating_add(paralyze.delay_ticks);
            }
        }
        SkillEffect::Passive(_) => {}
    }
    wear_weapon(state, data);
}

fn basic_attack(state: &mut GameState, data: &GameData, sink: &mut impl EventSink) {
    let interval = state.derived_stats(data).attack_interval;
    if let Some(combat) = state.combat.as_mut() {
        combat.next_player_attack_at = state.tick_count.saturating_add(interval);
    }
    let target = state
        .combat
        .as_ref()
        .map(|c| c.monster)
        .unwrap_or(MonsterId(0));
    let monster = data.monster(target);
    let derived = *state.derived_stats(data);
    let hit = hit_chance(data, derived.accuracy, monster.agility);
    if !state.rng.gen_bool(hit) {
        sink.push_event(GameEvent::Missed { target });
        return;
    }
    let mut bonus = charged_power(state, DamageSource::Attack);
    let procs: Vec<(f32, f32, u8)> = state
        .skills
        .iter()
        .filter_map(|(id, progress)| {
            let def = data.skill(*id);
            match &def.effect {
                SkillEffect::Passive(passive) if passive.proc_chance > 0.0 => {
                    let rank = skill_rank(def, progress, state.level)?;
                    Some((passive.proc_chance, passive.proc_power, rank as u8 + 1))
                }
                _ => None,
            }
        })
        .collect();
    for (chance, power, level) in procs {
        if state.rng.gen_bool(f64::from(chance).clamp(0.0, 1.0)) {
            bonus += power * f32::from(level);
        }
    }
    let rolled = derived.attack.roll(&mut state.rng);
    let raw = ((rolled as f32) * bonus).round() as i32;
    let mitigated = effective_defense(state, monster, DamageSource::Attack).roll(&mut state.rng);
    emit_player_damage(state, sink, target, raw, mitigated);
    wear_weapon(state, data);
}

/// 烈火这类蓄力：物理出手时取走并返回倍率，没有就是 1。
fn charged_power(state: &mut GameState, source: DamageSource) -> f32 {
    if source != DamageSource::Attack {
        return 1.0;
    }
    match state
        .combat
        .as_mut()
        .and_then(|c| c.next_attack_power.take())
    {
        Some((DamageSource::Attack, power)) => power,
        _ => 1.0,
    }
}

/// 怪物这一下吃哪条防御。神圣战甲术的降防对物理、法术和宝宝都生效。
fn effective_defense(state: &GameState, monster: &MonsterDef, source: DamageSource) -> Range {
    let defense = if source == DamageSource::Attack {
        monster.defense
    } else {
        monster.magic_defense
    };
    match state.combat.as_ref() {
        Some(combat) if combat.defense_down > 0.0 => {
            let keep = 1.0 - combat.defense_down;
            Range::new(
                (defense.min as f32 * keep) as i32,
                (defense.max as f32 * keep) as i32,
            )
        }
        _ => defense,
    }
}

fn spell_hit(
    state: &mut GameState,
    data: &GameData,
    sink: &mut impl EventSink,
    source: DamageSource,
    mult: f32,
) {
    let target = match state.combat.as_ref() {
        Some(combat) => combat.monster,
        None => return,
    };
    let monster = data.monster(target);
    let derived = *state.derived_stats(data);
    // 法术必中；刺杀、半月这类物理技能和普攻一样要判命中。
    if source == DamageSource::Attack {
        let hit = hit_chance(data, derived.accuracy, monster.agility);
        if !state.rng.gen_bool(hit) {
            sink.push_event(GameEvent::Missed { target });
            return;
        }
    }
    let mult = mult * charged_power(state, source);
    let raw = ((source_range(&derived, source).roll(&mut state.rng) as f32) * mult).round() as i32;
    let mitigated = effective_defense(state, monster, source).roll(&mut state.rng);
    emit_player_damage(state, sink, target, raw, mitigated);
}

fn emit_player_damage(
    state: &mut GameState,
    sink: &mut impl EventSink,
    target: MonsterId,
    raw: i32,
    mitigated: i32,
) {
    let damage = raw.saturating_sub(mitigated).max(0);
    if damage == 0 {
        sink.push_event(GameEvent::Blocked { target });
    } else {
        if let Some(combat) = state.combat.as_mut() {
            combat.hp -= damage;
        }
        sink.push_event(GameEvent::Attacked {
            target,
            damage,
            crit: false,
        });
    }
}

fn wear_weapon(state: &mut GameState, data: &GameData) {
    let Some(stack) = state.equipment[game_data::EquipSlot::Weapon.index()].as_mut() else {
        return;
    };
    if stack.durability > 0 {
        stack.durability -= 1;
        if stack.durability == 0 {
            state.invalidate_derived();
        }
    }
    let _ = data;
}

fn pet_act(state: &mut GameState, data: &GameData, sink: &mut impl EventSink) {
    let now = state.tick_count;
    let Some(pet) = state.pet.clone() else {
        return;
    };
    if pet.hp <= 0 || now >= pet.expires_at {
        state.pet = None;
        sink.push_event(GameEvent::PetGone);
        return;
    }
    if now < pet.next_attack_at {
        return;
    }
    let target = state
        .combat
        .as_ref()
        .map(|c| c.monster)
        .unwrap_or(MonsterId(0));
    let monster = data.monster(target);
    let defense = effective_defense(state, monster, DamageSource::Attack).roll(&mut state.rng);
    let raw = pet.attack.roll(&mut state.rng);
    let damage = raw.saturating_sub(defense).max(0);
    if let Some(combat) = state.combat.as_mut() {
        combat.hp -= damage;
    }
    if let Some(pet) = state.pet.as_mut() {
        pet.next_attack_at = now.saturating_add(pet.interval);
    }
    if damage > 0 {
        sink.push_event(GameEvent::PetAttacked { damage });
    }
}

fn monster_act(state: &mut GameState, data: &GameData, sink: &mut impl EventSink) {
    let Some(combat) = state.combat.as_ref() else {
        return;
    };
    if combat.hp <= 0 || state.tick_count < combat.next_monster_attack_at {
        return;
    }
    let monster_id = combat.monster;
    let monster = data.monster(monster_id);
    let interval = ms_to_ticks(monster.attack_interval_ms);
    if let Some(combat) = state.combat.as_mut() {
        combat.next_monster_attack_at = state.tick_count.saturating_add(interval);
    }
    if let Some(pet) = state.pet.as_mut() {
        if pet.hp > 0 && state.rng.gen_bool(0.5) {
            let dmg = monster.attack.roll(&mut state.rng).max(1);
            pet.hp -= dmg;
            if pet.hp <= 0 {
                state.pet = None;
                sink.push_event(GameEvent::PetGone);
            }
            return;
        }
    }
    let derived = *state.derived_stats(data);
    if !monster.attack_type.is_spell() {
        let hit = hit_chance(data, monster.accuracy, derived.agility);
        if !state.rng.gen_bool(hit) {
            sink.push_event(GameEvent::MonsterMissed { source: monster_id });
            return;
        }
    }
    let raw = monster.attack.roll(&mut state.rng);
    let mitigation_range = if monster.attack_type.uses_magic_defense() {
        derived.magic_defense
    } else {
        derived.defense
    };
    let mitigated = mitigation_range.roll(&mut state.rng);
    let mut damage = raw.saturating_sub(mitigated).max(0);
    if state.shield_left > 0 && state.shield_reduction > 0.0 {
        damage = ((damage as f32) * (1.0 - state.shield_reduction)).round() as i32;
    }
    if damage == 0 {
        sink.push_event(GameEvent::MonsterBlocked { source: monster_id });
        return;
    }
    state.hp -= damage;
    sink.push_event(GameEvent::MonsterHit {
        source: monster_id,
        damage,
    });
    if state.hp <= 0 {
        player_dead(state, data, sink, monster_id);
    }
}

fn player_dead(
    state: &mut GameState,
    data: &GameData,
    sink: &mut impl EventSink,
    monster: MonsterId,
) {
    state.hp = 0;
    let bar = data.balance.exp_to_next(state.level);
    let lost = ((bar as f64) * data.balance.death_penalty_exp_pct).floor() as u64;
    let lost = lost.min(state.exp);
    state.exp -= lost;
    state.stats.deaths = state.stats.deaths.saturating_add(1);
    let mut dropped = None;
    if state.high_risk {
        let mut candidates = Vec::new();
        for (index, stack) in state.equipment.iter().enumerate() {
            if stack.as_ref().is_some_and(|item| !item.locked) {
                candidates.push(index);
            }
        }
        if !candidates.is_empty() {
            let pick = candidates[state.rng.gen_range(0..candidates.len())];
            if let Some(stack) = state.equipment[pick].take() {
                dropped = Some(stack.item);
                state.invalidate_derived();
            }
        }
    }
    if state.combat.as_ref().is_some_and(|combat| combat.boss) {
        let wipes = state.boss_wipes.entry(monster).or_insert(0);
        *wipes = wipes.saturating_add(1);
    }
    state.combat = None;
    let died_on = state.current_map;
    state.death_streak = state.death_streak.saturating_add(1);
    let limit = data.balance.fallback_after_deaths;
    let mut back_to = died_on;
    if limit > 0 && state.death_streak % limit == 0 {
        if let Some(prev) = data.map(died_on).req_map_id {
            back_to = prev;
        }
    }
    if !data.map(died_on).safe_zone {
        state.return_map = Some(back_to);
    }
    if let Some(safe) = data.safe_map() {
        state.current_map = safe;
    }
    state.resting_until = state
        .tick_count
        .saturating_add(data.balance.death_rest_ticks);
    let revive_hp = state.derived_stats(data).max_hp / 2;
    state.hp = revive_hp.max(1);
    state.hot_hp = 0;
    sink.push_event(GameEvent::PlayerDied {
        lost_exp: lost,
        dropped_item: dropped,
    });
    if back_to != died_on {
        sink.push_event(GameEvent::FellBack {
            map: back_to,
            deaths: state.death_streak,
        });
    }
    sink.push_event(GameEvent::Resting {
        ticks: data.balance.death_rest_ticks,
    });
}

fn finish_kill(state: &mut GameState, data: &GameData, sink: &mut impl EventSink) {
    let Some(combat) = state.combat.take() else {
        return;
    };
    let monster = data.monster(combat.monster);
    let exp = monster.exp;
    state.exp = state.exp.saturating_add(exp);
    state.stats.exp_gained = state.stats.exp_gained.saturating_add(exp);
    state.stats.kills = state.stats.kills.saturating_add(1);
    *state.stats.monster_kills.entry(combat.monster).or_insert(0) += 1;
    let here = state.current_map;
    let kills = {
        let count = state.map_kills.entry(here).or_insert(0);
        *count += 1;
        *count
    };
    if combat.boss {
        state.boss_wipes.insert(combat.monster, 0);
        let respawn = data
            .map(here)
            .boss
            .as_ref()
            .map(|b| ms_to_ticks(b.respawn_ms))
            .unwrap_or(6000);
        state
            .boss_ready_at
            .insert(combat.monster, state.tick_count.saturating_add(respawn));
        if state.achievements.insert(Achievement::FirstBoss) {
            sink.push_event(GameEvent::AchievementUnlocked {
                id: Achievement::FirstBoss.label().to_string(),
            });
        }
    }
    sink.push_event(GameEvent::MonsterSlain {
        id: combat.monster,
        exp,
    });
    roll_drops(state, data, sink, combat.monster);
    for map in &data.maps {
        let just_met = map.req_map_id == Some(here)
            && map
                .req_map_kills
                .as_ref()
                .is_some_and(|(_, need)| *need == kills);
        if just_met && state.level >= map.req_level {
            sink.push_event(GameEvent::MapUnlocked {
                map: map.runtime_id,
            });
        }
    }
    let interval = ms_to_ticks(data.map(here).spawn_interval_ms.max(100));
    state.next_spawn_at = state.tick_count.saturating_add(interval);
}

fn roll_drops(
    state: &mut GameState,
    data: &GameData,
    sink: &mut impl EventSink,
    monster_id: MonsterId,
) {
    let drops = data.monster(monster_id).drops.clone();
    let mult = if state.high_risk {
        data.balance.high_risk_drop_multiplier
    } else {
        1.0
    };
    for drop in drops {
        let boosted = (f64::from(drop.chance) * mult).clamp(0.0, 1.0) as f32;
        let key = crate::state::PityKey {
            monster: monster_id,
            item: drop.item_id,
        };
        let mut forced = false;
        if let Some(threshold) = game_data::BalanceConfig::pity_threshold(drop.chance) {
            let count = state.pity.entry(key).or_insert(0);
            *count = count.saturating_add(1);
            if *count >= threshold {
                forced = true;
                *count = 0;
            }
        }
        let hit =
            forced || (boosted > 0.0 && state.rng.gen_bool(f64::from(boosted).clamp(0.0, 1.0)));
        if !hit {
            continue;
        }
        if state.pity.contains_key(&key) && !forced {
            state.pity.insert(key, 0);
        }
        let count = if drop.min >= drop.max {
            drop.min
        } else {
            state.rng.gen_range(drop.min..=drop.max)
        };
        if count == 0 {
            continue;
        }
        grant_item(state, data, sink, drop.item_id, count, drop.chance < 0.01);
    }
}

pub(crate) fn grant_item(
    state: &mut GameState,
    data: &GameData,
    sink: &mut impl EventSink,
    item: ItemId,
    count: u32,
    rare: bool,
) {
    if matches!(data.item(item).kind, ItemKind::Currency) {
        state.gold = state.gold.saturating_add(u64::from(count));
        state.stats.gold_gained = state.stats.gold_gained.saturating_add(u64::from(count));
        sink.push_event(GameEvent::GoldGained {
            amount: u64::from(count),
        });
        return;
    }
    state.stats.drop_events = state.stats.drop_events.saturating_add(1);
    if !push_stack(state, data, item, count) {
        let price = data.item(item).price;
        let gold = u64::from(price) * u64::from(count) * 3 / 10;
        state.gold = state.gold.saturating_add(gold);
        sink.push_event(GameEvent::SoldOverflow { id: item, gold });
    } else {
        note_quality(state, data, item, sink);
    }
    sink.push_event(GameEvent::ItemDropped {
        id: item,
        count,
        rare,
    });
}

fn push_stack(state: &mut GameState, data: &GameData, item: ItemId, count: u32) -> bool {
    if data.item(item).stackable {
        if let Some(stack) = state.inventory.iter_mut().find(|stack| stack.item == item) {
            stack.count = stack.count.saturating_add(count);
            return true;
        }
    }
    if state.inventory.len() >= data.balance.inventory_slots as usize {
        return false;
    }
    let amount = if data.item(item).stackable { count } else { 1 };
    state.inventory.push(crate::state::ItemStack::fresh(
        item,
        amount,
        data,
        &mut state.rng,
    ));
    if !data.item(item).stackable && count > 1 {
        for _ in 1..count {
            if state.inventory.len() >= data.balance.inventory_slots as usize {
                return false;
            }
            state.inventory.push(crate::state::ItemStack::fresh(
                item,
                1,
                data,
                &mut state.rng,
            ));
        }
    }
    true
}

fn while_level(state: &mut GameState, data: &GameData, sink: &mut impl EventSink) {
    let mut leveled = false;
    while state.level < data.balance.max_level {
        let need = data.balance.exp_to_next(state.level);
        if state.exp < need {
            break;
        }
        state.exp -= need;
        state.level += 1;
        crate::state::recompute_base(state, data);
        let derived = *state.derived_stats(data);
        state.hp = derived.max_hp;
        state.mp = derived.max_mp;
        state.death_streak = 0;
        sink.push_event(GameEvent::LevelUp { level: state.level });
        for map in &data.maps {
            if map.req_level == state.level
                && !map.safe_zone
                && crate::command::map_unlocked(state, data, map.runtime_id)
            {
                sink.push_event(GameEvent::MapUnlocked {
                    map: map.runtime_id,
                });
            }
        }
        leveled = true;
    }
    if leveled {
        state.invalidate_derived();
    }
}

fn auto_advance(state: &mut GameState, data: &GameData, sink: &mut impl EventSink) {
    if !state.automation.auto_advance
        || state.level < data.balance.auto_advance_level
        || state.combat.is_some()
        || data.map(state.current_map).safe_zone
    {
        return;
    }
    // 刚因为连死退回来，这一级先不往深处走，升级后再试。
    let limit = data.balance.fallback_after_deaths;
    if limit > 0 && state.death_streak >= limit {
        return;
    }
    let current = state.current_map;
    // 地图是一棵树。半兽庙、沃玛七层、石墓阵→尸王殿这些支线走到头就没有下一张了，
    // 自动推进只沿主线走，支线留给玩家手动去打 BOSS。人在支线上时，主线下一张解锁了也会回去。
    let floor = data.map(current).req_level;
    let mut candidate: Option<MapId> = None;
    for map in main_line(data) {
        let def = data.map(map);
        if map == current || def.req_level <= floor {
            continue;
        }
        if candidate.is_some_and(|id| def.req_level >= data.map(id).req_level) {
            continue;
        }
        if crate::command::map_unlocked(state, data, map) {
            candidate = Some(map);
        }
    }
    if let Some(map) = candidate {
        state.current_map = map;
        state.next_spawn_at = state.tick_count;
        sink.push_event(GameEvent::MapChanged { map });
    }
}

/// 主线：从门槛最高的野外图往回顺着前置走到起点。顺序是从深到浅。
pub fn main_line(data: &GameData) -> Vec<MapId> {
    let deepest = data
        .maps
        .iter()
        .filter(|map| !map.safe_zone)
        .max_by_key(|map| map.req_level)
        .map(|map| map.runtime_id);
    let mut line = Vec::new();
    let mut cursor = deepest;
    while let Some(map) = cursor {
        if line.contains(&map) || line.len() > data.maps.len() {
            break;
        }
        line.push(map);
        cursor = data.map(map).req_map_id;
    }
    line
}

fn check_achievements(state: &mut GameState, sink: &mut impl EventSink) {
    let unlock = |state: &mut GameState, sink: &mut dyn EventSink, id: Achievement, ok: bool| {
        if ok && state.achievements.insert(id) {
            sink.push_event(GameEvent::AchievementUnlocked {
                id: id.label().to_string(),
            });
        }
    };
    let kills = state.stats.kills;
    let level = state.level;
    unlock(state, sink, Achievement::FirstKill, kills >= 1);
    unlock(state, sink, Achievement::Kills100, kills >= 100);
    unlock(state, sink, Achievement::Kills1000, kills >= 1000);
    unlock(state, sink, Achievement::Level10, level >= 10);
    unlock(state, sink, Achievement::Level30, level >= 30);
}

pub(crate) fn note_quality(
    state: &mut GameState,
    data: &GameData,
    item: ItemId,
    sink: &mut impl EventSink,
) {
    if data.item(item).quality >= Quality::Green
        && state.achievements.insert(Achievement::FirstGreen)
    {
        sink.push_event(GameEvent::AchievementUnlocked {
            id: Achievement::FirstGreen.label().to_string(),
        });
    }
}

fn hit_chance(data: &GameData, accuracy: i32, agility: i32) -> f64 {
    let cfg = &data.balance.hit_chance;
    let raw = cfg.base + f64::from(accuracy - agility) * cfg.per_point;
    raw.clamp(cfg.min, cfg.max)
}
