use std::io::Write;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use marko::app::{App, Mode};
use tempfile::{NamedTempFile, TempDir};

// ─── Helpers ─────────────────────────────────────────────────────────────

/// Creates an App backed by a temp file with the given content.
fn app_with_content(content: &str) -> (App<'static>, NamedTempFile) {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(content.as_bytes()).unwrap();
    tmp.flush().unwrap();
    let app = App::new(tmp.path().to_path_buf());
    (app, tmp)
}

/// Creates an App with a file inside a TempDir (for rename tests).
fn app_in_tempdir(content: &str) -> (App<'static>, TempDir) {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("test.md");
    std::fs::write(&file_path, content).unwrap();
    let app = App::new(file_path);
    (app, dir)
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn ctrl_char(ch: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL))
}

fn char_key(ch: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE))
}

fn ctrl_shift_key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(
        code,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ))
}

// ═══════════════════════════════════════════════════════════════════════
// A. Global Keybindings
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ctrl_q_quits_from_editor() {
    let (mut app, _tmp) = app_with_content("hello");
    assert_eq!(app.mode, Mode::Editor);
    app.handle_event(ctrl_char('q'));
    assert!(app.should_quit);
}

#[test]
fn ctrl_q_saves_modified_content_before_quitting() {
    let (mut app, tmp) = app_with_content("hello");
    // Modify content
    app.handle_event(char_key('x'));
    assert!(app.modified);
    // Ctrl+Q should save then quit
    app.handle_event(ctrl_char('q'));
    assert!(app.should_quit);
    assert!(!app.modified);
    // Verify file was persisted to disk
    let on_disk = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(on_disk.contains('x'));
}

#[test]
fn ctrl_q_quits_from_preview() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(key(KeyCode::Tab)); // switch to Preview
    assert_eq!(app.mode, Mode::Preview);
    app.handle_event(ctrl_char('q'));
    assert!(app.should_quit);
}

#[test]
fn ctrl_s_saves_from_editor() {
    let (mut app, tmp) = app_with_content("hello");
    // Modify content
    app.handle_event(char_key('x'));
    assert!(app.modified);
    app.handle_event(ctrl_char('s'));
    assert!(!app.modified);
    // Verify file content on disk
    let on_disk = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(on_disk.contains('x'));
}

#[test]
fn ctrl_s_saves_from_preview() {
    let (mut app, _tmp) = app_with_content("hello");
    // Modify in editor first
    app.handle_event(char_key('x'));
    assert!(app.modified);
    // Switch to Preview, then save
    app.handle_event(key(KeyCode::Tab));
    assert_eq!(app.mode, Mode::Preview);
    app.handle_event(ctrl_char('s'));
    assert!(!app.modified);
}

#[test]
fn ctrl_s_sets_status_message() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('s'));
    assert_eq!(app.status_message, "Saved");
}

#[test]
fn ctrl_t_starts_rename_from_editor() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    assert!(app.renaming);
    assert!(!app.rename_buf.is_empty());
}

#[test]
fn ctrl_t_starts_rename_from_preview() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(key(KeyCode::Tab));
    assert_eq!(app.mode, Mode::Preview);
    app.handle_event(ctrl_char('t'));
    assert!(app.renaming);
}

#[test]
fn f1_shows_help() {
    let (mut app, _tmp) = app_with_content("hello");
    assert!(!app.show_help);
    app.handle_event(key(KeyCode::F(1)));
    assert!(app.show_help);
}

#[test]
fn tab_toggles_editor_preview() {
    let (mut app, _tmp) = app_with_content("hello");
    assert_eq!(app.mode, Mode::Editor);
    app.handle_event(key(KeyCode::Tab));
    assert_eq!(app.mode, Mode::Preview);
    app.handle_event(key(KeyCode::Tab));
    assert_eq!(app.mode, Mode::Editor);
}

// ═══════════════════════════════════════════════════════════════════════
// C. Help Modal
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn any_key_dismisses_help() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(key(KeyCode::F(1)));
    assert!(app.show_help);
    app.handle_event(char_key('a'));
    assert!(!app.show_help);
}

