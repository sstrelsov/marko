//! Unit tests for the App module: mode switching, scrolling, mouse handling,
//! selection, rename, tick timers, and scroll tracking.

use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

// ─── Helpers ─────────────────────────────────────────────────────

/// Creates an App backed by a temp file with the given content.
fn app_with_content(content: &str) -> (App<'static>, NamedTempFile) {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(content.as_bytes()).unwrap();
    tmp.flush().unwrap();
    let app = App::new(tmp.path().to_path_buf());
    (app, tmp)
}

fn key_event(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn ctrl_key(ch: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL))
}

fn char_event(ch: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE))
}

fn setup_viewport(app: &mut App, width: u16, height: u16) {
    app.viewport_height = height;
    app.content_area = Rect::new(0, 1, width, height);
}

// ─── Esc-as-Back Tests ────────────────────────────────────────────

#[test]
fn esc_returns_to_editor_from_preview() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(key_event(KeyCode::Tab)); // → Preview
    assert_eq!(app.mode, Mode::Preview);
    app.handle_event(key_event(KeyCode::Esc));
    assert_eq!(app.mode, Mode::Editor);
}

#[test]
fn esc_is_noop_in_editor_mode() {
    let (mut app, _tmp) = app_with_content("hello");
    assert_eq!(app.mode, Mode::Editor);
    app.handle_event(key_event(KeyCode::Esc));
    assert_eq!(app.mode, Mode::Editor);
    assert!(!app.should_quit);
}

#[test]
fn esc_does_not_quit() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(key_event(KeyCode::Esc));
    assert!(!app.should_quit);
    // Double Esc should also not quit
    app.handle_event(key_event(KeyCode::Esc));
    assert!(!app.should_quit);
}

#[test]
fn esc_in_rename_mode_cancels_rename_not_mode_switch() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(key_event(KeyCode::Tab)); // → Preview
    assert_eq!(app.mode, Mode::Preview);
    app.handle_event(ctrl_key('t')); // enter rename mode
    assert!(app.renaming);

    app.handle_event(key_event(KeyCode::Esc));
    assert!(!app.renaming);
    // Should stay in Preview — Esc was consumed by rename cancel
    assert_eq!(app.mode, Mode::Preview);
    assert!(!app.should_quit);
}

// ─── Preview Scrolling Tests ─────────────────────────────────────

#[test]
fn preview_up_at_top_is_noop() {
    let (mut app, _tmp) = app_with_content("hello");
    app.mode = Mode::Preview;
    setup_viewport(&mut app, 80, 20);
    app.preview.scroll_offset = 0;
    app.handle_event(key_event(KeyCode::Up));
    assert_eq!(app.preview.scroll_offset, 0);
}

#[test]
fn preview_down_scrolls_by_one() {
    let (mut app, _tmp) = app_with_content("hello");
    app.mode = Mode::Preview;
    setup_viewport(&mut app, 80, 20);
    app.preview.content_height = 100;
    app.preview.scroll_offset = 0;
    app.handle_event(key_event(KeyCode::Down));
    assert_eq!(app.preview.scroll_offset, 1);
}

#[test]
fn preview_up_scrolls_by_one() {
    let (mut app, _tmp) = app_with_content("hello");
    app.mode = Mode::Preview;
    setup_viewport(&mut app, 80, 20);
    app.preview.content_height = 100;
    app.preview.scroll_offset = 5;
    app.handle_event(key_event(KeyCode::Up));
    assert_eq!(app.preview.scroll_offset, 4);
}

#[test]
fn preview_page_down_scrolls_by_viewport_minus_2() {
    let (mut app, _tmp) = app_with_content("hello");
    app.mode = Mode::Preview;
    setup_viewport(&mut app, 80, 20);
    app.preview.content_height = 200;
    app.preview.scroll_offset = 0;
    app.handle_event(key_event(KeyCode::PageDown));
    assert_eq!(app.preview.scroll_offset, 18); // viewport_height (20) - 2
}

