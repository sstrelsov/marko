use std::io::Write;

use marko::app::{App, Mode};
use ratatui::{
    backend::TestBackend,
    buffer::Buffer,
    style::Color,
    Terminal,
};
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

/// Creates an App with a named file inside a TempDir.
fn app_with_named_file(content: &str, filename: &str) -> (App<'static>, TempDir) {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join(filename);
    std::fs::write(&file_path, content).unwrap();
    let app = App::new(file_path);
    (app, dir)
}

/// Renders the app into a TestBackend buffer and returns the buffer for inspection.
fn render_app(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    terminal.backend().buffer().clone()
}

/// Extracts the text content of a single row from the buffer (stripping trailing spaces).
fn buffer_line_text(buf: &Buffer, row: u16) -> String {
    let width = buf.area.width;
    let mut text = String::new();
    for col in 0..width {
        if let Some(cell) = buf.cell((col, row)) {
            text.push_str(cell.symbol());
        }
    }
    text.trim_end().to_string()
}

/// Returns the foreground color of a specific cell.
fn cell_fg(buf: &Buffer, x: u16, y: u16) -> Color {
    buf.cell((x, y)).unwrap().fg
}

/// Returns the background color of a specific cell.
fn cell_bg(buf: &Buffer, x: u16, y: u16) -> Color {
    buf.cell((x, y)).unwrap().bg
}

/// Searches the entire buffer for a substring and returns true if found.
fn buffer_contains(buf: &Buffer, needle: &str) -> bool {
    let height = buf.area.height;
    for row in 0..height {
        let line = buffer_line_text(buf, row);
        if line.contains(needle) {
            return true;
        }
    }
    false
}

// ═══════════════════════════════════════════════════════════════════════
// A. Header Rendering
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn header_shows_filename() {
    let (mut app, _dir) = app_with_named_file("hello", "myfile.md");
    let buf = render_app(&mut app, 80, 24);
    let header = buffer_line_text(&buf, 0);
    assert!(
        header.contains("myfile.md"),
        "Header should contain filename, got: '{}'",
        header
    );
}

#[test]
fn header_shows_modified_indicator_when_modified() {
    let (mut app, _dir) = app_with_named_file("hello", "test.md");
    app.modified = true;
    let buf = render_app(&mut app, 80, 24);
    let header = buffer_line_text(&buf, 0);
    // The bullet character U+2022 (•) is the modified indicator
    assert!(
        header.contains('\u{2022}'),
        "Header should show modified indicator (•), got: '{}'",
        header
    );
}

#[test]
fn header_no_modified_indicator_when_clean() {
    let (mut app, _dir) = app_with_named_file("hello", "test.md");
    assert!(!app.modified);
    let buf = render_app(&mut app, 80, 24);
    let header = buffer_line_text(&buf, 0);
    assert!(
        !header.contains('\u{2022}'),
        "Header should NOT show modified indicator, got: '{}'",
        header
    );
}

#[test]
fn header_shows_tab_labels() {
    let (mut app, _tmp) = app_with_content("hello");
    let buf = render_app(&mut app, 80, 24);
    let header = buffer_line_text(&buf, 0);
    assert!(header.contains("EDITOR"), "Header should show EDITOR tab");
    assert!(header.contains("PREVIEW"), "Header should show PREVIEW tab");
}

#[test]
fn header_active_tab_has_correct_color_editor() {
    let (mut app, _tmp) = app_with_content("hello");
    assert_eq!(app.mode, Mode::Editor);
    let buf = render_app(&mut app, 80, 24);
    // Find "EDITOR" text in header and check its background color
    let header = buffer_line_text(&buf, 0);
    let editor_start = header.find("EDITOR").expect("EDITOR tab not found in header");
    let bg = cell_bg(&buf, editor_start as u16, 0);
    assert_eq!(bg, Color::Blue, "Active EDITOR tab should have blue background");
}

#[test]
fn header_active_tab_has_correct_color_preview() {
    let (mut app, _tmp) = app_with_content("hello");
    app.mode = Mode::Preview;
    let buf = render_app(&mut app, 80, 24);
    let header = buffer_line_text(&buf, 0);
    let preview_start = header.find("PREVIEW").expect("PREVIEW tab not found");
    let bg = cell_bg(&buf, preview_start as u16, 0);
    assert_eq!(bg, Color::Blue, "Active PREVIEW tab should have blue background");
}

#[test]
fn header_rename_input_replaces_filename() {
    let (mut app, _dir) = app_with_named_file("hello", "original.md");
    app.renaming = true;
    app.rename_buf = "newname.md".to_string();
    app.rename_cursor = app.rename_buf.len();
    let buf = render_app(&mut app, 80, 24);
    let header = buffer_line_text(&buf, 0);
    assert!(
        header.contains("newname.md"),
        "Header should show rename input, got: '{}'",
        header
    );
}

