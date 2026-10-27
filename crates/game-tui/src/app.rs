//! 主循环。逻辑 tick 和渲染分开，输入只变成 `Command`。

use crate::logfmt::{format_event, LogLine};
use crate::saveio::{CharacterMeta, SaveStore};
use crate::ui;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use game_core::{validate_name, Command, CoreError, GameEvent, GameState};
use game_data::{Class, GameData, Gender};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(100);
const MAX_CATCHUP: u32 = 5;
const LOG_CAP: usize = 500;

/// 右侧面板。左侧角色栏一直在。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// 战斗。
    Combat,
    /// 背包。
    Bag,
    /// 技能。
    Skill,
    /// 地图。
    Map,
    /// 商店。
    Shop,
    /// 统计。
    Stats,
}

/// 当前界面。
pub enum Screen {
    /// 选角。
    List,
    /// 创建。
    Create(CreateForm),
    /// 删除确认。
    Delete(DeleteForm),
    /// 出售确认。战斗仍在暂停，填数量后才卖掉。
    Sell(SellForm),
    /// 离线报告。
    Report(String),
    /// 游戏中。
    Play,
    /// 帮助。
    Help(Box<Screen>),
}

/// 创建向导。职业、性别、高风险在确认前都能改，写进存档后不能改。
pub struct CreateForm {
    /// 0 职业，1 性别，2 名字，3 高风险，4 确认。
    pub step: u8,
    /// 职业下标。
    pub class_index: usize,
    /// 性别下标。
    pub gender_index: usize,
    /// 名字。
    pub name: String,
    /// 高风险。
    pub high_risk: bool,
    /// 校验错误。
    pub error: String,
}

/// 删除需要重新输入名字。
pub struct DeleteForm {
    /// 列表下标。
    pub index: usize,
    /// 已输入的确认文字。
    pub confirm: String,
}

/// 出售确认。数量是玩家正在输入的数字。
pub struct SellForm {
    /// 背包下标。确认期间不能换选中。
    pub index: usize,
    /// 已输入的数量，空着表示还没填。
    pub digits: String,
    /// 校验错误。
    pub error: String,
}

/// 整个前端状态。
pub struct App {
    /// 静态数据。
    pub data: GameData,
    /// 存档。
    pub store: SaveStore,
    /// 界面。
    pub screen: Screen,
    /// 角色列表。
    pub characters: Vec<CharacterMeta>,
    /// 当前槽位。
    pub slot: Option<String>,
    /// 当前角色。
    pub state: Option<GameState>,
    /// 日志。
    pub log: VecDeque<LogLine>,
    /// 面板。
    pub panel: Panel,
    /// 列表选中。
    pub selected: usize,
    /// 底部提示。
    pub message: String,
    /// 提示出现的时间，过几秒就收回，按键栏继续显示。
    pub message_at: Option<Instant>,
    /// 武器升级已经看过一次成功率，再按一次才执行。
    pub upgrade_armed: bool,
    /// 伪装成时钟。游戏还在跑，画面不显示。
    pub disguised: bool,
    /// 当前店铺，只算有货的 NPC。
    pub shop_index: usize,
    quit: bool,
    last_save: Instant,
}

impl App {
    /// 打开存档目录并进入选角。
    pub fn open(data: GameData, store: SaveStore) -> Result<Self, String> {
        let characters = store.list()?;
        Ok(Self {
            data,
            store,
            screen: Screen::List,
            characters,
            slot: None,
            state: None,
            log: VecDeque::new(),
            panel: Panel::Combat,
            selected: 0,
            message: "方向键选择，Enter 进入，N 创建，D 删除".to_string(),
            message_at: None,
            upgrade_armed: false,
            disguised: false,
            shop_index: 0,
            quit: false,
            last_save: Instant::now(),
        })
    }