#[test]
fn preview_page_up_scrolls_by_viewport_minus_2() {
    let (mut app, _tmp) = app_with_content("hello");
    app.mode = Mode::Preview;
    setup_viewport(&mut app, 80, 20);
    app.preview.content_height = 200;
    app.preview.scroll_offset = 50;
    app.handle_event(key_event(KeyCode::PageUp));
    assert_eq!(app.preview.scroll_offset, 32); // 50 - 18
}

#[test]
fn preview_home_jumps_to_top() {
    let (mut app, _tmp) = app_with_content("hello");
    app.mode = Mode::Preview;
    setup_viewport(&mut app, 80, 20);
    app.preview.scroll_offset = 42;
    app.handle_event(key_event(KeyCode::Home));
    assert_eq!(app.preview.scroll_offset, 0);
}

#[test]
fn preview_end_jumps_to_bottom() {
    let (mut app, _tmp) = app_with_content("hello");
    app.mode = Mode::Preview;
    setup_viewport(&mut app, 80, 20);
    app.preview.content_height = 100;
    app.preview.scroll_offset = 0;
    app.handle_event(key_event(KeyCode::End));
    assert_eq!(app.preview.scroll_offset, 80); // 100 - 20
}

#[test]
fn preview_unrecognized_key_is_noop() {
    let (mut app, _tmp) = app_with_content("hello");
    app.mode = Mode::Preview;
    setup_viewport(&mut app, 80, 20);
    app.preview.scroll_offset = 5;
    app.handle_event(char_event('x'));
    assert_eq!(app.preview.scroll_offset, 5);
}

// ─── Mouse Tests ─────────────────────────────────────────────────

fn mouse_event(kind: MouseEventKind, col: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn mouse_scroll_up_in_preview() {
    let (mut app, _tmp) = app_with_content("hello");
    app.mode = Mode::Preview;
    setup_viewport(&mut app, 80, 20);
    app.preview.content_height = 100;
    app.preview.scroll_offset = 10;
    app.handle_event(mouse_event(MouseEventKind::ScrollUp, 40, 10));
    assert_eq!(app.preview.scroll_offset, 7); // 10 - SCROLL_LINES(3)
}

#[test]
fn mouse_scroll_down_in_preview() {
    let (mut app, _tmp) = app_with_content("hello");
    app.mode = Mode::Preview;
    setup_viewport(&mut app, 80, 20);
    app.preview.content_height = 100;
    app.preview.scroll_offset = 0;
    app.handle_event(mouse_event(MouseEventKind::ScrollDown, 40, 10));
    assert_eq!(app.preview.scroll_offset, 3); // 0 + SCROLL_LINES(3)
}

#[test]
fn mouse_click_editor_tab_switches_to_editor() {
    let (mut app, _tmp) = app_with_content("hello");
    app.mode = Mode::Preview;
    setup_viewport(&mut app, 80, 20);
    // Tab area: right-aligned in header (row 0). Total width = 17 (8+9).
    // With content_area x=0, width=80: tabs_start = 80 - 17 = 63
    // EDITOR tab: cols 63..70
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        65, 0,
    ));
    assert_eq!(app.mode, Mode::Editor);
}

#[test]
fn mouse_click_preview_tab_switches_to_preview() {
    let (mut app, _tmp) = app_with_content("hello");
    setup_viewport(&mut app, 80, 20);
    // PREVIEW tab: cols 71..79
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        74, 0,
    ));
    assert_eq!(app.mode, Mode::Preview);
}

#[test]
fn mouse_click_filename_starts_rename() {
    let (mut app, _tmp) = app_with_content("hello");
    setup_viewport(&mut app, 80, 20);
    // Click on filename area (left of tabs, row 0)
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        5, 0,
    ));
    assert!(app.renaming);
}

