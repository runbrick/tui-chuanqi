//! 面板绘制。战斗日志一直留在下面。

use crate::app::{App, CreateForm, DeleteForm, Panel, Screen};
use crate::theme;
use crate::util::{bar, pad_to};
use game_core::{compare_equip, CoreError};
use game_data::{Class, EquipSlot, Gender, ItemKind, Quality};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

const MIN_W: u16 = 80;
const MIN_H: u16 = 24;

/// 画当前这一帧。
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if app.disguised {
        draw_clock(frame, area);
        return;
    }
    if area.width < MIN_W || area.height < MIN_H {
        let lines = vec![
            Line::from(format!(
                "请把窗口放大到至少 {MIN_W}×{MIN_H}。当前 {}×{}",
                area.width, area.height
            )),
            Line::from(app.message.clone()),
        ];
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        return;
    }
    match &app.screen {
        Screen::List => draw_list(frame, app, area),
        Screen::Create(form) => draw_create(frame, form, area),
        Screen::Delete(form) => draw_delete(frame, app, form, area),
        Screen::Report(text) => draw_report(frame, text, area),
        Screen::Help(_) => draw_help(frame, area),
        Screen::Play => draw_play(frame, app, area),
    }
}

fn draw_clock(frame: &mut Frame, area: Rect) {
    let (hour, minute, second, date) = beijing_now();
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("{hour:02}:{minute:02}:{second:02}"),
            Style::default()
                .fg(ratatui::style::Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(date),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn beijing_now() -> (u32, u32, u32, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        + 8 * 3600;
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    (
        (tod / 3600) as u32,
        ((tod % 3600) / 60) as u32,
        (tod % 60) as u32,
        format!("{year:04}-{month:02}-{day:02}"),
    )
}

/// 公历，`days` 是 Unix 纪元起的天数。
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day)
}

#[cfg(test)]
mod clock_tests {
    #[test]
    fn unix_epoch_is_1970() {
        assert_eq!(super::civil_from_days(0), (1970, 1, 1));
    }
}

