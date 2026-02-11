//! Text selection helpers: get selected text, word selection, paragraph selection.
//!
//! Used by clipboard copy (Ctrl+C), double-click (word), and triple-click (paragraph).

use super::*;

impl<'a> App<'a> {
    /// Extracts the currently selected text from tui-textarea using selection_range().
    pub(super) fn get_selected_text(&self) -> Option<String> {
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
    pub(super) fn select_word_at_cursor(&mut self) {
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
    pub(super) fn select_paragraph_at_cursor(&mut self) {
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
}