#[test]
fn mouse_click_in_editor_starts_selection() {
    let (mut app, _tmp) = app_with_content("hello world");
    setup_viewport(&mut app, 80, 20);
    // Click in content area
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        5, 1,
    ));
    assert!(app.mouse_dragging);
}

#[test]
fn mouse_release_cancels_zero_length_selection() {
    let (mut app, _tmp) = app_with_content("hello world");
    setup_viewport(&mut app, 80, 20);
    // Click and release at same position (no drag)
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        5, 1,
    ));
    assert!(app.mouse_dragging);
    app.handle_event(mouse_event(
        MouseEventKind::Up(MouseButton::Left),
        5, 1,
    ));
    assert!(!app.mouse_dragging);
    // Zero-length selection should be cancelled
    assert!(app.textarea.selection_range().is_none());
}

#[test]
fn mouse_click_outside_content_area_is_noop() {
    let (mut app, _tmp) = app_with_content("hello");
    setup_viewport(&mut app, 80, 20);
    let mode_before = app.mode.clone();
    // Click below content area (row 21 = content_area.y(1) + height(20))
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        5, 21,
    ));
    assert_eq!(app.mode, mode_before);
    assert!(!app.mouse_dragging);
}

// ─── Tick/Timer Tests ────────────────────────────────────────────

#[test]
fn tick_clears_expired_status_message() {
    let (mut app, _tmp) = app_with_content("hello");
    app.status_message = "test message".to_string();
    app.status_time = Some(Instant::now() - Duration::from_secs(4));
    app.tick();
    assert!(app.status_message.is_empty());
    assert!(app.status_time.is_none());
}

#[test]
fn tick_keeps_fresh_status_message() {
    let (mut app, _tmp) = app_with_content("hello");
    app.status_message = "fresh message".to_string();
    app.status_time = Some(Instant::now());
    app.tick();
    assert_eq!(app.status_message, "fresh message");
    assert!(app.status_time.is_some());
}

// ─── Selection Tests ────────────────────────────────────────────

#[test]
fn select_word_at_cursor_selects_word() {
    let (mut app, _tmp) = app_with_content("hello world");
    // Move cursor to col 1 (in "hello")
    app.textarea.move_cursor(CursorMove::Jump(0, 1));
    app.select_word_at_cursor();
    let range = app.textarea.selection_range();
    assert!(range.is_some(), "Should have a selection");
    let ((sr, sc), (er, ec)) = range.unwrap();
    assert_eq!((sr, sc), (0, 0));
    assert_eq!((er, ec), (0, 5)); // "hello" = 5 chars
}

#[test]
fn select_word_at_cursor_selects_second_word() {
    let (mut app, _tmp) = app_with_content("hello world");
    app.textarea.move_cursor(CursorMove::Jump(0, 7));
    app.select_word_at_cursor();
    let range = app.textarea.selection_range();
    assert!(range.is_some());
    let ((sr, sc), (er, ec)) = range.unwrap();
    assert_eq!((sr, sc), (0, 6));
    assert_eq!((er, ec), (0, 11)); // "world" = 5 chars at offset 6
}

#[test]
fn select_word_at_cursor_selects_punctuation() {
    let (mut app, _tmp) = app_with_content("hello...world");
    app.textarea.move_cursor(CursorMove::Jump(0, 6));
    app.select_word_at_cursor();
    let range = app.textarea.selection_range();
    assert!(range.is_some());
    let ((sr, sc), (er, ec)) = range.unwrap();
    assert_eq!((sr, sc), (0, 5));
    assert_eq!((er, ec), (0, 8)); // "..." = 3 chars at offset 5
}

#[test]
fn select_paragraph_single_paragraph() {
    let (mut app, _tmp) = app_with_content("line one\nline two\nline three");
    app.textarea.move_cursor(CursorMove::Jump(1, 0));
    app.select_paragraph_at_cursor();
    let range = app.textarea.selection_range();
    assert!(range.is_some());
    let ((sr, sc), (er, ec)) = range.unwrap();
    assert_eq!((sr, sc), (0, 0));
    assert_eq!(er, 2);
    assert_eq!(ec, 10); // "line three".len()
}

