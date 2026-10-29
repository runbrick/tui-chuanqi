//! 面板绘制。战斗日志一直留在下面。

use crate::app::{App, BuyForm, CreateForm, DeleteForm, HitMap, Panel, Screen, SellForm};
use crate::theme;
use crate::util::{bar, display_width, pad_to, truncate_to};
use game_core::{compare_equip, map_block, skill_rank, GameState, MapBlock, StatDelta};
use game_data::{Class, EquipSlot, GameData, Gender, ItemDef, ItemKind, Quality};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

const MIN_W: u16 = 80;
const MIN_H: u16 = 24;

/// 画当前这一帧。
pub fn draw(frame: &mut Frame, app: &App) {
    *app.hits.borrow_mut() = HitMap::default();
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
        Screen::Sell(form) => draw_sell(frame, app, form, area),
        Screen::Buy(form) => draw_buy(frame, app, form, area),
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

fn draw_list(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = vec![Line::from(
        "选择角色。↑↓ 或滚轮选，Enter/双击 进入，N 创建，D 删除，Q 退出。",
    )];
    let visible = area.height.saturating_sub(4) as usize;
    let total = app.characters.len();
    let selected = app.selected.min(total.saturating_sub(1));
    let range = page_of(selected, total, visible);
    if total == 0 {
        lines.push(Line::from("还没有角色。按 N 创建一个。"));
    }
    for index in range.clone() {
        let meta = &app.characters[index];
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
            "{}  {}  {}  Lv.{:<3}{risk}",
            pad_to(&meta.name, 12),
            class,
            gender,
            meta.level
        );
        let style = if index == selected {
            theme::selected()
        } else if meta.high_risk {
            theme::danger()
        } else {
            theme::text()
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    lines.push(Line::from(Span::styled(
        app.message.clone(),
        theme::level_up(),
    )));
    record_list(app, area, 1, range);
    frame.render_widget(list_panel("角色列表", lines), area);
}

fn draw_create(frame: &mut Frame, form: &CreateForm, area: Rect) {
    let class = ["战士", "法师", "道士"][form.class_index];
    let about = [
        "血厚防高，普攻就很疼，上手最快。",
        "伤害最高但很脆，蓝是瓶颈，要多带蓝药。",
        "前期慢，有毒、治疗和宝宝，后期最稳。",
    ][form.class_index];
    let gender = if form.gender_index == 0 { "男" } else { "女" };
    let risk = if form.high_risk {
        "开启：掉落 +50%，死亡会掉一件未锁定装备"
    } else {
        "关闭"
    };
    let lines = vec![
        Line::from("↑↓ 或 Tab 换行，←→ 或空格改当前行，Enter 下一步。Esc 返回列表。"),
        Line::from("职业、性别、高风险在创建后都不能再改。"),
        Line::from(""),
        Line::from(mark(form.step == 0, &format!("职业：‹ {class} ›"))),
        Line::from(Span::styled(format!("      {about}"), theme::dim())),
        Line::from(mark(form.step == 1, &format!("性别：‹ {gender} ›"))),
        Line::from(mark(
            form.step == 2,
            &format!(
                "名字：{}{}",
                form.name,
                if form.step == 2 { "_" } else { "" }
            ),
        )),
        Line::from(mark(form.step == 3, &format!("高风险：‹ {risk} ›"))),
        Line::from(mark(form.step == 4, "[ 确认创建 ]")),
        Line::from(""),
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
        Line::from(format!("{}_", form.confirm)),
    ];
    frame.render_widget(panel("删除角色", lines), area);
}

const QUANTITY_KEYS: &str =
    "直接输数字，↑↓ ±1，PgUp/PgDn ±10，End 或 M 最多。Enter 确认，Esc 取消。";

fn quantity_line(digits: &str, fresh: bool) -> Line<'static> {
    let typed = if digits.is_empty() { "_" } else { digits };
    let style = if fresh {
        theme::selected()
    } else {
        theme::text()
    };
    Line::from(vec![
        Span::raw("数量："),
        Span::styled(format!(" {typed} "), style),
    ])
}

fn draw_sell(frame: &mut Frame, app: &App, form: &SellForm, area: Rect) {
    let stack = app
        .state
        .as_ref()
        .and_then(|state| state.inventory.get(form.index));
    let (name, owned, gold) = match stack {
        Some(stack) => {
            let def = app.data.item(stack.item);
            let count = form.digits.parse::<u32>().unwrap_or(0).min(stack.count);
            (
                def.name.clone(),
                stack.count,
                game_core::sale_gold(&app.data, stack.item, count),
            )
        }
        None => ("这件东西已经不在了".to_string(), 0, 0),
    };
    let lines = vec![
        Line::from(format!("出售 {name}，这一格有 {owned} 个。")),
        Line::from(QUANTITY_KEYS),
        quantity_line(&form.digits, form.fresh),
        Line::from(format!("可得 {gold} 金币")),
        Line::from(Span::styled(form.error.clone(), theme::death())),
    ];
    frame.render_widget(panel("确认出售", lines), area);
}

fn draw_buy(frame: &mut Frame, app: &App, form: &BuyForm, area: Rect) {
    let offer = buy_offer(app, form.npc, form.stock_index);
    let count = form.digits.parse::<u32>().unwrap_or(0);
    let limit = app.buy_limit(form.npc, form.stock_index);
    let lines = match offer {
        Some((def, unit)) => {
            let cost = unit.saturating_mul(u64::from(count));
            let gold = app.state.as_ref().map(|state| state.gold).unwrap_or(0);
            let short = cost > gold;
            let mut lines = vec![
                Line::from(format!("购买 {}{}", def.name, item_note(def))),
                Line::from(format!(
                    "单价 {unit} 金币，身上有 {gold} 金币，最多能买 {limit} 个。"
                )),
            ];
            if let Some(reason) = app.state.as_ref().and_then(|state| requirement(state, def)) {
                lines.push(Line::from(Span::styled(
                    format!("注意：{reason}，买了暂时用不了。"),
                    theme::death(),
                )));
            }
            lines.push(Line::from(QUANTITY_KEYS));
            lines.push(quantity_line(&form.digits, form.fresh));
            lines.push(Line::from(Span::styled(
                format!("共需 {cost} 金币{}", if short { "，钱不够" } else { "" }),
                if short { theme::death() } else { theme::text() },
            )));
            lines.push(Line::from(Span::styled(form.error.clone(), theme::death())));
            lines
        }
        None => vec![
            Line::from("这件东西已经不卖了。"),
            Line::from("Esc 返回。"),
            Line::from(Span::styled(form.error.clone(), theme::death())),
        ],
    };
    frame.render_widget(panel("购买详情", lines), area);
}

fn buy_offer(app: &App, npc: usize, stock_index: usize) -> Option<(&ItemDef, u64)> {
    let shop = shop_of(&app.data, npc)?;
    let item = *shop.stock_ids.get(stock_index)?;
    let def = app.data.item(item);
    let unit = (f64::from(def.price) * f64::from(shop.buy_markup)).ceil() as u64;
    Some((def, unit))
}

fn shop_of(data: &GameData, npc: usize) -> Option<&game_data::ShopService> {
    data.npcs.get(npc)?.services.iter().find_map(|service| {
        if let game_data::NpcService::Shop(shop) = service {
            Some(shop)
        } else {
            None
        }
    })
}

fn draw_report(frame: &mut Frame, text: &str, area: Rect) {
    let mut lines: Vec<Line> = text.lines().map(Line::from).collect();
    lines.push(Line::from(""));
    lines.push(Line::from("按任意键或点一下继续。"));
    frame.render_widget(panel("离线收益", lines), area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from("F1 或 ? 打开和关闭本页，Esc、Enter 也能关。"),
        Line::from(""),
        Line::from("切面板  1–6，Tab / Shift+Tab，或点底部标签"),
        Line::from("列表    ↑↓（j k）选择并会绕回，PgUp/PgDn 翻页，Home/End 到头尾"),
        Line::from("鼠标    滚轮选择或翻日志，点一行选中，再点一次等于 Enter"),
        Line::from(""),
        Line::from("1 战斗  ↑↓ / PgUp / 滚轮 翻日志，End 回到最新"),
        Line::from("2 背包  Enter 使用或穿戴，S 出售，L 锁定，E 或 ←→ 切到身上的装备"),
        Line::from("        看身上时 Enter 脱下，L 锁定（高风险死亡不掉）"),
        Line::from("3 技能  Enter 开关自动施放"),
        Line::from("4 地图  Enter 进入，A 开关自动推进（只走主线，BOSS 支线要手动进）"),
        Line::from("5 商店  只在比奇城。←→ 或点标签换店铺，Enter 填数量后购买"),
        Line::from("        数量框：输数字，↑↓ ±1，PgUp/PgDn ±10，End 或 M 最多"),
        Line::from("6 统计  击杀、金币、经验效率"),
        Line::from(""),
        Line::from("城里    X 卖未锁定的白装   R 修理   F 按职业把红蓝药补满"),
        Line::from("吃药    P 开关   [ ] 调血线   - = 调蓝线（中文输入法下也能按）"),
        Line::from("升级    U 先看材料和成功率，再按 U 才执行；按别的键取消"),
        Line::from("其它    F2 伪装成时钟   Q 保存并退出"),
        Line::from(""),
        Line::from("装备颜色：白、绿、蓝、黄、红。死亡或没药回城后，休息完自动回原来的图。"),
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
    draw_footer(frame, app, rows[1]);
}

fn draw_character(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.state else {
        return;
    };
    let derived = state.derived_stats(&app.data);
    let exp_need = app.data.balance.exp_to_next(state.level);
    let permille = if exp_need == 0 {
        0
    } else {
        (u128::from(state.exp) * 1000 / u128::from(exp_need)).min(1000) as i32
    };
    let hp_ratio = state.hp as f64 / derived.max_hp.max(1) as f64;
    let eta = app
        .exp_per_second()
        .map(|rate| eta_text(exp_need.saturating_sub(state.exp) as f64 / rate))
        .unwrap_or_default();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(state.name.clone(), theme::title()),
            Span::raw(format!(" {} Lv.{}", state.class.label(), state.level)),
            Span::styled(
                if state.high_risk { " 高风险" } else { "" },
                theme::danger(),
            ),
        ]),
        Line::from(vec![
            Span::raw("HP "),
            Span::styled(
                bar(state.hp, derived.max_hp, 16),
                Style::default().fg(theme::hp_color(hp_ratio)),
            ),
            Span::raw(format!(" {}", format_pair(state.hp, derived.max_hp))),
        ]),
        Line::from(vec![
            Span::raw("MP "),
            Span::styled(
                bar(state.mp, derived.max_mp.max(1), 16),
                Style::default().fg(theme::mp_color()),
            ),
            Span::raw(format!(" {}", format_pair(state.mp, derived.max_mp))),
        ]),
        Line::from(vec![
            Span::raw("经验 "),
            Span::styled(
                bar(permille, 1000, 16),
                Style::default().fg(theme::exp_color()),
            ),
            Span::raw(format!(" {:.1}%", f64::from(permille) / 10.0)),
        ]),
        Line::from(Span::styled(
            format!("     {}/{} {eta}", state.exp, exp_need),
            theme::dim(),
        )),
        Line::from(format!(
            "攻击 {}-{}  魔法 {}-{}",
            derived.attack.min, derived.attack.max, derived.magic.min, derived.magic.max
        )),
        Line::from(format!(
            "道术 {}-{}  防御 {}-{}",
            derived.taoism.min, derived.taoism.max, derived.defense.min, derived.defense.max
        )),
        Line::from(format!(
            "魔防 {}-{}  准确 {}  敏捷 {}",
            derived.magic_defense.min, derived.magic_defense.max, derived.accuracy, derived.agility
        )),
        Line::from(format!(
            "金币 {}  吃药线 血{} 蓝{}{}",
            state.gold,
            potion_left(derived.max_hp, state.automation.hp_ratio),
            potion_left(derived.max_mp, state.automation.mp_ratio),
            if state.automation.auto_potion {
                ""
            } else {
                "（关）"
            }
        )),
    ];
    if let Some(warning) = character_warning(app, state, hp_ratio) {
        lines.push(Line::from(Span::styled(warning, theme::death())));
    }
    frame.render_widget(panel("角色", lines), area);
}

