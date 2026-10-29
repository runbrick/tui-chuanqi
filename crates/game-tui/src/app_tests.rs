//! 不开终端的界面测试：用 `TestBackend` 画一帧，再模拟按键和鼠标。

use super::*;
use crossterm::event::{KeyEventState, MouseEvent};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn playing(class: Class) -> (App, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "tui-mir-app-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let data = GameData::load(game_core::repo_data_dir()).unwrap();
    let store = SaveStore::open(&root).unwrap();
    store
        .create(&data, "测试", class, Gender::Male, false)
        .unwrap();
    let mut app = App::open(data, store).unwrap();
    app.enter_selected();
    app.screen = Screen::Play;
    (app, root)
}

fn render(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut text = String::new();
    for y in 0..height {
        for x in 0..width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

/// 宽字符后面会补空格，比较前去掉空白。
fn squash(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
}

fn click(app: &mut App, column: u16, row: u16) {
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

#[test]
fn every_panel_renders_at_min_and_large_size() {
    for class in [Class::Warrior, Class::Mage, Class::Taoist] {
        let (mut app, root) = playing(class);
        for _ in 0..300 {
            app.step_logic();
        }
        for key in ['1', '2', 'e', '3', '4', '6'] {
            press(&mut app, KeyCode::Char(key));
            for (w, h) in [(80, 24), (120, 40)] {
                let screen = render(&app, w, h);
                assert!(squash(&screen).contains("1战斗"), "{screen}");
            }
        }
        press(&mut app, KeyCode::F(1));
        assert!(squash(&render(&app, 80, 24)).contains("帮助"));
        press(&mut app, KeyCode::Esc);
        assert!(matches!(app.screen, Screen::Play), "Esc 应该关掉帮助");
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn worn_view_shows_and_locks_starter_gear() {
    let (mut app, root) = playing(Class::Warrior);
    press(&mut app, KeyCode::Char('E'));
    assert_eq!(app.panel, Panel::Bag);
    assert!(app.bag_worn);
    assert!(squash(&render(&app, 100, 30)).contains("木剑"));
    press(&mut app, KeyCode::Char('l'));
    let state = app.state.as_ref().unwrap();
    assert!(state.equipment[EquipSlot::Weapon.index()]
        .as_ref()
        .is_some_and(|stack| stack.locked));
    assert!(app.message.contains("已锁定"), "{}", app.message);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ime_punctuation_and_caps_still_work() {
    let (mut app, root) = playing(Class::Warrior);
    app.state.as_mut().unwrap().level = 20;
    let before = app.state.as_ref().unwrap().automation.hp_ratio;
    press(&mut app, KeyCode::Char('【'));
    let after = app.state.as_ref().unwrap().automation.hp_ratio;
    assert!(after < before, "全角【 应该等于 [：{before} -> {after}");
    press(&mut app, KeyCode::Char('２'));
    assert_eq!(app.panel, Panel::Bag, "全角数字也能切面板");
    press(&mut app, KeyCode::Char('Q'));
    assert!(app.quit, "大写 Q 也能退出");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn panel_selection_is_remembered_and_map_starts_on_current() {
    let (mut app, root) = playing(Class::Warrior);
    press(&mut app, KeyCode::Char('4'));
    let here = app.current_map_row();
    assert_eq!(app.selected, here);
    press(&mut app, KeyCode::Down);
    let moved = app.selected;
    press(&mut app, KeyCode::Char('1'));
    press(&mut app, KeyCode::Char('4'));
    assert_eq!(app.selected, moved);
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.selected, app.data.maps.len() - 1, "↑ 到顶会绕到最后");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tab_skips_the_shop_outside_town() {
    let (mut app, root) = playing(Class::Warrior);
    assert!(!app.in_town());
    let mut seen = Vec::new();
    for _ in 0..6 {
        press(&mut app, KeyCode::Tab);
        seen.push(app.panel);
    }
    assert!(!seen.contains(&Panel::Shop), "{seen:?}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn clicking_tabs_and_rows_works() {
    let (mut app, root) = playing(Class::Warrior);
    let _ = render(&app, 100, 30);
    let (rect, _) = app
        .hits
        .borrow()
        .tabs
        .iter()
        .copied()
        .find(|(_, panel)| *panel == Panel::Map)
        .unwrap();
    click(&mut app, rect.x + 1, rect.y);
    assert_eq!(app.panel, Panel::Map);
    let city = app.data.safe_map().unwrap();
    let city_row = app
        .data
        .maps
        .iter()
        .position(|map| map.runtime_id == city)
        .unwrap();
    app.selected = city_row;
    let _ = render(&app, 100, 30);
    let (list, first) = app.hits.borrow().list.unwrap();
    let row = list.y + (city_row - first) as u16;
    click(&mut app, list.x + 2, row);
    assert_eq!(app.selected, city_row);
    click(&mut app, list.x + 2, row);
    assert!(app.in_town(), "双击地图行应该进图");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn buy_quantity_replaces_prefill_and_caps_at_limit() {
    let (mut app, root) = playing(Class::Warrior);
    let city = app.data.safe_map().unwrap();
    app.state.as_mut().unwrap().current_map = city;
    app.state.as_mut().unwrap().gold = 100;
    press(&mut app, KeyCode::Char('5'));
    assert_eq!(app.panel, Panel::Shop);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('3'));
    let Screen::Buy(form) = &app.screen else {
        panic!("应该在购买详情");
    };
    assert_eq!(form.digits, "3", "第一下数字应该替换预填的 1");
    let limit = app.buy_limit(form.npc, form.stock_index);
    press(&mut app, KeyCode::End);
    let Screen::Buy(form) = &app.screen else {
        panic!("应该还在购买详情");
    };
    assert_eq!(form.digits, limit.max(1).to_string());
    assert!(squash(&render(&app, 80, 24)).contains("最多能买"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn upgrade_confirmation_is_cancelled_by_other_keys() {
    let (mut app, root) = playing(Class::Warrior);
    app.state.as_mut().unwrap().gold = 100_000;
    let ore = app.data.item_id("black_iron_ore").unwrap();
    let ring = app.data.item_id("iron_ring").unwrap();
    {
        let data = &app.data;
        let state = app.state.as_mut().unwrap();
        let ore = game_core::ItemStack::fresh(ore, 1, data, &mut state.rng);
        let ring = game_core::ItemStack::fresh(ring, 1, data, &mut state.rng);
        state.inventory.push(ore);
        state.inventory.push(ring);
    }
    press(&mut app, KeyCode::Char('u'));
    assert!(app.upgrade_armed, "{}", app.message);
    assert!(app.message.contains("铁戒指"), "{}", app.message);
    press(&mut app, KeyCode::Char('2'));
    assert!(!app.upgrade_armed);
    press(&mut app, KeyCode::Char('u'));
    assert!(app.upgrade_armed, "取消后重新按 U 只是再看一次");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn gold_after_a_kill_joins_the_kill_line() {
    let (mut app, root) = playing(Class::Warrior);
    for _ in 0..3000 {
        app.step_logic();
    }
    assert!(app
        .log
        .iter()
        .any(|line| line.text.starts_with("击杀") && line.text.contains("金币")));
    assert!(!app.log.iter().any(|line| line.text.starts_with("获得金币")));
    let _ = std::fs::remove_dir_all(root);
}
