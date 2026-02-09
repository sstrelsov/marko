use std::collections::HashMap;
use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use tui_textarea::{CursorMove, Input, Key, TextArea};

use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use crate::components::{editor, header, preview, status};
use crate::git::{self, diff::GutterMark, repo::GitRepo};
use crate::markdown::autocomplete::{self, Continuation};
use crate::markdown::code_highlight::{self, CodeFenceRegion};
use crate::markdown::table_format;
use crate::pandoc;
use crate::theme;

/// State for round-trip .docx editing.
pub struct DocxState {
    /// Path to the original .docx file.
    pub docx_path: PathBuf,
    /// Used as --reference-doc when exporting back to .docx.
    pub reference_doc: PathBuf,
}

/// How long status bar messages stay visible before auto-clearing.
const STATUS_DURATION: Duration = Duration::from_secs(3);

/// Lines to scroll per mouse wheel tick in preview mode.
const SCROLL_LINES: u16 = 3;

/// Maximum time between clicks to count as multi-click (double/triple).
const MULTI_CLICK_MS: u64 = 500;

// Header tab widths: " EDITOR " = 8, " PREVIEW " = 9
const TAB_EDITOR_W: u16 = 8;
const TAB_PREVIEW_W: u16 = 9;
const TAB_TOTAL_W: u16 = TAB_EDITOR_W + TAB_PREVIEW_W;

/// Maximum width for the UI content area. Wider terminals get centered, capped layout.
const MAX_WIDTH: u16 = 120;

/// The two top-level view modes, toggled via Tab or header tab clicks.
#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Editor,
    Preview,
}

pub struct App<'a> {
    // --- Core state ---
    pub mode: Mode,
    pub file_path: PathBuf,
    pub textarea: TextArea<'a>,
    pub modified: bool,
    pub original_content: String,
    pub should_quit: bool,

    // --- Docx round-trip state ---
    pub docx_state: Option<DocxState>,

    // --- Mode-specific state ---
    pub preview: preview::PreviewState,

    // --- Git gutter marks ---
    pub gutter_marks: HashMap<usize, GutterMark>,

    // --- Status bar ---
    pub status_message: String,
    pub status_time: Option<Instant>,

    // --- Git integration ---
    pub git_repo: Option<GitRepo>,
    pub git_branch: String,
    pub git_file_status: String,

    // --- Rename mode (Ctrl+T or click filename) ---
    pub renaming: bool,
    pub rename_buf: String,
    pub rename_cursor: usize,

    // --- Help modal (F1) ---
    pub show_help: bool,

    // --- Internal tracking ---
    viewport_height: u16,
    /// Cached content area rect from last render (used for mouse hit-testing).
    content_area: Rect,
    /// Tracks tui-textarea's scroll position for mouse click → buffer position math.
    editor_scroll_top: u16,
    /// True while left mouse button is held down for drag selection.
    mouse_dragging: bool,
    /// Timestamp of last left-click in content area, for double/triple-click detection.
    last_click_time: Option<Instant>,
    /// Terminal position of last click, for multi-click detection.
    last_click_pos: (u16, u16),
    /// Click count (1=single, 2=double, 3=triple), resets on timeout or position change.
    click_count: u8,

    // --- Background initialization ---
    gutter_handle: Option<JoinHandle<HashMap<usize, GutterMark>>>,

    // --- Syntax highlighting cache ---
    code_fence_regions: Vec<CodeFenceRegion>,
    /// Pre-computed highlight spans per region, per line: [region_idx][line_offset] -> spans.
    code_fence_highlights: Vec<Vec<Vec<(ratatui::style::Color, String)>>>,
    code_fence_dirty: bool,
}

/// Classifies a character for word-boundary detection (double-click selection).
/// Same class = same "word". Classes: 0=word, 1=whitespace, 2=punctuation.
fn char_class(c: char) -> u8 {
    if c.is_alphanumeric() || c == '_' {
        0
    } else if c.is_whitespace() {
        1
    } else {
        2
    }
}

/// Pre-computes syntax highlighting for all code fence regions.
/// Returns a parallel vec: [region_idx][line_offset] -> Vec<(fg_color, text)>.
fn highlight_code_regions(
    regions: &[CodeFenceRegion],
    lines: &[String],
    syntax_set: &SyntaxSet,
    theme_set: &ThemeSet,
) -> Vec<Vec<Vec<(ratatui::style::Color, String)>>> {
    let syntax_theme = &theme_set.themes["base16-ocean.dark"];
    let mut all_highlights = Vec::with_capacity(regions.len());

    for region in regions {
        let syntax = if region.language.is_empty() {
            syntax_set.find_syntax_plain_text()
        } else {
            syntax_set
                .find_syntax_by_token(&region.language)
                .unwrap_or_else(|| syntax_set.find_syntax_plain_text())
        };

        let mut highlighter = syntect::easy::HighlightLines::new(syntax, syntax_theme);
        let mut region_highlights = Vec::new();

        let content_start = region.start_line + 1;
        let content_end = region.end_line;

        for line_idx in content_start..content_end {
            if line_idx >= lines.len() {
                break;
            }
            let line_with_nl = format!("{}\n", lines[line_idx]);

            let spans = match highlighter.highlight_line(&line_with_nl, syntax_set) {
                Ok(hl_regions) => hl_regions
                    .iter()
                    .filter_map(|(style, content)| {
                        let text = content.trim_end_matches('\n');
                        if text.is_empty() {
                            return None;
                        }
                        let color = ratatui::style::Color::Rgb(
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                        );
                        Some((color, text.to_string()))
                    })
                    .collect(),
                Err(_) => Vec::new(),
            };
            region_highlights.push(spans);
        }

        all_highlights.push(region_highlights);
    }

    all_highlights
}

impl<'a> App<'a> {
    pub fn new(file_path: PathBuf) -> Self {
        let content = std::fs::read_to_string(&file_path).unwrap_or_default();
        let lines: Vec<String> = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(String::from).collect()
        };

        let mut textarea = TextArea::new(lines.clone());
        editor::configure_textarea(&mut textarea);

        // Try to open the git repo for branch/status/gutter info
        let git_repo = GitRepo::open(&file_path);
        let git_branch = git_repo
            .as_ref()
            .map(|g| g.branch_name())
            .unwrap_or_default();
        let git_file_status = git_repo
            .as_ref()
            .map(|g| g.file_status(&file_path))
            .unwrap_or_default();

        // Spawn background thread for gutter marks (expensive git diff)
        let gutter_handle = if git_repo.is_some() {
            let fp = file_path.clone();
            Some(std::thread::spawn(move || {
                match git2::Repository::discover(&fp) {
                    Ok(repo) => git::diff::compute_gutter_marks(&repo, &fp),
                    Err(_) => HashMap::new(),
                }
            }))
        } else {
            None
        };

