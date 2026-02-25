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

// ─── No-Wrap Tests ─────────────────────────────────────────

#[test]
fn navigation_keys_do_not_modify_buffer() {
    // Create a line longer than the viewport width
    let long_line = "a ".repeat(50); // 100 chars
    let (mut app, _tmp) = app_with_content(&long_line.trim());
    setup_viewport(&mut app, 40, 20);
    // Store line content before navigation
    let line_before = app.textarea.lines()[0].to_string();
    let line_count_before = app.textarea.lines().len();

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

    // Line should be unchanged — navigation must not modify the buffer
    assert_eq!(
        app.textarea.lines()[0], line_before,
        "Navigation keys should not modify the line"
    );
    assert_eq!(
        app.textarea.lines().len(),
        line_count_before,
        "Navigation keys should not change line count"
    );
}

#[test]
fn typing_does_not_add_newlines() {
    let (mut app, _tmp) = app_with_content("hello world");
    setup_viewport(&mut app, 20, 20);
    // Move to end and type text that would exceed viewport width
    app.handle_event(key_event(KeyCode::End));
    for ch in " this is extra text that overflows".chars() {
        app.handle_event(char_event(ch));
    }
    // Should stay as one logical line — no hard wrapping
    assert_eq!(
        app.textarea.lines().len(),
        1,
        "Typing should not insert newlines (no hard wrapping)"
    );
}

#[test]
fn save_preserves_raw_content() {
    let content = "a long line that should not be wrapped when saved to disk regardless of width";
    let (mut app, _tmp) = app_with_content(content);
    setup_viewport(&mut app, 20, 20);
    // Type something to mark as modified
    app.handle_event(key_event(KeyCode::End));
    app.handle_event(char_event('.'));
    assert!(app.modified);
    app.save();
    // Read back saved content
    let saved = std::fs::read_to_string(app.file_path.clone()).unwrap();
    // Should be one line — no width-dependent wrapping
    assert_eq!(
        saved.lines().count(),
        1,
        "Saved content should not contain width-dependent newlines"
    );
    assert!(saved.starts_with("a long line"));
    assert!(saved.ends_with('.'));
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
fn scroll_does_not_move_cursor() {
    // Place cursor at row 10, scroll down 5 times — cursor should stay at row 10.
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    app.textarea.move_cursor(CursorMove::Jump(10, 5));
    assert_eq!(app.textarea.cursor(), (10, 5));
    for _ in 0..5 {
        app.handle_event(mouse_event(MouseEventKind::ScrollDown, 40, 10));
    }
    assert_eq!(app.editor_scroll_top, 5);
    assert_eq!(app.textarea.cursor(), (10, 5), "cursor should not move on scroll");
    // Also scroll up — cursor should stay put
    for _ in 0..3 {
        app.handle_event(mouse_event(MouseEventKind::ScrollUp, 40, 10));
    }
    assert_eq!(app.editor_scroll_top, 2);
    assert_eq!(app.textarea.cursor(), (10, 5), "cursor should not move on scroll up");
}

#[test]
fn scroll_cursor_off_screen_saves_true_position() {
    // Cursor at row 5, scroll down 20 — cursor goes off-screen.
    // scroll_cursor should store the true position.
    // Scroll back — cursor should be restored.
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    app.textarea.move_cursor(CursorMove::Jump(5, 3));
    assert_eq!(app.textarea.cursor(), (5, 3));
    // Scroll down 20 — viewport [20, 39], cursor at row 5 is off-screen
    for _ in 0..20 {
        app.handle_event(mouse_event(MouseEventKind::ScrollDown, 40, 10));
    }
    assert_eq!(app.editor_scroll_top, 20);
    assert!(app.scroll_cursor.is_some(), "true cursor should be saved");
    assert_eq!(app.scroll_cursor.unwrap(), (5, 3));
    // Scroll back up 20
    for _ in 0..20 {
        app.handle_event(mouse_event(MouseEventKind::ScrollUp, 40, 10));
    }
    assert_eq!(app.editor_scroll_top, 0);
    assert!(app.scroll_cursor.is_none(), "scroll_cursor cleared when cursor is back in viewport");
    assert_eq!(app.textarea.cursor(), (5, 3), "cursor restored to original position");
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
    // 5-line doc in 20-row viewport: content fits entirely, max_scroll = 0
    let content = "a\nb\nc\nd\ne";
    let (mut app, _tmp) = app_with_content(content);
    setup_viewport(&mut app, 80, 20);
    for _ in 0..20 {
        app.handle_event(mouse_event(MouseEventKind::ScrollDown, 40, 10));
    }
    assert_eq!(app.editor_scroll_top, 0, "content fits viewport, scroll should stay at 0");
}

#[test]
fn mouse_scroll_down_clamps_tall_content() {
    // 50-line doc in 20-row viewport: max_scroll = 50 - 20 = 30
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    for _ in 0..60 {
        app.handle_event(mouse_event(MouseEventKind::ScrollDown, 40, 10));
    }
    assert_eq!(app.editor_scroll_top, 30, "scroll should clamp at total_lines - viewport_height");
}

#[test]
fn mouse_scroll_up_not_forwarded_at_zero() {
    // When already at top, scrolling up should not change anything
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    assert_eq!(app.editor_scroll_top, 0);
    // Scroll up multiple times at top
    for _ in 0..5 {
        app.handle_event(mouse_event(MouseEventKind::ScrollUp, 40, 10));
    }
    assert_eq!(app.editor_scroll_top, 0);
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

#[test]
fn click_in_tilde_area_clamps_to_last_line() {
    // 5-line doc in 20-row viewport: clicking row 15 (in tilde area) should
    // clamp to last buffer line (4)
    let content = "a\nb\nc\nd\ne";
    let (mut app, _tmp) = app_with_content(content);
    setup_viewport(&mut app, 80, 20);
    // Row 16 = content_area.y(1) + 15, which is well past the 5 lines
    let (buffer_row, _) = app.mouse_to_buffer_pos(10, 16);
    assert_eq!(buffer_row, 4, "click in tilde area should clamp to last line");
}

// ─── Drag Auto-Scroll Tests ────────────────────────────────────────

#[test]
fn drag_below_viewport_sets_auto_scroll_down() {
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    // Start drag in editor (content_area.y=1)
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        10, 5,
    ));
    assert!(app.mouse_dragging);
    let cursor_before = app.textarea.cursor().0;
    // Drag below viewport (row 21 is past content_area.y(1) + height(20))
    app.handle_event(mouse_event(
        MouseEventKind::Drag(MouseButton::Left),
        10, 21,
    ));
    assert_eq!(app.drag_auto_scroll, Some(DragAutoScroll::Down));
    assert!(app.textarea.cursor().0 > cursor_before, "cursor should move down");
}