/// 最要紧的一条提醒：血少、武器快坏、药快没了。
fn character_warning(app: &App, state: &GameState, hp_ratio: f64) -> Option<String> {
    if hp_ratio < 0.3 && state.combat.is_some() {
        return Some("血量危险".to_string());
    }
    if let Some(weapon) = &state.equipment[EquipSlot::Weapon.index()] {
        if weapon.is_broken(&app.data) {
            return Some("武器坏了，回城按 R 修理".to_string());
        }
        if let Some(gear) = app.data.item(weapon.item).kind.gear() {
            if gear.durability > 0 && weapon.durability * 5 < gear.durability {
                return Some(format!(
                    "武器耐久 {}%，回城按 R 修理",
                    weapon.durability * 100 / gear.durability
                ));
            }
        }
    }
    let hp_potions: u32 = state
        .inventory
        .iter()
        .filter(|stack| {
            matches!(&app.data.item(stack.item).kind, ItemKind::Consumable(p) if p.heal_hp > 0)
        })
        .map(|stack| stack.count)
        .sum();
    if state.automation.auto_potion && hp_potions < 5 {
        return Some(format!("红药只剩 {hp_potions} 瓶，回城按 F 补"));
    }
    None
}

fn eta_text(seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return String::new();
    }
    let minutes = (seconds / 60.0).round() as u64;
    if minutes < 1 {
        "约 1 分钟内升级".to_string()
    } else if minutes < 60 {
        format!("约 {minutes} 分钟升级")
    } else {
        format!("约 {} 小时 {} 分升级", minutes / 60, minutes % 60)
    }
}