// ═══════════════════════════════════════════════════════════════════════
// B. Editor Rendering
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn editor_shows_line_numbers() {
    let (mut app, _tmp) = app_with_content("line one\nline two\nline three");
    let buf = render_app(&mut app, 80, 24);
    // Line numbers should appear in the content area (row 2+, after header+divider)
    // tui-textarea renders line numbers starting from 1
    let line1 = buffer_line_text(&buf, 2);
    assert!(
        line1.contains("1"),
        "Should show line number 1, got: '{}'",
        line1
    );
}

#[test]
fn editor_shows_content_text() {
    let (mut app, _tmp) = app_with_content("hello world");
    let buf = render_app(&mut app, 80, 24);
    let line1 = buffer_line_text(&buf, 2);
    assert!(
        line1.contains("hello world"),
        "Should show content text, got: '{}'",
        line1
    );
}

#[test]
fn editor_shows_tilde_markers_for_empty_lines() {
    let (mut app, _tmp) = app_with_content("one line");
    // Render with enough height so there are empty lines below the content
    let buf = render_app(&mut app, 80, 10);
    // Content is 1 line, header+divider are rows 0-1, content starts row 2
    // So row 3 (after the single content line at row 2) should have a tilde
    let row = buffer_line_text(&buf, 4);
    assert!(
        row.contains('~'),
        "Empty lines should show tilde markers, got: '{}'",
        row
    );
}

#[test]
fn editor_tilde_has_correct_color() {
    let (mut app, _tmp) = app_with_content("one");
    let buf = render_app(&mut app, 80, 10);
    // Find a row with a tilde (should be row 3+, after header+divider+content)
    for row in 3..8 {
        let text = buffer_line_text(&buf, row);
        if text.contains('~') {
            // Find the column of the tilde
            let col = text.find('~').unwrap() as u16;
            let fg = cell_fg(&buf, col, row);
            assert_eq!(fg, Color::DarkGray, "Tilde should be gray");
            return;
        }
    }
    panic!("No tilde marker found in empty lines");
}

// ═══════════════════════════════════════════════════════════════════════
// C. Status Bar Rendering
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn status_bar_shows_cursor_position() {
    let (mut app, _tmp) = app_with_content("hello");
    let buf = render_app(&mut app, 80, 24);
    // Status bar is the last row
    let status = buffer_line_text(&buf, 23);
    assert!(
        status.contains("Ln 1, Col 0"),
        "Status bar should show cursor position, got: '{}'",
        status
    );
}

#[test]
fn status_bar_shows_status_message() {
    let (mut app, _tmp) = app_with_content("hello");
    app.status_message = "Test message".to_string();
    app.status_time = Some(std::time::Instant::now());
    let buf = render_app(&mut app, 80, 24);
    let status = buffer_line_text(&buf, 23);
    assert!(
        status.contains("Test message"),
        "Status bar should show message, got: '{}'",
        status
    );
}

#[test]
fn status_bar_shows_word_count_and_save_status() {
    let (mut app, _tmp) = app_with_content("hello world foo");
    let buf = render_app(&mut app, 80, 24);
    let status = buffer_line_text(&buf, 23);
    assert!(
        status.contains("3 words"),
        "Status bar should show word count, got: '{}'",
        status
    );
    assert!(
        status.contains("Saved"),
        "Status bar should show save status, got: '{}'",
        status
    );
}

#[test]
fn status_bar_has_correct_background() {
    let (mut app, _tmp) = app_with_content("hello");
    let buf = render_app(&mut app, 80, 24);
    // Status bar is row 23 (last row)
    let bg = cell_bg(&buf, 5, 23);
    assert_eq!(bg, Color::Reset, "Status bar should have terminal default background");
}

// ═══════════════════════════════════════════════════════════════════════
// D. Help Modal Rendering
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn help_modal_renders_when_show_help_true() {
    let (mut app, _tmp) = app_with_content("hello");
    app.show_help = true;
    let buf = render_app(&mut app, 80, 30);
    assert!(
        buffer_contains(&buf, "Keybindings"),
        "Help modal should contain 'Keybindings' header"
    );
}

#[test]
fn help_modal_contains_keybinding_entries() {
    let (mut app, _tmp) = app_with_content("hello");
    app.show_help = true;
    let buf = render_app(&mut app, 80, 30);
    assert!(buffer_contains(&buf, "Ctrl+S"), "Help should list Ctrl+S");
    assert!(buffer_contains(&buf, "Ctrl+Q"), "Help should list Ctrl+Q");
    assert!(buffer_contains(&buf, "Tab"), "Help should list Tab");
    assert!(buffer_contains(&buf, "Esc"), "Help should list Esc");
    assert!(buffer_contains(&buf, "Back to editor"), "Help should describe Esc function");
    assert!(!buffer_contains(&buf, "Esc+S"), "Help should NOT contain Esc+S");
    assert!(!buffer_contains(&buf, "Esc+Q"), "Help should NOT contain Esc+Q");
}

