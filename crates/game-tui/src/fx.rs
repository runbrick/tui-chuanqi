//! 战斗舞台动画。
//!
//! 只读逻辑层的事件和状态，不影响任何规则。时间用逻辑 tick（100ms）计，
//! 所以动画和战斗节奏同步，测试也不用等真实时间。全部字符都是单列宽的 ASCII，
//! 网格不会被中文宽度撑歪。

use crate::theme;
use game_core::{GameEvent, GameState};
use game_data::{Class, DamageSource, GameData, MonsterId, SkillEffect, SkillId};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

const PLAYER_X: i32 = 2;
const PLAYER_W: i32 = 5;
const PET_X: i32 = 8;
const PET_W: i32 = 3;
const MON_W: i32 = 6;

const DASH_TTL: u32 = 3;
const SLASH_TTL: u32 = 3;
const BOLT_TTL: u32 = 4;
const BLAST_TTL: u32 = 3;
const FLASH_TTL: u32 = 3;
const FLOAT_TTL: u32 = 10;
const HEAL_TTL: u32 = 6;
const STUN_TTL: u32 = 8;
const DISSOLVE_TTL: u32 = 6;

type Sprite = [&'static str; 3];

const LYING: &str = "o-|-_";
const PET: [&str; 2] = ["(o)", "/ \\"];

/// 动画挂在谁身上。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Who {
    Player,
    Pet,
    Monster,
}

/// 法术颜色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Element {
    Fire,
    Spirit,
}

impl Element {
    fn style(self) -> Style {
        match self {
            Self::Fire => theme::danger(),
            Self::Spirit => theme::up(),
        }
    }

    fn head(self) -> char {
        match self {
            Self::Fire => '*',
            Self::Spirit => '#',
        }
    }
}

/// 飘字颜色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Hit,
    Hurt,
    Poison,
    Pet,
    Dim,
    Level,
}

impl Tone {
    fn style(self) -> Style {
        match self {
            Self::Hit => theme::rare(),
            Self::Hurt => theme::hurt(),
            Self::Poison => theme::up(),
            Self::Pet => theme::title(),
            Self::Dim => theme::dim(),
            Self::Level => theme::level_up(),
        }
    }
}

/// 飘字内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Label {
    Damage(i32),
    Miss,
    Block,
    LevelUp,
}

impl Label {
    fn text(self) -> String {
        match self {
            Self::Damage(n) => format!("-{n}"),
            Self::Miss => "MISS".to_string(),
            Self::Block => "BLOCK".to_string(),
            Self::LevelUp => "LEVEL UP".to_string(),
        }
    }
}

/// 怪物外观。只分三类，够区分就行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    Beast,
    Undead,
    Boss,
}

impl Look {
    fn of(data: &GameData, id: MonsterId, boss: bool) -> Self {
        if boss {
            Self::Boss
        } else if data.monster(id).undead {
            Self::Undead
        } else {
            Self::Beast
        }
    }

    fn style(self) -> Style {
        match self {
            Self::Beast => theme::beast(),
            Self::Undead => theme::undead(),
            Self::Boss => theme::danger(),
        }
    }

    fn rows(self, frame: u32) -> &'static Sprite {
        match (self, frame % 2) {
            (Self::Beast, 0) => &["/\\__/\\", "(o  o)", " /  \\ "],
            (Self::Beast, _) => &["/\\__/\\", "(-  -)", " /  \\ "],
            (Self::Undead, 0) => &[" ,--, ", "(o  o)", "/|##|\\"],
            (Self::Undead, _) => &[" ,--, ", "(o  o)", "\\|##|/"],
            (Self::Boss, 0) => &["\\^^^^/", "[O  O]", "/|WW|\\"],
            (Self::Boss, _) => &["\\^^^^/", "[0  0]", "\\|WW|/"],
        }
    }
}