#[test]
fn select_paragraph_stops_at_empty_line() {
    let (mut app, _tmp) = app_with_content("para one\n\npara two");
    // Cursor on "para two" (line 2)
    app.textarea.move_cursor(CursorMove::Jump(2, 0));
    app.select_paragraph_at_cursor();
    let range = app.textarea.selection_range();
    assert!(range.is_some());
    let ((sr, sc), (er, ec)) = range.unwrap();
    assert_eq!((sr, sc), (2, 0)); // starts at line 2 (after empty line)
    assert_eq!((er, ec), (2, 8)); // "para two".len()
}

#[test]
fn ctrl_l_moves_to_line_start() {
    let (mut app, _tmp) = app_with_content("hello world");
    // Move cursor to middle of line
    app.handle_event(key_event(KeyCode::End));
    assert_eq!(app.textarea.cursor().1, 11);
    app.handle_event(ctrl_key('l'));
    assert_eq!(app.textarea.cursor().1, 0, "Ctrl+L should move to column 0");
}

#[test]
fn ctrl_l_cancels_selection() {
    let (mut app, _tmp) = app_with_content("hello world");
    // Create a selection
    app.handle_event(ctrl_key('a'));
    assert!(app.textarea.selection_range().is_some());
    app.handle_event(ctrl_key('l'));
    assert!(
        app.textarea.selection_range().is_none(),
        "Ctrl+L should cancel any active selection"
    );
}

// ─── Gutter Mark Tests ──────────────────────────────────────────

#[test]
fn gutter_marks_empty_for_non_git_file() {
    let (app, _tmp) = app_with_content("hello");
    // Temp files are not in a git repo, so gutter_marks should be empty
    assert!(app.gutter_marks.is_empty());
}

// ─── Auto-Wrap Tests ─────────────────────────────────────────

#[test]
fn navigation_keys_do_not_trigger_wrap() {
    // Create a line longer than the viewport width
    let long_line = "a ".repeat(50); // 100 chars
    let (mut app, _tmp) = app_with_content(&long_line.trim());
    setup_viewport(&mut app, 40, 20);
    // Store line content before navigation
    let line_before = app.textarea.lines()[0].to_string();

    // Press various navigation keys
    for code in &[
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::PageDown,
    ] {
        app.handle_event(key_event(*code));
    }

    // Line should be unchanged — navigation must not trigger wrapping
    assert_eq!(
        app.textarea.lines()[0], line_before,
        "Navigation keys should not modify the line"
    );
}

#[test]
fn typing_triggers_wrap() {
    let (mut app, _tmp) = app_with_content("hello world");
    setup_viewport(&mut app, 20, 20);
    // Move to end and type enough to exceed viewport width
    app.handle_event(key_event(KeyCode::End));
    for ch in " this is extra text that overflows".chars() {
        app.handle_event(char_event(ch));
    }
    // Should have wrapped into more than one line
    assert!(
        app.textarea.lines().len() > 1,
        "Typing past viewport width should trigger auto-wrap"
    );
}

#[test]
fn first_render_wraps_long_lines() {
    // Create a temp file with a very long line
    let long_line = "word ".repeat(40); // 200 chars
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(long_line.trim().as_bytes()).unwrap();
    tmp.flush().unwrap();

    let mut app = App::new(tmp.path().to_path_buf());
    // Before first render, content is raw (unwrapped)
    assert_eq!(app.textarea.lines().len(), 1);

    // Simulate first render: set content_area and trigger reflow
    setup_viewport(&mut app, 40, 20);
    let text_width = app.available_text_width();
    assert!(text_width > 0);
    // last_wrap_width starts at 0, so reflow is triggered
    app.reflow_content(text_width);

    // Content should now be wrapped into multiple lines
    assert!(
        app.textarea.lines().len() > 1,
        "Long lines should be hard-wrapped on first render reflow"
    );
    // File should not be marked as modified
    assert!(
        !app.modified,
        "Reflowed content should not mark file as modified"
    );
}