fn draw_list(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = vec![Line::from(
        "选择角色。N 创建，D 删除，Enter 进入，Q 退出。同名可以同时存在。",
    )];
    if app.characters.is_empty() {
        lines.push(Line::from("还没有角色。"));
    }
    for (index, meta) in app.characters.iter().enumerate() {
        let class = match meta.class.as_str() {
            "mage" => "法师",
            "taoist" => "道士",
            _ => "战士",
        };
        let gender = if meta.gender == "female" {
            "女"
        } else {
            "男"
        };
        let risk = if meta.high_risk {
            " 【高风险】"
        } else {
            ""
        };
        let text = format!(
            "{}  {}  {} {}  Lv.{:<3}{risk}",
            pad_to(&meta.name, 12),
            class,
            gender,
            meta.id,
            meta.level
        );
        let style = if index == app.selected {
            theme::selected()
        } else if meta.high_risk {
            theme::danger()
        } else {
            theme::text()
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    lines.push(Line::from(app.message.clone()));
    frame.render_widget(panel("角色列表", lines), area);
}

fn draw_create(frame: &mut Frame, form: &CreateForm, area: Rect) {
    let class = ["战士", "法师", "道士"][form.class_index];
    let gender = if form.gender_index == 0 { "男" } else { "女" };
    let risk = if form.high_risk {
        "开启：掉落 +50%，死亡会掉一件未锁定装备"
    } else {
        "关闭"
    };
    let lines = vec![
        Line::from("上下移动，左右改当前行，Enter 确认创建。Esc 返回列表。"),
        Line::from("职业、性别、高风险在创建后都不能再改。"),
        Line::from(mark(form.step == 0, &format!("职业：{class}"))),
        Line::from(mark(form.step == 1, &format!("性别：{gender}"))),
        Line::from(mark(form.step == 2, &format!("名字：{}", form.name))),
        Line::from(mark(form.step == 3, &format!("高风险：{risk}"))),
        Line::from(mark(form.step == 4, "确认创建")),
        Line::from(Span::styled(form.error.clone(), theme::death())),
    ];
    frame.render_widget(panel("创建角色", lines), area);
}

fn draw_delete(frame: &mut Frame, app: &App, form: &DeleteForm, area: Rect) {
    let name = app
        .characters
        .get(form.index)
        .map(|meta| meta.name.as_str())
        .unwrap_or("?");
    let lines = vec![
        Line::from("删除后不能恢复。"),
        Line::from(format!("请输入角色名「{name}」后按 Enter。Esc 取消。")),
        Line::from(form.confirm.clone()),
    ];
    frame.render_widget(panel("删除角色", lines), area);
}

fn draw_report(frame: &mut Frame, text: &str, area: Rect) {
    let mut lines: Vec<Line> = text.lines().map(Line::from).collect();
    lines.push(Line::from("Enter 继续。"));
    frame.render_widget(panel("离线收益", lines), area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from("F1、? 或 ？ 打开和关闭本页。Esc 也可以关闭。"),
        Line::from(""),
        Line::from("1 战斗    看当前的怪和自己的出手"),
        Line::from("2 背包    ↑↓ 选择，Enter 使用或穿戴，L 锁定，PgUp/PgDn 翻页"),
        Line::from("3 技能    Enter 开关自动施放"),
        Line::from("4 地图    Enter 进入，A 开关自动换图"),
        Line::from("5 商店    只在比奇城。←→ 换店铺，Enter 买 1 个，+ 买 10 个"),
        Line::from("6 统计    击杀、金币、每小时经验"),
        Line::from(""),
        Line::from("X 卖掉白色装备    R 修理    F 把药补到 20 瓶"),
        Line::from("U 武器升级，先看成功率，再按一次才执行"),
        Line::from("F2 伪装成时钟，再按一次恢复"),
        Line::from("Q 保存并退出"),
        Line::from(""),
        Line::from("装备颜色：白、绿、蓝、黄、红。打不过的 BOSS 会自动跳过。"),
    ];
    frame.render_widget(panel("帮助", lines), area);
}

fn draw_play(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(2)])
        .split(area);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(36), Constraint::Min(20)])
        .split(rows[0]);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(14), Constraint::Min(6)])
        .split(cols[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(12)])
        .split(cols[1]);
    draw_character(frame, app, left[0]);
    draw_equipment(frame, app, left[1]);
    match app.panel {
        Panel::Combat => draw_combat(frame, app, right[0]),
        Panel::Bag => draw_bag(frame, app, right[0]),
        Panel::Skill => draw_skills(frame, app, right[0]),
        Panel::Map => draw_maps(frame, app, right[0]),
        Panel::Shop => draw_shop(frame, app, right[0]),
        Panel::Stats => draw_stats(frame, app, right[0]),
    }
    draw_log(frame, app, right[1]);
    frame.render_widget(
        Paragraph::new(footer(app)).wrap(Wrap { trim: false }),
        rows[1],
    );
}

fn draw_character(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.state else {
        return;
    };
    let derived = state.derived_stats(&app.data);
    let exp_need = app.data.balance.exp_to_next(state.level);
    let exp_ratio = if exp_need == 0 {
        0.0
    } else {
        state.exp as f64 / exp_need as f64
    };
    let hp_ratio = state.hp as f64 / derived.max_hp.max(1) as f64;
    let lines = vec![
        Line::from(format!(
            "{} {} Lv.{} {}",
            state.name,
            state.class.label(),
            state.level,
            if state.high_risk { "高风险" } else { "" }
        )),
        Line::from(format!(
            "HP {} {}",
            bar(state.hp, derived.max_hp, 16),
            format_pair(state.hp, derived.max_hp)
        )),
        Line::from(format!(
            "MP {} {}",
            bar(state.mp, derived.max_mp.max(1), 16),
            format_pair(state.mp, derived.max_mp)
        )),
        Line::from(format!(
            "攻击 {}-{}",
            derived.attack.min, derived.attack.max
        )),
        Line::from(format!(
            "魔法 {}-{}  道术 {}-{}",
            derived.magic.min, derived.magic.max, derived.taoism.min, derived.taoism.max
        )),
        Line::from(format!(
            "防御 {}-{}  魔防 {}-{}",
            derived.defense.min,
            derived.defense.max,
            derived.magic_defense.min,
            derived.magic_defense.max
        )),
        Line::from(format!(
            "准确 {}  敏捷 {}  金币 {}",
            derived.accuracy, derived.agility, state.gold
        )),
        Line::from(format!(
            "经验 {} {:.0}%",
            bar((exp_ratio * 10.0) as i32, 10, 16),
            exp_ratio * 100.0
        )),
        Line::from(Span::styled(
            if hp_ratio < 0.3 { "血量危险" } else { "" },
            Style::default().fg(theme::hp_color(hp_ratio)),
        )),
    ];
    frame.render_widget(panel("角色", lines), area);
}