#[test]
fn help_swallows_ctrl_q() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(key(KeyCode::F(1)));
    assert!(app.show_help);
    // Ctrl+Q while help is open should dismiss help, NOT quit
    app.handle_event(ctrl_char('q'));
    assert!(!app.show_help);
    assert!(!app.should_quit);
}

#[test]
fn f1_dismisses_help() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(key(KeyCode::F(1)));
    assert!(app.show_help);
    app.handle_event(key(KeyCode::F(1)));
    assert!(!app.show_help);
}

#[test]
fn esc_dismisses_help_without_mode_switch() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(key(KeyCode::Tab)); // → Preview
    assert_eq!(app.mode, Mode::Preview);
    app.handle_event(key(KeyCode::F(1)));
    assert!(app.show_help);
    app.handle_event(key(KeyCode::Esc));
    assert!(!app.show_help);
    // Help swallowed the Esc — mode should still be Preview
    assert_eq!(app.mode, Mode::Preview);
}

#[test]
fn esc_returns_to_editor_from_preview() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(key(KeyCode::Tab));
    assert_eq!(app.mode, Mode::Preview);
    app.handle_event(key(KeyCode::Esc));
    assert_eq!(app.mode, Mode::Editor);
}

#[test]
fn esc_is_noop_in_editor() {
    let (mut app, _tmp) = app_with_content("hello");
    assert_eq!(app.mode, Mode::Editor);
    app.handle_event(key(KeyCode::Esc));
    assert_eq!(app.mode, Mode::Editor);
    assert!(!app.should_quit);
}

#[test]
fn esc_does_not_quit() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(key(KeyCode::Esc));
    assert!(!app.should_quit);
    app.handle_event(key(KeyCode::Esc));
    assert!(!app.should_quit);
}

// ═══════════════════════════════════════════════════════════════════════
// D. Rename Mode
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn rename_esc_cancels() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    assert!(app.renaming);
    app.handle_event(key(KeyCode::Esc));
    assert!(!app.renaming);
}

#[test]
fn rename_enter_with_new_name_renames_file() {
    let (mut app, dir) = app_in_tempdir("hello");
    let old_path = app.file_path.clone();
    app.handle_event(ctrl_char('t'));
    assert!(app.renaming);

    // Clear rename buf and type new name
    // First, select all and delete using Home + shift to end pattern
    // Easier: just manipulate rename_buf directly since the typing tests cover that
    app.rename_buf = "renamed.md".to_string();
    app.rename_cursor = app.rename_buf.len();
    app.handle_event(key(KeyCode::Enter));

    assert!(!app.renaming);
    assert!(!old_path.exists());
    assert!(dir.path().join("renamed.md").exists());
    assert_eq!(app.file_path, dir.path().join("renamed.md"));
}

#[test]
fn rename_enter_with_empty_name_shows_cancelled() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    app.rename_buf.clear();
    app.rename_cursor = 0;
    app.handle_event(key(KeyCode::Enter));
    assert!(!app.renaming);
    assert!(app.status_message.contains("cancelled"));
}

#[test]
fn rename_enter_with_same_name_silently_exits() {
    let (mut app, _tmp) = app_with_content("hello");
    let original_name = app
        .file_path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    app.handle_event(ctrl_char('t'));
    assert_eq!(app.rename_buf, original_name);
    app.handle_event(key(KeyCode::Enter));
    assert!(!app.renaming);
}

#[test]
fn rename_backspace_at_start_is_noop() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    app.rename_cursor = 0;
    let buf_before = app.rename_buf.clone();
    app.handle_event(key(KeyCode::Backspace));
    assert_eq!(app.rename_buf, buf_before);
    assert_eq!(app.rename_cursor, 0);
}

#[test]
fn rename_delete_at_end_is_noop() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    let end = app.rename_buf.len();
    app.rename_cursor = end;
    let buf_before = app.rename_buf.clone();
    app.handle_event(key(KeyCode::Delete));
    assert_eq!(app.rename_buf, buf_before);
}

