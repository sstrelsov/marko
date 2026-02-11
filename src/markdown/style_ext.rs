//! Extended markdown style parsing for ==highlight==, ^superscript^, and ~subscript~.
//!
//! These extensions aren't part of CommonMark but are widely used in note-taking
//! apps. We scan for delimiter pairs and convert to styled spans.

use ratatui::{
    style::{Color, Style},
    text::Span,
};

use super::math::{to_superscript, to_subscript};

/// Scan text for ==highlight==, ^superscript^, ~subscript~ and return styled spans.
pub fn style_extensions(text: &str, base_style: Style) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut spans = Vec::new();
    let mut i = 0;
    let mut plain_start = 0;

    while i < len {
        // ==highlight==
        if i + 3 < len && chars[i] == '=' && chars[i + 1] == '=' {
            if let Some(close) = find_double(&chars, i + 2, '=') {
                if close > i + 2 {
                    if i > plain_start {
                        let s: String = chars[plain_start..i].iter().collect();
                        spans.push(Span::styled(s, base_style));
                    }
                    let content: String = chars[i + 2..close].iter().collect();
                    spans.push(Span::styled(
                        content,
                        Style::default().bg(Color::Yellow).fg(Color::Black),
                    ));
                    i = close + 2;
                    plain_start = i;
                    continue;
                }
            }
        }
        // ^superscript^ (not ^^)
        if chars[i] == '^' && i + 1 < len && chars[i + 1] != '^' && chars[i + 1] != ' ' {
            if let Some(close) = find_single(&chars, i + 1, '^') {
                if close > i + 1 && !chars[i + 1..close].contains(&' ') {
                    if i > plain_start {
                        let s: String = chars[plain_start..i].iter().collect();
                        spans.push(Span::styled(s, base_style));
                    }
                    let content: String = chars[i + 1..close].iter().collect();
                    spans.push(Span::styled(to_superscript(&content), base_style));
                    i = close + 1;
                    plain_start = i;
                    continue;
                }
            }
        }
        // ~subscript~ (not ~~)
        if chars[i] == '~' && i + 1 < len && chars[i + 1] != '~' && chars[i + 1] != ' ' {
            if let Some(close) = find_single(&chars, i + 1, '~') {
                if close > i + 1
                    && !chars[i + 1..close].contains(&' ')
                    && (close + 1 >= len || chars[close + 1] != '~')
                {
                    if i > plain_start {
                        let s: String = chars[plain_start..i].iter().collect();
                        spans.push(Span::styled(s, base_style));
                    }
                    let content: String = chars[i + 1..close].iter().collect();
                    spans.push(Span::styled(to_subscript(&content), base_style));
                    i = close + 1;
                    plain_start = i;
                    continue;
                }
            }
        }
        i += 1;
    }

    if plain_start < len {
        let s: String = chars[plain_start..].iter().collect();
        spans.push(Span::styled(s, base_style));
    }

    if spans.is_empty() {
        vec![Span::styled(text.to_string(), base_style)]
    } else {
        spans
    }
}

fn find_double(chars: &[char], start: usize, c: char) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == c && chars[i + 1] == c {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_single(chars: &[char], start: usize, c: char) -> Option<usize> {
    for i in start..chars.len() {
        if chars[i] == c {
            return Some(i);
        }
    }
    None
}