fn draw_equipment(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.state else {
        return;
    };
    let focus = (app.panel == Panel::Bag && app.bag_worn)
        .then(|| app.worn_slots().get(app.selected).copied())
        .flatten();
    let mut lines = Vec::new();
    for slot in EquipSlot::ALL {
        let (text, style) = match &state.equipment[slot.index()] {
            Some(stack) => {
                let def = app.data.item(stack.item);
                let style = if Some(slot) == focus {
                    theme::selected()
                } else {
                    Style::default().fg(theme::quality(def.quality))
                };
                (format!("{} {}", slot.label(), worn_text(app, stack)), style)
            }
            None => (format!("{} （空）", slot.label()), theme::dim()),
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    let title = if app.panel == Panel::Bag && app.bag_worn {
        "装备（Enter 脱下 L 锁）"
    } else {
        "装备（E 管理）"
    };
    frame.render_widget(list_panel(title, lines), area);
}

/// 已穿装备的一行：名字、强化、耐久（不满才显示）、锁、损坏。
fn worn_text(app: &App, stack: &game_core::ItemStack) -> String {
    let def = app.data.item(stack.item);
    let mut text = def.name.clone();
    if stack.upgrade > 0 {
        text.push_str(&format!(" +{}", stack.upgrade));
    }
    if stack.is_broken(&app.data) {
        text.push_str(" 损坏");
    } else if let Some(gear) = def.kind.gear() {
        if gear.durability > 0 && stack.durability < gear.durability {
            text.push_str(&format!(
                " {}%",
                u64::from(stack.durability) * 100 / u64::from(gear.durability)
            ));
        }
    }
    if stack.locked {
        text.push_str(" 锁");
    }
    text
}

fn draw_combat(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.state else {
        return;
    };
    let data = &app.data;
    let map = data.map(state.current_map);
    let kills = state
        .map_kills
        .get(&state.current_map)
        .copied()
        .unwrap_or(0);
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("地图：{}", map.name), theme::title()),
        Span::raw(if map.safe_zone {
            String::new()
        } else {
            format!("  本图击杀 {kills}")
        }),
    ])];
    if let Some(hint) = next_map_hint(state, data) {
        lines.push(Line::from(Span::styled(hint, theme::dim())));
    }
    if let Some(boss) = boss_hint(state, data) {
        lines.push(Line::from(Span::styled(boss, theme::dim())));
    }
    lines.push(Line::from(""));
    if let Some(combat) = &state.combat {
        let monster = data.monster(combat.monster);
        let mut head = vec![Span::styled(
            format!("{} Lv.{}", monster.name, monster.level),
            theme::text().add_modifier(Modifier::BOLD),
        )];
        if combat.boss {
            head.insert(0, Span::styled("BOSS ", theme::danger()));
        }
        if monster.undead {
            head.push(Span::styled(" 不死系", theme::dim()));
        }
        lines.push(Line::from(head));
        lines.push(Line::from(vec![
            Span::raw("HP "),
            Span::styled(
                bar(combat.hp.max(0), monster.hp, 18),
                Style::default().fg(theme::hp_color(
                    f64::from(combat.hp.max(0)) / f64::from(monster.hp.max(1)),
                )),
            ),
            Span::raw(format!(" {}/{}", combat.hp.max(0), monster.hp)),
        ]));
        lines.push(Line::from(Span::styled(
            format!(
                "攻 {}-{}  防 {}-{}  魔防 {}-{}  {}",
                monster.attack.min,
                monster.attack.max,
                monster.defense.min,
                monster.defense.max,
                monster.magic_defense.min,
                monster.magic_defense.max,
                if monster.attack_type.uses_magic_defense() {
                    "法术攻击"
                } else {
                    "物理攻击"
                }
            ),
            theme::dim(),
        )));
        let mut effects = Vec::new();
        if combat.poison_left > 0 {
            effects.push(format!(
                "中毒 {}秒/每秒{}",
                combat.poison_left.div_ceil(10),
                combat.poison_dps
            ));
        }
        if combat.defense_down_left > 0 {
            effects.push(format!("降防 {:.0}%", combat.defense_down * 100.0));
        }
        if combat.next_attack_power.is_some() {
            effects.push("烈火蓄力".to_string());
        }
        if !effects.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("怪物状态：{}", effects.join("  ")),
                theme::rare(),
            )));
        }
    } else if state.tick_count < state.resting_until {
        let secs = (state.resting_until - state.tick_count).div_ceil(10);
        let back = state
            .return_map
            .map(|id| format!("，之后回 {}", data.map(id).name))
            .unwrap_or_default();
        lines.push(Line::from(Span::styled(
            format!("正在休息，还剩 {secs} 秒{back}"),
            theme::level_up(),
        )));
    } else if map.safe_zone {
        lines.push(Line::from("在城里，不会刷怪。5 商店  R 修理  F 补药"));
        lines.push(Line::from(Span::styled(
            "去打怪：按 4 打开地图，选一张按 Enter",
            theme::dim(),
        )));
    } else {
        lines.push(Line::from("等待刷怪"));
    }
    let mut buffs = Vec::new();
    if state.shield_left > 0 {
        buffs.push(format!(
            "减伤 {:.0}% {}秒",
            state.shield_reduction * 100.0,
            state.shield_left.div_ceil(10)
        ));
    }
    if state.buff_left > 0 {
        buffs.push(format!("敏捷+{}", state.buff_agility));
    }
    if let Some(pet) = &state.pet {
        let left = pet.expires_at.saturating_sub(state.tick_count).div_ceil(10);
        buffs.push(format!(
            "{} {}/{} 剩{left}秒",
            data.skill(pet.skill).name,
            pet.hp.max(0),
            pet.max_hp
        ));
    }
    if !buffs.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("自身：{}", buffs.join("  ")),
            theme::up(),
        )));
    }
    if !state.automation.auto_advance && state.level >= data.balance.auto_advance_level {
        lines.push(Line::from(Span::styled(
            "自动推进没开：按 A，打够了会自己去下一张图",
            theme::dim(),
        )));
    }
    frame.render_widget(panel("战斗", lines), area);
}