#[test]
fn rename_left_right_navigation() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    let end = app.rename_buf.len();
    assert_eq!(app.rename_cursor, end);

    app.handle_event(key(KeyCode::Left));
    assert_eq!(app.rename_cursor, end - 1);
    app.handle_event(key(KeyCode::Right));
    assert_eq!(app.rename_cursor, end);
}

#[test]
fn rename_home_end_navigation() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    app.handle_event(key(KeyCode::Home));
    assert_eq!(app.rename_cursor, 0);
    app.handle_event(key(KeyCode::End));
    assert_eq!(app.rename_cursor, app.rename_buf.len());
}

#[test]
fn rename_forward_slash_rejected() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    let buf_before = app.rename_buf.clone();
    app.handle_event(char_key('/'));
    assert_eq!(app.rename_buf, buf_before);
}

#[test]
fn rename_backslash_rejected() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    let buf_before = app.rename_buf.clone();
    app.handle_event(char_key('\\'));
    assert_eq!(app.rename_buf, buf_before);
}

#[test]
fn rename_unicode_chars_accepted() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    app.rename_buf = "test".to_string();
    app.rename_cursor = 4;
    app.handle_event(char_key('é'));
    assert_eq!(app.rename_buf, "testé");
    assert_eq!(app.rename_cursor, 5);
}

#[test]
fn rename_char_insertion_at_mid_cursor() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    app.rename_buf = "abcd".to_string();
    app.rename_cursor = 2;
    app.handle_event(char_key('X'));
    assert_eq!(app.rename_buf, "abXcd");
    assert_eq!(app.rename_cursor, 3);
}

#[test]
fn rename_right_at_end_is_noop() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    let end = app.rename_buf.len();
    app.rename_cursor = end;
    app.handle_event(key(KeyCode::Right));
    assert_eq!(app.rename_cursor, end);
}

#[test]
fn rename_left_at_start_is_noop() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    app.rename_cursor = 0;
    app.handle_event(key(KeyCode::Left));
    assert_eq!(app.rename_cursor, 0);
}

#[test]
fn rename_backspace_deletes_char_before_cursor() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    app.rename_buf = "abcd".to_string();
    app.rename_cursor = 2;
    app.handle_event(key(KeyCode::Backspace));
    assert_eq!(app.rename_buf, "acd");
    assert_eq!(app.rename_cursor, 1);
}

#[test]
fn rename_delete_removes_char_at_cursor() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    app.rename_buf = "abcd".to_string();
    app.rename_cursor = 1;
    app.handle_event(key(KeyCode::Delete));
    assert_eq!(app.rename_buf, "acd");
    assert_eq!(app.rename_cursor, 1);
}

// ═══════════════════════════════════════════════════════════════════════
// E. Editor Keybindings
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn editor_ctrl_z_undo() {
    let (mut app, _tmp) = app_with_content("hello");
    // Type a char to create undo history
    app.handle_event(char_key('x'));
    assert!(app.textarea.lines()[0].starts_with('x'));
    app.handle_event(ctrl_char('z'));
    assert_eq!(app.textarea.lines()[0], "hello");
}

#[test]
fn editor_ctrl_y_redo() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(char_key('x'));
    app.handle_event(ctrl_char('z')); // undo
    assert_eq!(app.textarea.lines()[0], "hello");
    app.handle_event(ctrl_char('y')); // redo
    assert!(app.textarea.lines()[0].starts_with('x'));
}

#[test]
fn editor_ctrl_shift_z_redo() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(char_key('x'));
    app.handle_event(ctrl_char('z')); // undo
    assert_eq!(app.textarea.lines()[0], "hello");
    app.handle_event(ctrl_shift_key(KeyCode::Char('Z')));
    assert!(app.textarea.lines()[0].starts_with('x'));
}

