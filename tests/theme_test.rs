use ratatui::style::Color;

// Theme color verification tests.
// These ensure the ANSI color constants match the terminal-inherited theme.

#[test]
fn test_base_colors() {
    assert_eq!(marko::theme::BG, Color::Reset);
    assert_eq!(marko::theme::FG, Color::Reset);
    assert_eq!(marko::theme::BORDER, Color::DarkGray);
}

#[test]
fn test_ui_colors() {
    assert_eq!(marko::theme::BAR_BG, Color::Reset);
    assert_eq!(marko::theme::BAR_FG, Color::Reset);
    assert_eq!(marko::theme::LINE_NUMBER, Color::DarkGray);
    assert_eq!(marko::theme::SELECTION, Color::Blue);
}

#[test]
fn test_markdown_syntax_colors() {
    assert_eq!(marko::theme::HEADING, Color::Blue);
    assert_eq!(marko::theme::BOLD, Color::Yellow);
    assert_eq!(marko::theme::ITALIC, Color::Cyan);
    assert_eq!(marko::theme::LINK, Color::Cyan);
    assert_eq!(marko::theme::CODE, Color::Red);
    assert_eq!(marko::theme::QUOTE, Color::Green);
}

#[test]
fn test_git_diff_colors() {
    assert_eq!(marko::theme::GIT_ADDED, Color::Green);
    assert_eq!(marko::theme::GIT_REMOVED, Color::Red);
    assert_eq!(marko::theme::GIT_MODIFIED, Color::Yellow);
}

#[test]
fn test_status_indicator_colors() {
    assert_eq!(marko::theme::SUCCESS, Color::Green);
    assert_eq!(marko::theme::WARNING, Color::Yellow);
    assert_eq!(marko::theme::ERROR, Color::Red);
}

#[test]
fn test_tab_colors() {
    assert_eq!(marko::theme::ACTIVE_TAB, Color::Blue);
    assert_eq!(marko::theme::INACTIVE_TAB, Color::Gray);
}

#[test]
fn test_misc_colors() {
    assert_eq!(marko::theme::WHITE, Color::White);
    assert_eq!(marko::theme::TILDE, Color::DarkGray);
}
