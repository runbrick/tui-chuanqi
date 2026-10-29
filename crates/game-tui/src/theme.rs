//! 颜色集中在这里，面板里不要再写死。

use game_data::Quality;
use ratatui::style::{Color, Modifier, Style};

/// 品质色。
pub fn quality(q: Quality) -> Color {
    match q {
        Quality::White => Color::White,
        Quality::Green => Color::Green,
        Quality::Blue => Color::LightBlue,
        Quality::Purple => Color::Yellow,
        Quality::Orange => Color::Red,
    }
}

/// 玩家看到的品质名：白、绿、蓝、黄、红。
pub fn quality_name(q: Quality) -> &'static str {
    match q {
        Quality::White => "白",
        Quality::Green => "绿",
        Quality::Blue => "蓝",
        Quality::Purple => "黄",
        Quality::Orange => "红",
    }
}
pub fn hp_color(ratio: f64) -> Color {
    if ratio < 0.3 {
        Color::Red
    } else if ratio < 0.6 {
        Color::Yellow
    } else {
        Color::Green
    }
}

/// 魔法条。
pub fn mp_color() -> Color {
    Color::Blue
}

/// 经验条。
pub fn exp_color() -> Color {
    Color::Cyan
}

/// 普通文字。
pub fn text() -> Style {
    Style::default().fg(Color::White)
}

/// 选中行。
pub fn selected() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

/// 高风险标记。
pub fn danger() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}

/// 稀有掉落。
pub fn rare() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

/// 升级。
pub fn level_up() -> Style {
    Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD)
}

/// 死亡。
pub fn death() -> Style {
    Style::default().fg(Color::Red)
}

/// 正的属性差。
pub fn up() -> Style {
    Style::default().fg(Color::Green)
}

/// 负的属性差。
pub fn down() -> Style {
    Style::default().fg(Color::Red)
}

/// 边框标题。
pub fn title() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

/// 次要文字：按键提示、用不了的物品、未解锁的地图。
pub fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

/// 自己掉血、钱不够这类不致命的坏消息。比死亡淡一些。
pub fn hurt() -> Style {
    Style::default().fg(Color::LightRed)
}