#[test]
fn editor_ctrl_z_with_nothing_to_undo_no_crash() {
    let (mut app, _tmp) = app_with_content("hello");
    // No edits yet — undo should be a no-op
    app.handle_event(ctrl_char('z'));
    assert_eq!(app.textarea.lines()[0], "hello");
}

#[test]
fn editor_ctrl_y_with_nothing_to_redo_no_crash() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('y'));
    assert_eq!(app.textarea.lines()[0], "hello");
}

#[test]
fn editor_ctrl_a_select_all() {
    let (mut app, _tmp) = app_with_content("hello\nworld");
    app.handle_event(ctrl_char('a'));
    let range = app.textarea.selection_range();
    assert!(range.is_some());
    let ((sr, sc), (er, ec)) = range.unwrap();
    assert_eq!((sr, sc), (0, 0));
    assert_eq!(er, 1); // second line
    assert_eq!(ec, 5); // "world".len()
}

#[test]
fn editor_ctrl_l_goes_to_line_start() {
    let (mut app, _tmp) = app_with_content("hello\nworld\nfoo");
    // Move cursor to end of first line
    app.handle_event(key(KeyCode::End));
    assert_eq!(app.textarea.cursor().1, 5);
    app.handle_event(ctrl_char('l'));
    assert_eq!(app.textarea.cursor().1, 0, "Ctrl+L should move to column 0");
    // Should not have a selection
    assert!(app.textarea.selection_range().is_none());
}

#[test]
fn editor_ctrl_l_on_empty_file_no_crash() {
    let (mut app, _tmp) = app_with_content("");
    app.handle_event(ctrl_char('l'));
    assert_eq!(app.textarea.cursor(), (0, 0));
}

#[test]
fn editor_ctrl_h_delete_word_backward() {
    let (mut app, _tmp) = app_with_content("hello world");
    // Move cursor to end of line
    app.handle_event(key(KeyCode::End));
    app.handle_event(ctrl_char('h'));
    // "world" should be deleted
    let line = &app.textarea.lines()[0];
    assert!(!line.contains("world"));
}

#[test]
fn editor_ctrl_d_delete_word_forward() {
    let (mut app, _tmp) = app_with_content("hello world");
    // Cursor at start
    app.handle_event(ctrl_char('d'));
    // "hello" should be deleted (or partially)
    let line = &app.textarea.lines()[0];
    assert!(!line.starts_with("hello"));
}

#[test]
fn typing_sets_modified_flag() {
    let (mut app, _tmp) = app_with_content("hello");
    assert!(!app.modified);
    app.handle_event(char_key('x'));
    assert!(app.modified);
}

// ═══════════════════════════════════════════════════════════════════════
// F. Editor Passthrough
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn arrow_keys_move_cursor() {
    let (mut app, _tmp) = app_with_content("hello\nworld");
    let (start_row, _) = app.textarea.cursor();
    app.handle_event(key(KeyCode::Down));
    let (new_row, _) = app.textarea.cursor();
    assert_eq!(new_row, start_row + 1);
}

#[test]
fn enter_inserts_newline() {
    let (mut app, _tmp) = app_with_content("hello");
    let lines_before = app.textarea.lines().len();
    app.handle_event(key(KeyCode::Enter));
    assert_eq!(app.textarea.lines().len(), lines_before + 1);
}

#[test]
fn backspace_deletes_character() {
    let (mut app, _tmp) = app_with_content("hello");
    // Move to end of line, then backspace
    app.handle_event(key(KeyCode::End));
    app.handle_event(key(KeyCode::Backspace));
    assert_eq!(app.textarea.lines()[0], "hell");
}

#[test]
fn home_end_navigation() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(key(KeyCode::End));
    let (_, col) = app.textarea.cursor();
    assert_eq!(col, 5); // "hello".len()
    app.handle_event(key(KeyCode::Home));
    let (_, col) = app.textarea.cursor();
    assert_eq!(col, 0);
}

#[test]
fn passthrough_updates_modified_flag() {
    let (mut app, _tmp) = app_with_content("hello");
    assert!(!app.modified);
    app.handle_event(key(KeyCode::Enter));
    assert!(app.modified);
}

