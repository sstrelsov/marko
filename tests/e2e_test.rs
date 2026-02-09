use std::process::Command;
use std::time::Duration;

use expectrl::{Eof, Regex, Session};
use tempfile::TempDir;

// ─── Raw byte constants (what iTerm2/macOS actually sends) ──────────────

const CTRL_Q: &[u8] = b"\x11";      // Ctrl+Q
const CTRL_S: &[u8] = b"\x13";      // Ctrl+S
const CTRL_T: &[u8] = b"\x14";      // Ctrl+T
const CTRL_H: &[u8] = b"\x08";      // Ctrl+H (Ctrl+Backspace on macOS)
const ESC: &[u8] = b"\x1b";         // Escape
const TAB: &[u8] = b"\x09";         // Tab
const BACKTAB: &[u8] = b"\x1b[Z";   // Shift+Tab
const ENTER: &[u8] = b"\r";         // Enter/Return
const BACKSPACE: &[u8] = b"\x7f";   // Backspace (iTerm2 default = DEL)
const F1: &[u8] = b"\x1bOP";        // F1

// ─── Helpers ─────────────────────────────────────────────────────────────

fn spawn_marko(content: &str) -> (Session, TempDir) {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("test.md");
    std::fs::write(&file, content).unwrap();

    let bin = env!("CARGO_BIN_EXE_marko");
    let mut cmd = Command::new(bin);
    cmd.arg(file.to_str().unwrap());
    cmd.env("TERM", "xterm-256color");

    let mut session = Session::spawn(cmd).expect("Failed to spawn marko");
    session.set_expect_timeout(Some(Duration::from_secs(5)));
    (session, dir)
}

/// Small delay to let the TUI render.
fn short_delay() {
    std::thread::sleep(Duration::from_millis(200));
}

/// Send bytes and wait a moment for the TUI to process.
fn send_and_wait(session: &mut Session, bytes: &[u8]) {
    session.send(bytes).expect("Failed to send bytes");
    short_delay();
}

/// Cleanly quit the marko process.
fn quit(session: &mut Session) {
    send_and_wait(session, CTRL_Q);
    // Wait for EOF (process exit)
    let _ = session.expect(Eof);
}

// ═══════════════════════════════════════════════════════════════════════
// A. App Lifecycle
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn app_launches_shows_filename_and_ctrl_q_exits() {
    let (mut session, _dir) = spawn_marko("hello world");
    // Wait for the app to render and show the filename
    session
        .expect(Regex("test\\.md"))
        .expect("Should see filename 'test.md' in output");
    // Quit
    send_and_wait(&mut session, CTRL_Q);
    let _ = session.expect(Eof);
}

#[test]
fn app_type_and_save_persists_to_disk() {
    let (mut session, dir) = spawn_marko("initial");
    short_delay();
    // Type some text
    session.send(b"ADDED").expect("send text");
    short_delay();
    // Save with Ctrl+S
    send_and_wait(&mut session, CTRL_S);
    // Verify "Saved" appears in status bar
    session
        .expect("Saved")
        .expect("Should see 'Saved' status message");
    // Quit
    quit(&mut session);
    // Verify file on disk contains the added text
    let content = std::fs::read_to_string(dir.path().join("test.md")).unwrap();
    assert!(
        content.contains("ADDED"),
        "File should contain typed text, got: '{}'",
        content
    );
}

#[test]
fn app_shows_initial_status_message() {
    let (mut session, _dir) = spawn_marko("hello");
    // The initial status message includes "F1"
    session
        .expect(Regex("F1"))
        .expect("Should show initial status message containing F1");
    quit(&mut session);
}

// ═══════════════════════════════════════════════════════════════════════
// B. Key Encoding Correctness (iTerm2/macOS-specific)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ctrl_h_deletes_word_backward() {
    let (mut session, dir) = spawn_marko("hello world");
    short_delay();
    // Move cursor to end of line: send End key (ESC[F)
    session.send(b"\x1b[F").expect("send End");
    short_delay();
    // Ctrl+H (0x08) should delete word backward
    send_and_wait(&mut session, CTRL_H);
    // Save to check result
    send_and_wait(&mut session, CTRL_S);
    quit(&mut session);
    let content = std::fs::read_to_string(dir.path().join("test.md")).unwrap();
    // "world" should be deleted (or partially)
    assert!(
        !content.contains("world"),
        "Ctrl+H should delete word backward, file still contains 'world': '{}'",
        content
    );
}

#[test]
fn backspace_0x7f_deletes_one_character() {
    let (mut session, dir) = spawn_marko("abcde");
    short_delay();
    // Move to end of line
    session.send(b"\x1b[F").expect("send End");
    short_delay();
    // Backspace (0x7F = DEL, iTerm2 default)
    send_and_wait(&mut session, BACKSPACE);
    // Save
    send_and_wait(&mut session, CTRL_S);
    quit(&mut session);
    let content = std::fs::read_to_string(dir.path().join("test.md")).unwrap();
    assert_eq!(
        content.trim(),
        "abcd",
        "Backspace should delete one char, got: '{}'",
        content.trim()
    );
}

