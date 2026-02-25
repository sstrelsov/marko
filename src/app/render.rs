//! UI rendering: main frame layout, custom visual-wrap editor, syntax highlighting,
//! preview delegation, and help modal overlay.

use super::wrap::WrapState;
use super::*;

/// Pre-computes syntax highlighting for all code fence regions.
/// Returns a parallel vec: [region_idx][line_offset] -> Vec<(fg_color, text)>.
pub(super) fn highlight_code_regions(
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
    /// Runs one frame of the main loop: draw + tick.
    /// This is the canonical render path -- tested by render_test to ensure
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
        let top_divider = Paragraph::new("\u{2500}".repeat(chunks[1].width as usize))
            .style(divider_style);
        frame.render_widget(top_divider, chunks[1]);
        let bottom_divider = Paragraph::new("\u{2500}".repeat(chunks[3].width as usize))
            .style(divider_style);
        frame.render_widget(bottom_divider, chunks[3]);

        // Content area -- render depends on current mode
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
                update_available: self.update_available,
            },
        );

        // Help modal overlay -- rendered last so it sits on top of everything
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
        let height = 25u16.min(area.height.saturating_sub(2));
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let help_area = Rect::new(x, y, width, height);

        // Clear the area behind the modal
        frame.render_widget(Clear, help_area);

        // Help content -- must match the actual keybinding handlers!
        // Grouped: global, editor, tui-textarea built-ins, mouse
        // Version + update line
        let version = self_update::cargo_crate_version!();
        let version_line = if self.update_available {
            Line::from(vec![
                Span::styled(
                    format!("  marko v{version}  "),
                    Style::default().fg(theme::LINE_NUMBER),
                ),
                Span::styled(
                    "Update available — run `marko upgrade`",
                    Style::default().fg(theme::WARNING),
                ),
            ])
        } else {
            Line::from(Span::styled(
                format!("  marko v{version}"),
                Style::default().fg(theme::LINE_NUMBER),
            ))
        };

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
            Line::from(""),
            version_line,
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

    /// Ensures the WrapState is up-to-date for the current content and width.
    fn ensure_wrap_state(&mut self, text_width: usize) {
        let needs_recompute = match &self.wrap_state {
            None => true,
            Some(ws) => ws.width != text_width,
        };
        if needs_recompute || self.code_fence_dirty {
            let lines: Vec<String> = self.textarea.lines().iter().map(|s| s.to_string()).collect();
            self.wrap_state = Some(WrapState::compute(&lines, text_width));
        }
    }

    /// Custom visual-wrap editor renderer.
    /// Reads from tui-textarea's buffer but renders with visual wrapping.
    fn render_editor(&mut self, frame: &mut Frame, area: Rect) {
        let lines: Vec<String> = self.textarea.lines().iter().map(|s| s.to_string()).collect();

        // Compute gutter width from logical line count
        let gutter_w = self.gutter_width();
        let text_width = (area.width as usize).saturating_sub(gutter_w as usize);

        // Recompute wrap state if needed
        self.ensure_wrap_state(text_width);
        let ws = self.wrap_state.as_ref().unwrap();

        // Get cursor position (logical)
        let (cursor_row, cursor_col) = self.textarea.cursor();
        let cursor_visual_row = ws.visual_row_for_cursor(cursor_row, cursor_col);

        // Ensure cursor is visible — but only when the user is NOT wheel-scrolling.
        // When scroll_cursor is set, the user is scrolling the viewport independently
        // of the cursor, so we must not snap back.
        let vp_height = area.height as usize;

        if self.scroll_cursor.is_none() {
            let scroll_top = self.editor_scroll_top as usize;
            if cursor_visual_row < scroll_top {
                self.editor_scroll_top = cursor_visual_row as u16;
            } else if cursor_visual_row >= scroll_top + vp_height {
                self.editor_scroll_top = (cursor_visual_row + 1).saturating_sub(vp_height) as u16;
            }
        }

        let scroll_top = self.editor_scroll_top as usize;
        let total_visual = ws.total_visual_rows();

        // Get selection range for highlighting
        let selection = self.textarea.selection_range();

        // Fill background
        let bg_style = theme::editor_style();
        let bg = Paragraph::new("").style(bg_style);
        frame.render_widget(bg, area);

        // Render visible visual rows
        let visible_count = vp_height.min(total_visual.saturating_sub(scroll_top));
        let buf = frame.buffer_mut();

        for screen_row in 0..visible_count {
            let vr_idx = scroll_top + screen_row;
            if vr_idx >= ws.rows.len() {
                break;
            }
            let vr = &ws.rows[vr_idx];
            let screen_y = area.y + screen_row as u16;

            // ── Line number gutter ──
            if gutter_w > 0 {
                let num_str = if vr.is_first {
                    format!("{}", vr.logical_line + 1)
                } else {
                    String::new()
                };
                // Right-align within gutter: " {num} "
                // gutter_w includes 1 leading space + digits + 1 trailing space
                let digit_width = (gutter_w - 2) as usize;
                let padded = format!(" {:>width$} ", num_str, width = digit_width);
                let ln_style = Style::default().fg(theme::LINE_NUMBER);
                for (i, ch) in padded.chars().enumerate() {
                    let x = area.x + i as u16;
                    if x < area.x + area.width {
                        if let Some(cell) = buf.cell_mut((x, screen_y)) {
                            cell.set_char(ch);
                            cell.set_style(ln_style);
                        }
                    }
                }
            }

            // ── Text content ──
            let text_x_start = area.x + gutter_w;
            let line_text = &lines[vr.logical_line];
            let slice = &line_text[vr.byte_start..vr.byte_end];

            // Build the display string: continuation indent (visual only) + text
            let display: String = if !vr.is_first {
                format!("{}{}", vr.indent, slice)
            } else {
                slice.to_string()
            };

            let indent_chars = if vr.is_first { 0 } else { vr.indent.chars().count() };

            // Determine if this row is on the cursor's logical line (for line highlight)
            let is_cursor_line = vr.logical_line == cursor_row;

            for (i, ch) in display.chars().enumerate() {
                let x = text_x_start + i as u16;
                if x >= area.x + area.width {
                    break;
                }
                if let Some(cell) = buf.cell_mut((x, screen_y)) {
                    cell.set_char(ch);

                    // Determine logical char position for this cell
                    let in_indent = i < indent_chars;
                    let logical_char = if in_indent {
                        None
                    } else {
                        Some(vr.char_start + (i - indent_chars))
                    };

                    // Check if this cell is in the selection
                    let in_selection = if let Some(((sr, sc), (er, ec))) = selection {
                        if let Some(lc) = logical_char {
                            let pos = (vr.logical_line, lc);
                            pos >= (sr, sc) && pos < (er, ec)
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // Check if this is the cursor cell
                    let is_cursor = is_cursor_line
                        && self.scroll_cursor.is_none()
                        && logical_char == Some(cursor_col);

                    if is_cursor {
                        cell.set_style(
                            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
                        );
                    } else if in_selection {
                        cell.set_style(Style::default().bg(theme::SELECTION));
                    } else if is_cursor_line {
                        // cursor line style (subtle highlight)
                        cell.set_style(theme::cursor_line_style());
                    }
                }
            }

            // If cursor is at end of line (past last char), render cursor block
            if is_cursor_line && self.scroll_cursor.is_none() {
                let cursor_visual_col = ws.visual_col_for_cursor(cursor_row, cursor_col);
                let cursor_x = text_x_start + cursor_visual_col as u16;
                // Only render if cursor is past the last character we drew
                let drawn_chars = display.chars().count();
                if cursor_visual_col >= drawn_chars && cursor_x < area.x + area.width {
                    if let Some(cell) = buf.cell_mut((cursor_x, screen_y)) {
                        cell.set_char(' ');
                        cell.set_style(
                            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
                        );
                    }
                }
            }

            // Fill remaining cells on cursor line with cursor_line_style
            if is_cursor_line {
                let filled = display.chars().count();
                let cursor_vis_col = if is_cursor_line && self.scroll_cursor.is_none() {
                    ws.visual_col_for_cursor(cursor_row, cursor_col)
                } else {
                    0
                };
                let fill_start = filled.max(if cursor_vis_col >= filled { cursor_vis_col + 1 } else { filled });
                for i in fill_start..text_width {
                    let x = text_x_start + i as u16;
                    if x >= area.x + area.width {
                        break;
                    }
                    if let Some(cell) = buf.cell_mut((x, screen_y)) {
                        cell.set_style(theme::cursor_line_style());
                    }
                }
            }
        }

        // ── Tilde markers for empty rows beyond content ──
        if visible_count < vp_height {
            for screen_row in visible_count..vp_height {
                let screen_y = area.y + screen_row as u16;
                // Gutter space
                if gutter_w > 0 {
                    let pad = " ".repeat(gutter_w as usize);
                    for (i, ch) in pad.chars().enumerate() {
                        let x = area.x + i as u16;
                        if let Some(cell) = buf.cell_mut((x, screen_y)) {
                            cell.set_char(ch);
                            cell.set_style(Style::default().fg(theme::TILDE));
                        }
                    }
                }
                // Tilde
                let tilde_x = area.x + gutter_w;
                if tilde_x < area.x + area.width {
                    if let Some(cell) = buf.cell_mut((tilde_x, screen_y)) {
                        cell.set_char('~');
                        cell.set_style(Style::default().fg(theme::TILDE));
                    }
                }
            }
        }

        // ── Code fence syntax highlighting overlay ──
        self.apply_code_fence_highlighting(frame, area, gutter_w);

        // ── Git gutter markers ──
        if !self.gutter_marks.is_empty() && self.wrap_state.is_some() {
            let ws = self.wrap_state.as_ref().unwrap();
            let scroll_top = self.editor_scroll_top as usize;
            let visible_count = vp_height.min(ws.total_visual_rows().saturating_sub(scroll_top));
            let buf = frame.buffer_mut();
            for screen_row in 0..visible_count {
                let vr_idx = scroll_top + screen_row;
                if vr_idx >= ws.rows.len() {
                    break;
                }
                let vr = &ws.rows[vr_idx];
                // Only show gutter mark on first visual row of a logical line
                if vr.is_first {
                    if let Some(mark) = self.gutter_marks.get(&vr.logical_line) {
                        let color = match mark {
                            GutterMark::Added => theme::GIT_ADDED,
                            GutterMark::Modified => theme::GIT_MODIFIED,
                            GutterMark::Removed => theme::GIT_REMOVED,
                        };
                        let screen_y = area.y + screen_row as u16;
                        if let Some(cell) = buf.cell_mut((area.x, screen_y)) {
                            cell.set_char('\u{258E}'); // left quarter block
                            cell.set_fg(color);
                        }
                    }
                }
            }
        }
    }

    /// Overlays syntax highlighting on the ratatui buffer for code fence regions.
    /// Post-processes cells after the custom renderer, overwriting foreground
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

        let ws = match &self.wrap_state {
            Some(ws) => ws,
            None => return,
        };

        let scroll_top = self.editor_scroll_top as usize;
        let vp_height = area.height as usize;
        let cursor_pos = self.textarea.cursor();

        for (region_idx, region) in self.code_fence_regions.iter().enumerate() {
            let highlights = match self.code_fence_highlights.get(region_idx) {
                Some(h) => h,
                None => continue,
            };

            let content_start = region.start_line + 1;

            for (line_offset, spans) in highlights.iter().enumerate() {
                let line_idx = content_start + line_offset;

                // Find visual rows for this logical line
                if line_idx >= ws.line_starts.len() {
                    continue;
                }
                let vr_start = ws.line_starts[line_idx];
                let vr_end = if line_idx + 1 < ws.line_starts.len() {
                    ws.line_starts[line_idx + 1]
                } else {
                    ws.rows.len()
                };

                // For simplicity, apply highlight spans to the first visual row only
                // (code fence content usually doesn't wrap much)
                for vr_idx in vr_start..vr_end {
                    if vr_idx < scroll_top || vr_idx >= scroll_top + vp_height {
                        continue;
                    }
                    let vr = &ws.rows[vr_idx];
                    let screen_row = area.y + (vr_idx - scroll_top) as u16;
                    let text_start_x = area.x + gutter_width;
                    let indent_chars = if vr.is_first { 0 } else { vr.indent.chars().count() };

                    let mut col_offset: u16 = indent_chars as u16;
                    // Compute which portion of the highlight spans falls in this visual row
                    let mut char_pos: usize = 0;

                    for (fg_color, text) in spans {
                        for _ch in text.chars() {
                            if char_pos >= vr.char_start && char_pos < vr.char_end {
                                let cell_x = text_start_x + col_offset;
                                if cell_x >= area.x + area.width {
                                    break;
                                }

                                let logical_col = char_pos;
                                let is_cursor_cell = line_idx == cursor_pos.0
                                    && logical_col == cursor_pos.1;

                                if !is_cursor_cell {
                                    let buf = frame.buffer_mut();
                                    if let Some(cell) = buf.cell_mut((cell_x, screen_row)) {
                                        let bg = cell.bg;
                                        if bg == ratatui::style::Color::Reset {
                                            cell.set_fg(*fg_color);
                                        }
                                    }
                                }
                                col_offset += 1;
                            }
                            char_pos += 1;
                        }
                    }
                }
            }
        }
    }
}
