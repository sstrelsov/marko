//! Visual soft-wrap state: maps logical buffer lines to visual rows
//! without modifying the underlying text buffer.

use crate::markdown::table_format;

/// A single visual row — a slice of one logical buffer line.
#[derive(Debug, Clone)]
pub struct VisualRow {
    /// Which logical buffer line this row comes from.
    pub logical_line: usize,
    /// Byte offset in the logical line where this visual row starts.
    pub byte_start: usize,
    /// Byte offset end (exclusive).
    pub byte_end: usize,
    /// Char offset start.
    pub char_start: usize,
    /// Char offset end (exclusive).
    pub char_end: usize,
    /// True if this is the first visual row of the logical line.
    pub is_first: bool,
    /// Visual-only continuation indent (e.g. "  " for list items).
    pub indent: String,
}

/// Precomputed mapping from logical lines to visual rows.
#[derive(Debug, Clone)]
pub struct WrapState {
    /// All visual rows in display order.
    pub rows: Vec<VisualRow>,
    /// `line_starts[i]` = index into `rows` where logical line `i` begins.
    pub line_starts: Vec<usize>,
    /// The width used to compute this wrap state.
    pub width: usize,
}

impl WrapState {
    /// Computes a new WrapState by visually wrapping each logical line to `width`.
    pub fn compute(lines: &[String], width: usize) -> Self {
        let mut rows = Vec::new();
        let mut line_starts = Vec::with_capacity(lines.len());

        for (line_idx, line) in lines.iter().enumerate() {
            line_starts.push(rows.len());

            if width == 0 {
                // Degenerate: just emit each line as one visual row
                rows.push(VisualRow {
                    logical_line: line_idx,
                    byte_start: 0,
                    byte_end: line.len(),
                    char_start: 0,
                    char_end: line.chars().count(),
                    is_first: true,
                    indent: String::new(),
                });
                continue;
            }

            let trimmed = line.trim_start();

            // Table lines: never wrap
            if trimmed.starts_with('|') {
                rows.push(VisualRow {
                    logical_line: line_idx,
                    byte_start: 0,
                    byte_end: line.len(),
                    char_start: 0,
                    char_end: line.chars().count(),
                    is_first: true,
                    indent: String::new(),
                });
                continue;
            }

            let char_count = line.chars().count();
            if char_count <= width {
                rows.push(VisualRow {
                    logical_line: line_idx,
                    byte_start: 0,
                    byte_end: line.len(),
                    char_start: 0,
                    char_end: char_count,
                    is_first: true,
                    indent: String::new(),
                });
                continue;
            }

            // Need to wrap: compute continuation indent
            let indent = table_format::continuation_indent(line);
            wrap_line_visual(line_idx, line, width, &indent, &mut rows);
        }

        WrapState {
            rows,
            line_starts,
            width,
        }
    }

    /// Returns the total number of visual rows.
    pub fn total_visual_rows(&self) -> usize {
        self.rows.len()
    }

    /// Returns the visual row index for a given logical cursor position (row, col in chars).
    pub fn visual_row_for_cursor(&self, row: usize, col: usize) -> usize {
        if row >= self.line_starts.len() {
            return self.rows.len().saturating_sub(1);
        }

        let start = self.line_starts[row];
        let end = if row + 1 < self.line_starts.len() {
            self.line_starts[row + 1]
        } else {
            self.rows.len()
        };

        // Find the visual row that contains this char column
        for i in start..end {
            let vr = &self.rows[i];
            if col >= vr.char_start && col < vr.char_end {
                return i;
            }
        }

        // Cursor at end of line: return the last visual row for this logical line
        end.saturating_sub(1)
    }

    /// Converts a visual row index + visual column to a logical (row, col) position.
    /// `visual_col` is the column within the visual row's text area (after gutter and indent).
    pub fn logical_pos_for_visual(&self, visual_row: usize, visual_col: usize) -> (usize, usize) {
        if visual_row >= self.rows.len() {
            if let Some(last) = self.rows.last() {
                return (last.logical_line, last.char_end);
            }
            return (0, 0);
        }

        let vr = &self.rows[visual_row];
        let indent_chars = if vr.is_first { 0 } else { vr.indent.chars().count() };
        let text_col = visual_col.saturating_sub(indent_chars);
        let logical_col = (vr.char_start + text_col).min(vr.char_end);
        (vr.logical_line, logical_col)
    }