/// 主线下一张图还差什么。
fn next_map_hint(state: &GameState, data: &GameData) -> Option<String> {
    let floor = data.map(state.current_map).req_level;
    let next = game_core::main_line(data)
        .into_iter()
        .filter(|id| *id != state.current_map && data.map(*id).req_level > floor)
        .min_by_key(|id| data.map(*id).req_level)?;
    let name = &data.map(next).name;
    Some(match map_block(state, data, next) {
        None => format!("下一张主线图 {name} 已解锁"),
        Some(MapBlock::Level(level)) => format!("下一张主线图 {name}：需 {level} 级"),
        Some(MapBlock::Kills(prev, have, need)) => {
            let level = data.map(next).req_level;
            let lv = if state.level < level {
                format!("，还要 {level} 级")
            } else {
                String::new()
            };
            format!(
                "下一张主线图 {name}：{} 击杀 {have}/{need}{lv}",
                data.map(prev).name
            )
        }
    })
}

fn boss_hint(state: &GameState, data: &GameData) -> Option<String> {
    let boss = data.map(state.current_map).boss.as_ref()?;
    let name = &data.monster(boss.monster_id).name;
    let kills = state
        .map_kills
        .get(&state.current_map)
        .copied()
        .unwrap_or(0);
    if kills < boss.kills_to_unlock {
        return Some(format!(
            "BOSS {name}：再杀 {} 只小怪出现",
            boss.kills_to_unlock - kills
        ));
    }
    let ready = state
        .boss_ready_at
        .get(&boss.monster_id)
        .copied()
        .unwrap_or(0);
    if ready > state.tick_count {
        let minutes = (ready - state.tick_count).div_ceil(600);
        return Some(format!("BOSS {name}：约 {minutes} 分钟后再出现"));
    }
    Some(format!("BOSS {name}：随时会出现"))
}

