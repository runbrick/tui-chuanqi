//! 颜色集中在这里，面板里不要再写死。

use game_data::Quality;
use ratatui::style::{Color, Modifier, Style};

/// 品质色。
pub fn quality(q: Quality) -> Color {
    match q {
        Quality::White => Color::Gray,
        Quality::Green => Color::Green,
        Quality::Blue => Color::Blue,
        Quality::Purple => Color::Magenta,
        Quality::Orange => Color::Rgb(255, 165, 0),
    }
}

/// 生命条颜色。
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