fn player_sprite(class: Class, frame: u32) -> &'static Sprite {
    match (class, frame % 2) {
        (Class::Warrior, 0) => &[" O  |", "/|\\ |", "/ \\  "],
        (Class::Warrior, _) => &[" O | ", "/|\\| ", "/ \\  "],
        (Class::Mage, 0) => &[" O  *", "/|\\ |", "/ \\ |"],
        (Class::Mage, _) => &[" O  +", "/|\\ |", "/ \\ |"],
        (Class::Taoist, 0) => &[" O   ", "/|\\=]", "/ \\  "],
        (Class::Taoist, _) => &[" O  ~", "/|\\=]", "/ \\  "],
    }
}

/// 一次出手的表现方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shot {
    Melee,
    Bolt(Element),
    Blast(Element),
}

fn shot_for(source: DamageSource, area: bool) -> Shot {
    let element = match source {
        DamageSource::Attack => return Shot::Melee,
        DamageSource::Magic => Element::Fire,
        DamageSource::Taoism => Element::Spirit,
    };
    if area {
        Shot::Blast(element)
    } else {
        Shot::Bolt(element)
    }
}

fn element_for(source: DamageSource) -> Element {
    match source {
        DamageSource::Magic => Element::Fire,
        _ => Element::Spirit,
    }
}

/// 一次命中的结果。
#[derive(Debug, Clone, Copy)]
enum Outcome {
    Hit(i32),
    Block,
    Miss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// 近身冲刺。第 1 拍冲到位。
    Dash(Who),
    Slash,
    Bolt(Element),
    MonsterBolt,
    Blast(Element),
    /// 被打中，闪白。
    Flash(Who),
    Float {
        who: Who,
        tone: Tone,
        label: Label,
        slot: u8,
    },
    /// 怪物散开。`wait` 之前还画完整的怪，等最后一击落地。
    Dissolve {
        look: Look,
        wait: u32,
    },
    Heal,
    Stun,
}

#[derive(Debug, Clone, Copy)]
struct Anim {
    kind: Kind,
    at: u32,
    ttl: u32,
}

/// 战斗动画状态。每个逻辑 tick 调一次 [`BattleFx::step`]，画的时候调 [`BattleFx::scene`]。
#[derive(Debug, Clone, Default)]
pub struct BattleFx {
    clock: u32,
    anims: Vec<Anim>,
    /// 当前这只怪开始显示的时刻。
    appeared: u32,
    /// 上一拍场上是否有怪。
    fighting: bool,
    /// 最近一击落在怪身上的时刻。
    land_at: u32,
    /// 技能已经放出，等紧跟着的命中事件决定怎么画。
    pending: Option<Shot>,
    /// 最近见过的怪外观，击杀时拿来画散开。
    look: Option<Look>,
    /// 飘字错位用，避免同一时刻的数字叠在一起。
    slot: u8,
}

/// 画布几何。
struct Geo {
    w: i32,
    feet: i32,
    mx: i32,
}

impl Geo {
    fn new(w: usize, h: usize) -> Self {
        let (w, h) = (w as i32, h as i32);
        Self {
            w,
            feet: h - 2,
            mx: w - MON_W - 1,
        }
    }

    /// 从 `x`、宽 `width` 的位置冲到怪物面前要走多远。
    fn reach(&self, x: i32, width: i32) -> i32 {
        (self.mx - (x + width)).max(0)
    }
}

impl BattleFx {
    /// 推进一个逻辑 tick。`events` 是这一拍 `GameState::tick` 的产出。
    pub fn step(&mut self, state: &GameState, data: &GameData, events: &[GameEvent]) {
        self.clock = self.clock.saturating_add(1);
        for event in events {
            self.apply(event, data);
        }
        let fighting = state.combat.is_some();
        if fighting && !self.fighting {
            self.appeared = self.clock.saturating_add(self.dissolve_left());
        }
        self.fighting = fighting;
        if let Some(combat) = &state.combat {
            self.look = Some(Look::of(data, combat.monster, combat.boss));
        }
        self.pending = None;
        let clock = self.clock;
        self.anims.retain(|a| clock < a.at.saturating_add(a.ttl));
    }

