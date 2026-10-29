//! 主循环。逻辑 tick 和渲染分开，输入只变成 `Command`。

use crate::logfmt::{format_event, LogLine};
use crate::saveio::{CharacterMeta, SaveStore};
use crate::ui;
use crate::update::{self, Release};
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use game_core::{validate_name, Command, CoreError, GameEvent, GameState};
use game_data::{Class, EquipSlot, GameData, Gender, ItemKind};
use ratatui::layout::Rect;
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(100);
const MAX_CATCHUP: u32 = 5;
const LOG_CAP: usize = 500;
/// 每隔这么久记一次累计经验，用来估算升级还要多久。
const EXP_SAMPLE: Duration = Duration::from_secs(5);
/// 最多留这么多个样本，约 5 分钟。
const EXP_SAMPLES: usize = 61;
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// 右侧面板。左侧角色栏一直在。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

impl Panel {
    /// 底部标签顺序，和数字键 1–6 一一对应。
    pub const ALL: [Panel; 6] = [
        Panel::Combat,
        Panel::Bag,
        Panel::Skill,
        Panel::Map,
        Panel::Shop,
        Panel::Stats,
    ];

    /// 标签文字。
    pub fn label(self) -> &'static str {
        match self {
            Panel::Combat => "战斗",
            Panel::Bag => "背包",
            Panel::Skill => "技能",
            Panel::Map => "地图",
            Panel::Shop => "商店",
            Panel::Stats => "统计",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }
}

/// 上一帧画出来的可点区域。鼠标事件拿它做命中测试，每帧重画时重写。
#[derive(Debug, Default, Clone)]
pub struct HitMap {
    /// 底部面板标签。
    pub tabs: Vec<(Rect, Panel)>,
    /// 商店里的店铺标签，值是店铺下标。
    pub shops: Vec<(Rect, usize)>,
    /// 列表：第一行所在的区域、它对应的下标。区域高度就是显示了几行。
    pub list: Option<(Rect, usize)>,
    /// 日志框。
    pub log: Option<Rect>,
}