    /// 跑到玩家退出。调用方负责终端模式。
    pub fn run(
        mut self,
        terminal: &mut ratatui::Terminal<impl ratatui::backend::Backend>,
    ) -> Result<(), String> {
        let mut last = Instant::now();
        let mut acc = Duration::ZERO;
        while !self.quit {
            let now = Instant::now();
            acc += now.saturating_duration_since(last);
            last = now;
            self.poll_input()?;
            self.expire_message();
            let mut steps = 0;
            while acc >= TICK && steps < MAX_CATCHUP {
                self.step_logic();
                acc -= TICK;
                steps += 1;
            }
            if acc > TICK * MAX_CATCHUP {
                acc = Duration::ZERO;
            }
            terminal
                .draw(|frame| ui::draw(frame, &self))
                .map_err(|err| err.to_string())?;
            let wait = TICK
                .saturating_sub(acc)
                .min(Duration::from_millis(TICK_WAIT));
            if event::poll(wait).map_err(|err| err.to_string())? {
                // 事件留到下一轮 poll_input，避免和上面的读取抢。poll 已经消费不了，
                // 所以这里如果 poll 为真，立刻再读一次。
                self.poll_input()?;
            }
        }
        self.save_if_playing();
        Ok(())
    }

    fn expire_message(&mut self) {
        let stale = self
            .message_at
            .is_some_and(|at| at.elapsed() >= Duration::from_secs(8));
        if stale {
            self.message.clear();
            self.message_at = None;
        }
    }

    fn note(&mut self, text: impl Into<String>) {
        let text = text.into();
        if text.is_empty() {
            self.message.clear();
            self.message_at = None;
        } else {
            self.message = text;
            self.message_at = Some(Instant::now());
        }
    }

    fn poll_input(&mut self) -> Result<(), String> {
        while event::poll(Duration::ZERO).map_err(|err| err.to_string())? {
            if let Event::Key(key) = event::read().map_err(|err| err.to_string())? {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                self.on_key(key);
            }
        }
        Ok(())
    }

