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

#[test]
fn small_window_while_playing_shows_idle_info_only() {
    let (mut app, root) = playing(Class::Warrior);
    for _ in 0..300 {
        app.step_logic();
    }
    for (w, h) in [(60, 16), (40, 10), (30, 6), (10, 2), (1, 1)] {
        let screen = render(&app, w, h);
        assert!(!squash(&screen).contains("放大"), "{screen}");
    }
    let screen = squash(&render(&app, 40, 10));
    assert!(screen.contains("测试"), "{screen}");
    assert!(screen.contains("HP"), "{screen}");
    assert!(!screen.contains("1战斗"), "{screen}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn small_window_outside_play_still_asks_for_more_room() {
    let (mut app, root) = playing(Class::Warrior);
    app.screen = Screen::List;
    assert!(squash(&render(&app, 40, 10)).contains("放大"));
    let _ = std::fs::remove_dir_all(root);
}

/// 假装后台安装线程已经起来，返回发送端给测试用。
fn fake_install(app: &mut App) -> mpsc::Sender<InstallMsg> {
    let (tx, rx) = mpsc::channel();
    app.install_rx = Some(rx);
    app.installing = Some(Progress::Connecting);
    app.update = Some(Release {
        tag: "v0.2.0".to_string(),
        version: "0.2.0".to_string(),
        asset_url: String::new(),
        asset_name: String::new(),
    });
    tx
}

#[test]
fn install_progress_shows_in_the_footer_and_does_not_expire() {
    let (mut app, root) = playing(Class::Warrior);
    let tx = fake_install(&mut app);
    tx.send(InstallMsg::Progress(Progress::Downloading {
        done: 1_048_576,
        total: Some(4 * 1_048_576),
    }))
    .unwrap();
    app.poll_install();
    assert!(app.message.contains("25%"), "{}", app.message);
    assert!(app.message_at.is_none(), "进度提示不能超时消失");
    app.expire_message();
    assert!(app.message.contains("25%"));
    assert!(squash(&render(&app, 100, 30)).contains("25%"));
    assert!(!app.quit);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn install_done_announces_success_then_quits_after_the_delay() {
    let (mut app, root) = playing(Class::Warrior);
    let tx = fake_install(&mut app);
    tx.send(InstallMsg::Progress(Progress::Done)).unwrap();
    drop(tx);
    app.poll_install();
    assert!(app.message.contains("成功"), "{}", app.message);
    assert!(app.message.contains("重启"), "{}", app.message);
    assert!(!app.quit, "得先让玩家看到提示");
    app.poll_install();
    assert!(app.message.contains("成功"), "等待期间提示不能被清掉");
    assert!(!app.quit);
    app.restart_at = Some(Instant::now());
    app.poll_install();
    assert!(app.quit);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn update_notice_stays_visible_after_temporary_messages_expire() {
    let (mut app, root) = playing(Class::Warrior);
    app.update = Some(Release {
        tag: "v0.2.0".to_string(),
        version: "0.2.0".to_string(),
        asset_url: String::new(),
        asset_name: String::new(),
    });

    app.note("临时操作提示");
    let screen = squash(&render(&app, 100, 30));
    assert!(screen.contains("临时操作提示"), "{screen}");
    assert!(!screen.contains("按U下载并重启"), "{screen}");

    app.message.clear();
    app.message_at = None;
    let screen = squash(&render(&app, 100, 30));
    assert!(screen.contains("有新版本0.2.0"), "{screen}");
    assert!(screen.contains("按U下载并重启"), "{screen}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn install_failure_is_reported_and_can_be_retried() {
    let (mut app, root) = playing(Class::Warrior);
    let tx = fake_install(&mut app);
    tx.send(InstallMsg::Progress(Progress::Extracting)).unwrap();
    tx.send(InstallMsg::Failed("下载中断".to_string())).unwrap();
    drop(tx);
    app.poll_install();
    assert!(app.message.contains("更新失败"), "{}", app.message);
    assert!(app.message.contains("下载中断"), "{}", app.message);
    assert!(app.installing.is_none());
    assert!(app.install_rx.is_none());
    assert!(app.update.is_some(), "失败后要能再按 U 重试");
    assert!(!app.quit);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn install_thread_dying_silently_counts_as_a_failure() {
    let (mut app, root) = playing(Class::Warrior);
    let tx = fake_install(&mut app);
    drop(tx);
    app.poll_install();
    assert!(app.message.contains("更新失败"), "{}", app.message);
    assert!(app.installing.is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn pressing_u_while_installing_is_swallowed() {
    let (mut app, root) = playing(Class::Warrior);
    let _tx = fake_install(&mut app);
    let before = app.panel;
    press(&mut app, KeyCode::Char('u'));
    assert_eq!(app.panel, before);
    assert!(!app.upgrade_armed, "更新中的 U 不能落到武器升级上");
    assert!(app.install_rx.is_some());
    let _ = std::fs::remove_dir_all(root);
}