#[test]
fn drag_above_viewport_sets_auto_scroll_up() {
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    // Move cursor down so we have room to scroll up
    app.textarea.move_cursor(CursorMove::Jump(10, 0));
    // Start drag
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        10, 10,
    ));
    assert!(app.mouse_dragging);
    let cursor_before = app.textarea.cursor().0;
    // Drag above viewport (row 0 is above content_area.y=1)
    app.handle_event(mouse_event(
        MouseEventKind::Drag(MouseButton::Left),
        10, 0,
    ));
    assert_eq!(app.drag_auto_scroll, Some(DragAutoScroll::Up));
    assert!(app.textarea.cursor().0 < cursor_before, "cursor should move up");
}

#[test]
fn drag_within_viewport_clears_auto_scroll() {
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    // Start drag, go below, then come back
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        10, 5,
    ));
    app.handle_event(mouse_event(
        MouseEventKind::Drag(MouseButton::Left),
        10, 21,
    ));
    assert_eq!(app.drag_auto_scroll, Some(DragAutoScroll::Down));
    // Move back within viewport
    app.handle_event(mouse_event(
        MouseEventKind::Drag(MouseButton::Left),
        10, 10,
    ));
    assert_eq!(app.drag_auto_scroll, None, "returning to viewport should clear auto-scroll");
}

#[test]
fn tick_continues_auto_scroll_down() {
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    // Start drag and move below viewport
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        10, 5,
    ));
    app.handle_event(mouse_event(
        MouseEventKind::Drag(MouseButton::Left),
        10, 21,
    ));
    let cursor_after_drag = app.textarea.cursor().0;
    // tick() should continue moving cursor down (simulating terminal no longer sending events)
    app.tick();
    assert!(app.textarea.cursor().0 > cursor_after_drag, "tick should keep scrolling down");
}

#[test]
fn mouse_release_clears_auto_scroll() {
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        10, 5,
    ));
    app.handle_event(mouse_event(
        MouseEventKind::Drag(MouseButton::Left),
        10, 21,
    ));
    assert!(app.drag_auto_scroll.is_some());
    app.handle_event(mouse_event(
        MouseEventKind::Up(MouseButton::Left),
        10, 21,
    ));
    assert!(!app.mouse_dragging);
    assert_eq!(app.drag_auto_scroll, None, "release should clear auto-scroll");
}

#[test]
fn scroll_during_drag_extends_selection_down() {
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    // Start drag selection
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        10, 5,
    ));
    assert!(app.mouse_dragging);
    let cursor_before = app.textarea.cursor().0;
    // Scroll down while dragging — should extend selection, not just scroll viewport
    app.handle_event(mouse_event(MouseEventKind::ScrollDown, 40, 10));
    assert!(app.textarea.cursor().0 > cursor_before, "scroll during drag should move cursor down");
    assert!(app.textarea.selection_range().is_some(), "selection should still be active");
}