    fn step_logic(&mut self) {
        if !matches!(self.screen, Screen::Play) {
            return;
        }
        let Some(state) = self.state.as_mut() else {
            return;
        };
        let events = state.tick(&self.data);
        let mut notable = false;
        let mut left_town = false;
        for event in events {
            if matches!(
                event,
                GameEvent::LevelUp { .. } | GameEvent::ItemDropped { rare: true, .. }
            ) {
                notable = true;
            }
            if matches!(
                event,
                GameEvent::MapChanged { .. } | GameEvent::Resting { .. }
            ) {
                left_town = true;
            }
            self.push_log(format_event(&event, &self.data));
        }
        if left_town && self.panel == Panel::Shop && !self.in_town() {
            self.panel = Panel::Combat;
            self.note("离开比奇城，商店已关闭。");
        }
        if notable || self.last_save.elapsed() >= Duration::from_secs(30) {
            self.save_if_playing();
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if key.code == KeyCode::F(2) {
            self.disguised = !self.disguised;
            return;
        }
        if self.disguised {
            return;
        }
        if self.help_key(key) {
            self.toggle_help();
            return;
        }
        match &self.screen {
            Screen::List => self.key_list(key.code),
            Screen::Create(_) => self.key_create(key),
            Screen::Delete(_) => self.key_delete(key),
            Screen::Sell(_) => self.key_sell(key),
            Screen::Report(_) => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                    self.screen = Screen::Play;
                }
            }
            Screen::Play => self.key_play(key.code),
            Screen::Help(_) => {}
        }
    }

    fn help_key(&self, key: KeyEvent) -> bool {
        if self.typing_name() && !matches!(self.screen, Screen::Help(_)) {
            return false;
        }
        match key.code {
            KeyCode::F(1) => true,
            KeyCode::Char('?' | '？') => true,
            KeyCode::Char('/') if key.modifiers.contains(KeyModifiers::SHIFT) => true,
            _ => false,
        }
    }

    fn toggle_help(&mut self) {
        if matches!(self.screen, Screen::Help(_)) {
            if let Screen::Help(prev) = std::mem::replace(&mut self.screen, Screen::List) {
                self.screen = *prev;
            }
        } else {
            let prev = std::mem::replace(&mut self.screen, Screen::List);
            self.screen = Screen::Help(Box::new(prev));
        }
    }

    fn typing_name(&self) -> bool {
        match &self.screen {
            Screen::Create(form) => form.step == 2,
            Screen::Delete(_) | Screen::Sell(_) => true,
            _ => false,
        }
    }

    fn key_list(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.quit = true,
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.characters.len().saturating_sub(1))
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.screen = Screen::Create(CreateForm {
                    step: 0,
                    class_index: 0,
                    gender_index: 0,
                    name: String::new(),
                    high_risk: false,
                    error: String::new(),
                });
            }
            KeyCode::Char('d') | KeyCode::Char('D') if !self.characters.is_empty() => {
                self.screen = Screen::Delete(DeleteForm {
                    index: self.selected.min(self.characters.len() - 1),
                    confirm: String::new(),
                });
            }
            KeyCode::Enter => self.enter_selected(),
            _ => {}
        }
    }

    fn enter_selected(&mut self) {
        let Some(meta) = self.characters.get(self.selected).cloned() else {
            self.note("还没有角色，按 N 创建");
            return;
        };
        match self.store.load(&meta.id, &self.data) {
            Ok((state, report)) => {
                self.slot = Some(meta.id);
                self.note(String::new());
                self.log.clear();
                let mut state = state;
                game_core::compact_inventory(&mut state, &self.data);
                self.state = Some(state);
                self.last_save = Instant::now();
                if report.ticks > 0 {
                    self.screen = Screen::Report(format_report(&report, &self.data));
                } else {
                    self.screen = Screen::Play;
                }
            }
            Err(err) => self.note(err),
        }
    }

    fn key_create(&mut self, key: KeyEvent) {
        let Some(step) = self.create_step() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.leave_create(),
            KeyCode::Up => self.move_create_row(-1),
            KeyCode::Down => self.move_create_row(1),
            KeyCode::Left => self.change_create_option(-1),
            KeyCode::Right => self.change_create_option(1),
            KeyCode::Char(ch) if step == 2 && !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Screen::Create(form) = &mut self.screen {
                    if form.name.chars().count() < 12 {
                        form.name.push(ch);
                    }
                }
            }
            KeyCode::Backspace if step == 2 => {
                if let Screen::Create(form) = &mut self.screen {
                    form.name.pop();
                }
            }
            KeyCode::Enter if step < 4 => self.move_create_row(1),
            KeyCode::Enter => self.finish_create(),
            _ => {}
        }
    }

    fn create_step(&self) -> Option<u8> {
        match &self.screen {
            Screen::Create(form) => Some(form.step),
            _ => None,
        }
    }

    fn move_create_row(&mut self, delta: i32) {
        let Screen::Create(form) = &mut self.screen else {
            return;
        };
        form.step = (i32::from(form.step) + delta).clamp(0, 4) as u8;
        form.error.clear();
    }

    fn change_create_option(&mut self, delta: i32) {
        let Screen::Create(form) = &mut self.screen else {
            return;
        };
        match form.step {
            0 => {
                form.class_index = (form.class_index as i32 + delta).rem_euclid(3) as usize;
            }
            1 => form.gender_index = 1 - form.gender_index,
            3 => form.high_risk = !form.high_risk,
            _ => {}
        }
    }

    fn leave_create(&mut self) {
        if matches!(self.screen, Screen::Create(_)) {
            self.screen = Screen::List;
        }
    }

    fn finish_create(&mut self) {
        let Screen::Create(form) = &self.screen else {
            return;
        };
        let class = [Class::Warrior, Class::Mage, Class::Taoist][form.class_index];
        let gender = if form.gender_index == 0 {
            Gender::Male
        } else {
            Gender::Female
        };
        let name = form.name.clone();
        let risk = form.high_risk;
        if let Err(err) = validate_name(&name) {
            if let Screen::Create(form) = &mut self.screen {
                form.step = 2;
                form.error = err;
            }
            return;
        }
        match self.store.create(&self.data, &name, class, gender, risk) {
            Ok(_) => {
                self.characters = self.store.list().unwrap_or_default();
                self.selected = self.characters.len().saturating_sub(1);
                self.note("角色已创建。职业、性别和高风险不能再改。");
                self.screen = Screen::List;
            }
            Err(err) => {
                if let Screen::Create(form) = &mut self.screen {
                    form.error = err;
                }
            }
        }
    }

    fn key_delete(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.screen = Screen::List,
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Screen::Delete(form) = &mut self.screen {
                    form.confirm.push(ch);
                }
            }
            KeyCode::Backspace => {
                if let Screen::Delete(form) = &mut self.screen {
                    form.confirm.pop();
                }
            }
            KeyCode::Enter => self.finish_delete(),
            _ => {}
        }
    }

    fn finish_delete(&mut self) {
        let Screen::Delete(form) = &self.screen else {
            return;
        };
        let index = form.index;
        let confirm = form.confirm.clone();
        let Some(meta) = self.characters.get(index) else {
            self.screen = Screen::List;
            return;
        };
        if confirm != meta.name {
            self.note("输入的名字不一致，没有删除");
            self.screen = Screen::List;
            return;
        }
        let id = meta.id.clone();
        self.screen = Screen::List;
        if let Err(err) = self.store.delete(&id) {
            self.note(err);
        } else {
            self.characters = self.store.list().unwrap_or_default();
            self.selected = 0;
            self.note("角色已删除");
        }
    }

    fn key_play(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.quit = true,
            KeyCode::Char('1') => self.switch_panel(Panel::Combat),
            KeyCode::Char('2') => self.switch_panel(Panel::Bag),
            KeyCode::Char('3') => self.switch_panel(Panel::Skill),
            KeyCode::Char('4') => self.switch_panel(Panel::Map),
            KeyCode::Char('5') => self.open_shop(),
            KeyCode::Char('6') => self.switch_panel(Panel::Stats),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Left if self.panel == Panel::Shop => self.cycle_shop(-1),
            KeyCode::Right if self.panel == Panel::Shop => self.cycle_shop(1),
            KeyCode::PageUp => self.move_selection(-8),
            KeyCode::PageDown => self.move_selection(8),
            KeyCode::Esc => {
                self.upgrade_armed = false;
                self.panel = Panel::Combat;
            }
            KeyCode::Enter => self.activate(),
            KeyCode::Char('l') => self.lock_selected(),
            KeyCode::Char('s') => self.sell_selected(),
            KeyCode::Char('x') => self.sell_junk(),
            KeyCode::Char('r') => self.town_service(Command::Repair),
            KeyCode::Char('f') => self.town_service(Command::Restock { hp: 20, mp: 20 }),
            KeyCode::Char('a') => self.toggle_advance(),
            KeyCode::Char('u') => self.upgrade(),
            KeyCode::Char('+') => self.buy(10),
            _ => {}
        }
    }

    fn switch_panel(&mut self, panel: Panel) {
        self.panel = panel;
        self.selected = 0;
    }

    fn move_selection(&mut self, delta: i32) {
        let len = self.selection_len();
        if len == 0 {
            self.selected = 0;
            return;
        }
        let next = self.selected as i32 + delta;
        self.selected = next.clamp(0, len as i32 - 1) as usize;
    }

    fn selection_len(&self) -> usize {
        let Some(state) = &self.state else {
            return 0;
        };
        match self.panel {
            Panel::Bag => state.inventory.len(),
            Panel::Skill => self
                .data
                .skills
                .iter()
                .filter(|skill| state.skills.contains_key(&skill.runtime_id))
                .count(),
            Panel::Map => self.data.maps.len(),
            Panel::Shop => self.current_shop_items().len(),
            Panel::Combat | Panel::Stats => 0,
        }
    }

    fn open_shop(&mut self) {
        if self.in_town() {
            self.switch_panel(Panel::Shop);
            self.note(String::new());
        } else if self.panel == Panel::Shop {
            self.panel = Panel::Combat;
            self.note("商店只在比奇城。先回城。");
        } else {
            self.note("商店只在比奇城。先回城。");
        }
    }

    fn town_service(&mut self, command: Command) {
        if self.in_town() {
            self.command(command);
        } else {
            self.note("修理和补药要在比奇城。");
        }
    }

    fn in_town(&self) -> bool {
        self.state.as_ref().is_some_and(|state| {
            let map = self.data.map(state.current_map);
            map.safe_zone || map.id == "bichen_city"
        })
    }

    fn activate(&mut self) {
        match self.panel {
            Panel::Bag => self.command(Command::Use {
                index: self.selected,
            }),
            Panel::Skill => self.command(Command::ToggleSkill {
                index: self.selected_skill(),
            }),
            Panel::Map => {
                if let Some(map) = self.selected_map() {
                    self.command(Command::ChangeMap { map });
                    if !self.in_town() && self.panel == Panel::Shop {
                        self.panel = Panel::Combat;
                    }
                }
            }
            Panel::Shop => self.buy(1),
            _ => {}
        }
    }

    fn buy(&mut self, count: u32) {
        if !self.in_town() {
            self.panel = Panel::Combat;
            self.note("商店只在比奇城。先回城。");
            return;
        }
        let Some((npc, stock_index)) = self.selected_shop() else {
            return;
        };
        let notice = self.purchase_notice(npc, stock_index, count);
        self.command(Command::Buy {
            npc,
            stock_index,
            count,
        });
        if let Some(text) = notice {
            if self.message.is_empty() {
                self.note(text);
            }
        }
    }

    /// 购买成功才提示。失败时 `command` 已经写了原因，这里不再覆盖。
    fn purchase_notice(&self, npc: usize, stock_index: usize, count: u32) -> Option<String> {
        let state = self.state.as_ref()?;
        let shop = self
            .data
            .npcs
            .get(npc)?
            .services
            .iter()
            .find_map(|service| {
                if let game_data::NpcService::Shop(shop) = service {
                    Some(shop)
                } else {
                    None
                }
            })?;
        let item = *shop.stock_ids.get(stock_index)?;
        let def = self.data.item(item);
        let unit = (f64::from(def.price) * f64::from(shop.buy_markup)).ceil() as u64;
        let cost = unit.saturating_mul(u64::from(count));
        if count == 0 || state.gold < cost {
            return None;
        }
        let fits = if def.stackable {
            state.inventory.iter().any(|stack| stack.item == item)
                || state.inventory.len() < self.data.balance.inventory_slots as usize
        } else {
            state.inventory.len() + count as usize <= self.data.balance.inventory_slots as usize
        };
        if !fits {
            return None;
        }
        Some(format!("买下 {} ×{count}，花费 {cost} 金币", def.name))
    }

    fn toggle_advance(&mut self) {
        let enabled = self
            .state
            .as_ref()
            .is_some_and(|state| !state.automation.auto_advance);
        self.command(Command::SetAutoAdvance { enabled });
    }

    fn lock_selected(&mut self) {
        if self.panel == Panel::Bag {
            self.command(Command::LockInventory {
                index: self.selected,
            });
        }
    }

    /// 卖掉背包里选中的那一格。先填数量，回车才成交。
    fn sell_selected(&mut self) {
        if self.panel != Panel::Bag {
            self.note("先打开背包，再按 S 卖掉选中的东西。");
            return;
        }
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let Some(stack) = state.inventory.get(self.selected) else {
            self.note("背包是空的。");
            return;
        };
        if stack.locked {
            self.note(CoreError::ItemLocked.to_string());
            return;
        }
        if stack.upgrade > 0 {
            self.note(CoreError::Upgraded.to_string());
            return;
        }
        self.screen = Screen::Sell(SellForm {
            index: self.selected,
            digits: stack.count.to_string(),
            error: String::new(),
        });
    }

    fn key_sell(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.screen = Screen::Play,
            KeyCode::Char(ch) if ch.is_ascii_digit() => {
                if let Screen::Sell(form) = &mut self.screen {
                    if form.digits.len() < 6 {
                        form.digits.push(ch);
                        form.error.clear();
                    }
                }
            }
            KeyCode::Backspace => {
                if let Screen::Sell(form) = &mut self.screen {
                    form.digits.pop();
                    form.error.clear();
                }
            }
            KeyCode::Enter => self.finish_sell(),
            _ => {}
        }
    }

    fn finish_sell(&mut self) {
        let Screen::Sell(form) = &self.screen else {
            return;
        };
        let index = form.index;
        let digits = form.digits.clone();
        let count: u32 = match digits.parse() {
            Ok(count) if count > 0 => count,
            _ => {
                if let Screen::Sell(form) = &mut self.screen {
                    form.error = "请输入大于 0 的数量".to_string();
                }
                return;
            }
        };
        let owned = self
            .state
            .as_ref()
            .and_then(|state| state.inventory.get(index))
            .map(|stack| stack.count)
            .unwrap_or(0);
        if count > owned {
            if let Screen::Sell(form) = &mut self.screen {
                form.error = format!("这一格只有 {owned} 个");
            }
            return;
        }
        let notice = self.sale_notice(index, count);
        self.screen = Screen::Play;
        self.command(Command::Sell { index, count });
        if let Some(text) = notice {
            if self.message.is_empty() {
                self.note(text);
            }
        }
        self.clamp_selection();
    }

    fn sale_notice(&self, index: usize, count: u32) -> Option<String> {
        let state = self.state.as_ref()?;
        let stack = state.inventory.get(index)?;
        if stack.locked || stack.upgrade > 0 || count == 0 || count > stack.count {
            return None;
        }
        let def = self.data.item(stack.item);
        let gold = game_core::sale_gold(&self.data, stack.item, count);
        Some(format!("卖出 {} ×{count}，获得 {gold} 金币", def.name))
    }

    /// 清空白装。没有可卖的也要说一声，避免按了没反应。
    fn sell_junk(&mut self) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let mut pieces = 0u32;
        let mut gold = 0u64;
        for stack in &state.inventory {
            let def = self.data.item(stack.item);
            let junk = def.quality == game_data::Quality::White
                && def.kind.gear().is_some()
                && !stack.locked
                && stack.upgrade == 0;
            if junk {
                pieces = pieces.saturating_add(stack.count);
                gold =
                    gold.saturating_add(game_core::sale_gold(&self.data, stack.item, stack.count));
            }
        }
        self.command(Command::SellJunk);
        if self.message.is_empty() {
            if pieces == 0 {
                self.note("没有可卖的白装。锁定的不会卖掉。");
            } else {
                self.note(format!("卖掉白装 {pieces} 件，获得 {gold} 金币"));
            }
        }
        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        let len = self.selection_len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    fn upgrade(&mut self) {
        if !self.upgrade_armed {
            let Some(state) = self.state.as_ref() else {
                return;
            };
            match game_core::upgrade_preview(state, &self.data) {
                Ok((rate, gold, plus)) => {
                    self.upgrade_armed = true;
                    self.note(format!(
                        "升级到 +{plus}：成功率 {:.0}%，花费 {gold} 金币。再按 U 确认，Esc 取消。",
                        rate * 100.0
                    ));
                }
                Err(err) => self.note(err.to_string()),
            }
            return;
        }
        self.upgrade_armed = false;
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let Some(index) = state
            .inventory
            .iter()
            .position(|stack| self.data.item(stack.item).kind.gear().is_some())
        else {
            self.note(CoreError::MissingMaterials.to_string());
            return;
        };
        self.command(Command::UpgradeWeapon {
            accessory_index: index,
        });
    }

    fn command(&mut self, command: Command) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        match command.apply(state, &self.data) {
            Ok(events) => {
                for event in events {
                    self.push_log(format_event(&event, &self.data));
                }
                self.note(String::new());
            }
            Err(err) => self.note(err.to_string()),
        }
    }

    fn selected_skill(&self) -> usize {
        let Some(state) = &self.state else {
            return 0;
        };
        let known: Vec<_> = self
            .data
            .skills
            .iter()
            .enumerate()
            .filter(|(_, skill)| state.skills.contains_key(&skill.runtime_id))
            .map(|(i, _)| i)
            .collect();
        known.get(self.selected).copied().unwrap_or(0)
    }

    fn selected_map(&self) -> Option<game_data::MapId> {
        self.data.maps.get(self.selected).map(|map| map.runtime_id)
    }

    fn shop_npcs(&self) -> Vec<usize> {
        self.data
            .npcs
            .iter()
            .enumerate()
            .filter(|(_, npc)| {
                npc.services
                    .iter()
                    .any(|service| matches!(service, game_data::NpcService::Shop(_)))
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn cycle_shop(&mut self, delta: i32) {
        let shops = self.shop_npcs();
        if shops.is_empty() {
            return;
        }
        let len = shops.len() as i32;
        self.shop_index = (self.shop_index as i32 + delta).rem_euclid(len) as usize;
        self.selected = 0;
    }

    fn current_shop_items(&self) -> Vec<(usize, usize)> {
        let shops = self.shop_npcs();
        let Some(npc_index) = shops.get(self.shop_index.min(shops.len().saturating_sub(1))) else {
            return Vec::new();
        };
        let mut items = Vec::new();
        for service in &self.data.npcs[*npc_index].services {
            if let game_data::NpcService::Shop(shop) = service {
                for stock_index in 0..shop.stock_ids.len() {
                    items.push((*npc_index, stock_index));
                }
            }
        }
        items
    }

    fn selected_shop(&self) -> Option<(usize, usize)> {
        self.current_shop_items().get(self.selected).copied()
    }

    fn push_log(&mut self, line: LogLine) {
        self.log.push_back(line);
        while self.log.len() > LOG_CAP {
            self.log.pop_front();
        }
    }

    fn save_if_playing(&mut self) {
        let (Some(id), Some(state)) = (self.slot.clone(), self.state.as_ref()) else {
            return;
        };
        if let Err(err) = self.store.save(&id, state, &self.data) {
            self.note(err);
        } else {
            self.last_save = Instant::now();
            self.characters = self.store.list().unwrap_or_default();
        }
    }
}

fn format_report(report: &game_core::OfflineReport, data: &GameData) -> String {
    let mut lines = vec![
        format!("离线 {} 分钟", report.ticks / 600),
        format!(
            "经验 {}，金币 {}，升级 {}，死亡 {}",
            report.exp, report.gold, report.levels_gained, report.deaths
        ),
        format!("击杀 {}", report.kills),
    ];
    for (item, count) in report.drops.iter().take(8) {
        lines.push(format!("掉落 {} ×{count}", data.item(*item).name));
    }
    lines.join("\n")
}

/// 给主循环的轮询上限，避免空转。
pub const TICK_WAIT_MS: u64 = 50;
const TICK_WAIT: u64 = TICK_WAIT_MS;