fn page_of(selected: usize, len: usize, visible: usize) -> std::ops::Range<usize> {
    let visible = visible.max(1);
    if len <= visible {
        return 0..len;
    }
    let start = selected.saturating_sub(visible / 2).min(len - visible);
    start..start + visible
}

/// 记下列表从第几行开始、对应哪些下标，给鼠标点选用。`header` 是边框下面的说明行数。
fn record_list(app: &App, area: Rect, header: u16, range: std::ops::Range<usize>) {
    let top = area.y + 1 + header;
    let bottom = area.y + area.height.saturating_sub(1);
    let rows = (range.len() as u16).min(bottom.saturating_sub(top));
    if rows == 0 {
        return;
    }
    app.hits.borrow_mut().list = Some((
        Rect::new(area.x + 1, top, area.width.saturating_sub(2), rows),
        range.start,
    ));
}

fn draw_bag(frame: &mut Frame, app: &App, area: Rect) {
    if app.bag_worn {
        draw_worn(frame, app, area);
        return;
    }
    let Some(state) = &app.state else {
        return;
    };
    let visible = area.height.saturating_sub(4) as usize;
    let total = state.inventory.len();
    let selected = app.selected.min(total.saturating_sub(1));
    let range = page_of(selected, total, visible);
    let slots = app.data.balance.inventory_slots;
    let mut lines = vec![Line::from(Span::styled(
        format!("Enter 使用/穿戴  S 卖  X 卖白装  L 锁  E 看身上   {total}/{slots} 格"),
        theme::dim(),
    ))];
    let detail = state.inventory.get(selected).map(|stack| {
        let def = app.data.item(stack.item);
        let gold = game_core::sale_gold(&app.data, stack.item, stack.count);
        let mut spans = vec![Span::raw(format!("选中 {}  能卖 {gold} 金", def.name))];
        if let Some(delta) = compare_equip(state, &app.data, selected) {
            spans.push(Span::raw("  换上后 "));
            let parts = delta_parts(delta);
            if parts.is_empty() {
                spans.push(Span::styled("属性不变", theme::dim()));
            }
            for (text, better) in parts {
                spans.push(Span::styled(
                    format!("{text} "),
                    if better { theme::up() } else { theme::down() },
                ));
            }
        }
        Line::from(spans)
    });
    lines.push(detail.unwrap_or_else(|| Line::from("背包是空的。打怪会掉东西，商店也能买。")));
    for index in range.clone() {
        let stack = &state.inventory[index];
        let def = app.data.item(stack.item);
        let rank = if def.kind.gear().is_some() {
            format!(" [{}]", theme::quality_name(def.quality))
        } else {
            String::new()
        };
        let count = if def.stackable || stack.count > 1 {
            format!(" ×{}", stack.count)
        } else {
            String::new()
        };
        let lock = if stack.locked { " 锁" } else { "" };
        let mut spans = vec![Span::raw(format!(
            "{}{rank}{}{count}{lock}",
            def.name,
            item_note(def)
        ))];
        let reason = requirement(state, def);
        if let Some(reason) = &reason {
            spans.push(Span::raw(format!("  {reason}")));
        } else if let Some(delta) = compare_equip(state, &app.data, index) {
            for (text, better) in delta_parts(delta).into_iter().take(3) {
                spans.push(Span::styled(
                    format!(" {text}"),
                    if better { theme::up() } else { theme::down() },
                ));
            }
        }
        let base = if index == selected {
            theme::selected()
        } else if reason.is_some() {
            theme::dim()
        } else {
            Style::default().fg(theme::quality(def.quality))
        };
        let spans = spans
            .into_iter()
            .map(|span| {
                if index == selected || reason.is_some() || span.style == Style::default() {
                    Span::styled(span.content, base)
                } else {
                    span
                }
            })
            .collect::<Vec<_>>();
        lines.push(Line::from(spans));
    }
    record_list(app, area, 2, range);
    frame.render_widget(list_panel("背包", lines), area);
}