        // Code fence regions found immediately (cheap), but highlights deferred
        // until syntect finishes loading in background (code_fence_dirty=true).
        let code_fence_regions = code_highlight::find_code_fence_regions(&lines);

        Self {
            mode: Mode::Editor,
            file_path,
            textarea,
            modified: false,
            original_content: content,
            should_quit: false,
            docx_state: None,
            preview: preview::PreviewState::new(),
            gutter_marks: HashMap::new(),
            status_message: "F1: help | Tab: switch mode | Ctrl+S: save | Ctrl+Q: quit"
                .to_string(),
            status_time: Some(Instant::now()),
            git_repo,
            git_branch,
            git_file_status,
            renaming: false,
            rename_buf: String::new(),
            rename_cursor: 0,
            show_help: false,
            viewport_height: 0,
            content_area: Rect::default(),
            editor_scroll_top: 0,
            mouse_dragging: false,
            last_click_time: None,
            last_click_pos: (0, 0),
            click_count: 0,
            gutter_handle,
            code_fence_regions,
            code_fence_highlights: vec![],
            code_fence_dirty: true,
        }
    }

    /// Runs one frame of the main loop: draw + tick.
    /// This is the canonical render path — tested by render_test to ensure
    /// no accidental screen clears (which cause flicker).
    pub fn render_frame<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut ratatui::Terminal<B>,
    ) -> std::io::Result<()> {
        terminal.draw(|frame| self.render(frame))?;
        self.tick();
        Ok(())
    }

    // ─── Rendering ───────────────────────────────────────────────────────

    pub fn render(&mut self, frame: &mut Frame) {
        let full = frame.area();

        // Fill entire frame background first (covers margins outside capped area)
        let bg = Paragraph::new("").style(theme::editor_style());
        frame.render_widget(bg, full);

        // Cap width and center horizontally
        let capped_width = full.width.min(MAX_WIDTH);
        let x_offset = (full.width - capped_width) / 2;
        let usable_area = Rect::new(x_offset, full.y, capped_width, full.height);

        let chunks = Layout::vertical([
            Constraint::Length(1),  // Header
            Constraint::Length(1),  // Divider
            Constraint::Min(1),    // Content
            Constraint::Length(1),  // Divider
            Constraint::Length(1),  // Status
        ])
        .split(usable_area);

        self.viewport_height = chunks[2].height;
        self.content_area = chunks[2];

        // Header bar: filename (or rename input) + mode tabs
        // When editing a .docx, show the .docx filename instead of the .md sibling
        let filename = if let Some(ref ds) = self.docx_state {
            ds.docx_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("untitled")
        } else {
            self.file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("untitled")
        };
        header::render(
            frame,
            chunks[0],
            filename,
            self.modified,
            &self.mode,
            self.renaming,
            &self.rename_buf,
            self.rename_cursor,
        );

        // Thin dividers between bars and content
        let divider_style = Style::default().fg(theme::BORDER);
        let top_divider = Paragraph::new("─".repeat(chunks[1].width as usize))
            .style(divider_style);
        frame.render_widget(top_divider, chunks[1]);
        let bottom_divider = Paragraph::new("─".repeat(chunks[3].width as usize))
            .style(divider_style);
        frame.render_widget(bottom_divider, chunks[3]);

        // Content area — render depends on current mode
        match self.mode {
            Mode::Editor => {
                self.render_editor(frame, chunks[2]);
            }
            Mode::Preview => {
                let content = self.textarea_content();
                let base_dir = self.file_path.parent().unwrap_or(std::path::Path::new("."));
                preview::render(frame, chunks[2], &content, &mut self.preview, base_dir);
            }
        }

        // Status bar: cursor position, word count, save status
        let (line, col) = self.textarea.cursor();
        status::render(
            frame,
            chunks[4],
            status::StatusInfo {
                line: line + 1,
                col,
                message: &self.status_message,
                word_count: self.word_count(),
                modified: self.modified,
            },
        );

        // Help modal overlay — rendered last so it sits on top of everything
        if self.show_help {
            self.render_help(frame);
        }
    }

    /// Renders a centered modal overlay listing all keybindings.
    /// Dismissed by pressing any key.
    fn render_help(&self, frame: &mut Frame) {
        let area = frame.area();
        // Size the modal to fit content, clamped to terminal size
        let width = 45u16.min(area.width.saturating_sub(4));
        let height = 23u16.min(area.height.saturating_sub(2));
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let help_area = Rect::new(x, y, width, height);

        // Clear the area behind the modal
        frame.render_widget(Clear, help_area);

        // Help content — must match the actual keybinding handlers!
        // Grouped: global, editor, tui-textarea built-ins, mouse
        let help_text = vec![
            Line::from(Span::styled(
                "Keybindings",
                Style::default()
                    .fg(theme::HEADING)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            // -- Global (work in all modes) --
            Line::from(vec![
                Span::styled("  Tab              ", Style::default().fg(theme::LINK)),
                Span::raw("Switch mode"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+S           ", Style::default().fg(theme::LINK)),
                Span::raw("Save"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+Q           ", Style::default().fg(theme::LINK)),
                Span::raw("Save & quit"),
            ]),
            Line::from(vec![
                Span::styled("  Esc              ", Style::default().fg(theme::LINK)),
                Span::raw("Back to editor"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+T           ", Style::default().fg(theme::LINK)),
                Span::raw("Rename file"),
            ]),
            Line::from(vec![
                Span::styled("  F1               ", Style::default().fg(theme::LINK)),
                Span::raw("This help"),
            ]),
            Line::from(""),
            // -- Editor mode --
            Line::from(vec![
                Span::styled("  Ctrl+Z / Ctrl+Y  ", Style::default().fg(theme::LINK)),
                Span::raw("Undo / Redo"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+A           ", Style::default().fg(theme::LINK)),
                Span::raw("Select all"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+L           ", Style::default().fg(theme::LINK)),
                Span::raw("Go to line start"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+C / Ctrl+V  ", Style::default().fg(theme::LINK)),
                Span::raw("Copy / Paste (system)"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+H           ", Style::default().fg(theme::LINK)),
                Span::raw("Delete word before"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+D           ", Style::default().fg(theme::LINK)),
                Span::raw("Delete word after"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+K           ", Style::default().fg(theme::LINK)),
                Span::raw("Delete to end of line"),
            ]),
            Line::from(""),
            // -- Mouse --
            Line::from(vec![
                Span::styled("  Click + drag     ", Style::default().fg(theme::LINK)),
                Span::raw("Select text"),
            ]),
            Line::from(vec![
                Span::styled("  Click filename   ", Style::default().fg(theme::LINK)),
                Span::raw("Rename file"),
            ]),
            Line::from(vec![
                Span::styled("  Click tabs       ", Style::default().fg(theme::LINK)),
                Span::raw("Switch mode"),
            ]),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .style(Style::default().fg(theme::FG).bg(theme::BAR_BG));

        let paragraph = Paragraph::new(help_text)
            .block(block)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, help_area);
    }

    /// Renders the tui-textarea widget plus tilde markers for empty lines,
    /// then overlays syntax highlighting for code fence regions.
    fn render_editor(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(&self.textarea, area);

        // Track scroll position (mirrors tui-textarea's internal viewport logic)
        // so we can translate mouse coordinates → buffer positions correctly.
        let cursor_row = self.textarea.cursor().0 as u16;
        if cursor_row < self.editor_scroll_top {
            self.editor_scroll_top = cursor_row;
        } else if self.editor_scroll_top + area.height <= cursor_row {
            self.editor_scroll_top = cursor_row + 1 - area.height;
        }

        // Render vim-style tilde markers for lines beyond the file content
        let total_lines = self.textarea.lines().len();
        let gutter_width = format!("{}", total_lines).len() as u16 + 1;
        if (total_lines as u16) < area.height {
            for row in total_lines as u16..area.height {
                let tilde_area = Rect {
                    x: area.x,
                    y: area.y + row,
                    width: area.width,
                    height: 1,
                };
                let tilde = Paragraph::new(Line::from(vec![
                    Span::styled(
                        " ".repeat(gutter_width as usize),
                        Style::default().fg(theme::TILDE),
                    ),
                    Span::styled(
                        "~",
                        Style::default().fg(theme::TILDE),
                    ),
                ]));
                frame.render_widget(tilde, tilde_area);
            }
        }

        // Apply syntax highlighting overlay for code fence regions
        self.apply_code_fence_highlighting(frame, area, gutter_width);

        // Overlay git gutter markers on the first column of changed lines
        if !self.gutter_marks.is_empty() {
            let scroll_top = self.editor_scroll_top as usize;
            let visible_rows = area.height.min(total_lines.saturating_sub(scroll_top) as u16);
            for row in 0..visible_rows {
                let buf_line = scroll_top + row as usize;
                if let Some(mark) = self.gutter_marks.get(&buf_line) {
                    let color = match mark {
                        GutterMark::Added => theme::GIT_ADDED,
                        GutterMark::Modified => theme::GIT_MODIFIED,
                        GutterMark::Removed => theme::GIT_REMOVED,
                    };
                    let buf = frame.buffer_mut();
                    if let Some(cell) = buf.cell_mut((area.x, area.y + row)) {
                        cell.set_char('\u{258E}'); // ▎ left quarter block
                        cell.set_fg(color);
                    }
                }
            }
        }
    }

    /// Overlays syntax highlighting on the ratatui buffer for code fence regions.
    /// Post-processes cells after tui-textarea has rendered, overwriting foreground
    /// colors only (preserving cursor/selection backgrounds).
    fn apply_code_fence_highlighting(&mut self, frame: &mut Frame, area: Rect, gutter_width: u16) {
        // Refresh code fence regions and cached highlights if dirty
        if self.code_fence_dirty {
            // Non-blocking: if syntect hasn't finished loading, skip and retry next frame
            let (ss, ts) = match code_highlight::try_get() {
                Some(pair) => pair,
                None => return,
            };
            let lines: Vec<String> = self.textarea.lines().iter().map(|s| s.to_string()).collect();
            self.code_fence_regions = code_highlight::find_code_fence_regions(&lines);
            self.code_fence_highlights =
                highlight_code_regions(&self.code_fence_regions, &lines, ss, ts);
            self.code_fence_dirty = false;
        }

        if self.code_fence_regions.is_empty() {
            return;
        }

        let scroll_top = self.editor_scroll_top as usize;
        let visible_end = scroll_top + area.height as usize;
        let cursor_pos = self.textarea.cursor();

        for (region_idx, region) in self.code_fence_regions.iter().enumerate() {
            // Skip regions completely outside the viewport
            if region.end_line < scroll_top || region.start_line >= visible_end {
                continue;
            }

            let highlights = match self.code_fence_highlights.get(region_idx) {
                Some(h) => h,
                None => continue,
            };

            let content_start = region.start_line + 1;

            for (line_offset, spans) in highlights.iter().enumerate() {
                let line_idx = content_start + line_offset;

                // Only overlay visible lines
                if line_idx < scroll_top || line_idx >= visible_end {
                    continue;
                }

                let screen_row = area.y + (line_idx - scroll_top) as u16;
                if screen_row >= area.y + area.height {
                    continue;
                }

                // Map cached highlight spans onto buffer cells
                let text_start_x = area.x + gutter_width + 1; // +1 for leading space in gutter
                let mut col_offset: u16 = 0;

                for (fg_color, text) in spans {
                    for _ch in text.chars() {
                        let cell_x = text_start_x + col_offset;
                        if cell_x >= area.x + area.width {
                            break;
                        }

                        // Skip cursor cell (preserve cursor visibility)
                        let is_cursor_cell = line_idx == cursor_pos.0
                            && col_offset as usize == cursor_pos.1;

                        if !is_cursor_cell {
                            let buf = frame.buffer_mut();
                            if let Some(cell) = buf.cell_mut((cell_x, screen_row)) {
                                // Only override foreground, preserve background
                                // (keeps selection/cursor highlighting intact)
                                let bg = cell.bg;
                                if bg == ratatui::style::Color::Reset {
                                    cell.set_fg(*fg_color);
                                }
                            }
                        }

                        col_offset += 1;
                    }
                }
            }
        }
    }

    /// Returns the full editor content as a single string.
    fn textarea_content(&self) -> String {
        self.textarea.lines().join("\n")
    }

    // ─── Tick / timers ───────────────────────────────────────────────────

    /// Called every 100ms from the main loop. Handles timer-based state cleanup.
    pub fn tick(&mut self) {
        // Drain decoded images from background threads
        self.preview.poll_decoded_images();

        // Poll background gutter marks computation
        if let Some(ref handle) = self.gutter_handle {
            if handle.is_finished() {
                if let Some(handle) = self.gutter_handle.take() {
                    if let Ok(marks) = handle.join() {
                        self.gutter_marks = marks;
                    }
                }
            }
        }

        // Auto-clear status messages after STATUS_DURATION
        if let Some(time) = self.status_time {
            if time.elapsed() >= STATUS_DURATION {
                self.status_message.clear();
                self.status_time = None;
            }
        }
    }

    // ─── Event dispatch ──────────────────────────────────────────────────

    /// Top-level event handler. Dispatches to key, mouse, or paste handlers.
    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            // Bracketed paste: terminal sends entire clipboard as one event
            // (enabled via EnableBracketedPaste in main.rs)
            Event::Paste(text) => self.handle_paste(text),
            Event::Resize(_, _) => {} // ratatui handles resize
            _ => {}
        }
    }

    /// Handles bracketed paste events (Cmd+V in iTerm2, etc).
    /// Inserts text into the rename buffer if renaming, otherwise into the editor.
    fn handle_paste(&mut self, text: String) {
        if self.renaming {
            for ch in text.chars() {
                if ch != '\n' && ch != '\r' {
                    self.rename_buf.insert(self.rename_cursor, ch);
                    self.rename_cursor += 1;
                }
            }
            return;
        }
        if self.mode == Mode::Editor {
            self.textarea.insert_str(text);
            self.update_modified();
            self.auto_wrap_line();
        }
    }

    // ─── Key handling ────────────────────────────────────────────────────

    /// Main key handler. Processes modal states first, then Esc-as-back,
    /// then global keybindings, then delegates to mode-specific handlers.
    fn handle_key(&mut self, key: KeyEvent) {
        // Help modal: any key dismisses it (swallows the keypress)
        if self.show_help {
            self.show_help = false;
            return;
        }

        // Rename mode: all keys go to the inline rename input
        if self.renaming {
            self.handle_rename_key(key);
            return;
        }

        // Esc: return to Editor mode (back/cancel)
        if key.code == KeyCode::Esc && key.modifiers.is_empty() {
            if self.mode != Mode::Editor {
                self.set_mode(Mode::Editor);
            }
            return;
        }

        // Global keybindings (work in all modes)
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('q')) => {
                if self.modified {
                    self.save();
                }
                self.should_quit = true;
                return;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
                self.save();
                return;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('t')) => {
                self.start_rename();
                return;
            }
            (_, KeyCode::F(1)) => {
                self.show_help = true;
                return;
            }
            (_, KeyCode::Tab) => {
                // Toggle between Editor and Preview
                let target = match self.mode {
                    Mode::Editor => Mode::Preview,
                    _ => Mode::Editor,
                };
                self.set_mode(target);
                return;
            }
            _ => {}
        }

        // Mode-specific keybindings
        match self.mode {
            Mode::Editor => self.handle_editor_key(key),
            Mode::Preview => self.handle_preview_key(key),
        }
    }

    /// Editor mode key handler. Intercepts standard keybindings (Ctrl+Z, Ctrl+C, etc.)
    /// BEFORE passing to tui-textarea, which has non-standard defaults:
    ///   tui-textarea: Ctrl+U=undo, Ctrl+Y=paste, Ctrl+V=PageDown, Ctrl+A=line-start
    ///   We remap:     Ctrl+Z=undo, Ctrl+Y=redo,  Ctrl+V=paste,    Ctrl+A=select-all
    fn handle_editor_key(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            // Undo
            (KeyModifiers::CONTROL, KeyCode::Char('z')) => {
                self.textarea.undo();
                self.update_modified();
                return;
            }
            // Redo
            (KeyModifiers::CONTROL, KeyCode::Char('y')) => {
                self.textarea.redo();
                self.update_modified();
                return;
            }
            // Redo (alternative: Ctrl+Shift+Z)
            (m, KeyCode::Char('Z')) if m.contains(KeyModifiers::CONTROL) && m.contains(KeyModifiers::SHIFT) => {
                self.textarea.redo();
                self.update_modified();
                return;
            }
            // Select all (overrides tui-textarea's Ctrl+A = move to line start)
            (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
                self.textarea.select_all();
                return;
            }
            // Go to beginning of line
            (KeyModifiers::CONTROL, KeyCode::Char('l')) => {
                self.textarea.cancel_selection();
                self.textarea.move_cursor(CursorMove::Head);
                return;
            }
            // Copy selection to system clipboard (overrides tui-textarea's internal-only yank)
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                if let Some(text) = self.get_selected_text() {
                    self.copy_to_clipboard(&text);
                }
                // Also yank internally so Ctrl+V fallback works within the editor
                self.textarea.copy();
                return;
            }
            // Paste from system clipboard (overrides tui-textarea's Ctrl+V = PageDown)
            (KeyModifiers::CONTROL, KeyCode::Char('v')) => {
                if let Some(text) = self.paste_from_clipboard() {
                    self.textarea.insert_str(text);
                    self.update_modified();
                    self.auto_wrap_line();
                } else if let Some(md_text) = self.paste_image_from_clipboard() {
                    self.textarea.insert_str(md_text);
                    self.update_modified();
                }
                return;
            }
            // Delete word before cursor
            // On macOS, Ctrl+Backspace sends Ctrl+H (0x08), so we match both
            (KeyModifiers::CONTROL, KeyCode::Backspace)
            | (KeyModifiers::CONTROL, KeyCode::Char('h')) => {
                self.textarea.delete_word();
                self.update_modified();
                return;
            }
            // Delete word after cursor (forward)
            (KeyModifiers::CONTROL, KeyCode::Delete) => {
                self.textarea.delete_next_word();
                self.update_modified();
                return;
            }
            // Delete word after cursor (Mac-friendly: no forward-delete key on Magic Keyboard)
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                self.textarea.delete_next_word();
                self.update_modified();
                return;
            }
            // Enter: list/blockquote continuation
            (KeyModifiers::NONE, KeyCode::Enter) => {
                if self.handle_enter_continuation() {
                    return;
                }
            }
            // Auto-close pairs for bracket/quote characters
            (KeyModifiers::NONE, KeyCode::Char(ch))
                if autocomplete::auto_close_pair(ch).is_some() =>
            {
                if self.handle_auto_close(ch) {
                    return;
                }
            }
            _ => {}
        }

        // Everything else: pass through to tui-textarea's built-in handling.
        // This covers: arrow keys, Enter, Backspace, Delete, Home, End,
        // Ctrl+K (delete to EOL), Ctrl+W/Alt+Backspace (delete word),
        // Ctrl+E (move to EOL), word navigation, etc.
        let input = Input::from(key);
        self.textarea.input(input);
        self.update_modified();
        self.auto_wrap_line();
    }

    /// Preview mode key handler: arrow key scrolling only.
    fn handle_preview_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.preview.scroll_up(1),
            KeyCode::Down => self.preview.scroll_down(1, self.viewport_height),
            KeyCode::PageUp => self.preview.page_up(self.viewport_height),
            KeyCode::PageDown => self.preview.page_down(self.viewport_height),
            KeyCode::Home => self.preview.scroll_offset = 0,
            KeyCode::End => {
                self.preview.scroll_offset = self
                    .preview
                    .content_height
                    .saturating_sub(self.viewport_height);
            }
            _ => {}
        }
    }

    // ─── Mouse handling ──────────────────────────────────────────────────

    /// Handles all mouse events: scroll, click (positioning + tab/filename clicks),
    /// drag (text selection), and release.
    fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            // Scroll wheel: delegate to tui-textarea in editor, manual in preview
            MouseEventKind::ScrollUp => match self.mode {
                Mode::Editor => {
                    self.textarea.input(Input {
                        key: Key::MouseScrollUp,
                        ctrl: false,
                        alt: false,
                        shift: false,
                    });
                    self.editor_scroll_top = self.editor_scroll_top.saturating_sub(1);
                }
                Mode::Preview => self.preview.scroll_up(SCROLL_LINES),
            },
            MouseEventKind::ScrollDown => match self.mode {
                Mode::Editor => {
                    self.textarea.input(Input {
                        key: Key::MouseScrollDown,
                        ctrl: false,
                        alt: false,
                        shift: false,
                    });
                    let total_lines = self.textarea.lines().len() as u16;
                    let max_scroll = total_lines.saturating_sub(1);
                    self.editor_scroll_top = (self.editor_scroll_top + 1).min(max_scroll);
                }
                Mode::Preview => self.preview.scroll_down(SCROLL_LINES, self.viewport_height),
            },

            // Left click: header tabs/filename or editor cursor positioning + drag start
            MouseEventKind::Down(MouseButton::Left) => {
                let area = self.content_area;

                // Ignore clicks outside the capped area's x-range
                if mouse.column < area.x || mouse.column >= area.x + area.width {
                    return;
                }

                // Click on header row (above content area)
                if mouse.row < area.y {
                    let right_edge = area.x + area.width;
                    let tabs_start = right_edge.saturating_sub(TAB_TOTAL_W);

                    if mouse.column >= tabs_start {
                        // Click on mode tabs
                        let offset = mouse.column - tabs_start;
                        if offset < TAB_EDITOR_W {
                            self.set_mode(Mode::Editor);
                        } else {
                            self.set_mode(Mode::Preview);
                        }
                    } else {
                        // Click on filename area → enter rename mode
                        self.start_rename();
                    }
                    return;
                }

                // Click on link in preview mode → open URL
                if self.mode == Mode::Preview {
                    if let Some(url) = self.preview.url_at(mouse.column, mouse.row) {
                        crate::components::preview::open_url(url);
                    }
                    return;
                }

                // Click in editor content area: single/double/triple click handling
                if self.mode == Mode::Editor
                    && mouse.column >= area.x
                    && mouse.column < area.x + area.width
                    && mouse.row >= area.y
                    && mouse.row < area.y + area.height
                {
                    // Multi-click detection
                    let now = Instant::now();
                    let is_repeat = self
                        .last_click_time
                        .map(|t| now.duration_since(t).as_millis() < MULTI_CLICK_MS as u128)
                        .unwrap_or(false)
                        && self.last_click_pos == (mouse.column, mouse.row);
                    self.click_count = if is_repeat {
                        (self.click_count % 3) + 1
                    } else {
                        1
                    };
                    self.last_click_time = Some(now);
                    self.last_click_pos = (mouse.column, mouse.row);

                    let (buffer_row, buffer_col) =
                        self.mouse_to_buffer_pos(mouse.column, mouse.row);

                    match self.click_count {
                        2 => {
                            // Double-click: select word
                            self.textarea
                                .move_cursor(CursorMove::Jump(buffer_row, buffer_col));
                            self.select_word_at_cursor();
                            self.mouse_dragging = false;
                        }
                        3 => {
                            // Triple-click: select paragraph
                            self.textarea
                                .move_cursor(CursorMove::Jump(buffer_row, buffer_col));
                            self.select_paragraph_at_cursor();
                            self.mouse_dragging = false;
                        }
                        _ => {
                            // Single click: position cursor + start drag selection
                            self.textarea.cancel_selection();
                            self.textarea
                                .move_cursor(CursorMove::Jump(buffer_row, buffer_col));
                            self.textarea.start_selection();
                            self.mouse_dragging = true;
                        }
                    }
                }
            }

            // Left drag: extend selection to current mouse position
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.mode == Mode::Editor && self.mouse_dragging {
                    let area = self.content_area;
                    if mouse.column >= area.x
                        && mouse.column < area.x + area.width
                        && mouse.row >= area.y
                        && mouse.row < area.y + area.height
                    {
                        let (buffer_row, buffer_col) = self.mouse_to_buffer_pos(mouse.column, mouse.row);
                        self.textarea
                            .move_cursor(CursorMove::Jump(buffer_row, buffer_col));
                    }
                }
            }

            // Left release: finalize selection (cancel if it was just a click with no drag)
            MouseEventKind::Up(MouseButton::Left) => {
                if self.mouse_dragging {
                    self.mouse_dragging = false;
                    if let Some(((sr, sc), (er, ec))) = self.textarea.selection_range() {
                        if sr == er && sc == ec {
                            self.textarea.cancel_selection();
                        }
                    } else {
                        self.textarea.cancel_selection();
                    }
                }
            }
            _ => {}
        }
    }

    /// Converts terminal mouse coordinates to buffer (row, col) positions,
    /// accounting for the line number gutter width and scroll offset.
    fn mouse_to_buffer_pos(&self, column: u16, row: u16) -> (u16, u16) {
        let area = self.content_area;
        let total_lines = self.textarea.lines().len();
        // tui-textarea gutter = leading space + digits + trailing space
        let gutter_width = if self.textarea.line_number_style().is_some() {
            (total_lines as f64).log10() as u16 + 1 + 2
        } else {
            0
        };
        let relative_row = row - area.y;
        let buffer_row = relative_row + self.editor_scroll_top;
        let relative_col = column - area.x;
        let buffer_col = relative_col.saturating_sub(gutter_width);
        (buffer_row, buffer_col)
    }

    // ─── Rename mode ─────────────────────────────────────────────────────

    /// Enter rename mode: populates the rename buffer with the current filename
    /// and places the cursor at the end.
    fn start_rename(&mut self) {
        let source_path = if let Some(ref ds) = self.docx_state {
            &ds.docx_path
        } else {
            &self.file_path
        };
        let filename = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("untitled")
            .to_string();
        self.rename_buf = filename;
        self.rename_cursor = self.rename_buf.len();
        self.renaming = true;
    }

    /// Handles keypresses while in rename mode.
    /// Enter confirms, Esc cancels, printable chars edit the name.
    fn handle_rename_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.renaming = false;
                self.rename_buf.clear();
            }
            KeyCode::Enter => {
                self.confirm_rename();
            }
            KeyCode::Backspace => {
                if self.rename_cursor > 0 {
                    self.rename_cursor -= 1;
                    self.rename_buf.remove(self.rename_cursor);
                }
            }
            KeyCode::Delete => {
                if self.rename_cursor < self.rename_buf.len() {
                    self.rename_buf.remove(self.rename_cursor);
                }
            }
            KeyCode::Left => {
                if self.rename_cursor > 0 {
                    self.rename_cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.rename_cursor < self.rename_buf.len() {
                    self.rename_cursor += 1;
                }
            }
            KeyCode::Home => {
                self.rename_cursor = 0;
            }
            KeyCode::End => {
                self.rename_cursor = self.rename_buf.len();
            }
            KeyCode::Char(ch) => {
                // Reject path separators to keep the name a bare filename
                if ch != '/' && ch != '\\' {
                    self.rename_buf.insert(self.rename_cursor, ch);
                    self.rename_cursor += 1;
                }
            }
            _ => {}
        }
    }

    /// Performs the actual file rename via fs::rename, updates internal state.
    /// When in docx mode, renames both the .docx and .md files.
    fn confirm_rename(&mut self) {
        let new_name = self.rename_buf.trim().to_string();
        if new_name.is_empty() {
            self.set_status("Rename cancelled: empty name");
            self.renaming = false;
            return;
        }

        if let Some(ref ds) = self.docx_state {
            // Docx mode: the user is renaming the .docx file
            let current_name = ds
                .docx_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            if new_name == current_name {
                self.renaming = false;
                return;
            }

            let new_docx_path = ds.docx_path.with_file_name(&new_name);
            // Derive the .md sibling name from the new .docx name
            let new_md_path = new_docx_path.with_extension("md");

            // Rename the .docx file
            match std::fs::rename(&ds.docx_path, &new_docx_path) {
                Ok(_) => {
                    // Rename the .md file too
                    let md_renamed = std::fs::rename(&self.file_path, &new_md_path);
                    self.file_path = new_md_path;
                    self.docx_state = Some(DocxState {
                        docx_path: new_docx_path.clone(),
                        reference_doc: new_docx_path,
                    });
                    if md_renamed.is_ok() {
                        self.set_status("Renamed");
                    } else {
                        self.set_status("Renamed .docx (but .md rename failed)");
                    }
                    self.refresh_git_status();
                    self.refresh_gutter_marks();
                }
                Err(e) => {
                    self.set_status(&format!("Rename failed: {}", e));
                }
            }
        } else {
            // Regular .md mode
            let current_name = self
                .file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            if new_name == current_name {
                self.renaming = false;
                return;
            }

            let new_path = self.file_path.with_file_name(&new_name);
            match std::fs::rename(&self.file_path, &new_path) {
                Ok(_) => {
                    self.file_path = new_path;
                    self.set_status("Renamed");
                    self.refresh_git_status();
                    self.refresh_gutter_marks();
                }
                Err(e) => {
                    self.set_status(&format!("Rename failed: {}", e));
                }
            }
        }
        self.renaming = false;
    }

    // ─── Clipboard helpers ───────────────────────────────────────────────
    // arboard::Clipboard is created on demand (not stored in App — it's not Send
    // and creating it is cheap).

    /// Writes text to the system clipboard via arboard.
    fn copy_to_clipboard(&self, text: &str) {
        if let Ok(mut clip) = arboard::Clipboard::new() {
            let _ = clip.set_text(text.to_string());
        }
    }

    /// Reads text from the system clipboard. Returns None on failure.
    fn paste_from_clipboard(&self) -> Option<String> {
        arboard::Clipboard::new().ok()?.get_text().ok()
    }

    /// Returns a markdown image link immediately and spawns a background
    /// thread that saves the clipboard image as a PNG file.
    ///
    /// On macOS, uses NSPasteboard to grab raw PNG bytes directly — no
    /// decode/re-encode needed, so the file appears in ~100ms instead of ~10s.
    ///
    /// The background thread also sends the decoded `DynamicImage` through the
    /// preview channel so the first render doesn't block on a redundant decode.
    fn paste_image_from_clipboard(&self) -> Option<String> {
        let parent = self.file_path.parent()?;
        let images_dir = parent.join(".marko").join("images");
        std::fs::create_dir_all(&images_dir).ok()?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let filename = format!("screenshot-{}.png", now.as_secs());
        let file_path = images_dir.join(&filename);
        let relative_url = format!(".marko/images/{}", filename);
        let md_text = format!("![screenshot]({})\n", relative_url);

        let image_tx = self.preview.image_sender();
        let url_hint = relative_url.clone();

        std::thread::spawn(move || {
            use std::io::Write;
            let log = |msg: &str| {
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/marko-debug.log")
                {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default();
                    let _ = writeln!(f, "[{:.3}] [paste_image] {}", ts.as_secs_f64(), msg);
                }
            };

            let send_image = |img: Option<image::DynamicImage>| {
                if let Some(ref i) = img {
                    crate::components::preview::save_thumbnail(i, &file_path);
                }
                let _ = image_tx.send(crate::components::preview::DecodedImage {
                    path: file_path.clone(),
                    image: img,
                    url_hint: Some(url_hint.clone()),
                });
            };

            if let Some(raw_bytes) = clipboard_png_bytes() {
                log(&format!("got clipboard bytes: {} bytes", raw_bytes.len()));
                // macOS often provides TIFF even when asked for PNG — check magic bytes
                let is_png = raw_bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]);
                if is_png {
                    log("data is actual PNG, writing directly");
                    match std::fs::write(&file_path, &raw_bytes) {
                        Ok(_) => log("PNG saved (raw)"),
                        Err(e) => log(&format!("write failed: {}", e)),
                    }
                    let img = crate::components::preview::load_image_from_bytes(&raw_bytes);
                    send_image(img);
                } else {
                    log("data is TIFF, transcoding to PNG");
                    let img = transcode_to_png(&raw_bytes, &file_path, &log);
                    send_image(img);
                }
            } else {
                log("no image data on clipboard, falling back to arboard");
                save_clipboard_image_arboard(&file_path, &log);
                let img = crate::components::preview::load_image(&file_path);
                send_image(img);
            }
        });

        Some(md_text)
    }

    /// Extracts the currently selected text from tui-textarea using selection_range().
    fn get_selected_text(&self) -> Option<String> {
        let ((sr, sc), (er, ec)) = self.textarea.selection_range()?;
        let lines = self.textarea.lines();

        if sr == er {
            // Single line selection
            let line = &lines[sr];
            Some(line[sc..ec].to_string())
        } else {
            // Multi-line selection
            let mut result = String::new();
            for (i, line) in lines.iter().enumerate().skip(sr).take(er - sr + 1) {
                if i == sr {
                    result.push_str(&line[sc..]);
                } else if i == er {
                    result.push_str(&line[..ec]);
                } else {
                    result.push_str(line);
                }
                if i < er {
                    result.push('\n');
                }
            }
            Some(result)
        }
    }

    // ─── Selection helpers ────────────────────────────────────────────────

    /// Selects the word under the cursor (for double-click).
    /// Groups: alphanumeric+underscore, whitespace, punctuation.
    fn select_word_at_cursor(&mut self) {
        let (row, col) = self.textarea.cursor();
        let lines = self.textarea.lines();
        if row >= lines.len() {
            return;
        }
        let line = &lines[row];
        let chars: Vec<char> = line.chars().collect();
        if col >= chars.len() {
            return;
        }

        let target = char_class(chars[col]);

        let mut start = col;
        while start > 0 && char_class(chars[start - 1]) == target {
            start -= 1;
        }

        let mut end = col;
        while end < chars.len() && char_class(chars[end]) == target {
            end += 1;
        }

        self.textarea.cancel_selection();
        self.textarea
            .move_cursor(CursorMove::Jump(row as u16, start as u16));
        self.textarea.start_selection();
        self.textarea
            .move_cursor(CursorMove::Jump(row as u16, end as u16));
    }

    /// Selects the paragraph around the cursor (for triple-click).
    /// A paragraph is a contiguous block of non-empty lines.
    fn select_paragraph_at_cursor(&mut self) {
        let (row, _) = self.textarea.cursor();
        let lines = self.textarea.lines();
        if row >= lines.len() {
            return;
        }

        // Find paragraph start: walk backward to first empty line
        let mut start = row;
        while start > 0 && !lines[start - 1].trim().is_empty() {
            start -= 1;
        }

        // Find paragraph end: walk forward to last non-empty line
        let mut end = row;
        while end + 1 < lines.len() && !lines[end + 1].trim().is_empty() {
            end += 1;
        }

        let end_col = lines[end].len();
        self.textarea.cancel_selection();
        self.textarea
            .move_cursor(CursorMove::Jump(start as u16, 0));
        self.textarea.start_selection();
        self.textarea
            .move_cursor(CursorMove::Jump(end as u16, end_col as u16));
    }

    // ─── Internal helpers ────────────────────────────────────────────────

    /// Handles Enter key with list/blockquote continuation.
    /// Returns true if the key was handled (caller should not pass to tui-textarea).
    fn handle_enter_continuation(&mut self) -> bool {
        let (row, col) = self.textarea.cursor();
        let lines = self.textarea.lines();
        if row >= lines.len() {
            return false;
        }
        let line = lines[row].clone();

        // Only handle when cursor is at end of line
        if col != line.len() {
            return false;
        }

        match autocomplete::analyze_line_for_continuation(&line) {
            Continuation::Continue(prefix) => {
                self.textarea.insert_newline();
                self.textarea.insert_str(&prefix);
                self.update_modified();
                true
            }
            Continuation::ClearLine => {
                // Select the entire line content and cut it
                self.textarea.move_cursor(CursorMove::Head);
                self.textarea.start_selection();
                self.textarea.move_cursor(CursorMove::End);
                self.textarea.cut();
                self.update_modified();
                true
            }
            Continuation::None => false,
        }
    }

    /// Handles auto-close pair insertion for bracket/quote characters.
    /// Returns true if the key was handled.
    fn handle_auto_close(&mut self, ch: char) -> bool {
        let close = match autocomplete::auto_close_pair(ch) {
            Some(c) => c,
            None => return false,
        };

        // Get the character before the cursor for context-sensitive skipping
        let (row, col) = self.textarea.cursor();
        let lines = self.textarea.lines();
        let prev_char = if row < lines.len() && col > 0 {
            lines[row].chars().nth(col - 1)
        } else {
            None
        };

        // Skip backtick pairing when previous char is backtick (code fences)
        if ch == '`' && autocomplete::should_skip_backtick_pair(prev_char) {
            return false;
        }

        // Skip quote pairing when previous char is alphanumeric (contractions)
        if autocomplete::should_skip_quote_pair(ch, prev_char) {
            return false;
        }

        // Insert the pair and move cursor back between them
        self.textarea.insert_char(ch);
        self.textarea.insert_char(close);
        self.textarea.move_cursor(CursorMove::Back);
        self.update_modified();
        true
    }

    /// Counts the total number of words in the editor.
    fn word_count(&self) -> usize {
        self.textarea
            .lines()
            .iter()
            .map(|line| line.split_whitespace().count())
            .sum()
    }

    /// Recomputes the `modified` flag by comparing current content to original.
    fn update_modified(&mut self) {
        self.modified = self.textarea.lines().join("\n") != self.original_content;
        self.code_fence_dirty = true;
    }

    /// Auto-wraps the current line if it exceeds the visible text width.
    /// Called after text insertions to enforce line-width limits while typing.
    fn auto_wrap_line(&mut self) {
        // Safety limit to prevent infinite loops on very long pastes
        for _ in 0..500 {
            let (row, col) = self.textarea.cursor();
            let lines = self.textarea.lines();
            if row >= lines.len() {
                break;
            }
            let line = lines[row].to_string();

            // Compute visible text width (content area minus gutter)
            let total_lines = lines.len();
            let gutter: usize = if self.textarea.line_number_style().is_some() {
                (total_lines as f64).log10() as usize + 1 + 2
            } else {
                0
            };
            let text_width = (self.content_area.width as usize).saturating_sub(gutter);
            if text_width == 0 || line.len() <= text_width {
                break;
            }

            // Don't wrap headings, code fences, or table lines
            let trimmed = line.trim_start();
            if trimmed.starts_with('#')
                || trimmed.starts_with("```")
                || trimmed.starts_with("~~~")
                || trimmed.starts_with('|')
            {
                break;
            }

            // Find last space at or before the width limit
            let search_end = text_width.min(line.len());
            let break_pos = match line[..search_end].rfind(' ') {
                Some(pos) if pos > 0 => pos,
                _ => break, // no good break point — leave as-is
            };

            // Determine continuation indent for the new line
            let indent = table_format::continuation_indent(&line);

            // Split the line: move to the space, delete it, insert newline + indent
            self.textarea
                .move_cursor(CursorMove::Jump(row as u16, break_pos as u16));
            self.textarea.delete_next_char();
            self.textarea.insert_newline();
            if !indent.is_empty() {
                self.textarea.insert_str(&indent);
            }

            // Restore cursor to the equivalent position on the new line
            if col > break_pos {
                let new_row = row + 1;
                let new_col = indent.len() + (col - break_pos - 1);
                let actual_len = self
                    .textarea
                    .lines()
                    .get(new_row)
                    .map_or(0, |l| l.len());
                self.textarea.move_cursor(CursorMove::Jump(
                    new_row as u16,
                    new_col.min(actual_len) as u16,
                ));
            }
        }
    }

    /// Switches to a new mode, resetting scroll as needed.
    fn set_mode(&mut self, target: Mode) {
        if self.mode == target {
            return;
        }
        if target == Mode::Preview {
            self.preview.scroll_offset = 0;
        }
        self.mode = target;
    }

    /// Recomputes gutter marks from the git HEAD version of the file.
    fn refresh_gutter_marks(&mut self) {
        // Discard any pending background computation
        self.gutter_handle = None;
        if let Some(ref git_repo) = self.git_repo {
            self.gutter_marks =
                git::diff::compute_gutter_marks(git_repo.repository(), &self.file_path);
        } else {
            self.gutter_marks.clear();
        }
    }

    /// Writes the current editor content to disk and resets the modified flag.
    /// Runs table auto-formatting before writing.
    fn save(&mut self) {
        let content = self.textarea_content();
        // Subtract the line-number gutter so tables fit the visible text area.
        // tui-textarea gutter = leading space + digits + trailing space
        let total_lines = self.textarea.lines().len();
        let gutter: usize = if self.textarea.line_number_style().is_some() {
            (total_lines as f64).log10() as usize + 1 + 2
        } else {
            0
        };
        let width = (self.content_area.width as usize).saturating_sub(gutter);
        let after_tables = table_format::format_tables(&content, width);
        let formatted = table_format::hard_wrap(&after_tables, width);

        // If formatting changed the content, reconstruct the textarea
        if formatted != content {
            let (row, col) = self.textarea.cursor();
            let lines: Vec<String> = formatted.lines().map(String::from).collect();
            self.textarea = TextArea::new(if lines.is_empty() { vec![String::new()] } else { lines });
            editor::configure_textarea(&mut self.textarea);
            // Restore cursor position (clamped to valid range)
            let max_row = self.textarea.lines().len().saturating_sub(1);
            let target_row = row.min(max_row);
            let max_col = self.textarea.lines().get(target_row).map_or(0, |l| l.len());
            let target_col = col.min(max_col);
            self.textarea
                .move_cursor(CursorMove::Jump(target_row as u16, target_col as u16));
        }

        let save_content = self.textarea_content();
        match std::fs::write(&self.file_path, &save_content) {
            Ok(_) => {
                self.original_content = save_content;
                self.modified = false;

                // Round-trip: also export back to .docx if we're in docx mode
                if let Some(ref ds) = self.docx_state {
                    match pandoc::md_to_docx(&self.file_path, &ds.docx_path, Some(&ds.reference_doc)) {
                        Ok(_) => self.set_status("Saved (.md + .docx)"),
                        Err(e) => self.set_status(&format!("Saved .md, but .docx failed: {}", e)),
                    }
                } else {
                    self.set_status("Saved");
                }

                self.refresh_git_status();
                self.refresh_gutter_marks();
            }
            Err(e) => {
                self.set_status(&format!("Error saving: {}", e));
            }
        }
    }

    /// Refreshes the git file status indicator in the status bar.
    fn refresh_git_status(&mut self) {
        if let Some(ref git_repo) = self.git_repo {
            self.git_file_status = git_repo.file_status(&self.file_path);
        }
    }

    /// Shows a temporary message in the status bar.
    pub fn set_status(&mut self, msg: &str) {
        self.status_message = msg.to_string();
        self.status_time = Some(Instant::now());
    }
}

