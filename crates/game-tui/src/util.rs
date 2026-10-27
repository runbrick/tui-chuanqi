//! 中文宽度。布局一律用显示列，不用字节长度。

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// 终端显示列数。
pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// 截到不超过 `width` 列。截断时末尾用 `..`，并保证结果宽度不超过 `width`。
pub fn truncate_to(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    if width <= 2 {
        return ".".repeat(width);
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > width - 2 {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push_str("..");
    out
}

/// 截断后右侧补空格，使显示宽度正好是 `width`。
pub fn pad_to(text: &str, width: usize) -> String {
    let clipped = truncate_to(text, width);
    let gap = width.saturating_sub(display_width(&clipped));
    let mut out = clipped;
    out.extend(std::iter::repeat_n(' ', gap));
    out
}

/// 一条进度条，`width` 含两端括号。
pub fn bar(current: i32, max: i32, width: usize) -> String {
    let width = width.max(3);
    let inner = width - 2;
    let ratio = if max <= 0 {
        0.0
    } else {
        f64::from(current.max(0)) / f64::from(max)
    };
    let filled = ((ratio.clamp(0.0, 1.0)) * inner as f64).round() as usize;
    format!(
        "[{}{}]",
        "#".repeat(filled.min(inner)),
        ".".repeat(inner - filled.min(inner))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_names_align() {
        assert_eq!(display_width("木剑"), 4);
        assert_eq!(pad_to("木剑", 6), "木剑  ");
        assert_eq!(display_width(&pad_to("木剑", 6)), 6);
    }

    #[test]
    fn long_name_truncates_without_overflow() {
        let text = truncate_to("重盔甲(男)再加上很长的尾巴", 8);
        assert!(
            display_width(&text) <= 8,
            "{text} width={}",
            display_width(&text)
        );
        assert!(text.ends_with(".."));
    }
}