    fn apply(&mut self, event: &GameEvent, data: &GameData) {
        match event {
            GameEvent::SkillCast { id } => self.cast(data, *id),
            GameEvent::Attacked { damage, .. } => self.player_strike(Outcome::Hit(*damage)),
            GameEvent::Blocked { .. } => self.player_strike(Outcome::Block),
            GameEvent::Missed { .. } => self.player_strike(Outcome::Miss),
            GameEvent::MonsterHit { source, damage } => {
                self.monster_strike(data, *source, Outcome::Hit(*damage));
            }
            GameEvent::MonsterBlocked { source } => {
                self.monster_strike(data, *source, Outcome::Block);
            }
            GameEvent::MonsterMissed { source } => {
                self.monster_strike(data, *source, Outcome::Miss);
            }
            GameEvent::PetAttacked { damage } => {
                self.add(Kind::Dash(Who::Pet), 0, DASH_TTL);
                self.land(Who::Monster, Outcome::Hit(*damage), 1, Tone::Pet);
            }
            GameEvent::DotDamage { damage, .. } => {
                let delay = self.until_land();
                self.float(Who::Monster, Tone::Poison, Label::Damage(*damage), delay);
            }
            GameEvent::MonsterSlain { .. } => self.slain(),
            GameEvent::LevelUp { .. } => {
                let delay = self.until_land() + 1;
                self.float(Who::Player, Tone::Level, Label::LevelUp, delay);
            }
            _ => {}
        }
    }

    fn cast(&mut self, data: &GameData, id: SkillId) {
        match &data.skill(id).effect {
            SkillEffect::Damage(e) => self.pending = Some(shot_for(e.source, false)),
            SkillEffect::AoeDamage(e) => self.pending = Some(shot_for(e.source, true)),
            SkillEffect::Heal(_) => self.add(Kind::Heal, 0, HEAL_TTL),
            SkillEffect::Poison(e) => {
                self.bolt(element_for(e.source), 0);
            }
            SkillEffect::DebuffDefense(_) => {
                self.bolt(Element::Spirit, 0);
            }
            SkillEffect::Paralyze(_) => {
                let delay = self.bolt(Element::Spirit, 0);
                self.add(Kind::Stun, delay, STUN_TTL);
            }
            _ => {}
        }
    }

    /// 放一发飞行物，返回它到达目标还要几拍。
    fn bolt(&mut self, element: Element, delay: u32) -> u32 {
        self.add(Kind::Bolt(element), delay, BOLT_TTL);
        delay + BOLT_TTL
    }

    fn player_strike(&mut self, outcome: Outcome) {
        let delay = match self.pending.take().unwrap_or(Shot::Melee) {
            Shot::Melee => {
                self.add(Kind::Dash(Who::Player), 0, DASH_TTL);
                if !matches!(outcome, Outcome::Miss) {
                    self.add(Kind::Slash, 1, SLASH_TTL);
                }
                1
            }
            Shot::Bolt(element) => self.bolt(element, 0),
            Shot::Blast(element) => {
                let delay = self.bolt(element, 0);
                self.add(Kind::Blast(element), delay, BLAST_TTL);
                delay
            }
        };
        self.land(Who::Monster, outcome, delay, Tone::Hit);
    }

    fn monster_strike(&mut self, data: &GameData, source: MonsterId, outcome: Outcome) {
        let delay = if data.monster(source).attack_type.is_spell() {
            self.add(Kind::MonsterBolt, 0, BOLT_TTL);
            BOLT_TTL
        } else {
            self.add(Kind::Dash(Who::Monster), 0, DASH_TTL);
            1
        };
        self.land(Who::Player, outcome, delay, Tone::Hurt);
    }

    /// 一击落地：闪白加飘字。
    fn land(&mut self, who: Who, outcome: Outcome, delay: u32, hit_tone: Tone) {
        if who == Who::Monster {
            self.land_at = self.land_at.max(self.clock.saturating_add(delay));
        }
        match outcome {
            Outcome::Hit(damage) => {
                self.add(Kind::Flash(who), delay, FLASH_TTL);
                self.float(who, hit_tone, Label::Damage(damage), delay);
            }
            Outcome::Block => self.float(who, Tone::Dim, Label::Block, delay),
            Outcome::Miss => self.float(who, Tone::Dim, Label::Miss, delay),
        }
    }

