use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::theme;

pub struct StatusInfo<'a> {
    pub line: usize,
    pub col: usize,
    pub message: &'a str,
    pub word_count: usize,
    pub modified: bool,
}

pub fn render(frame: &mut Frame, area: Rect, info: StatusInfo) {
    // Fill the entire status bar background
    let bg = Paragraph::new("").style(theme::status_style());
    frame.render_widget(bg, area);

    let chunks = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Fill(2),
        Constraint::Fill(1),
    ])
    .split(area);

    // Left: Ln/Col
    let left = Paragraph::new(Line::from(Span::styled(
        format!("  Ln {}, Col {}", info.line, info.col),
        theme::status_style(),
    )));
    frame.render_widget(left, chunks[0]);

    // Center: status message
    if !info.message.is_empty() {
        let center = Paragraph::new(Line::from(Span::styled(
            info.message.to_string(),
            theme::status_style(),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(center, chunks[1]);
    }

    // Right: word count + save status
    let save_status = if info.modified { "Modified" } else { "Saved" };
    let right = Paragraph::new(Line::from(Span::styled(
        format!("{} words | {}  ", info.word_count, save_status),
        theme::status_style(),
    )))
    .alignment(Alignment::Right);
    frame.render_widget(right, chunks[2]);
}