#[test]
fn reflow_to_wider_width_unwraps_lines() {
    // Simulate: content wrapped at narrow width, then terminal expanded
    let long_line = "word ".repeat(20); // 100 chars
    let (mut app, _tmp) = app_with_content(long_line.trim());
    // Wrap at narrow width first
    setup_viewport(&mut app, 30, 20);
    let narrow_width = app.available_text_width();
    app.reflow_content(narrow_width);
    let narrow_line_count = app.textarea.lines().len();
    assert!(narrow_line_count > 1, "Should wrap at narrow width");

    // Now expand to wider width
    setup_viewport(&mut app, 80, 20);
    let wide_width = app.available_text_width();
    app.reflow_content(wide_width);
    let wide_line_count = app.textarea.lines().len();
    assert!(
        wide_line_count < narrow_line_count,
        "Expanding should unwrap lines: {} should be less than {}",
        wide_line_count,
        narrow_line_count
    );
    assert!(!app.modified, "Reflow should not mark file as modified");
}

// ─── Docx State Tests ──────────────────────────────────────────

#[test]
fn docx_state_is_none_for_regular_md() {
    let (app, _tmp) = app_with_content("hello");
    assert!(app.docx_state.is_none());
}

// ─── Scroll Tracking Tests ────────────────────────────────────────

#[test]
fn mouse_scroll_down_updates_editor_scroll_top() {
    // 50-line doc: scroll down 5 times, verify offset = 5
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    for _ in 0..5 {
        app.handle_event(mouse_event(MouseEventKind::ScrollDown, 40, 10));
    }
    assert_eq!(app.editor_scroll_top, 5);
}

#[test]
fn mouse_scroll_up_updates_editor_scroll_top() {
    // Scroll down 10, then up 3, verify offset = 7
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    for _ in 0..10 {
        app.handle_event(mouse_event(MouseEventKind::ScrollDown, 40, 10));
    }
    for _ in 0..3 {
        app.handle_event(mouse_event(MouseEventKind::ScrollUp, 40, 10));
    }
    assert_eq!(app.editor_scroll_top, 7);
}

#[test]
fn mouse_scroll_up_clamps_at_zero() {
    let (mut app, _tmp) = app_with_content("hello");
    setup_viewport(&mut app, 80, 20);
    assert_eq!(app.editor_scroll_top, 0);
    app.handle_event(mouse_event(MouseEventKind::ScrollUp, 40, 10));
    assert_eq!(app.editor_scroll_top, 0);
}

#[test]
fn mouse_scroll_down_clamps_at_max() {
    // 5-line doc in 20-row viewport: max_scroll = 4 (total_lines - 1)
    let content = "a\nb\nc\nd\ne";
    let (mut app, _tmp) = app_with_content(content);
    setup_viewport(&mut app, 80, 20);
    for _ in 0..20 {
        app.handle_event(mouse_event(MouseEventKind::ScrollDown, 40, 10));
    }
    assert!(app.editor_scroll_top <= 4, "scroll should clamp at max (got {})", app.editor_scroll_top);
}

#[test]
fn click_after_scroll_maps_to_correct_buffer_row() {
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    // Scroll down 10
    for _ in 0..10 {
        app.handle_event(mouse_event(MouseEventKind::ScrollDown, 40, 10));
    }
    assert_eq!(app.editor_scroll_top, 10);
    // Click on row 1 of the content area (content_area.y = 1, so click row = 2)
    let (buffer_row, _) = app.mouse_to_buffer_pos(10, 2);
    // row 2 - content_area.y(1) = relative_row 1, + scroll 10 = buffer_row 11
    assert_eq!(buffer_row, 11);
}