fn draw_worn(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.state else {
        return;
    };
    let slots = app.worn_slots();
    let visible = area.height.saturating_sub(3) as usize;
    let selected = app.selected.min(slots.len().saturating_sub(1));
    let range = page_of(selected, slots.len(), visible);
    let mut lines = vec![Line::from(Span::styled(
        "身上的装备。Enter 脱下  L 锁定（高风险死亡不掉）  E/Esc 回背包",
        theme::dim(),
    ))];
    if slots.is_empty() {
        lines.push(Line::from("身上什么都没穿。"));
    }
    for index in range.clone() {
        let slot = slots[index];
        let Some(stack) = &state.equipment[slot.index()] else {
            continue;
        };
        let def = app.data.item(stack.item);
        let style = if index == selected {
            theme::selected()
        } else {
            Style::default().fg(theme::quality(def.quality))
        };
        lines.push(Line::from(Span::styled(
            format!("{} {}", pad_to(slot.label(), 6), worn_text(app, stack)),
            style,
        )));
    }
    record_list(app, area, 1, range);
    frame.render_widget(list_panel("身上", lines), area);
}

/// 换装属性差，按“攻 +1/+3”这种短写法。第二项是变好还是变差。
fn delta_parts(delta: StatDelta) -> Vec<(String, bool)> {
    let mut parts = Vec::new();
    let mut range = |label: &str, min: i32, max: i32| {
        if min == 0 && max == 0 {
            return;
        }
        let text = if min == max {
            format!("{label}{min:+}")
        } else {
            format!("{label}{min:+}/{max:+}")
        };
        parts.push((text, min + max > 0));
    };
    range("攻", delta.attack_min, delta.attack_max);
    range("魔", delta.magic_min, delta.magic_max);
    range("道", delta.taoism_min, delta.taoism_max);
    range("防", delta.defense_min, delta.defense_max);
    range("魔防", delta.magic_defense_min, delta.magic_defense_max);
    range("准", delta.accuracy, delta.accuracy);
    range("敏", delta.agility, delta.agility);
    range("血", delta.hp, delta.hp);
    range("蓝", delta.mp, delta.mp);
    parts
}