    fn slain(&mut self) {
        let wait = self.until_land();
        let look = self.look.unwrap_or(Look::Beast);
        self.add(Kind::Dissolve { look, wait }, 0, wait + DISSOLVE_TTL);
        // 同一拍里紧跟着刷新的怪也要重新播入场。
        self.fighting = false;
    }

    fn until_land(&self) -> u32 {
        self.land_at.saturating_sub(self.clock)
    }

    fn dissolve_left(&self) -> u32 {
        self.anims
            .iter()
            .filter(|a| matches!(a.kind, Kind::Dissolve { .. }))
            .map(|a| a.at.saturating_add(a.ttl).saturating_sub(self.clock))
            .max()
            .unwrap_or(0)
    }

    fn float(&mut self, who: Who, tone: Tone, label: Label, delay: u32) {
        let slot = self.slot;
        self.slot = (self.slot + 1) % 3;
        self.add(
            Kind::Float {
                who,
                tone,
                label,
                slot,
            },
            delay,
            FLOAT_TTL,
        );
    }

    fn add(&mut self, kind: Kind, delay: u32, ttl: u32) {
        self.anims.push(Anim {
            kind,
            at: self.clock.saturating_add(delay),
            ttl,
        });
    }

    fn live(&self, anim: &Anim) -> bool {
        self.clock >= anim.at && self.clock < anim.at.saturating_add(anim.ttl)
    }

    /// 符合条件、正在播放的动画里最新的那个已经播了几拍。
    fn age_of(&self, pick: impl Fn(&Kind) -> bool) -> Option<u32> {
        self.anims
            .iter()
            .filter(|a| self.live(a) && pick(&a.kind))
            .map(|a| self.clock - a.at)
            .min()
    }

    fn dash(&self, who: Who, reach: i32) -> i32 {
        match self.age_of(|k| *k == Kind::Dash(who)) {
            Some(1) => reach,
            Some(_) => reach / 2,
            None => 0,
        }
    }

    fn dissolving(&self) -> Option<(Look, u32, u32)> {
        self.anims.iter().find_map(|a| match a.kind {
            Kind::Dissolve { look, wait } if self.live(a) => Some((look, wait, self.clock - a.at)),
            _ => None,
        })
    }