/// Grabs raw PNG bytes directly from the macOS pasteboard (no decode).
#[cfg(target_os = "macos")]
fn clipboard_png_bytes() -> Option<Vec<u8>> {
    use objc2::rc::Retained;
    use objc2::ClassType;
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeTIFF};
    use objc2_foundation::NSData;

    let pasteboard: Option<Retained<NSPasteboard>> =
        unsafe { objc2::msg_send![NSPasteboard::class(), generalPasteboard] };
    let pasteboard = pasteboard?;

    // Try PNG first (already the right format), fall back to TIFF
    let data: Retained<NSData> =
        unsafe { pasteboard.dataForType(NSPasteboardTypePNG) }
            .or_else(|| unsafe { pasteboard.dataForType(NSPasteboardTypeTIFF) })?;

    Some(unsafe { data.as_bytes_unchecked() }.to_vec())
}

#[cfg(not(target_os = "macos"))]
fn clipboard_png_bytes() -> Option<Vec<u8>> {
    None
}

/// Decodes image bytes (TIFF, etc.), re-encodes as PNG, and returns the decoded image.
fn transcode_to_png(raw_bytes: &[u8], file_path: &std::path::Path, log: &dyn Fn(&str)) -> Option<image::DynamicImage> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use std::io::Cursor;

    let reader = match image::ImageReader::new(Cursor::new(raw_bytes)).with_guessed_format() {
        Ok(r) => r,
        Err(e) => {
            log(&format!("format guess failed: {}", e));
            return None;
        }
    };
    let img = match reader.decode() {
        Ok(i) => i,
        Err(e) => {
            log(&format!("decode failed: {}", e));
            return None;
        }
    };
    log(&format!("decoded to {}x{}", img.width(), img.height()));
    let file = match std::fs::File::create(file_path) {
        Ok(f) => f,
        Err(e) => {
            log(&format!("file create failed: {}", e));
            return Some(img);
        }
    };
    let encoder = PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        CompressionType::Fast,
        FilterType::Sub,
    );
    match img.write_with_encoder(encoder) {
        Ok(_) => log("PNG saved (transcoded)"),
        Err(e) => log(&format!("PNG encode failed: {}", e)),
    }
    Some(img)
}

/// Fallback: use arboard to decode clipboard image, then re-encode as PNG.
fn save_clipboard_image_arboard(file_path: &std::path::Path, log: &dyn Fn(&str)) {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};

    let mut clip = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            log(&format!("Clipboard::new failed: {}", e));
            return;
        }
    };
    let img_data = match clip.get_image() {
        Ok(d) => d,
        Err(e) => {
            log(&format!("get_image failed: {}", e));
            return;
        }
    };
    let Some(rgba_image) = image::RgbaImage::from_raw(
        img_data.width as u32,
        img_data.height as u32,
        img_data.bytes.into_owned(),
    ) else {
        log("RgbaImage::from_raw returned None");
        return;
    };
    let file = match std::fs::File::create(file_path) {
        Ok(f) => f,
        Err(e) => {
            log(&format!("file create failed: {}", e));
            return;
        }
    };
    let encoder = PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        CompressionType::Fast,
        FilterType::Sub,
    );
    if let Err(e) = rgba_image.write_with_encoder(encoder) {
        log(&format!("PNG encode failed: {}", e));
        return;
    }
    log("PNG saved (arboard fallback)");
}

#[cfg(test)]
mod tests {
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

}
