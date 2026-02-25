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
    pub update_available: bool,
}

pub fn render(frame: &mut Frame, area: Rect, info: StatusInfo) {
    // Fill the entire status bar background
    let bg = Paragraph::new("").style(theme::status_style());
    frame.render_widget(bg, area);

    let chunks = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Fill(1),
    ])
    .split(area);

    // Left: message replaces Ln/Col while active, otherwise Ln/Col
    if !info.message.is_empty() {
        let style = if info.update_available {
            theme::status_style().fg(theme::WARNING)
        } else {
            theme::status_style()
        };
        let left = Paragraph::new(Line::from(Span::styled(
            format!("  {}", info.message),
            style,
        )));
        frame.render_widget(left, chunks[0]);
    } else {
        let left = Paragraph::new(Line::from(Span::styled(
            format!("  Ln {}, Col {}", info.line, info.col),
            theme::status_style(),
        )));
        frame.render_widget(left, chunks[0]);
    }

    // Right: word count + save status
    let save_status = if info.modified { "Modified" } else { "Saved" };
    let right = Paragraph::new(Line::from(Span::styled(
        format!("{} words | {}  ", info.word_count, save_status),
        theme::status_style(),
    )))
    .alignment(Alignment::Right);
    frame.render_widget(right, chunks[1]);
}