    /// 画一帧，返回正好 `h` 行、每行 `w` 列的内容。
    pub fn scene(
        &self,
        state: &GameState,
        data: &GameData,
        w: usize,
        h: usize,
    ) -> Vec<Line<'static>> {
        if w == 0 || h == 0 {
            return Vec::new();
        }
        let mut cv = Canvas::new(w, h);
        let g = Geo::new(w, h);
        for x in 0..g.w {
            cv.put(x, g.feet + 1, '-', theme::dim());
        }
        self.draw_player(&mut cv, &g, state);
        self.draw_pet(&mut cv, &g, state);
        self.draw_monster(&mut cv, &g, state, data);
        self.draw_effects(&mut cv, &g);
        cv.into_lines()
    }

    fn draw_player(&self, cv: &mut Canvas, g: &Geo, state: &GameState) {
        let beat = self.clock / 5;
        if state.combat.is_none() && state.tick_count < state.resting_until {
            cv.text(PLAYER_X, g.feet, LYING, theme::dim());
            let z = if beat % 2 == 0 { 'z' } else { 'Z' };
            cv.put(
                PLAYER_X + PLAYER_W,
                g.feet - 1 - (beat % 2) as i32,
                z,
                theme::dim(),
            );
            return;
        }
        let hurt = self.age_of(|k| *k == Kind::Flash(Who::Player)).is_some();
        let charged = state
            .combat
            .as_ref()
            .is_some_and(|c| c.next_attack_power.is_some());
        let style = if hurt {
            theme::danger()
        } else if charged {
            theme::rare()
        } else {
            theme::text()
        };
        let x = PLAYER_X + self.dash(Who::Player, g.reach(PLAYER_X, PLAYER_W));
        cv.blit(x, g.feet - 2, player_sprite(state.class, beat), style);
        if state.shield_left > 0 {
            for dy in 0..3 {
                cv.put(x - 1, g.feet - 2 + dy, '(', theme::title());
                cv.put(x + PLAYER_W, g.feet - 2 + dy, ')', theme::title());
            }
        }
    }

    fn draw_pet(&self, cv: &mut Canvas, g: &Geo, state: &GameState) {
        if state.pet.is_none() {
            return;
        }
        let x = PET_X + self.dash(Who::Pet, g.reach(PET_X, PET_W));
        cv.blit(x, g.feet - 1, &PET, theme::title());
    }

    fn monster_x(&self, g: &Geo, slide: i32) -> i32 {
        let shake = self
            .age_of(|k| *k == Kind::Flash(Who::Monster))
            .map_or(0, |age| i32::from(age % 2 == 0));
        g.mx + slide + shake - self.dash(Who::Monster, g.reach(PLAYER_X, PLAYER_W))
    }

    fn blit_monster(&self, cv: &mut Canvas, g: &Geo, x: i32, look: Look) {
        let flashing = self.age_of(|k| *k == Kind::Flash(Who::Monster)).is_some();
        let style = if flashing {
            theme::flash()
        } else {
            look.style()
        };
        cv.blit(x, g.feet - 2, look.rows(self.clock / 5 + 1), style);
    }

    fn draw_monster(&self, cv: &mut Canvas, g: &Geo, state: &GameState, data: &GameData) {
        if let Some((look, wait, age)) = self.dissolving() {
            let x = self.monster_x(g, 0);
            if age <= wait {
                self.blit_monster(cv, g, x, look);
            } else {
                draw_dissolve(cv, x, g.feet - 2, look, age - wait);
            }
        }
        let Some(combat) = &state.combat else {
            return;
        };
        if self.clock < self.appeared {
            return;
        }
        let slide = match self.clock - self.appeared {
            0 => 12,
            1 => 6,
            2 => 3,
            3 => 1,
            _ => 0,
        };
        let x = self.monster_x(g, slide);
        self.blit_monster(cv, g, x, Look::of(data, combat.monster, combat.boss));
        if combat.poison_left > 0 {
            for i in 0..3_u32 {
                let phase = (self.clock / 2 + i * 2) % 4;
                let ch = ['.', 'o', 'O', '.'][phase as usize];
                cv.put(
                    x + 1 + i as i32 * 2,
                    g.feet - 2 - phase as i32,
                    ch,
                    theme::up(),
                );
            }
        }
    }

    fn draw_effects(&self, cv: &mut Canvas, g: &Geo) {
        for anim in self.anims.iter().filter(|a| self.live(a)) {
            let age = self.clock - anim.at;
            match anim.kind {
                Kind::Slash => draw_slash(cv, g, age),
                Kind::Bolt(el) => draw_bolt(cv, g, el.style(), el.head(), (age, anim.ttl), false),
                Kind::MonsterBolt => {
                    draw_bolt(cv, g, theme::hurt(), 'o', (age, anim.ttl), true);
                }
                Kind::Blast(el) => draw_blast(cv, g, el, age),
                Kind::Heal => draw_heal(cv, g, age),
                Kind::Stun => {
                    let ch = if (age / 2) % 2 == 0 { 'z' } else { 'Z' };
                    cv.put(g.mx + 2, g.feet - 3, ch, theme::title());
                }
                Kind::Float {
                    who,
                    tone,
                    label,
                    slot,
                } => draw_float(cv, g, (who, tone, label, slot), age),
                Kind::Dash(_) | Kind::Flash(_) | Kind::Dissolve { .. } => {}
            }
        }
    }
}

fn draw_slash(cv: &mut Canvas, g: &Geo, age: u32) {
    let style = theme::rare();
    let top = g.feet - 2;
    for i in 0..=age.min(2) as i32 {
        cv.put(g.mx + 1 + i, top + i, '\\', style);
    }
    if age >= 2 {
        for i in 0..3 {
            cv.put(g.mx + 3 - i, top + i, '/', style);
        }
    }
}

fn draw_bolt(
    cv: &mut Canvas,
    g: &Geo,
    style: Style,
    head: char,
    (age, ttl): (u32, u32),
    rtl: bool,
) {
    let muzzle = PLAYER_X + PLAYER_W;
    let (from, to) = if rtl {
        (g.mx - 1, muzzle)
    } else {
        (muzzle, g.mx - 1)
    };
    let x = from + (to - from) * (age as i32 + 1) / ttl.max(1) as i32;
    let y = g.feet - 1;
    cv.put(x - (to - from).signum(), y, '.', style);
    cv.put(x, y, head, style);
}

