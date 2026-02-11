//! File saving: write editor content to disk with table formatting and docx export.

use super::*;

impl<'a> App<'a> {
    /// Writes the current editor content to disk and resets the modified flag.
    /// Runs table auto-formatting before writing.
    pub(super) fn save(&mut self) {
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
}
