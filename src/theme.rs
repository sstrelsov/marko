use ratatui::style::{Color, Modifier, Style};

// Base colors — Color::Reset inherits terminal defaults
pub const BG: Color = Color::Reset;
pub const FG: Color = Color::Reset;
pub const BORDER: Color = Color::DarkGray;

// UI elements
pub const BAR_BG: Color = Color::Reset;
pub const BAR_FG: Color = Color::Reset;
pub const LINE_NUMBER: Color = Color::DarkGray;
pub const SELECTION: Color = Color::Blue;

// Markdown syntax
pub const HEADING: Color = Color::Rgb(130, 170, 255);
pub const BOLD: Color = Color::Yellow;
pub const ITALIC: Color = Color::Cyan;
pub const LINK: Color = Color::Cyan;
pub const CODE: Color = Color::Red;
pub const CODE_BG: Color = Color::Rgb(40, 42, 54);
pub const QUOTE: Color = Color::Green;
pub const QUOTE_BORDER: Color = Color::Rgb(106, 190, 120);

// Git diff
pub const GIT_ADDED: Color = Color::Green;
pub const GIT_REMOVED: Color = Color::Red;
pub const GIT_MODIFIED: Color = Color::Yellow;

// Status indicators
pub const SUCCESS: Color = Color::Green;
pub const WARNING: Color = Color::Yellow;
pub const ERROR: Color = Color::Red;

// White for text on colored backgrounds
pub const WHITE: Color = Color::White;

// Tilde color for empty lines beyond file content
pub const TILDE: Color = Color::DarkGray;

// Tab colors
pub const ACTIVE_TAB: Color = Color::Blue;
pub const INACTIVE_TAB: Color = Color::Gray;

// Pre-built styles
pub fn editor_style() -> Style {
    Style::default()
}

pub fn header_style() -> Style {
    Style::default()
}

pub fn status_style() -> Style {
    Style::default()
}

pub fn line_number_style() -> Style {
    Style::default().fg(LINE_NUMBER)
}

pub fn cursor_line_style() -> Style {
    Style::default()
}

pub fn heading_style() -> Style {
    Style::default()
        .fg(HEADING)
        .add_modifier(Modifier::BOLD)
}

pub fn bold_style() -> Style {
    Style::default()
        .fg(BOLD)
        .add_modifier(Modifier::BOLD)
}

pub fn italic_style() -> Style {
    Style::default()
        .fg(ITALIC)
        .add_modifier(Modifier::ITALIC)
}

pub fn code_style() -> Style {
    Style::default().fg(CODE)
}

pub fn quote_style() -> Style {
    Style::default()
        .fg(QUOTE)
        .add_modifier(Modifier::ITALIC)
}

pub fn link_style() -> Style {
    Style::default()
        .fg(LINK)
        .add_modifier(Modifier::UNDERLINED)
}