fn draw_blast(cv: &mut Canvas, g: &Geo, element: Element, age: u32) {
    let (cx, cy) = (g.mx + 3, g.feet - 1);
    let style = element.style();
    let (cells, ch): (&[(i32, i32)], char) = match age {
        0 => (&[(0, 0)], '*'),
        1 => (&[(-1, 0), (1, 0), (0, -1), (0, 1)], '*'),
        _ => (&[(-2, 0), (2, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)], '.'),
    };
    for (dx, dy) in cells {
        cv.put(cx + dx, cy + dy, ch, style);
    }
}

fn draw_heal(cv: &mut Canvas, g: &Geo, age: u32) {
    for (dx, lag) in [(0, 0), (2, 1), (4, 2)] {
        let y = g.feet - 3 - ((age + lag) / 2) as i32;
        cv.put(PLAYER_X + dx, y, '+', theme::up());
    }
}

fn draw_float(
    cv: &mut Canvas,
    g: &Geo,
    (who, tone, label, slot): (Who, Tone, Label, u8),
    age: u32,
) {
    let text = label.text();
    let len = text.len() as i32;
    let (base, width) = match who {
        Who::Player => (PLAYER_X, PLAYER_W),
        Who::Pet => (PET_X, PET_W),
        Who::Monster => (g.mx, MON_W),
    };
    let x = base + (width - len) / 2 + (i32::from(slot) - 1) * 2;
    let x = x.min(g.w - len).max(0);
    let y = (g.feet - 3 - (age / 3) as i32).max(0);
    cv.text(x, y, &text, tone.style());
}

/// 每个格子固定一个消失时刻，逐帧只会少不会多，不闪烁。
fn draw_dissolve(cv: &mut Canvas, x: i32, y: i32, look: Look, progress: u32) {
    for (dy, row) in look.rows(0).iter().enumerate() {
        for (dx, ch) in row.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let hash = (dx as u32).wrapping_mul(2_654_435_761) ^ (dy as u32).wrapping_mul(40_503);
            let gone_at = 2 + (hash >> 5) % 5;
            let (px, py) = (x + dx as i32, y + dy as i32);
            if progress < gone_at {
                cv.put(px, py, ch, look.style());
            } else if progress == gone_at {
                let spark = if (dx + dy) % 2 == 0 { '*' } else { '.' };
                cv.put(px, py, spark, theme::dim());
            }
        }
    }
}

