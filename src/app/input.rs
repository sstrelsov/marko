//! Input handling: keyboard events, mouse events, paste, auto-close pairs,
//! and list continuation.

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
        }
    }

    // ─── Key handling ────────────────────────────────────────────────────

    /// Main key handler. Processes modal states first, then Esc-as-back,
    /// then global keybindings, then delegates to mode-specific handlers.
    pub(super) fn handle_key(&mut self, key: KeyEvent) {
        // Any keystroke clears scroll-preserved state and restores cursor
        self.clear_scroll_state();

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
            // Visual Up/Down: navigate visual rows (soft-wrapped lines)
            (KeyModifiers::NONE, KeyCode::Up) => {
                self.move_cursor_visual(true);
                return;
            }
            (KeyModifiers::NONE, KeyCode::Down) => {
                self.move_cursor_visual(false);
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
        let is_navigation = matches!(
            key.code,
            KeyCode::Left
                | KeyCode::Right
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown
        );

        let input = Input::from(key);
        self.textarea.input(input);

        if !is_navigation {
            self.update_modified();
        }
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
            // Scroll wheel: when mid-drag, extend selection via CursorMove so the
            // highlight follows the scroll. Otherwise delegate to tui-textarea.
            MouseEventKind::ScrollUp => match self.mode {
                Mode::Editor => {
                    if self.mouse_dragging {
                        self.textarea.move_cursor(CursorMove::Up);
                    } else {
                        self.editor_scroll(true);
                    }
                }
                Mode::Preview => self.preview.scroll_up(SCROLL_LINES),
            },
            MouseEventKind::ScrollDown => match self.mode {
                Mode::Editor => {
                    if self.mouse_dragging {
                        self.textarea.move_cursor(CursorMove::Down);
                    } else {
                        self.editor_scroll(false);
                    }
                }
                Mode::Preview => self.preview.scroll_down(SCROLL_LINES, self.viewport_height),
            },

            // Left click: header tabs/filename or editor cursor positioning + drag start
            MouseEventKind::Down(MouseButton::Left) => {
                // Any click clears scroll-preserved state and restores cursor
                self.clear_scroll_state();
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

            // Left drag: extend selection to current mouse position.
            // When the mouse is at or beyond a viewport edge (including outside the
            // terminal window), set drag_auto_scroll so tick() keeps scrolling via
            // CursorMove::Up/Down — the terminal stops sending events once coords
            // are clamped to the boundary, so a timer is needed for continuous scroll.
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.mode == Mode::Editor && self.mouse_dragging {
                    let area = self.content_area;

                    if mouse.row < area.y {
                        // At or above viewport top — initial jump + enable timer
                        self.drag_auto_scroll = Some(DragAutoScroll::Up);
                        self.textarea.move_cursor(CursorMove::Up);
                    } else if mouse.row >= area.y + area.height {
                        // At or below viewport bottom — initial jump + enable timer
                        self.drag_auto_scroll = Some(DragAutoScroll::Down);
                        self.textarea.move_cursor(CursorMove::Down);
                    } else {
                        // Within viewport: cancel any auto-scroll and move cursor to mouse position
                        self.drag_auto_scroll = None;
                        let clamped_col = mouse.column.max(area.x).min(area.x + area.width - 1);
                        let (buffer_row, buffer_col) = self.mouse_to_buffer_pos(clamped_col, mouse.row);
                        self.textarea
                            .move_cursor(CursorMove::Jump(buffer_row, buffer_col));
                    }
                }
            }

            // Left release: finalize selection (cancel if it was just a click with no drag)
            MouseEventKind::Up(MouseButton::Left) => {
                if self.mouse_dragging {
                    self.mouse_dragging = false;
                    self.drag_auto_scroll = None;
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
    /// accounting for the line number gutter width, scroll offset, and visual wrapping.
    pub(super) fn mouse_to_buffer_pos(&self, column: u16, row: u16) -> (u16, u16) {
        let area = self.content_area;
        let gutter_width = self.gutter_width();
        let relative_row = row.saturating_sub(area.y) as usize;
        let visual_row = relative_row + self.editor_scroll_top as usize;
        let relative_col = column.saturating_sub(area.x).saturating_sub(gutter_width) as usize;

        if let Some(ref ws) = self.wrap_state {
            let (logical_row, logical_col) = ws.logical_pos_for_visual(visual_row, relative_col);
            let total_lines = self.textarea.lines().len();
            let clamped_row = logical_row.min(total_lines.saturating_sub(1));
            (clamped_row as u16, logical_col as u16)
        } else {
            // Fallback: no wrap state yet, treat as 1:1 mapping
            let total_lines = self.textarea.lines().len();
            let buffer_row = visual_row.min(total_lines.saturating_sub(1));
            (buffer_row as u16, relative_col as u16)
        }
    }

    // ─── Scroll helpers ──────────────────────────────────────────────────

    /// Clears scroll-preserved state and restores cursor visibility.
    /// Called on click or keystroke — any direct user interaction that ends
    /// the "just scrolling" session.
    fn clear_scroll_state(&mut self) {
        if self.scroll_cursor.is_some() {
            // Restore normal cursor style
            self.textarea.set_cursor_style(
                Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
            );
        }
        self.scroll_cursor = None;
        self.scroll_anchor = None;
    }

    /// Scrolls the editor viewport by one visual row without moving the logical cursor.
    ///
    /// Since we now render our own editor (not tui-textarea's widget), we control
    /// scrolling directly via `editor_scroll_top` which counts visual rows.
    /// We still track the true cursor/selection in `scroll_cursor`/`scroll_anchor`
    /// so the custom renderer can display them correctly.
    fn editor_scroll(&mut self, up: bool) -> bool {
        // Compute max scroll using visual rows
        let total_visual = self
            .wrap_state
            .as_ref()
            .map(|ws| ws.total_visual_rows() as u16)
            .unwrap_or_else(|| self.textarea.lines().len() as u16);
        let max_scroll = total_visual.saturating_sub(self.viewport_height);

        // Bounds check
        if up {
            if self.editor_scroll_top == 0 {
                return false;
            }
        } else if self.editor_scroll_top >= max_scroll {
            return false;
        }

        // ── 1. Determine the TRUE cursor and anchor ──────────────────
        let true_cursor = self.scroll_cursor.unwrap_or_else(|| self.textarea.cursor());

        let true_anchor: Option<(usize, usize)> = self.scroll_anchor.or_else(|| {
            self.textarea.selection_range().map(|range| {
                if true_cursor == range.0 {
                    range.1
                } else {
                    range.0
                }
            })
        });

        // ── 2. Update scroll position ─────────────────────────────────
        self.textarea.cancel_selection();

        if up {
            self.editor_scroll_top = self.editor_scroll_top.saturating_sub(1);
        } else {
            self.editor_scroll_top = (self.editor_scroll_top + 1).min(max_scroll);
        }

        // Check if cursor's visual row is still in viewport
        let vp_top = self.editor_scroll_top as usize;
        let vp_bottom =
            (self.editor_scroll_top + self.viewport_height).saturating_sub(1) as usize;

        // Map the logical cursor to a visual row to check viewport visibility
        let cursor_vr = self
            .wrap_state
            .as_ref()
            .map(|ws| ws.visual_row_for_cursor(true_cursor.0, true_cursor.1))
            .unwrap_or(true_cursor.0);

        let cursor_in_vp = cursor_vr >= vp_top && cursor_vr <= vp_bottom;

        // ── 3. Restore cursor + selection ────────────────────────────
        if cursor_in_vp {
            self.textarea.move_cursor(CursorMove::Jump(
                true_cursor.0 as u16,
                true_cursor.1 as u16,
            ));

            if let Some(anchor) = true_anchor {
                self.textarea
                    .move_cursor(CursorMove::Jump(anchor.0 as u16, anchor.1 as u16));
                self.textarea.start_selection();
                self.textarea.move_cursor(CursorMove::Jump(
                    true_cursor.0 as u16,
                    true_cursor.1 as u16,
                ));
            }

            if self.scroll_cursor.is_some() {
                self.textarea.set_cursor_style(
                    Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
                );
            }
            self.scroll_cursor = None;
            self.scroll_anchor = None;
        } else {
            self.scroll_cursor = Some(true_cursor);
            if true_anchor.is_some() {
                self.scroll_anchor = true_anchor;
            }

            self.textarea.set_cursor_style(Style::default());

            if let Some(anchor) = true_anchor {
                let sel_top = anchor.0.min(true_cursor.0);
                let sel_bottom = anchor.0.max(true_cursor.0);

                if sel_top <= vp_bottom && sel_bottom >= vp_top {
                    let vis_start = anchor.0.clamp(vp_top, vp_bottom).clamp(sel_top, sel_bottom);
                    let vis_end = true_cursor
                        .0
                        .clamp(vp_top, vp_bottom)
                        .clamp(sel_top, sel_bottom);

                    self.textarea
                        .move_cursor(CursorMove::Jump(vis_start as u16, anchor.1 as u16));
                    self.textarea.start_selection();
                    self.textarea.move_cursor(CursorMove::Jump(
                        vis_end as u16,
                        true_cursor.1 as u16,
                    ));
                }
            }
        }

        true
    }

    // ─── Internal helpers ────────────────────────────────────────────────

    /// Moves the cursor up or down by one visual row.
    /// Within a wrapped logical line, this moves between visual rows.
    /// At line boundaries, this moves to the adjacent logical line.
    fn move_cursor_visual(&mut self, up: bool) {
        let ws = match &self.wrap_state {
            Some(ws) => ws,
            None => {
                // No wrap state — fall back to tui-textarea's movement
                let input = Input::from(KeyEvent::new(
                    if up { KeyCode::Up } else { KeyCode::Down },
                    KeyModifiers::NONE,
                ));
                self.textarea.input(input);
                return;
            }
        };

        let (row, col) = self.textarea.cursor();
        let cur_vr = ws.visual_row_for_cursor(row, col);
        let cur_vis_col = ws.visual_col_for_cursor(row, col);

        let target_vr = if up {
            if cur_vr == 0 {
                return; // at top
            }
            cur_vr - 1
        } else {
            if cur_vr + 1 >= ws.total_visual_rows() {
                return; // at bottom
            }
            cur_vr + 1
        };

        let (new_row, new_col) = ws.logical_pos_for_visual(target_vr, cur_vis_col);
        self.textarea
            .move_cursor(CursorMove::Jump(new_row as u16, new_col as u16));
    }

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

}
