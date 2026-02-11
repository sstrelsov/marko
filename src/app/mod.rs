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

mod clipboard;
mod input;
mod render;
mod rename;
mod save;
mod selection;

#[cfg(test)]
mod tests;