/// 现在用不了的原因。能用返回 `None`。
fn requirement(state: &GameState, def: &ItemDef) -> Option<String> {
    match &def.kind {
        ItemKind::SkillBook(book) => {
            if !book.req_class.allows(state.class) {
                Some(format!("{}用", book.req_class.label()))
            } else if state.skills.contains_key(&book.skill_id) {
                Some("已学会".to_string())
            } else if state.level < book.req_level {
                Some(format!("需 {} 级", book.req_level))
            } else {
                None
            }
        }
        kind => {
            let gear = kind.gear()?;
            if !gear.req_class.allows(state.class) {
                Some(format!("{}装备", gear.req_class.label()))
            } else if kind.is_armor() && !gear.gender.allows(state.gender) {
                Some(format!("{}款", gear.gender.label()))
            } else if state.level < gear.req_level {
                Some(format!("需 {} 级", gear.req_level))
            } else {
                None
            }
        }
    }
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
    let mut lines = vec![Line::from(Span::styled(
        "Enter 开关自动施放。熟练度满了要角色等级够才升下一级。",
        theme::dim(),
    ))];
    if known.is_empty() {
        lines.push(Line::from(
            "还没学会技能。书店有卖，怪也会掉书。在背包里按 Enter 学。",
        ));
    }
    for (offset, skill) in known.iter().enumerate().skip(range.start).take(range.len()) {
        let progress = &state.skills[&skill.runtime_id];
        let rank = skill_rank(skill, progress, state.level);
        let passive = matches!(skill.effect, game_data::SkillEffect::Passive(_));
        let auto = if passive {
            "被动"
        } else if progress.auto {
            "自动"
        } else {
            "手动"
        };
        let mut text = format!(
            "{} Lv.{}/{} {auto}",
            pad_to(&skill.name, 10),
            rank.map_or(0, |r| r + 1),
            skill.max_level
        );
        if let Some(tier) = rank.map(|r| &skill.levels[r]) {
            if !passive {
                text.push_str(&format!("  耗蓝 {}", tier.mp_cost));
                if tier.cooldown_ms > 0 {
                    text.push_str(&format!(
                        " 冷却 {:.1}秒",
                        f64::from(tier.cooldown_ms) / 1000.0
                    ));
                }
            }
        }
        let need = app.data.balance.skill_exp_base * u32::from(progress.level);
        let note = if rank.is_none() {
            format!("  需 {} 级才能用", skill.levels[0].req_level)
        } else if progress.level >= skill.max_level {
            "  已满级".to_string()
        } else {
            let next = &skill.levels[progress.level as usize];
            if progress.exp >= need && state.level < next.req_level {
                format!("  熟练满，{} 级升", next.req_level)
            } else {
                format!("  熟练 {}/{need}", progress.exp)
            }
        };
        text.push_str(&note);
        let style = if offset == selected {
            theme::selected()
        } else if rank.is_none() || (!progress.auto && !passive) {
            theme::dim()
        } else {
            theme::text()
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    record_list(app, area, 1, range);
    frame.render_widget(list_panel("技能", lines), area);
}

fn draw_maps(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.state else {
        return;
    };
    let main = game_core::main_line(&app.data);
    let visible = area.height.saturating_sub(3) as usize;
    let total = app.data.maps.len();
    let selected = app.selected.min(total.saturating_sub(1));
    let range = page_of(selected, total, visible);
    let auto = if state.automation.auto_advance {
        "开"
    } else {
        "关"
    };
    let mut lines = vec![Line::from(Span::styled(
        format!("Enter 进入。A 自动推进（{auto}）只走主线，BOSS 支线手动进。"),
        theme::dim(),
    ))];
    for index in range.clone() {
        let map = &app.data.maps[index];
        let here = map.runtime_id == state.current_map;
        let (status, open) = if map.safe_zone {
            ("安全区".to_string(), true)
        } else {
            match map_block(state, &app.data, map.runtime_id) {
                None => ("可进入".to_string(), true),
                Some(MapBlock::Level(level)) => (format!("需 {level} 级"), false),
                Some(MapBlock::Kills(prev, have, need)) => {
                    (format!("{} {have}/{need}", app.data.map(prev).name), false)
                }
            }
        };
        let tag = if map.safe_zone {
            String::new()
        } else if main.contains(&map.runtime_id) {
            "主线".to_string()
        } else {
            match &map.boss {
                Some(boss) => format!("BOSS {}", app.data.monster(boss.monster_id).name),
                None => "支线".to_string(),
            }
        };
        let text = format!(
            "{} {} Lv.{:<3}{}  {}",
            if here { "*" } else { " " },
            pad_to(&map.name, 12),
            map.req_level,
            pad_to(&tag, 14),
            status
        );
        let style = if index == selected {
            theme::selected()
        } else if here {
            theme::level_up()
        } else if open {
            theme::text()
        } else {
            theme::dim()
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    record_list(app, area, 1, range);
    frame.render_widget(list_panel("地图", lines), area);
}

fn draw_shop(frame: &mut Frame, app: &App, area: Rect) {
    if !app.in_town() {
        frame.render_widget(
            panel(
                "商店",
                vec![
                    Line::from("比奇城才有商店。人在野外时不能买东西、修理或补药。"),
                    Line::from("按 4 打开地图，选比奇城按 Enter。"),
                ],
            ),
            area,
        );
        return;
    }
    let Some(state) = &app.state else {
        return;
    };
    let shops = app.shop_npcs();
    if shops.is_empty() {
        frame.render_widget(panel("商店", vec![Line::from("没有营业的店铺")]), area);
        return;
    }
    let shop_at = app.shop_index % shops.len();
    let npc = &app.data.npcs[shops[shop_at]];
    let mut tabs = Vec::new();
    let mut x = area.x + 1;
    for (index, npc_index) in shops.iter().enumerate() {
        let name = &app.data.npcs[*npc_index].name;
        let label = format!(" {name} ");
        let width = display_width(&label) as u16;
        if x + width <= area.x + area.width.saturating_sub(1) {
            app.hits
                .borrow_mut()
                .shops
                .push((Rect::new(x, area.y + 1, width, 1), index));
        }
        x += width;
        let style = if index == shop_at {
            theme::selected()
        } else {
            theme::dim()
        };
        tabs.push(Span::styled(label, style));
    }
    let mut rows = Vec::new();
    if let Some(shop) = shop_of(&app.data, shops[shop_at]) {
        for item_id in &shop.stock_ids {
            let def = app.data.item(*item_id);
            let price = (f64::from(def.price) * f64::from(shop.buy_markup)).ceil() as u64;
            rows.push((def, price));
        }
    }
    let visible = area.height.saturating_sub(4) as usize;
    let selected = app.selected.min(rows.len().saturating_sub(1));
    let range = page_of(selected, rows.len(), visible);
    let mut lines = vec![
        Line::from(tabs),
        Line::from(Span::styled(
            truncate_to(
                &format!("{}  ←→/点标签 换店，Enter 买", npc.greeting),
                area.width.saturating_sub(2) as usize,
            ),
            theme::dim(),
        )),
    ];
    for index in range.clone() {
        let (def, price) = rows[index];
        let reason = requirement(state, def);
        let short = price > state.gold;
        let mut text = format!("{}{}  {price} 金", def.name, item_note(def));
        if let Some(reason) = &reason {
            text.push_str(&format!("  {reason}"));
        } else if short {
            text.push_str("  钱不够");
        }
        let style = if index == selected {
            theme::selected()
        } else if reason.is_some() {
            theme::dim()
        } else if short {
            theme::hurt()
        } else {
            Style::default().fg(theme::quality(def.quality))
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    record_list(app, area, 2, range);
    frame.render_widget(list_panel("商店", lines), area);
}

fn draw_stats(frame: &mut Frame, app: &App, area: Rect) {
    let Some(state) = &app.state else {
        return;
    };
    let hours = state.tick_count as f64 / 36000.0;
    let derived = state.derived_stats(&app.data);
    let per_hour = if hours < 0.01 {
        0.0
    } else {
        state.stats.exp_gained as f64 / hours
    };
    let recent = app
        .exp_per_second()
        .map(|rate| format!("  最近每小时 {:.0}", rate * 3600.0))
        .unwrap_or_default();
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
            "累计 {:.1} 小时（含离线）  平均每小时经验 {:.0}{recent}",
            hours, per_hour
        )),
        Line::from(format!(
            "自动吃药 {}  血低于 {:.0}%（{}） 蓝低于 {:.0}%（{}）",
            if state.automation.auto_potion {
                "开"
            } else {
                "关"
            },
            state.automation.hp_ratio * 100.0,
            potion_left(derived.max_hp, state.automation.hp_ratio),
            state.automation.mp_ratio * 100.0,
            potion_left(derived.max_mp, state.automation.mp_ratio),
        )),
        Line::from(format!(
            "自动推进 {}",
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
    app.hits.borrow_mut().log = Some(area);
    let height = area.height.saturating_sub(2) as usize;
    let max_scroll = app.log.len().saturating_sub(height);
    let scroll = app.log_scroll.min(max_scroll);
    let end = app.log.len() - scroll;
    let start = end.saturating_sub(height);
    let lines: Vec<Line> = app
        .log
        .range(start..end)
        .map(|line| {
            let style = if line.rare {
                theme::rare()
            } else if line.bad {
                theme::death()
            } else if line.level {
                theme::level_up()
            } else if line.hurt {
                theme::hurt()
            } else {
                theme::text()
            };
            Line::from(Span::styled(line.text.clone(), style))
        })
        .collect();
    let title = if scroll > 0 {
        format!("日志（往上翻了 {scroll} 行，End 回到最新）")
    } else {
        "日志".to_string()
    };
    frame.render_widget(list_panel(title, lines), area);
}

fn panel<'a>(title: impl Into<String>, lines: Vec<Line<'a>>) -> Paragraph<'a> {
    list_panel(title, lines).wrap(Wrap { trim: false })
}

/// 列表不换行：一行对应一个下标，鼠标点选才对得上。超出宽度的部分直接截掉。
fn list_panel<'a>(title: impl Into<String>, lines: Vec<Line<'a>>) -> Paragraph<'a> {
    Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(title.into(), theme::title())),
    )
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();
    let mut x = area.x;
    for (index, panel) in Panel::ALL.iter().enumerate() {
        let label = format!(" {} {} ", index + 1, panel.label());
        let width = display_width(&label) as u16;
        let disabled = *panel == Panel::Shop && !app.in_town();
        let style = if *panel == app.panel {
            theme::selected()
        } else if disabled {
            theme::dim()
        } else {
            theme::text().add_modifier(Modifier::BOLD)
        };
        app.hits
            .borrow_mut()
            .tabs
            .push((Rect::new(x, area.y, width, 1), *panel));
        x += width;
        spans.push(Span::styled(label, style));
    }
    spans.push(Span::styled("  F1帮助  Q退出  F2伪装", theme::dim()));
    let notice = if app.message.is_empty() {
        Line::from(Span::styled(panel_hint(app), theme::dim()))
    } else {
        Line::from(Span::styled(app.message.clone(), theme::level_up()))
    };
    frame.render_widget(Paragraph::new(vec![Line::from(spans), notice]), area);
}