#[test]
fn delete_key_works() {
    let (mut app, _tmp) = app_with_content("hello");
    // Cursor at start, delete should remove 'h'
    app.handle_event(key(KeyCode::Delete));
    assert_eq!(app.textarea.lines()[0], "ello");
}

// ═══════════════════════════════════════════════════════════════════════
// H. Paste Handling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn paste_in_editor_inserts_text_and_sets_modified() {
    let (mut app, _tmp) = app_with_content("hello");
    assert!(!app.modified);
    app.handle_event(Event::Paste("world".to_string()));
    assert!(app.modified);
    assert!(app.textarea.lines()[0].contains("world"));
}

#[test]
fn paste_in_rename_inserts_chars_strips_newlines() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(ctrl_char('t'));
    app.rename_buf = "test".to_string();
    app.rename_cursor = 4;
    app.handle_event(Event::Paste("new\nname\r".to_string()));
    assert_eq!(app.rename_buf, "testnewname");
}

#[test]
fn paste_in_preview_is_ignored() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(key(KeyCode::Tab)); // Switch to Preview
    assert_eq!(app.mode, Mode::Preview);
    app.handle_event(Event::Paste("pasted".to_string()));
    assert!(!app.modified);
}

#[test]
fn paste_empty_string_in_editor() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(Event::Paste(String::new()));
    // Empty paste should not set modified (content hasn't changed)
    assert!(!app.modified);
}

// ═══════════════════════════════════════════════════════════════════════
// J. Modified Flag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn initial_state_not_modified() {
    let (app, _tmp) = app_with_content("hello");
    assert!(!app.modified);
}

#[test]
fn typing_sets_modified() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(char_key('x'));
    assert!(app.modified);
}

#[test]
fn undo_back_to_original_clears_modified() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(char_key('x'));
    assert!(app.modified);
    app.handle_event(ctrl_char('z'));
    assert!(!app.modified);
}

#[test]
fn save_clears_modified() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(char_key('x'));
    assert!(app.modified);
    app.handle_event(ctrl_char('s'));
    assert!(!app.modified);
}

#[test]
fn paste_sets_modified() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(Event::Paste("inserted".to_string()));
    assert!(app.modified);
}

#[test]
fn delete_word_sets_modified() {
    let (mut app, _tmp) = app_with_content("hello world");
    app.handle_event(key(KeyCode::End));
    app.handle_event(ctrl_char('h'));
    assert!(app.modified);
}

// ═══════════════════════════════════════════════════════════════════════
// K. Mode Transitions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn preview_scroll_resets_on_entry() {
    let (mut app, _tmp) = app_with_content("hello");
    // Manually set a non-zero scroll offset
    app.preview.scroll_offset = 42;
    // Switch to preview — should reset scroll
    app.handle_event(key(KeyCode::Tab));
    assert_eq!(app.mode, Mode::Preview);
    assert_eq!(app.preview.scroll_offset, 0);
}

#[test]
fn resize_event_ignored_no_crash() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(Event::Resize(120, 40));
    // Just verify no crash, mode unchanged
    assert_eq!(app.mode, Mode::Editor);
}

#[test]
fn focus_gained_ignored_no_crash() {
    let (mut app, _tmp) = app_with_content("hello");
    app.handle_event(Event::FocusGained);
    assert_eq!(app.mode, Mode::Editor);
}

#[test]
fn rapid_mode_switching_correct_final_state() {
    let (mut app, _tmp) = app_with_content("hello");
    // Editor → Preview → Editor → Preview → Editor
    app.handle_event(key(KeyCode::Tab));
    assert_eq!(app.mode, Mode::Preview);
    app.handle_event(key(KeyCode::Tab));
    assert_eq!(app.mode, Mode::Editor);
    app.handle_event(key(KeyCode::Tab));
    assert_eq!(app.mode, Mode::Preview);
    app.handle_event(key(KeyCode::Tab));
    assert_eq!(app.mode, Mode::Editor);
}