/// 字符网格。坐标越界一律忽略，所以窗口再小也不会 panic。
struct Canvas {
    w: usize,
    h: usize,
    cells: Vec<(char, Style)>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            cells: vec![(' ', Style::default()); w * h],
        }
    }

    fn put(&mut self, x: i32, y: i32, ch: char, style: Style) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        self.cells[y as usize * self.w + x as usize] = (ch, style);
    }

    fn text(&mut self, x: i32, y: i32, text: &str, style: Style) {
        for (i, ch) in text.chars().enumerate() {
            self.put(x + i as i32, y, ch, style);
        }
    }

    /// 贴精灵，空格是透明的。
    fn blit(&mut self, x: i32, y: i32, rows: &[&str], style: Style) {
        for (dy, row) in rows.iter().enumerate() {
            for (dx, ch) in row.chars().enumerate() {
                if ch != ' ' {
                    self.put(x + dx as i32, y + dy as i32, ch, style);
                }
            }
        }
    }

    /// 相邻同样式的格子合成一个 Span。
    fn into_lines(self) -> Vec<Line<'static>> {
        self.cells
            .chunks(self.w)
            .map(|row| {
                let mut spans = Vec::new();
                let mut run = String::new();
                let mut current: Option<Style> = None;
                for &(ch, style) in row {
                    if current != Some(style) {
                        if let Some(done) = current {
                            spans.push(Span::styled(std::mem::take(&mut run), done));
                        }
                        current = Some(style);
                    }
                    run.push(ch);
                }
                if let Some(done) = current {
                    spans.push(Span::styled(run, done));
                }
                Line::from(spans)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::display_width;
    use game_data::Gender;

    fn data() -> GameData {
        GameData::load(game_core::repo_data_dir()).unwrap()
    }

    fn text_of(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    fn hit(damage: i32) -> GameEvent {
        GameEvent::Attacked {
            target: MonsterId(0),
            damage,
            crit: false,
        }
    }

    fn has_damage(rows: &[String]) -> bool {
        rows.iter().any(|r| {
            r.as_bytes()
                .windows(2)
                .any(|w| w[0] == b'-' && w[1].is_ascii_digit())
        })
    }

    #[test]
    fn melee_hit_dashes_slashes_and_floats_damage() {
        let data = data();
        let state = GameState::new("t", Class::Warrior, Gender::Male, false, &data, 1);
        let mut fx = BattleFx::default();
        fx.step(&state, &data, &[hit(42)]);
        assert!(fx.anims.iter().any(|a| a.kind == Kind::Dash(Who::Player)));
        assert!(fx.anims.iter().any(|a| a.kind == Kind::Slash));
        fx.step(&state, &data, &[]);
        let rows = text_of(&fx.scene(&state, &data, 30, 9));
        assert!(rows.iter().any(|r| r.contains("-42")), "{rows:#?}");
    }

    #[test]
    fn spell_flies_first_and_lands_after_the_bolt() {
        let data = data();
        let state = GameState::new("t", Class::Mage, Gender::Male, false, &data, 1);
        let index = data
            .skills
            .iter()
            .position(
                |s| matches!(&s.effect, SkillEffect::Damage(e) if e.source == DamageSource::Magic),
            )
            .expect("数据里应该有单体法术");
        let mut fx = BattleFx::default();
        fx.step(
            &state,
            &data,
            &[
                GameEvent::SkillCast {
                    id: SkillId(index as u32),
                },
                hit(30),
            ],
        );
        assert!(fx.anims.iter().any(|a| a.kind == Kind::Bolt(Element::Fire)));
        assert!(!fx.anims.iter().any(|a| a.kind == Kind::Slash));
        let flash = fx
            .anims
            .iter()
            .find(|a| a.kind == Kind::Flash(Who::Monster))
            .unwrap();
        assert_eq!(flash.at, fx.clock + BOLT_TTL);
    }

    #[test]
    fn killing_blow_lands_before_the_monster_dissolves() {
        let data = data();
        let state = GameState::new("t", Class::Warrior, Gender::Male, false, &data, 1);
        let mut fx = BattleFx::default();
        fx.step(
            &state,
            &data,
            &[
                hit(99),
                GameEvent::MonsterSlain {
                    id: MonsterId(0),
                    exp: 1,
                },
            ],
        );
        let wait = fx.anims.iter().find_map(|a| match a.kind {
            Kind::Dissolve { wait, .. } => Some(wait),
            _ => None,
        });
        assert_eq!(wait, Some(1));
    }

    #[test]
    fn scene_always_matches_requested_size() {
        let data = data();
        for class in [Class::Warrior, Class::Mage, Class::Taoist] {
            let mut state = GameState::new("t", class, Gender::Male, false, &data, 7);
            let mut fx = BattleFx::default();
            let mut saw_damage = false;
            for tick in 0..1200 {
                let events = state.tick(&data);
                fx.step(&state, &data, &events);
                if tick % 3 != 0 {
                    continue;
                }
                assert!(fx.scene(&state, &data, 0, 5).is_empty());
                assert!(fx.scene(&state, &data, 5, 0).is_empty());
                for (w, h) in [(1, 1), (5, 3), (28, 6), (30, 9), (60, 14)] {
                    let rows = text_of(&fx.scene(&state, &data, w, h));
                    assert_eq!(rows.len(), h);
                    for row in &rows {
                        assert_eq!(display_width(row), w, "{row:?}");
                    }
                }
                saw_damage |= has_damage(&text_of(&fx.scene(&state, &data, 30, 9)));
            }
            assert!(saw_damage, "{class:?} 打了 2 分钟也没见过飘字");
        }
    }
}