/// 没有提示消息时，底部第二行显示当前面板最常用的键。
fn panel_hint(app: &App) -> &'static str {
    match app.panel {
        Panel::Combat => "Tab 换面板  ↑↓/滚轮 翻日志  A 自动推进  P 自动吃药  [ ] 血线  - = 蓝线",
        Panel::Bag if app.bag_worn => "Enter 脱下  L 锁定  E/Esc 回背包  U 升级武器",
        Panel::Bag => "Enter 使用/穿戴  S 卖  X 卖白装  L 锁  E 身上的装备  U 升级武器",
        Panel::Skill => "Enter 开关自动施放  ↑↓ 选择",
        Panel::Map => "Enter 进入  A 自动推进  * 是当前地图",
        Panel::Shop => "←→ 换店  Enter 买  R 修理  F 补药  X 卖白装",
        Panel::Stats => "↑↓/滚轮 翻日志  End 回到最新",
    }
}

fn mark(on: bool, text: &str) -> String {
    format!("{} {text}", if on { ">" } else { " " })
}

fn format_pair(current: i32, max: i32) -> String {
    format!("{current}/{max}")
}

/// 低于这条线就吃药，换算成当前上限下的剩余点数。
fn potion_left(max: i32, ratio: f64) -> i32 {
    (ratio.clamp(0.0, 1.0) * f64::from(max.max(0))).round() as i32
}

/// 技能书标出职业，药水标出回多少血、蓝。装备仍走原来的品质和对比。
fn item_note(def: &ItemDef) -> String {
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

#[cfg(test)]
mod clock_tests {
    #[test]
    fn unix_epoch_is_1970() {
        assert_eq!(super::civil_from_days(0), (1970, 1, 1));
    }
}
