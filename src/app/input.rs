//! Input handling: keyboard events, mouse events, paste, auto-close pairs,
//! list continuation, and auto-wrap.

use super::*;

impl<'a> App<'a> {
    /// Handles bracketed paste events (Cmd+V in iTerm2, etc).
    /// Inserts text into the rename buffer if renaming, otherwise into the editor.
    pub(super) fn handle_paste(&mut self, text: String) {
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
    pub(super) fn handle_key(&mut self, key: KeyEvent) {
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
    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent) {
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
                        // Click on filename area -> enter rename mode
                        self.start_rename();
                    }
                    return;
                }

                // Click on link in preview mode -> open URL
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
    pub(super) fn mouse_to_buffer_pos(&self, column: u16, row: u16) -> (u16, u16) {
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

    /// Auto-wraps the current line if it exceeds the visible text width.
    /// Called after text insertions to enforce line-width limits while typing.
    pub(super) fn auto_wrap_line(&mut self) {
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
                _ => break, // no good break point -- leave as-is
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
}