#[test]
fn scroll_after_drag_preserves_selection() {
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    // Drag-select: press down, drag to different row, release
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        5, 3,
    ));
    app.handle_event(mouse_event(
        MouseEventKind::Drag(MouseButton::Left),
        20, 6,
    ));
    app.handle_event(mouse_event(
        MouseEventKind::Up(MouseButton::Left),
        20, 6,
    ));
    assert!(!app.mouse_dragging);
    let range_before = app.textarea.selection_range();
    assert!(range_before.is_some(), "selection should exist after drag-release");
    // Now scroll — selection must survive
    app.handle_event(mouse_event(MouseEventKind::ScrollDown, 40, 10));
    assert!(
        app.textarea.selection_range().is_some(),
        "selection should persist after scroll"
    );
}

#[test]
fn scroll_far_and_back_restores_selection() {
    // Select rows 2-5 in a 50-line doc with 20-line viewport, scroll far
    // past the selection, then scroll back — the full selection should reappear.
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);

    // Click row 3 (content_area.y=1, so buffer_row = 3-1+scroll(0) = 2)
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        5, 3,
    ));
    // Drag to row 6 (buffer_row = 5)
    app.handle_event(mouse_event(
        MouseEventKind::Drag(MouseButton::Left),
        20, 6,
    ));
    app.handle_event(mouse_event(
        MouseEventKind::Up(MouseButton::Left),
        20, 6,
    ));
    let original_range = app.textarea.selection_range();
    assert!(original_range.is_some(), "selection should exist after drag");

    // Scroll down 25 times — cursor will be forced off the selection
    for _ in 0..25 {
        app.handle_event(mouse_event(MouseEventKind::ScrollDown, 40, 10));
    }
    // Selection may or may not be visible, but preserved_sel should be set
    assert!(app.scroll_cursor.is_some(), "scroll_cursor should be saved");

    // Now scroll back up 25 times to restore original viewport
    for _ in 0..25 {
        app.handle_event(mouse_event(MouseEventKind::ScrollUp, 40, 10));
    }
    // The selection should be restored to its original range
    let restored_range = app.textarea.selection_range();
    assert!(restored_range.is_some(), "selection should be restored after scrolling back");
    assert_eq!(
        original_range, restored_range,
        "selection range should match original after scroll round-trip"
    );
}

#[test]
fn scroll_during_drag_extends_selection_up() {
    let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&content);
    setup_viewport(&mut app, 80, 20);
    // Move cursor down first so there's room to scroll up
    app.textarea.move_cursor(CursorMove::Jump(10, 5));
    // Start drag selection
    app.handle_event(mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        10, 11, // row 11 = content_area.y(1) + cursor_row(10)
    ));
    assert!(app.mouse_dragging);
    let cursor_before = app.textarea.cursor().0;
    // Scroll up while dragging
    app.handle_event(mouse_event(MouseEventKind::ScrollUp, 40, 10));
    assert!(app.textarea.cursor().0 < cursor_before, "scroll during drag should move cursor up");
    assert!(app.textarea.selection_range().is_some(), "selection should still be active");
}

// ─── Visual Cursor Navigation Tests ─────────────────────────────

#[test]
fn visual_cursor_up_down_within_wrapped_line() {
    // Create a long line that wraps at width 20, set up wrap state, then
    // verify Up/Down moves between visual rows within the same logical line.
    let long_line = "the quick brown fox jumps over the lazy dog and more text here";
    let (mut app, _tmp) = app_with_content(long_line);
    setup_viewport(&mut app, 24, 20);
    // Force wrap state computation
    let text_width = (app.content_area.width as usize).saturating_sub(app.gutter_width() as usize);
    app.wrap_state = Some(super::wrap::WrapState::compute(
        &app.textarea.lines().iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        text_width,
    ));

    // Cursor starts at (0, 0) — first visual row
    assert_eq!(app.textarea.cursor(), (0, 0));

    // Press Down — should stay on logical line 0, move to second visual row
    app.handle_event(key_event(KeyCode::Down));
    let (row, _col) = app.textarea.cursor();
    assert_eq!(row, 0, "Down should stay on same logical line within wrapped content");

    // Press Up — should go back to first visual row position
    app.handle_event(key_event(KeyCode::Up));
    assert_eq!(app.textarea.cursor().0, 0, "Up should stay on same logical line");
}

#[test]
fn visual_cursor_navigates_across_logical_lines() {
    let content = "short\nanother short";
    let (mut app, _tmp) = app_with_content(content);
    setup_viewport(&mut app, 80, 20);
    let text_width = (app.content_area.width as usize).saturating_sub(app.gutter_width() as usize);
    app.wrap_state = Some(super::wrap::WrapState::compute(
        &app.textarea.lines().iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        text_width,
    ));

    assert_eq!(app.textarea.cursor(), (0, 0));
    app.handle_event(key_event(KeyCode::Down));
    assert_eq!(app.textarea.cursor().0, 1, "Down should move to next logical line");
    app.handle_event(key_event(KeyCode::Up));
    assert_eq!(app.textarea.cursor().0, 0, "Up should move back to first line");
}