#[test]
fn help_modal_not_visible_when_show_help_false() {
    let (mut app, _tmp) = app_with_content("hello");
    app.show_help = false;
    let buf = render_app(&mut app, 80, 30);
    // "Keybindings" header text should not be in the buffer
    assert!(
        !buffer_contains(&buf, "Keybindings"),
        "Help modal should NOT be visible"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// E. Mode-Specific Content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn preview_mode_renders_markdown_not_raw_text() {
    let (mut app, _tmp) = app_with_content("# Hello World\n\nSome **bold** text.");
    app.mode = Mode::Preview;
    let buf = render_app(&mut app, 80, 24);
    // In preview, the heading should be rendered (pulldown-cmark processes it)
    // The raw "# " prefix may or may not appear (renderer adds "# " prefix for headings)
    assert!(
        buffer_contains(&buf, "Hello World"),
        "Preview should render the heading text"
    );
}

#[test]
fn switching_modes_changes_content_area() {
    let (mut app, _tmp) = app_with_content("# Test\n\nContent");

    // Render in editor mode
    let buf_editor = render_app(&mut app, 80, 24);
    let editor_row2 = buffer_line_text(&buf_editor, 2);

    // Switch to preview
    app.mode = Mode::Preview;
    let buf_preview = render_app(&mut app, 80, 24);
    let preview_row2 = buffer_line_text(&buf_preview, 2);

    // Content should differ between modes (editor shows raw text with line numbers,
    // preview shows rendered markdown)
    // At minimum, the raw lines won't have tui-textarea line numbers in preview
    assert_ne!(
        editor_row2, preview_row2,
        "Content should differ between Editor and Preview modes"
    );
}

#[test]
fn render_sets_viewport_height() {
    // Generate long content so preview content_height > viewport_height
    let long_content = (0..100).map(|i| format!("Line {}", i)).collect::<Vec<_>>().join("\n");
    let (mut app, _tmp) = app_with_content(&long_content);
    app.mode = Mode::Preview;
    // Render to set viewport_height and content_height from actual preview rendering
    let _ = render_app(&mut app, 80, 24);
    // After rendering with height 24: header=1, divider=1, content=20, divider=1, status=1
    // viewport_height should be 20
    // PageDown scrolls by (viewport_height - 2) = 18
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    app.handle_event(Event::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)));
    assert_eq!(
        app.preview.scroll_offset, 18,
        "After render, viewport_height should be set (24 - header - 2*divider - status = 20, page = 20-2 = 18)"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// F. Flicker Regression
// ═══════════════════════════════════════════════════════════════════════

/// Backend wrapper that counts how many cells are written per draw() call.
/// Used to detect unnecessary full repaints (flicker).
struct TrackingBackend {
    inner: TestBackend,
    last_draw_count: usize,
}

impl TrackingBackend {
    fn new(width: u16, height: u16) -> Self {
        Self {
            inner: TestBackend::new(width, height),
            last_draw_count: 0,
        }
    }
}

impl ratatui::backend::Backend for TrackingBackend {
    fn draw<'a, I>(&mut self, content: I) -> std::io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
    {
        let cells: Vec<_> = content.collect();
        self.last_draw_count = cells.len();
        self.inner.draw(cells.into_iter())
    }

    fn hide_cursor(&mut self) -> std::io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> std::io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> std::io::Result<ratatui::layout::Position> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<ratatui::layout::Position>>(
        &mut self,
        position: P,
    ) -> std::io::Result<()> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> std::io::Result<()> {
        self.inner.clear()
    }

    fn size(&self) -> std::io::Result<ratatui::layout::Size> {
        self.inner.size()
    }

    fn window_size(&mut self) -> std::io::Result<ratatui::backend::WindowSize> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[test]
fn render_frame_idle_writes_zero_cells() {
    // Regression test for flickering. ratatui diffs each frame against
    // the previous one and only writes changed cells. On an idle frame
    // (no state changes), the diff should be empty: 0 cells written.
    //
    // If render_frame() ever calls terminal.resize() or terminal.clear(),
    // the diff baseline is wiped and ALL cells get rewritten every frame
    // — visible as flickering. This test catches that: it would see
    // ~1920 cells written instead of 0.
    let (mut app, _tmp) = app_with_content("hello world");
    let mut terminal = Terminal::new(TrackingBackend::new(80, 24)).unwrap();

    // Frame 1: initial draw — all cells are new
    app.render_frame(&mut terminal).unwrap();
    assert!(
        terminal.backend().last_draw_count > 0,
        "First frame should write cells",
    );

    // Frame 2: no state changes — diff should be empty
    app.render_frame(&mut terminal).unwrap();
    assert_eq!(
        terminal.backend().last_draw_count,
        0,
        "Idle frame wrote {} cells instead of 0 — resize/clear is causing full repaint (flicker)",
        terminal.backend().last_draw_count
    );
}
