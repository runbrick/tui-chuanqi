//! 无界面的数值推演。平衡调参用它，不画终端。

use game_core::GameState;
use game_data::{Class, GameData, Gender};
use game_tui::find_data_dir;
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
            other => return Err(format!("未知参数 {other}")),
        }
    }
    let data_dir = find_data_dir().ok_or("找不到 data 目录")?;
    let data = GameData::load(&data_dir).map_err(|err| err.to_string())?;
    let mut state = GameState::new("模拟", class, Gender::Male, false, &data, seed);
    let started = Instant::now();
    for _ in 0..ticks {
        let _ = state.tick(&data);
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
        "击杀={} 金币={} 死亡={}",
        state.stats.kills, state.gold, state.stats.deaths
    );
    let mut kills: Vec<_> = state.stats.monster_kills.iter().collect();
    kills.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (id, count) in kills.iter().take(8) {
        println!("  {} ×{count}", data.monster(**id).name);
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