#[test]
fn esc_returns_to_editor_from_preview() {
    let (mut session, _dir) = spawn_marko("# Hello");
    short_delay();
    // Tab → Preview mode
    send_and_wait(&mut session, TAB);
    // Send double-Esc so crossterm parses it as a standalone Esc event
    // (single \x1b causes crossterm to wait for more bytes as an escape sequence prefix)
    session.send(b"\x1b\x1b").expect("send Esc");
    short_delay();
    // Type a char to verify we're back in editor mode (typing only works in editor mode)
    session.send(b"Z").expect("send Z");
    short_delay();
    // Save and verify the typed char was inserted (proves we're in editor mode)
    send_and_wait(&mut session, CTRL_S);
    quit(&mut session);
    let content = std::fs::read_to_string(_dir.path().join("test.md")).unwrap();
    assert!(
        content.contains('Z'),
        "After Esc from Preview, typing should work (editor mode), got: '{}'",
        content
    );
}

#[test]
fn esc_does_not_quit_app() {
    let (mut session, _dir) = spawn_marko("hello");
    short_delay();
    // Send double-Esc so crossterm parses as standalone Esc event
    session.send(b"\x1b\x1b").expect("send Esc");
    short_delay();
    assert!(
        session.is_alive().unwrap_or(false),
        "Esc should NOT quit the application"
    );
    // Double Esc — still should not quit
    session.send(b"\x1b\x1b").expect("send Esc again");
    short_delay();
    assert!(
        session.is_alive().unwrap_or(false),
        "Double Esc should NOT quit the application"
    );
    quit(&mut session);
}

#[test]
fn tab_switches_to_preview_mode() {
    let (mut session, _dir) = spawn_marko("# Hello");
    short_delay();
    // Tab should switch to Preview mode
    send_and_wait(&mut session, TAB);
    // The PREVIEW tab should now be highlighted / active
    session
        .expect(Regex("PREVIEW"))
        .expect("Tab should show PREVIEW in header");
    quit(&mut session);
}

// ═══════════════════════════════════════════════════════════════════════
// C. Mode Switching
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn shift_tab_is_noop() {
    let (mut session, _dir) = spawn_marko("hello");
    short_delay();
    // Shift+Tab (ESC[Z) should be a no-op (no Diff mode anymore)
    send_and_wait(&mut session, BACKTAB);
    // App should still be alive and in editor mode
    assert!(
        session.is_alive().unwrap_or(false),
        "App should survive Shift+Tab"
    );
    quit(&mut session);
}

#[test]
fn f1_shows_help_modal() {
    let (mut session, _dir) = spawn_marko("hello");
    short_delay();
    // F1 should show help modal
    send_and_wait(&mut session, F1);
    session
        .expect(Regex("Keybindings"))
        .expect("F1 should show help modal with 'Keybindings'");
    // Any key should dismiss
    send_and_wait(&mut session, b"x");
    quit(&mut session);
}

// ═══════════════════════════════════════════════════════════════════════
// D. Rename Flow
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn rename_flow_confirm() {
    let (mut session, dir) = spawn_marko("hello");
    short_delay();
    // Ctrl+T enters rename mode
    send_and_wait(&mut session, CTRL_T);
    // Clear existing name: send Home then delete forward repeatedly
    // Simpler: send Ctrl+A (select all in rename? no, rename doesn't support that)
    // Actually rename starts with cursor at end of filename "test.md"
    // We need to clear it. Send Home, then delete forward for each char.
    session.send(b"\x1b[H").expect("send Home");
    short_delay();
    // Delete forward 7 chars ("test.md" = 7 chars)
    for _ in 0..7 {
        session.send(b"\x1b[3~").expect("send Delete");
        std::thread::sleep(Duration::from_millis(20));
    }
    // Type new name
    session.send(b"renamed.md").expect("send new name");
    short_delay();
    // Confirm with Enter
    send_and_wait(&mut session, ENTER);
    // Verify file was renamed on disk
    quit(&mut session);
    assert!(
        dir.path().join("renamed.md").exists(),
        "File should be renamed to renamed.md"
    );
    assert!(
        !dir.path().join("test.md").exists(),
        "Original test.md should no longer exist"
    );
}

#[test]
fn rename_flow_cancel_with_esc() {
    let (mut session, dir) = spawn_marko("hello");
    short_delay();
    // Ctrl+T enters rename mode
    send_and_wait(&mut session, CTRL_T);
    // Type something
    session.send(b"xxx").expect("send chars");
    short_delay();
    // Escape cancels
    send_and_wait(&mut session, ESC);
    quit(&mut session);
    // File should still be named test.md
    assert!(
        dir.path().join("test.md").exists(),
        "File should still be test.md after rename cancel"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// E. Error Resilience
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn rapid_key_mashing_does_not_crash() {
    let (mut session, _dir) = spawn_marko("hello world");
    short_delay();
    // Send a burst of random-ish bytes
    let garbage: Vec<u8> = (0..100)
        .map(|i| match i % 5 {
            0 => b'a' + (i % 26) as u8,
            1 => b'\x1b',
            2 => b'[',
            3 => b'A',
            _ => b' ',
        })
        .collect();
    session.send(&garbage).expect("send garbage");
    short_delay();
    // App should still be alive
    assert!(
        session.is_alive().unwrap_or(false),
        "App should survive rapid key mashing"
    );
    quit(&mut session);
}

#[test]
fn resize_escape_sequence_does_not_crash() {
    let (mut session, _dir) = spawn_marko("hello");
    short_delay();
    // Send a window resize ANSI escape (SIGWINCH is usually sent by the terminal,
    // but we can also send CSI 8 ; rows ; cols t)
    session.send(b"\x1b[8;40;100t").expect("send resize");
    short_delay();
    assert!(
        session.is_alive().unwrap_or(false),
        "App should survive resize sequence"
    );
    quit(&mut session);
}