fn draw_equipment(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.state else {
        return;
    };
    let mut lines = vec![Line::from(vec![
        Span::raw("品质 "),
        Span::styled("白 ", Style::default().fg(theme::quality(Quality::White))),
        Span::styled("绿 ", Style::default().fg(theme::quality(Quality::Green))),
        Span::styled("蓝 ", Style::default().fg(theme::quality(Quality::Blue))),
        Span::styled("黄 ", Style::default().fg(theme::quality(Quality::Purple))),
        Span::styled("红", Style::default().fg(theme::quality(Quality::Orange))),
    ])];
    for slot in EquipSlot::ALL {
        let (text, style) = match &state.equipment[slot.index()] {
            Some(stack) => {
                let name = app.data.item(stack.item).name.clone();
                let plus = if stack.upgrade > 0 {
                    format!(" +{}", stack.upgrade)
                } else {
                    String::new()
                };
                let lock = if stack.locked { " 锁" } else { "" };
                let broken = if stack.is_broken(&app.data) {
                    " 损坏"
                } else {
                    ""
                };
                let style = Style::default().fg(theme::quality(app.data.item(stack.item).quality));
                (
                    format!("{} {name}{plus}{lock}{broken}", slot.label()),
                    style,
                )
            }
            None => (format!("{} （空）", slot.label()), theme::text()),
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    frame.render_widget(panel("装备", lines), area);
}

fn draw_combat(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.state else {
        return;
    };
    let map = app.data.map(state.current_map).name.clone();
    let mut lines = vec![Line::from(format!("地图：{map}"))];
    if let Some(combat) = &state.combat {
        let monster = &app.data.monster(combat.monster);
        let tag = if combat.boss { " BOSS" } else { "" };
        lines.push(Line::from(format!("{tag}{}", monster.name)));
        lines.push(Line::from(format!(
            "HP {} {}/{}",
            bar(combat.hp.max(0), monster.hp, 18),
            combat.hp.max(0),
            monster.hp
        )));
    } else if state.tick_count < state.resting_until {
        lines.push(Line::from("正在休息"));
    } else {
        lines.push(Line::from("等待刷怪"));
    }
    if let Some(pet) = &state.pet {
        lines.push(Line::from(format!("宝宝 {}/{}", pet.hp.max(0), pet.max_hp)));
    }
    frame.render_widget(panel("战斗", lines), area);
}

fn page_of(selected: usize, len: usize, visible: usize) -> std::ops::Range<usize> {
    let visible = visible.max(1);
    if len <= visible {
        return 0..len;
    }
    let start = selected.saturating_sub(visible / 2).min(len - visible);
    start..start + visible
}

fn draw_bag(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.state else {
        return;
    };
    let visible = area.height.saturating_sub(3) as usize;
    let total = state.inventory.len();
    let selected = app.selected.min(total.saturating_sub(1));
    let range = page_of(selected, total, visible);
    let page = if total == 0 {
        1
    } else {
        selected / visible.max(1) + 1
    };
    let pages = total.div_ceil(visible.max(1)).max(1);
    let mut lines = vec![Line::from(format!(
        "Enter 使用或穿戴，L 锁定。PgUp/PgDn 翻页。{page}/{pages}"
    ))];
    if total == 0 {
        lines.push(Line::from("背包是空的"));
    }
    for index in range {
        let stack = &state.inventory[index];
        let def = app.data.item(stack.item);
        let rank = if def.kind.gear().is_some() {
            format!(" [{}]", theme::quality_name(def.quality))
        } else {
            String::new()
        };
        let mut text = format!("{}{rank}{} ×{}", def.name, item_note(def), stack.count);
        if let Some(delta) = compare_equip(state, &app.data, index) {
            text.push_str(&format_delta(delta.attack_min, delta.attack_max));
        }
        let style = if index == selected {
            theme::selected()
        } else {
            Style::default().fg(theme::quality(def.quality))
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    frame.render_widget(panel("背包", lines), area);
}

fn draw_skills(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.state else {
        return;
    };
    let known: Vec<_> = app
        .data
        .skills
        .iter()
        .filter(|skill| state.skills.contains_key(&skill.runtime_id))
        .collect();
    let visible = area.height.saturating_sub(3) as usize;
    let selected = app.selected.min(known.len().saturating_sub(1));
    let range = page_of(selected, known.len(), visible);
    let mut lines = vec![Line::from("Enter 开关自动施放。↑↓ 选择，PgUp/PgDn 翻页。")];
    if known.is_empty() {
        lines.push(Line::from("还没学会技能。书店有卖，怪也会掉书。"));
    }
    for (offset, skill) in known.iter().enumerate().skip(range.start).take(range.len()) {
        let progress = &state.skills[&skill.runtime_id];
        let auto = if progress.auto { "自动" } else { "手动" };
        let style = if offset == selected {
            theme::selected()
        } else {
            theme::text()
        };
        lines.push(Line::from(Span::styled(
            format!("{} Lv.{} {auto}", skill.name, progress.level),
            style,
        )));
    }
    frame.render_widget(panel("技能", lines), area);
}

fn draw_maps(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.state else {
        return;
    };
    let visible = area.height.saturating_sub(3) as usize;
    let total = app.data.maps.len();
    let selected = app.selected.min(total.saturating_sub(1));
    let range = page_of(selected, total, visible);
    let mut lines = vec![Line::from("Enter 进入，A 开关自动推进。PgUp/PgDn 翻页。")];
    for index in range {
        let map = &app.data.maps[index];
        let here = if map.runtime_id == state.current_map {
            " *"
        } else {
            ""
        };
        let reason = map_block(app, map.runtime_id);
        let text = format!("{here} {} {reason}", map.name);
        let style = if reason.is_empty() {
            theme::text()
        } else {
            Style::default().fg(ratatui::style::Color::DarkGray)
        };
        lines.push(Line::from(Span::styled(
            text.trim_start().to_string(),
            if index == selected {
                theme::selected()
            } else {
                style
            },
        )));
    }
    frame.render_widget(panel("地图", lines), area);
}

fn draw_shop(frame: &mut Frame, app: &App, area: Rect) {
    let in_town = app.state.as_ref().is_some_and(|state| {
        let map = app.data.map(state.current_map);
        map.safe_zone || map.id == "bichen_city"
    });
    if !in_town {
        frame.render_widget(
            panel(
                "商店",
                vec![Line::from(
                    "比奇城才有商店。人在野外时不能买东西、修理或补药。",
                )],
            ),
            area,
        );
        return;
    }
    let shops = app
        .data
        .npcs
        .iter()
        .enumerate()
        .filter(|(_, npc)| {
            npc.services
                .iter()
                .any(|service| matches!(service, game_data::NpcService::Shop(_)))
        })
        .map(|(index, npc)| (index, npc.name.clone(), npc.greeting.clone()))
        .collect::<Vec<_>>();
    if shops.is_empty() {
        frame.render_widget(panel("商店", vec![Line::from("没有营业的店铺")]), area);
        return;
    }
    let shop_at = app.shop_index % shops.len();
    let (npc_index, _, greeting) = &shops[shop_at];
    let mut tabs = Vec::new();
    for (index, (_, name, _)) in shops.iter().enumerate() {
        let label = if index == shop_at {
            format!("[{name}]")
        } else {
            name.clone()
        };
        let style = if index == shop_at {
            theme::selected()
        } else {
            theme::text()
        };
        tabs.push(Span::styled(format!("{label} "), style));
    }
    let mut rows = Vec::new();
    for service in &app.data.npcs[*npc_index].services {
        let game_data::NpcService::Shop(shop) = service else {
            continue;
        };
        for item_id in &shop.stock_ids {
            let def = app.data.item(*item_id);
            let price = (def.price as f64 * f64::from(shop.buy_markup)).ceil() as u32;
            rows.push((
                def.quality,
                format!("{}{}  {price}金", def.name, item_note(def)),
            ));
        }
    }
    let visible = area.height.saturating_sub(5) as usize;
    let selected = app.selected.min(rows.len().saturating_sub(1));
    let range = page_of(selected, rows.len(), visible);
    let mut lines = vec![
        Line::from(tabs),
        Line::from(format!("{greeting}  ←→ 换店，Enter 买 1，+ 买 10")),
    ];
    for (index, (quality, text)) in rows.iter().enumerate().skip(range.start).take(range.len()) {
        let style = if index == selected {
            theme::selected()
        } else {
            Style::default().fg(theme::quality(*quality))
        };
        lines.push(Line::from(Span::styled(text.clone(), style)));
    }
    frame.render_widget(panel("商店", lines), area);
}

fn draw_stats(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.state else {
        return;
    };
    let hours = state.tick_count as f64 / 36000.0;
    let per_hour = if hours < 0.01 {
        0.0
    } else {
        state.stats.exp_gained as f64 / hours
    };
    let mut lines = vec![
        Line::from(format!(
            "击杀 {}  死亡 {}  掉落 {}",
            state.stats.kills, state.stats.deaths, state.stats.drop_events
        )),
        Line::from(format!(
            "累计金币 {}  累计经验 {}",
            state.stats.gold_gained, state.stats.exp_gained
        )),
        Line::from(format!(
            "在线 {:.1} 小时  每小时经验 {:.0}",
            hours, per_hour
        )),
        Line::from(format!(
            "喝药 {}  阈值 {:.0}%/{:.0}%  自动换图 {}",
            if state.automation.auto_potion {
                "开"
            } else {
                "关"
            },
            state.automation.hp_ratio * 100.0,
            state.automation.mp_ratio * 100.0,
            if state.automation.auto_advance {
                "开"
            } else {
                "关"
            }
        )),
    ];
    for achievement in &state.achievements {
        lines.push(Line::from(format!("成就 {}", achievement.label())));
    }
    frame.render_widget(panel("统计", lines), area);
}

fn draw_log(frame: &mut Frame, app: &App, area: Rect) {
    let height = area.height.saturating_sub(2) as usize;
    let start = app.log.len().saturating_sub(height);
    let lines: Vec<Line> = app
        .log
        .iter()
        .skip(start)
        .map(|line| {
            let style = if line.rare {
                theme::rare()
            } else if line.bad {
                theme::death()
            } else if line.level {
                theme::level_up()
            } else {
                theme::text()
            };
            Line::from(Span::styled(line.text.clone(), style))
        })
        .collect();
    frame.render_widget(panel("日志", lines), area);
}

fn panel<'a>(title: &'a str, lines: Vec<Line<'a>>) -> Paragraph<'a> {
    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(title, theme::title())),
        )
        .wrap(Wrap { trim: false })
}

fn footer(app: &App) -> Vec<Line<'static>> {
    let keys = Line::from(Span::styled(
        "1战斗 2背包 3技能 4地图 5商店 6统计   F1帮助  Q退出  F2伪装",
        Style::default().add_modifier(Modifier::BOLD),
    ));
    let text = if app.message.is_empty() {
        " ".to_string()
    } else {
        app.message.clone()
    };
    let notice = Line::from(Span::styled(text, theme::level_up()));
    vec![keys, notice]
}

fn mark(on: bool, text: &str) -> String {
    format!("{} {text}", if on { ">" } else { " " })
}

fn format_pair(current: i32, max: i32) -> String {
    format!("{current}/{max}")
}

/// 技能书标出职业，药水标出回多少血、蓝。装备仍走原来的品质和对比。
fn item_note(def: &game_data::ItemDef) -> String {
    match &def.kind {
        ItemKind::SkillBook(book) => format!(" [{}]", book.req_class.label()),
        ItemKind::Consumable(potion) => potion_note(potion),
        _ => String::new(),
    }
}

fn potion_note(potion: &game_data::ConsumableDef) -> String {
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
    let pace = if potion.instant || potion.duration_ticks == 0 {
        "立刻"
    } else {
        "缓慢"
    };
    format!(" {}回{}", pace, parts.join(" "))
}

fn format_delta(min: i32, max: i32) -> String {
    if min == 0 && max == 0 {
        return String::new();
    }
    format!("  攻击 {min:+}/{max:+}")
}

fn map_block(app: &App, map: game_data::MapId) -> String {
    let Some(state) = &app.state else {
        return String::new();
    };
    let mut cloned = state.clone();
    let command = game_core::Command::ChangeMap { map };
    match command.apply(&mut cloned, &app.data) {
        Err(CoreError::MapLocked) | Err(CoreError::LevelTooLow(_)) => "未解锁".to_string(),
        _ => String::new(),
    }
}

/// 给测试和界面用的职业中文，避免面板里散落英文。
pub fn class_name(class: Class) -> &'static str {
    class.label()
}

/// 性别中文。
pub fn gender_name(gender: Gender) -> &'static str {
    gender.label()
}

/// 品质名，目前只给调试。
pub fn quality_name(quality: Quality) -> &'static str {
    theme::quality_name(quality)
}

/// 物品是不是衣服。界面解释性别不符时用得到。
pub fn is_armor(kind: &ItemKind) -> bool {
    kind.is_armor()
}