    /// Returns the visual column for a logical cursor position within a visual row.
    pub fn visual_col_for_cursor(&self, row: usize, col: usize) -> usize {
        let vr_idx = self.visual_row_for_cursor(row, col);
        if vr_idx >= self.rows.len() {
            return 0;
        }
        let vr = &self.rows[vr_idx];
        let indent_chars = if vr.is_first { 0 } else { vr.indent.chars().count() };
        let text_offset = col.saturating_sub(vr.char_start);
        indent_chars + text_offset
    }
}

/// Wraps a single logical line into visual rows, recording offsets.
fn wrap_line_visual(
    line_idx: usize,
    line: &str,
    width: usize,
    continuation: &str,
    out: &mut Vec<VisualRow>,
) {
    let mut byte_pos: usize = 0;
    let mut char_pos: usize = 0;
    let mut is_first = true;

    while byte_pos < line.len() {
        let remaining = &line[byte_pos..];
        let remaining_chars: usize = remaining.chars().count();

        let prefix_chars = if is_first { 0 } else { continuation.chars().count() };
        let avail = width.saturating_sub(prefix_chars);

        if avail == 0 {
            // Can't fit anything after indent — emit rest as one row
            out.push(VisualRow {
                logical_line: line_idx,
                byte_start: byte_pos,
                byte_end: line.len(),
                char_start: char_pos,
                char_end: char_pos + remaining_chars,
                is_first,
                indent: if is_first {
                    String::new()
                } else {
                    continuation.to_string()
                },
            });
            break;
        }

        if remaining_chars <= avail {
            // Rest fits on this visual row
            out.push(VisualRow {
                logical_line: line_idx,
                byte_start: byte_pos,
                byte_end: line.len(),
                char_start: char_pos,
                char_end: char_pos + remaining_chars,
                is_first,
                indent: if is_first {
                    String::new()
                } else {
                    continuation.to_string()
                },
            });
            break;
        }

        // Find break point: byte offset at `avail` chars into remaining
        let search_end_byte = remaining
            .char_indices()
            .nth(avail)
            .map(|(i, _)| i)
            .unwrap_or(remaining.len());
        let search_region = &remaining[..search_end_byte];

        match search_region.rfind(' ') {
            Some(space_pos) if space_pos > 0 => {
                // Word break at space
                let chunk = &remaining[..space_pos];
                let chunk_chars = chunk.chars().count();

                out.push(VisualRow {
                    logical_line: line_idx,
                    byte_start: byte_pos,
                    byte_end: byte_pos + space_pos,
                    char_start: char_pos,
                    char_end: char_pos + chunk_chars,
                    is_first,
                    indent: if is_first {
                        String::new()
                    } else {
                        continuation.to_string()
                    },
                });

                // Skip past the space and any leading whitespace on next row
                let after_space = &remaining[space_pos..];
                let trimmed = after_space.trim_start();
                let space_and_ws_bytes = space_pos + (after_space.len() - trimmed.len());
                let skipped_chars = remaining[..space_and_ws_bytes].chars().count();

                byte_pos += space_and_ws_bytes;
                char_pos += skipped_chars;
            }
            _ => {
                // No space found — force break at char boundary
                let chunk_chars = avail;
                out.push(VisualRow {
                    logical_line: line_idx,
                    byte_start: byte_pos,
                    byte_end: byte_pos + search_end_byte,
                    char_start: char_pos,
                    char_end: char_pos + chunk_chars,
                    is_first,
                    indent: if is_first {
                        String::new()
                    } else {
                        continuation.to_string()
                    },
                });

                byte_pos += search_end_byte;
                char_pos += chunk_chars;
            }
        }

        is_first = false;
    }

    // Handle empty lines
    if line.is_empty() {
        out.push(VisualRow {
            logical_line: line_idx,
            byte_start: 0,
            byte_end: 0,
            char_start: 0,
            char_end: 0,
            is_first: true,
            indent: String::new(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_state_short_line_no_wrap() {
        let lines = vec!["hello world".to_string()];
        let ws = WrapState::compute(&lines, 80);
        assert_eq!(ws.total_visual_rows(), 1);
        assert_eq!(ws.rows[0].logical_line, 0);
        assert!(ws.rows[0].is_first);
    }

    #[test]
    fn wrap_state_basic_long_line() {
        let lines = vec!["the quick brown fox jumps over the lazy dog".to_string()];
        let ws = WrapState::compute(&lines, 20);
        assert!(
            ws.total_visual_rows() > 1,
            "Long line should split into multiple visual rows, got {}",
            ws.total_visual_rows()
        );
        // All rows should reference logical line 0
        for row in &ws.rows {
            assert_eq!(row.logical_line, 0);
        }
        // First row is_first, rest are not
        assert!(ws.rows[0].is_first);
        for row in &ws.rows[1..] {
            assert!(!row.is_first);
        }
    }

    #[test]
    fn wrap_state_list_continuation() {
        let lines = vec![
            "- this is a very long list item that should wrap with proper indentation".to_string(),
        ];
        let ws = WrapState::compute(&lines, 40);
        assert!(ws.total_visual_rows() > 1);
        // Continuation rows should have "  " indent (2 spaces for "- ")
        for row in &ws.rows[1..] {
            assert_eq!(row.indent, "  ", "List continuation should have 2-space indent");
        }
    }

    #[test]
    fn wrap_state_table_no_wrap() {
        let lines = vec![
            "| a very long cell value here | another long cell value that exceeds width |"
                .to_string(),
        ];
        let ws = WrapState::compute(&lines, 20);
        assert_eq!(
            ws.total_visual_rows(),
            1,
            "Table lines should never wrap"
        );
    }

    #[test]
    fn wrap_state_cursor_mapping_round_trip() {
        let lines = vec!["the quick brown fox jumps over the lazy dog".to_string()];
        let ws = WrapState::compute(&lines, 20);

        // Cursor at beginning
        let vr = ws.visual_row_for_cursor(0, 0);
        assert_eq!(vr, 0);

        // Cursor at end of first visual row should be in row 0
        let first_end = ws.rows[0].char_end;
        let vr = ws.visual_row_for_cursor(0, first_end.saturating_sub(1));
        assert_eq!(vr, 0);

        // Cursor past first visual row should be in row 1
        let vr = ws.visual_row_for_cursor(0, first_end + 1);
        assert!(vr > 0);
    }

    #[test]
    fn wrap_state_multiple_lines() {
        let lines = vec![
            "short line".to_string(),
            "another short line".to_string(),
            "this is a much longer line that should definitely wrap at width twenty".to_string(),
        ];
        let ws = WrapState::compute(&lines, 20);
        // First two lines: 1 visual row each
        // Third line: multiple visual rows
        assert_eq!(ws.line_starts[0], 0);
        assert_eq!(ws.line_starts[1], 1);
        assert_eq!(ws.line_starts[2], 2);
        assert!(ws.total_visual_rows() > 3);
    }

    #[test]
    fn wrap_state_empty_line() {
        let lines = vec![
            "hello".to_string(),
            "".to_string(),
            "world".to_string(),
        ];
        let ws = WrapState::compute(&lines, 80);
        assert_eq!(ws.total_visual_rows(), 3);
        assert_eq!(ws.rows[1].logical_line, 1);
        assert_eq!(ws.rows[1].char_start, 0);
        assert_eq!(ws.rows[1].char_end, 0);
    }

    #[test]
    fn wrap_state_blockquote_continuation() {
        let lines = vec![
            "> this is a long blockquote that should wrap while preserving the quote marker prefix"
                .to_string(),
        ];
        let ws = WrapState::compute(&lines, 40);
        assert!(ws.total_visual_rows() > 1);
        for row in &ws.rows[1..] {
            assert_eq!(row.indent, "> ", "Blockquote continuation should have '> ' indent");
        }
    }
}
