//! UI rendering: main frame layout, editor view with syntax highlighting,
//! preview delegation, and help modal overlay.

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

        // Reflow editor content if terminal width changed
        let current_text_width = self.available_text_width();
        if current_text_width > 0 && current_text_width != self.last_wrap_width {
            self.reflow_content(current_text_width);
        }

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
        let height = 23u16.min(area.height.saturating_sub(2));
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let help_area = Rect::new(x, y, width, height);

        // Clear the area behind the modal
        frame.render_widget(Clear, help_area);

        // Help content -- must match the actual keybinding handlers!
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
        // so we can translate mouse coordinates -> buffer positions correctly.
        let cursor_row = self.textarea.cursor().0 as u16;
        if cursor_row < self.editor_scroll_top {
            self.editor_scroll_top = cursor_row;
        } else if self.editor_scroll_top + area.height <= cursor_row {
            self.editor_scroll_top = cursor_row + 1 - area.height;
        }

        // Render vim-style tilde markers for lines beyond the file content
        let total_lines = self.textarea.lines().len();
        let gutter_width = format!("{}", total_lines).len() as u16 + 1;
        let visible_content_lines = (total_lines as u16).saturating_sub(self.editor_scroll_top);
        if visible_content_lines < area.height {
            for row in visible_content_lines..area.height {
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
                        cell.set_char('\u{258E}'); // left quarter block
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
}
