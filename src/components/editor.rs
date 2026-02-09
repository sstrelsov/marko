use ratatui::style::{Modifier, Style};
use tui_textarea::TextArea;

use crate::theme;

pub fn configure_textarea(textarea: &mut TextArea) {
    // Cursor line highlighting
    textarea.set_cursor_line_style(theme::cursor_line_style());

    // Line numbers
    textarea.set_line_number_style(theme::line_number_style());

    // Editor area style
    textarea.set_style(theme::editor_style());

    // Cursor style
    textarea.set_cursor_style(
        Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
    );

    // Selection style
    textarea.set_selection_style(Style::default().bg(theme::SELECTION));

    // Tab = 2 spaces
    textarea.set_tab_length(2);

    // Hard tab to spaces
    textarea.set_hard_tab_indent(false);
}
