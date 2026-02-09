use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::Mode;
use crate::theme;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    filename: &str,
    modified: bool,
    mode: &Mode,
    renaming: bool,
    rename_buf: &str,
    rename_cursor: usize,
) {
    // Left side: filename (or rename input) + modified indicator
    let left_spans = if renaming {
        render_rename_input(rename_buf, rename_cursor, modified)
    } else {
        render_filename(filename, modified)
    };

    // Right side: mode tabs
    let modes = [
        ("EDITOR", Mode::Editor),
        ("PREVIEW", Mode::Preview),
    ];

    let mut right_spans: Vec<Span> = Vec::new();
    for (label, tab_mode) in &modes {
        let is_active = std::mem::discriminant(mode) == std::mem::discriminant(tab_mode);
        if is_active {
            right_spans.push(Span::styled(
                format!(" {} ", label),
                Style::default()
                    .fg(theme::WHITE)
                    .bg(theme::ACTIVE_TAB)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            right_spans.push(Span::styled(
                format!(" {} ", label),
                Style::default().fg(theme::INACTIVE_TAB).bg(theme::BAR_BG),
            ));
        }
    }

    let chunks = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(right_spans.iter().map(|s| s.width() as u16).sum()),
    ])
    .split(area);

    // Fill background
    let bg = Paragraph::new("").style(theme::header_style());
    frame.render_widget(bg, area);

    let left = Paragraph::new(Line::from(left_spans));
    frame.render_widget(left, chunks[0]);

    let right = Paragraph::new(Line::from(right_spans));
    frame.render_widget(right, chunks[1]);
}

fn render_filename<'a>(filename: &str, modified: bool) -> Vec<Span<'a>> {
    let mut spans = vec![Span::styled(
        format!("  {}", filename),
        theme::header_style(),
    )];
    if modified {
        spans.push(Span::styled(
            " \u{2022}",
            Style::default().fg(theme::WARNING).bg(theme::BAR_BG),
        ));
    }
    spans
}

fn render_rename_input<'a>(rename_buf: &str, rename_cursor: usize, modified: bool) -> Vec<Span<'a>> {
    let mut spans = vec![Span::styled("  ", theme::header_style())];

    // Text before cursor
    let before = &rename_buf[..rename_cursor];
    if !before.is_empty() {
        spans.push(Span::styled(
            before.to_string(),
            Style::default().fg(theme::WHITE).bg(theme::BAR_BG),
        ));
    }

    // Cursor character (or space if at end)
    let cursor_char = if rename_cursor < rename_buf.len() {
        rename_buf[rename_cursor..rename_cursor + 1].to_string()
    } else {
        " ".to_string()
    };
    spans.push(Span::styled(
        cursor_char,
        Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
    ));

    // Text after cursor
    if rename_cursor < rename_buf.len() {
        let after = &rename_buf[rename_cursor + 1..];
        if !after.is_empty() {
            spans.push(Span::styled(
                after.to_string(),
                Style::default().fg(theme::WHITE).bg(theme::BAR_BG),
            ));
        }
    }

    if modified {
        spans.push(Span::styled(
            " \u{2022}",
            Style::default().fg(theme::WARNING).bg(theme::BAR_BG),
        ));
    }

    spans
}