/// 这一点落在哪个矩形里。
fn hit(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x && column < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

/// 把输入法里的全角字符和中文标点换成键位上的 ASCII。
/// 中文输入法开着时按 `[` 会得到「【」，按 `-` 会得到「—」，不换就像按了没反应。
fn ascii_key(ch: char) -> char {
    match ch {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(ch as u32 - 0xFEE0).unwrap_or(ch),
        '【' | '「' => '[',
        '】' | '」' => ']',
        '—' => '-',
        '。' => '.',
        '、' => '\\',
        '·' => '`',
        _ => ch,
    }
}

/// 普通按键统一成小写 ASCII。大写锁定或按住 Shift 时快捷键也照样生效。
fn plain_key(code: KeyCode) -> KeyCode {
    match code {
        KeyCode::Char(ch) => KeyCode::Char(ascii_key(ch).to_ascii_lowercase()),
        other => other,
    }
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
    /// 购买详情。选中商品后填数量，回车才买下。
    Buy(BuyForm),
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
    /// 还是预填的数。第一次敲数字时整个替换，不接在后面。
    pub fresh: bool,
    /// 校验错误。
    pub error: String,
}

/// 购买详情。数量是玩家正在输入的数字。
pub struct BuyForm {
    /// NPC 下标。确认期间不能换店铺。
    pub npc: usize,
    /// 货架下标。
    pub stock_index: usize,
    /// 已输入的数量，空着表示还没填。
    pub digits: String,
    /// 还是预填的数。第一次敲数字时整个替换。
    pub fresh: bool,
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
    /// 背包面板在看身上的装备，而不是背包里的东西。
    pub bag_worn: bool,
    /// 日志往上翻了多少行。0 表示跟着最新。
    pub log_scroll: usize,
    /// 上一帧的可点区域。画的时候写，点的时候读。
    pub hits: RefCell<HitMap>,
    /// 后台查到的新版本。没有就是还没查完或已经是最新。
    pub update: Option<Release>,
    update_rx: Option<Receiver<Result<Option<Release>, String>>>,
    quit: bool,
    last_save: Instant,
    /// 各面板上次选到哪一行，来回切面板不丢位置。
    remembered: BTreeMap<(Panel, bool), usize>,
    /// 上一次左键点在哪一行、什么时候，用来认双击。
    last_click: Option<(Instant, usize)>,
    /// （时刻，累计经验）样本，给「预计升级」。
    exp_samples: VecDeque<(Instant, u64)>,
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
            bag_worn: false,
            log_scroll: 0,
            hits: RefCell::new(HitMap::default()),
            update: None,
            update_rx: None,
            quit: false,
            last_save: Instant::now(),
            remembered: BTreeMap::new(),
            last_click: None,
            exp_samples: VecDeque::new(),
        })
    }

    /// 后台问 GitHub Releases 有没有更新的版本。失败不打断游戏。
    pub fn start_update_check(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.update_rx = Some(rx);
        std::thread::spawn(move || {
            let found = update::check_latest().map_err(|err| err.to_string());
            let _ = tx.send(found);
        });
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
            self.poll_update();
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
            match event::read().map_err(|err| err.to_string())? {
                Event::Key(key) if key.kind != KeyEventKind::Release => self.on_key(key),
                Event::Mouse(mouse) => self.on_mouse(mouse),
                _ => {}
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
        let exp_total = state.stats.exp_gained;
        let mut notable = false;
        let mut left_town = false;
        // 击杀后紧跟着的金币并进击杀那一行，日志少刷一半。
        let mut kill_line = false;
        for event in events {
            if matches!(
                event,
                GameEvent::LevelUp { .. }
                    | GameEvent::ItemDropped { rare: true, .. }
                    | GameEvent::PlayerDied { .. }
                    | GameEvent::MapUnlocked { .. }
                    | GameEvent::SkillLevelUp { .. }
            ) {
                notable = true;
            }
            if matches!(
                event,
                GameEvent::MapChanged { .. }
                    | GameEvent::Resting { .. }
                    | GameEvent::ReturnedToHunt { .. }
            ) {
                left_town = true;
            }
            if let GameEvent::GoldGained { amount } = event {
                if kill_line {
                    if let Some(last) = self.log.back_mut() {
                        last.text.push_str(&format!("、{amount} 金币"));
                    }
                    kill_line = false;
                    continue;
                }
            }
            kill_line = matches!(event, GameEvent::MonsterSlain { .. });
            self.push_log(format_event(&event, &self.data));
        }
        self.sample_exp(exp_total);
        if left_town && self.panel == Panel::Shop && !self.in_town() {
            self.switch_panel(Panel::Combat);
            self.note("离开比奇城，商店已关闭。");
        }
        if notable || self.last_save.elapsed() >= Duration::from_secs(30) {
            self.save_if_playing();
        }
    }

    fn sample_exp(&mut self, total: u64) {
        let due = self
            .exp_samples
            .back()
            .is_none_or(|(at, _)| at.elapsed() >= EXP_SAMPLE);
        if !due {
            return;
        }
        self.exp_samples.push_back((Instant::now(), total));
        while self.exp_samples.len() > EXP_SAMPLES {
            self.exp_samples.pop_front();
        }
    }

    /// 最近几分钟每秒拿多少经验。样本不够 20 秒就不估。
    pub fn exp_per_second(&self) -> Option<f64> {
        let (first_at, first) = *self.exp_samples.front()?;
        let (last_at, last) = *self.exp_samples.back()?;
        let secs = last_at.saturating_duration_since(first_at).as_secs_f64();
        if secs < 20.0 || last <= first {
            return None;
        }
        Some((last - first) as f64 / secs)
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
        if matches!(key.code, KeyCode::Char('u' | 'U')) && self.apply_update() {
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
            Screen::Buy(_) => self.key_buy(key),
            Screen::Report(_) => {
                if !matches!(key.code, KeyCode::F(_) | KeyCode::Modifier(_)) {
                    self.screen = Screen::Play;
                }
            }
            Screen::Play => self.key_play(key.code),
            Screen::Help(_) => {
                if matches!(
                    plain_key(key.code),
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')
                ) {
                    self.toggle_help();
                }
            }
        }
    }

    /// 鼠标：滚轮移选中或翻日志，点标签切面板，点一行选中，再点一下（双击）等于 Enter。
    fn on_mouse(&mut self, mouse: MouseEvent) {
        if self.disguised {
            return;
        }
        let hits = self.hits.borrow().clone();
        let (col, row) = (mouse.column, mouse.row);
        let over_log = hits.log.is_some_and(|rect| hit(rect, col, row));
        match mouse.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let up = mouse.kind == MouseEventKind::ScrollUp;
                match &self.screen {
                    Screen::Play if over_log || self.selection_len() == 0 => {
                        self.scroll_log(if up { 3 } else { -3 });
                    }
                    Screen::Play => self.step_selection(if up { -1 } else { 1 }),
                    Screen::List => {
                        let len = self.characters.len();
                        self.selected = wrap_step(self.selected, if up { -1 } else { 1 }, len);
                    }
                    _ => {}
                }
            }
            MouseEventKind::Down(MouseButton::Left) => match &self.screen {
                Screen::Report(_) => self.screen = Screen::Play,
                Screen::Help(_) => self.toggle_help(),
                Screen::Play | Screen::List => self.click(&hits, col, row),
                _ => {}
            },
            _ => {}
        }
    }

    fn click(&mut self, hits: &HitMap, col: u16, row: u16) {
        if let Some((_, panel)) = hits.tabs.iter().find(|(rect, _)| hit(*rect, col, row)) {
            if *panel == Panel::Shop {
                self.open_shop();
            } else {
                self.switch_panel(*panel);
            }
            return;
        }
        if let Some((_, shop)) = hits.shops.iter().find(|(rect, _)| hit(*rect, col, row)) {
            self.select_shop(*shop);
            return;
        }
        let Some((rect, first)) = hits.list else {
            return;
        };
        if !hit(rect, col, row) {
            return;
        }
        let index = first + usize::from(row - rect.y);
        let on_list = matches!(self.screen, Screen::List);
        let len = if on_list {
            self.characters.len()
        } else {
            self.selection_len()
        };
        if index >= len {
            return;
        }
        let double = self
            .last_click
            .is_some_and(|(at, last)| last == index && at.elapsed() <= DOUBLE_CLICK);
        self.selected = index;
        self.last_click = if double {
            None
        } else {
            Some((Instant::now(), index))
        };
        if double {
            if on_list {
                self.enter_selected();
            } else {
                self.activate();
            }
        }
    }

    fn help_key(&self, key: KeyEvent) -> bool {
        if self.typing_name() && !matches!(self.screen, Screen::Help(_)) {
            return false;
        }
        match key.code {
            KeyCode::F(1) => true,
            KeyCode::Char('/') if key.modifiers.contains(KeyModifiers::SHIFT) => true,
            KeyCode::Char(ch) => ascii_key(ch) == '?',
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

    fn poll_update(&mut self) {
        let Some(rx) = &self.update_rx else {
            return;
        };
        let message = match rx.try_recv() {
            Ok(Ok(Some(release))) => {
                let text = format!(
                    "有新版本 {}（现在是 {}）。按 U 下载并重启。",
                    release.version,
                    update::current_version()
                );
                self.update = Some(release);
                text
            }
            Ok(Ok(None)) => String::new(),
            Ok(Err(err)) => {
                tracing::warn!(error = %err, "查更新失败");
                String::new()
            }
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => String::new(),
        };
        self.update_rx = None;
        if !message.is_empty() {
            self.note(message);
        }
    }

    /// 有待安装的版本就下载、换掉当前程序并退出。没有就返回 false，让原来的按键继续生效。
    fn apply_update(&mut self) -> bool {
        let Some(release) = self.update.clone() else {
            return false;
        };
        if self.typing_name() {
            return false;
        }
        self.save_if_playing();
        match update::install(&release) {
            Ok(()) => {
                self.quit = true;
                true
            }
            Err(err) => {
                self.note(err.to_string());
                true
            }
        }
    }

    fn typing_name(&self) -> bool {
        match &self.screen {
            Screen::Create(form) => form.step == 2,
            Screen::Delete(_) | Screen::Sell(_) | Screen::Buy(_) => true,
            _ => false,
        }
    }

    fn key_list(&mut self, code: KeyCode) {
        let len = self.characters.len();
        match plain_key(code) {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Up | KeyCode::Char('k') => self.selected = wrap_step(self.selected, -1, len),
            KeyCode::Down | KeyCode::Char('j') => self.selected = wrap_step(self.selected, 1, len),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = len.saturating_sub(1),
            KeyCode::Char('n') => {
                self.screen = Screen::Create(CreateForm {
                    step: 0,
                    class_index: 0,
                    gender_index: 0,
                    name: String::new(),
                    high_risk: false,
                    error: String::new(),
                });
            }
            KeyCode::Char('d') if !self.characters.is_empty() => {
                self.screen = Screen::Delete(DeleteForm {
                    index: self.selected.min(self.characters.len() - 1),
                    confirm: String::new(),
                });
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.enter_selected(),
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
                self.log_scroll = 0;
                self.exp_samples.clear();
                self.remembered.clear();
                self.bag_worn = false;
                self.panel = Panel::Combat;
                self.selected = 0;
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
            KeyCode::Up | KeyCode::BackTab => self.move_create_row(-1),
            KeyCode::Down | KeyCode::Tab => self.move_create_row(1),
            KeyCode::Left => self.change_create_option(-1),
            KeyCode::Right => self.change_create_option(1),
            KeyCode::Char(' ') if step != 2 => self.change_create_option(1),
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
        let code = plain_key(code);
        // 升级确认只认紧接着的那一下 U，中间按了别的就作废，免得过一会儿误触。
        if self.upgrade_armed && code != KeyCode::Char('u') {
            self.upgrade_armed = false;
            self.note("已取消武器升级。");
            if code == KeyCode::Esc {
                return;
            }
        }
        let log_view = matches!(self.panel, Panel::Combat | Panel::Stats);
        match code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('1') => self.switch_panel(Panel::Combat),
            KeyCode::Char('2') => self.switch_panel(Panel::Bag),
            KeyCode::Char('3') => self.switch_panel(Panel::Skill),
            KeyCode::Char('4') => self.switch_panel(Panel::Map),
            KeyCode::Char('5') => self.open_shop(),
            KeyCode::Char('6') => self.switch_panel(Panel::Stats),
            KeyCode::Tab => self.cycle_panel(1),
            KeyCode::BackTab => self.cycle_panel(-1),
            KeyCode::Char('e') => self.toggle_worn(),
            KeyCode::Up | KeyCode::Char('k') if log_view => self.scroll_log(1),
            KeyCode::Down | KeyCode::Char('j') if log_view => self.scroll_log(-1),
            KeyCode::PageUp if log_view => self.scroll_log(8),
            KeyCode::PageDown if log_view => self.scroll_log(-8),
            KeyCode::Home if log_view => self.scroll_log(isize::MAX),
            KeyCode::End if log_view => self.log_scroll = 0,
            KeyCode::Up | KeyCode::Char('k') => self.step_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.step_selection(1),
            KeyCode::Left if self.panel == Panel::Shop => self.cycle_shop(-1),
            KeyCode::Right if self.panel == Panel::Shop => self.cycle_shop(1),
            KeyCode::Left | KeyCode::Right if self.panel == Panel::Bag => self.toggle_worn(),
            KeyCode::PageUp => self.move_selection(-8),
            KeyCode::PageDown => self.move_selection(8),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.selection_len().saturating_sub(1),
            KeyCode::Esc => {
                if self.panel == Panel::Bag && self.bag_worn {
                    self.toggle_worn();
                } else {
                    self.switch_panel(Panel::Combat);
                    self.log_scroll = 0;
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.activate(),
            KeyCode::Char('l') => self.lock_selected(),
            KeyCode::Char('s') => self.sell_selected(),
            KeyCode::Char('x') => self.sell_junk(),
            KeyCode::Char('r') => self.town_service(Command::Repair),
            KeyCode::Char('f') => self.restock(),
            KeyCode::Char('a') => self.toggle_advance(),
            KeyCode::Char('p') => self.toggle_potion(),
            KeyCode::Char('[') => self.nudge_potion(true, -1),
            KeyCode::Char(']') => self.nudge_potion(true, 1),
            KeyCode::Char('-') | KeyCode::Char('_') => self.nudge_potion(false, -1),
            KeyCode::Char('=') | KeyCode::Char('+') => self.nudge_potion(false, 1),
            KeyCode::Char('u') => self.upgrade(),
            _ => {}
        }
    }

    fn cycle_panel(&mut self, delta: i32) {
        let len = Panel::ALL.len() as i32;
        let mut index = self.panel.index() as i32;
        // 不在城里时跳过商店，按 Tab 不会停在一个打不开的面板上。
        for _ in 0..len {
            index = (index + delta).rem_euclid(len);
            let next = Panel::ALL[index as usize];
            if next != Panel::Shop || self.in_town() {
                self.switch_panel(next);
                return;
            }
        }
    }

    /// 背包面板在「背包」和「身上」之间切换。在别的面板按 E 直接看身上。
    fn toggle_worn(&mut self) {
        if self.panel != Panel::Bag {
            self.switch_panel(Panel::Bag);
            if !self.bag_worn {
                self.set_worn(true);
            }
            return;
        }
        let worn = !self.bag_worn;
        self.set_worn(worn);
    }

    fn set_worn(&mut self, worn: bool) {
        self.remembered
            .insert((Panel::Bag, self.bag_worn), self.selected);
        self.bag_worn = worn;
        self.selected = self
            .remembered
            .get(&(Panel::Bag, worn))
            .copied()
            .unwrap_or(0);
        self.clamp_selection();
    }

    fn scroll_log(&mut self, delta: isize) {
        let max = self.log.len();
        self.log_scroll = if delta >= 0 {
            self.log_scroll.saturating_add(delta as usize).min(max)
        } else {
            self.log_scroll.saturating_sub(delta.unsigned_abs())
        };
    }

    fn switch_panel(&mut self, panel: Panel) {
        self.remembered.insert(
            (self.panel, self.panel == Panel::Bag && self.bag_worn),
            self.selected,
        );
        self.panel = panel;
        let key = (panel, panel == Panel::Bag && self.bag_worn);
        self.selected = match self.remembered.get(&key) {
            Some(index) => *index,
            None if panel == Panel::Map => self.current_map_row(),
            None => 0,
        };
        self.clamp_selection();
    }

    fn current_map_row(&self) -> usize {
        self.state
            .as_ref()
            .and_then(|state| {
                self.data
                    .maps
                    .iter()
                    .position(|map| map.runtime_id == state.current_map)
            })
            .unwrap_or(0)
    }

    /// ↑↓ 到头会绕回去；PgUp/PgDn 到头就停住。
    fn step_selection(&mut self, delta: i32) {
        self.selected = wrap_step(self.selected, delta, self.selection_len());
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

    /// 当前面板有多少行可选。
    pub fn selection_len(&self) -> usize {
        let Some(state) = &self.state else {
            return 0;
        };
        match self.panel {
            Panel::Bag if self.bag_worn => self.worn_slots().len(),
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

    /// 身上穿着东西的槽位，按装备栏顺序。
    pub fn worn_slots(&self) -> Vec<EquipSlot> {
        let Some(state) = &self.state else {
            return Vec::new();
        };
        EquipSlot::ALL
            .into_iter()
            .filter(|slot| state.equipment[slot.index()].is_some())
            .collect()
    }

    fn selected_worn(&self) -> Option<EquipSlot> {
        if self.panel != Panel::Bag || !self.bag_worn {
            return None;
        }
        self.worn_slots().get(self.selected).copied()
    }

    fn selected_bag(&self) -> Option<usize> {
        if self.panel != Panel::Bag || self.bag_worn {
            return None;
        }
        let len = self.state.as_ref().map_or(0, |state| state.inventory.len());
        (self.selected < len).then_some(self.selected)
    }

    fn open_shop(&mut self) {
        if self.in_town() {
            self.switch_panel(Panel::Shop);
            self.note(String::new());
        } else {
            if self.panel == Panel::Shop {
                self.switch_panel(Panel::Combat);
            }
            self.note("商店只在比奇城。先到地图面板（4）选比奇城。");
        }
    }

    fn town_service(&mut self, command: Command) {
        if self.in_town() {
            self.command(command);
        } else {
            self.note("修理和补药要在比奇城。先到地图面板（4）选比奇城。");
        }
    }

    fn restock(&mut self) {
        let Some(class) = self.state.as_ref().map(|state| state.class) else {
            return;
        };
        let (hp, mp) = game_core::restock_targets(class);
        self.town_service(Command::Restock { hp, mp });
    }

    /// 人在安全区。商店、修理、补药都要求这个。
    pub fn in_town(&self) -> bool {
        self.state.as_ref().is_some_and(|state| {
            let map = self.data.map(state.current_map);
            map.safe_zone || map.id == "bichen_city"
        })
    }

    fn activate(&mut self) {
        match self.panel {
            Panel::Bag => {
                if let Some(slot) = self.selected_worn() {
                    self.command(Command::Unequip { slot });
                } else if let Some(index) = self.selected_bag() {
                    self.command(Command::Use { index });
                }
            }
            Panel::Skill => {
                let index = self.selected_skill();
                self.command(Command::ToggleSkill { index });
                if self.message.is_empty() {
                    if let Some((name, auto)) = self.skill_state(index) {
                        self.note(if auto {
                            format!("{name} 改为自动施放")
                        } else {
                            format!("{name} 不再自动施放")
                        });
                    }
                }
            }
            Panel::Map => {
                if let Some(map) = self.selected_map() {
                    let here = self.state.as_ref().map(|state| state.current_map);
                    if here == Some(map) {
                        self.note("已经在这张图了。");
                        return;
                    }
                    self.command(Command::ChangeMap { map });
                }
            }
            Panel::Shop => self.open_buy(),
            Panel::Combat | Panel::Stats => {}
        }
    }

    fn skill_state(&self, index: usize) -> Option<(String, bool)> {
        let skill = self.data.skills.get(index)?;
        let progress = self.state.as_ref()?.skills.get(&skill.runtime_id)?;
        Some((skill.name.clone(), progress.auto))
    }

    /// 打开购买详情。数量在这里填，回车才买下。
    fn open_buy(&mut self) {
        if !self.in_town() {
            self.switch_panel(Panel::Combat);
            self.note("商店只在比奇城。先到地图面板（4）选比奇城。");
            return;
        }
        let Some((npc, stock_index)) = self.selected_shop() else {
            self.note("这家没有在卖的东西。");
            return;
        };
        self.screen = Screen::Buy(BuyForm {
            npc,
            stock_index,
            digits: "1".to_string(),
            fresh: true,
            error: String::new(),
        });
    }

    /// 这件最多能买几个：同时受钱和背包格子限制。
    pub fn buy_limit(&self, npc: usize, stock_index: usize) -> u32 {
        let Some(state) = &self.state else {
            return 0;
        };
        let Some(shop) = self.data.npcs.get(npc).and_then(|npc| {
            npc.services.iter().find_map(|service| match service {
                game_data::NpcService::Shop(shop) => Some(shop),
                _ => None,
            })
        }) else {
            return 0;
        };
        let Some(item) = shop.stock_ids.get(stock_index).copied() else {
            return 0;
        };
        let def = self.data.item(item);
        let unit = (f64::from(def.price) * f64::from(shop.buy_markup))
            .ceil()
            .max(1.0) as u64;
        let by_gold = (state.gold / unit).min(999_999) as u32;
        let free =
            (self.data.balance.inventory_slots as usize).saturating_sub(state.inventory.len());
        let by_bag = if def.stackable {
            if free > 0 || state.inventory.iter().any(|stack| stack.item == item) {
                u32::MAX
            } else {
                0
            }
        } else {
            free as u32
        };
        by_gold.min(by_bag)
    }

    fn key_buy(&mut self, key: KeyEvent) {
        let limit = match &self.screen {
            Screen::Buy(form) => self.buy_limit(form.npc, form.stock_index),
            _ => return,
        };
        match key.code {
            KeyCode::Esc => self.screen = Screen::Play,
            KeyCode::Enter => self.finish_buy(),
            _ => {
                if let Screen::Buy(form) = &mut self.screen {
                    if edit_quantity(&mut form.digits, &mut form.fresh, key, limit) {
                        form.error.clear();
                    }
                }
            }
        }
    }

    fn finish_buy(&mut self) {
        let Screen::Buy(form) = &self.screen else {
            return;
        };
        let npc = form.npc;
        let stock_index = form.stock_index;
        let digits = form.digits.clone();
        let count: u32 = match digits.parse() {
            Ok(count) if count > 0 => count,
            _ => {
                if let Screen::Buy(form) = &mut self.screen {
                    form.error = "请输入大于 0 的数量".to_string();
                }
                return;
            }
        };
        if !self.in_town() {
            self.screen = Screen::Play;
            self.switch_panel(Panel::Combat);
            self.note("商店只在比奇城。先到地图面板（4）选比奇城。");
            return;
        }
        let notice = self.purchase_notice(npc, stock_index, count);
        self.screen = Screen::Play;
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
        if self.message.is_empty() {
            self.note(if enabled {
                "自动推进已开：沿主线往更难的图走，BOSS 支线要手动进。"
            } else {
                "自动推进已关，留在当前地图。"
            });
        }
    }

    fn toggle_potion(&mut self) {
        let Some((auto, hp, mp)) = self.state.as_ref().map(|state| {
            (
                !state.automation.auto_potion,
                state.automation.hp_ratio,
                state.automation.mp_ratio,
            )
        }) else {
            return;
        };
        self.command(Command::SetThresholds { auto, hp, mp });
        if self.message.is_empty() {
            self.note(if auto {
                "自动吃药已开".to_string()
            } else {
                "自动吃药已关".to_string()
            });
        }
    }

    /// 调低或调高吃药线。血量低于这条线才喝。
    fn nudge_potion(&mut self, hp_bar: bool, delta: i32) {
        let Some((level, hp_ratio, mp_ratio)) = self.state.as_ref().map(|state| {
            (
                state.level,
                state.automation.hp_ratio,
                state.automation.mp_ratio,
            )
        }) else {
            return;
        };
        if level < self.data.balance.auto_coarse_level {
            self.note(format!(
                "{} 级才能调吃药线，现在用默认的 {:.0}%/{:.0}%",
                self.data.balance.auto_coarse_level,
                self.data.balance.potion_hp_ratio * 100.0,
                self.data.balance.potion_mp_ratio * 100.0
            ));
            return;
        }
        let fine = level >= self.data.balance.auto_fine_level;
        let mut hp = hp_ratio;
        let mut mp = mp_ratio;
        if fine {
            if hp_bar {
                hp = step_percent(hp, delta);
            } else {
                mp = step_percent(mp, delta);
            }
        } else {
            hp = snap_step(hp, &[0.25, 0.5, 0.75], if hp_bar { delta } else { 0 });
            mp = snap_step(mp, &[0.25, 0.5, 0.75], if hp_bar { 0 } else { delta });
        }
        self.command(Command::SetThresholds { auto: true, hp, mp });
        if self.message.is_empty() {
            let Some(state) = self.state.as_ref() else {
                return;
            };
            let derived = state.derived_stats(&self.data);
            let (label, ratio, max) = if hp_bar {
                ("血", hp, derived.max_hp)
            } else {
                ("蓝", mp, derived.max_mp)
            };
            let left = (ratio * f64::from(max)).round() as i32;
            self.note(format!(
                "{label}低于 {left}（{:.0}%）时吃药。另一边是 {:.0}%",
                ratio * 100.0,
                if hp_bar { mp } else { hp } * 100.0
            ));
        }
    }

    fn lock_selected(&mut self) {
        if self.panel != Panel::Bag {
            self.note("先打开背包（2）再按 L。按 E 可以锁身上的装备。");
            return;
        }
        let (command, item) = if let Some(slot) = self.selected_worn() {
            let item = self
                .state
                .as_ref()
                .and_then(|state| state.equipment[slot.index()].as_ref())
                .map(|stack| stack.item);
            (Command::LockEquipped { slot }, item)
        } else if let Some(index) = self.selected_bag() {
            let item = self
                .state
                .as_ref()
                .and_then(|state| state.inventory.get(index))
                .map(|stack| stack.item);
            (Command::LockInventory { index }, item)
        } else {
            self.note("这里没有东西可锁。");
            return;
        };
        self.command(command);
        if !self.message.is_empty() {
            return;
        }
        let locked = match (self.selected_worn(), self.selected_bag(), &self.state) {
            (Some(slot), _, Some(state)) => state.equipment[slot.index()]
                .as_ref()
                .is_some_and(|stack| stack.locked),
            (_, Some(index), Some(state)) => {
                state.inventory.get(index).is_some_and(|stack| stack.locked)
            }
            _ => false,
        };
        if let Some(item) = item {
            let name = &self.data.item(item).name;
            self.note(if locked {
                format!("已锁定 {name}：不会被卖掉、拿去升级，高风险死亡也不掉")
            } else {
                format!("已解锁 {name}")
            });
        }
    }

    /// 卖掉背包里选中的那一格。先填数量，回车才成交。
    fn sell_selected(&mut self) {
        if self.panel != Panel::Bag {
            self.note("先打开背包（2），再按 S 卖掉选中的东西。");
            return;
        }
        if self.bag_worn {
            self.note("身上的装备先按 Enter 脱下来再卖。");
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
            fresh: true,
            error: String::new(),
        });
    }

    fn key_sell(&mut self, key: KeyEvent) {
        let owned = match &self.screen {
            Screen::Sell(form) => self
                .state
                .as_ref()
                .and_then(|state| state.inventory.get(form.index))
                .map_or(0, |stack| stack.count),
            _ => return,
        };
        match key.code {
            KeyCode::Esc => self.screen = Screen::Play,
            KeyCode::Enter => self.finish_sell(),
            _ => {
                if let Screen::Sell(form) = &mut self.screen {
                    if edit_quantity(&mut form.digits, &mut form.fresh, key, owned) {
                        form.error.clear();
                    }
                }
            }
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
                    let Some(index) = self.upgrade_accessory() else {
                        self.note(
                            "升级要一块黑铁矿和一件没锁的装备当材料。不想被拿去的先按 L 锁上。",
                        );
                        return;
                    };
                    let has_ore = state.inventory.iter().any(|stack| {
                        matches!(self.data.item(stack.item).kind, ItemKind::Material(_))
                    });
                    if !has_ore {
                        self.note("缺黑铁矿。怪会掉。");
                        return;
                    }
                    let charm = state.inventory.iter().any(|stack| {
                        matches!(self.data.item(stack.item).kind, ItemKind::LuckCharm)
                    });
                    let material = &self.data.item(state.inventory[index].item).name;
                    let risk = if charm {
                        "失败会磎一张幸运符，武器保住"
                    } else {
                        "没有幸运符，失败武器会碎"
                    };
                    self.upgrade_armed = true;
                    self.note(format!(
                        "升到 +{plus}：成功率 {:.0}%，{gold} 金币 + 黑铁矿 + {material}。{risk}。再按 U 确认，其它键取消。",
                        rate * 100.0
                    ));
                }
                Err(err) => self.note(err.to_string()),
            }
            return;
        }
        self.upgrade_armed = false;
        let Some(index) = self.upgrade_accessory() else {
            self.note(CoreError::MissingMaterials.to_string());
            return;
        };
        self.command(Command::UpgradeWeapon {
            accessory_index: index,
        });
    }

    /// 升级要消耗的那件装备：没锁、没强化里最便宜的，免得拿掉好东西。
    fn upgrade_accessory(&self) -> Option<usize> {
        let state = self.state.as_ref()?;
        state
            .inventory
            .iter()
            .enumerate()
            .filter(|(_, stack)| {
                !stack.locked
                    && stack.upgrade == 0
                    && self.data.item(stack.item).kind.gear().is_some()
            })
            .min_by_key(|(_, stack)| self.data.item(stack.item).price)
            .map(|(index, _)| index)
    }

    fn command(&mut self, command: Command) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        // 动了钱或物品的指令立刻存档，崩溃也不会丢刚买的东西。
        let saves = !matches!(
            command,
            Command::ToggleSkill { .. }
                | Command::SetAutoAdvance { .. }
                | Command::SetThresholds { .. }
                | Command::SetAutoSell { .. }
        );
        match command.apply(state, &self.data) {
            Ok(events) => {
                let mut summary = None;
                for event in events {
                    if matches!(
                        event,
                        GameEvent::Restocked { .. }
                            | GameEvent::Repaired { .. }
                            | GameEvent::Equipped { .. }
                            | GameEvent::Unequipped { .. }
                            | GameEvent::UpgradeSucceeded { .. }
                            | GameEvent::UpgradeProtected
                            | GameEvent::UpgradeDestroyed
                            | GameEvent::SkillLearned { .. }
                    ) {
                        summary = Some(format_event(&event, &self.data).text);
                    }
                    self.push_log(format_event(&event, &self.data));
                }
                self.note(summary.unwrap_or_default());
                self.clamp_selection();
                if saves {
                    self.save_if_playing();
                }
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

    /// 商店面板里的店铺，只算有货架的 NPC。
    pub fn shop_npcs(&self) -> Vec<usize> {
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

    fn select_shop(&mut self, index: usize) {
        if index != self.shop_index {
            self.shop_index = index;
            self.selected = 0;
        }
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
        // 往上翻着的时候来了新日志，画面不跟着动。
        if self.log_scroll > 0 {
            self.log_scroll += 1;
        }
        while self.log.len() > LOG_CAP {
            self.log.pop_front();
        }
        self.log_scroll = self.log_scroll.min(self.log.len());
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

/// 精调一档是 5%。夹在 5% 和 95% 之间。
fn step_percent(current: f64, delta: i32) -> f64 {
    let step = (current * 20.0).round() as i32 + delta;
    (step.clamp(1, 19) as f64) / 20.0
}

/// ↑↓ 用的环绕移动。空列表停在 0。
fn wrap_step(current: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (current.min(len - 1) as i64 + i64::from(delta)).rem_euclid(len as i64) as usize
}

/// 数量输入框的公共按键。数字键输入（第一下替换预填的数），↑↓ 加减 1，
/// PgUp/PgDn 加减 10，Home 最少，End 或 M 最多。返回是否改了数。
fn edit_quantity(digits: &mut String, fresh: &mut bool, key: KeyEvent, max: u32) -> bool {
    let current: u32 = digits.parse().unwrap_or(0);
    let max = max.clamp(1, 999_999);
    let set = |digits: &mut String, fresh: &mut bool, value: u32| {
        *digits = value.clamp(1, max).to_string();
        *fresh = false;
    };
    match key.code {
        KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            let ch = ascii_key(ch);
            if ch.is_ascii_digit() {
                if *fresh {
                    digits.clear();
                    *fresh = false;
                }
                if digits.len() < 6 {
                    digits.push(ch);
                }
                true
            } else if ch.eq_ignore_ascii_case(&'m') {
                set(digits, fresh, max);
                true
            } else {
                false
            }
        }
        KeyCode::Backspace => {
            if *fresh {
                digits.clear();
                *fresh = false;
            } else {
                digits.pop();
            }
            true
        }
        KeyCode::Up | KeyCode::Right => {
            set(digits, fresh, current.saturating_add(1));
            true
        }
        KeyCode::Down | KeyCode::Left => {
            set(digits, fresh, current.saturating_sub(1));
            true
        }
        KeyCode::PageUp => {
            set(digits, fresh, current.saturating_add(10));
            true
        }
        KeyCode::PageDown => {
            set(digits, fresh, current.saturating_sub(10));
            true
        }
        KeyCode::Home => {
            set(digits, fresh, 1);
            true
        }
        KeyCode::End => {
            set(digits, fresh, max);
            true
        }
        _ => false,
    }
}

/// 粗调只落在给定档位上。不在档上的旧值先贴到最近的一档。
fn snap_step(current: f64, steps: &[f64], delta: i32) -> f64 {
    let mut best = 0;
    for (index, step) in steps.iter().enumerate() {
        if (current - step).abs() < (current - steps[best]).abs() {
            best = index;
        }
    }
    let next = best as i32 + delta.signum();
    steps[next.clamp(0, steps.len() as i32 - 1) as usize]
}

fn format_report(report: &game_core::OfflineReport, data: &GameData) -> String {
    let minutes = report.ticks / 600;
    let span = if minutes >= 60 {
        format!("{} 小时 {} 分钟", minutes / 60, minutes % 60)
    } else {
        format!("{minutes} 分钟")
    };
    let mut lines = vec![
        format!("离线 {span}"),
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

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
