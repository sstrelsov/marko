/// Hard-wraps long lines to fit within `width`, preserving markdown structure.
/// Skips code fences and table lines (tables are handled by `format_tables`).
pub fn hard_wrap(content: &str, width: usize) -> String {
    if width == 0 {
        return content.to_string();
    }
    let lines: Vec<&str> = content.lines().collect();
    let mut result: Vec<String> = Vec::new();
    let mut in_code_fence = false;

    for line in &lines {
        // Track code fences — never wrap inside them
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_fence = !in_code_fence;
            result.push(line.to_string());
            continue;
        }
        if in_code_fence {
            result.push(line.to_string());
            continue;
        }

        // Skip headings — wrapping breaks the # prefix syntax
        if trimmed.starts_with('#') {
            result.push(line.to_string());
            continue;
        }

        // Skip table lines (contain | and look like table rows)
        if line.contains('|') && is_separator_row(line) {
            result.push(line.to_string());
            continue;
        }
        if line.contains('|') && line.trim_start().starts_with('|') {
            result.push(line.to_string());
            continue;
        }

        // Line fits — keep as-is
        if line.len() <= width {
            result.push(line.to_string());
            continue;
        }

        // Determine continuation indent from the line's leading structure
        let indent = continuation_indent(line);
        wrap_line(line, width, &indent, &mut result);
    }

    result.join("\n")
}

/// Figures out what indent continuation lines should use.
/// e.g. "- item text" → "  " (align with content after bullet)
///      "> quoted"    → "> "
///      "  text"      → "  " (preserve leading whitespace)
pub fn continuation_indent(line: &str) -> String {
    let leading_ws: String = line.chars().take_while(|c| c.is_whitespace()).collect();
    let rest = &line[leading_ws.len()..];

    // Ordered list: "1. ", "12. ", etc.
    if let Some(pos) = rest.find(". ") {
        if rest[..pos].chars().all(|c| c.is_ascii_digit()) && pos <= 4 {
            return " ".repeat(leading_ws.len() + pos + 2);
        }
    }
    // Unordered list: "- ", "* ", "+ "
    if rest.starts_with("- ") || rest.starts_with("* ") || rest.starts_with("+ ") {
        return " ".repeat(leading_ws.len() + 2);
    }
    // Blockquote: "> "
    if rest.starts_with("> ") {
        return format!("{}> ", leading_ws);
    }
    // Plain text: preserve leading whitespace
    leading_ws
}

/// Word-wraps a single line, pushing wrapped segments into `out`.
fn wrap_line(line: &str, width: usize, continuation: &str, out: &mut Vec<String>) {
    let mut remaining = line;
    let mut is_first = true;

    while !remaining.is_empty() {
        let prefix = if is_first { "" } else { continuation };
        let avail = width.saturating_sub(prefix.len());
        if avail == 0 {
            // Can't fit even the prefix; just emit what's left
            out.push(format!("{}{}", prefix, remaining));
            break;
        }

        if prefix.len() + remaining.len() <= width {
            out.push(format!("{}{}", prefix, remaining));
            break;
        }

        // Find the last space within the available width to break at
        let search_region = &remaining[..avail.min(remaining.len())];
        let break_at = search_region.rfind(' ');
        match break_at {
            Some(pos) if pos > 0 => {
                out.push(format!("{}{}", prefix, &remaining[..pos]));
                remaining = remaining[pos..].trim_start();
            }
            _ => {
                // No space found — force break at avail
                let split = avail.min(remaining.len());
                out.push(format!("{}{}", prefix, &remaining[..split]));
                remaining = &remaining[split..];
            }
        }
        is_first = false;
    }
}

/// Formats markdown tables in the given content to fill the available terminal width.
///
/// Tables are detected as consecutive lines containing `|` characters where at least
/// one line matches the separator pattern `|---|`. Non-table content passes through unchanged.
pub fn format_tables(content: &str, terminal_width: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        // Try to detect a table block starting at this line
        if let Some((table_end, formatted)) = try_format_table(&lines, i, terminal_width) {
            result.extend(formatted);
            i = table_end;
        } else {
            result.push(lines[i].to_string());
            i += 1;
        }
    }

    result.join("\n")
}

/// Tries to parse and format a table block starting at line index `start`.
/// Returns `Some((end_index, formatted_lines))` if a valid table was found,
/// or `None` if this isn't a table.
fn try_format_table(lines: &[&str], start: usize, terminal_width: usize) -> Option<(usize, Vec<String>)> {
    // Collect consecutive lines that look like table rows (contain |)
    let mut end = start;
    while end < lines.len() && lines[end].contains('|') {
        end += 1;
    }

    // Need at least 2 lines (header + separator) to be a table
    if end - start < 2 {
        return None;
    }

    // Verify there's a separator row (contains |---| pattern)
    let has_separator = lines[start..end].iter().any(|line| is_separator_row(line));
    if !has_separator {
        return None;
    }

    // Parse cells from each row
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut separator_indices: Vec<usize> = Vec::new();

    for (idx, &line) in lines[start..end].iter().enumerate() {
        if is_separator_row(line) {
            separator_indices.push(idx);
            rows.push(Vec::new()); // placeholder
        } else {
            rows.push(parse_cells(line));
        }
    }

    if rows.is_empty() {
        return None;
    }

    // Determine number of columns from the widest row (excluding separator placeholders)
    let num_cols = rows.iter()
        .filter(|r| !r.is_empty())
        .map(|r| r.len())
        .max()
        .unwrap_or(0);

    if num_cols == 0 {
        return None;
    }

    // Calculate natural (minimum) column widths
    let mut natural_widths: Vec<usize> = vec![0; num_cols];
    for row in &rows {
        for (j, cell) in row.iter().enumerate() {
            if j < num_cols {
                natural_widths[j] = natural_widths[j].max(cell.len());
            }
        }
    }

    // Ensure minimum width of 3 for each column (for separator dashes)
    for w in &mut natural_widths {
        if *w < 3 {
            *w = 3;
        }
    }

    // Calculate available width for cell content:
    // Total = | col1 | col2 | col3 | → (num_cols + 1) pipes + 2 spaces per col
    let border_overhead = (num_cols + 1) + (num_cols * 2); // |'s + spaces
    let available = terminal_width.saturating_sub(border_overhead);
    let natural_total: usize = natural_widths.iter().sum();

    // Distribute widths proportionally to fill the screen (or shrink to fit)
    let col_widths: Vec<usize> = if natural_total > 0 && available > natural_total {
        distribute_widths(&natural_widths, available)
    } else if natural_total > available && available > 0 {
        shrink_widths(&natural_widths, available)
    } else {
        natural_widths.clone()
    };

    // Rebuild formatted rows
    let mut formatted = Vec::new();
    for (idx, row) in rows.iter().enumerate() {
        if separator_indices.contains(&idx) {
            // Separator row
            let sep: Vec<String> = col_widths.iter().map(|&w| "-".repeat(w)).collect();
            formatted.push(format!("| {} |", sep.join(" | ")));
        } else {
            // Data row — pad or truncate each cell to fit column width
            let mut cells: Vec<String> = Vec::new();
            for j in 0..num_cols {
                let content = row.get(j).map(|s| s.as_str()).unwrap_or("");
                let width = col_widths[j];
                let truncated: String = if content.len() > width {
                    content.chars().take(width).collect()
                } else {
                    content.to_string()
                };
                cells.push(format!("{:<width$}", truncated, width = width));
            }
            formatted.push(format!("| {} |", cells.join(" | ")));
        }
    }

    Some((end, formatted))
}

/// Returns true if the line looks like a markdown table separator row.
fn is_separator_row(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.contains('|') {
        return false;
    }
    // Check that cells contain only dashes, colons, and whitespace
    let cells: Vec<&str> = trimmed.split('|').collect();
    let inner = if cells.first().map_or(false, |c| c.trim().is_empty()) {
        &cells[1..]
    } else {
        &cells[..]
    };
    let inner = if inner.last().map_or(false, |c| c.trim().is_empty()) {
        &inner[..inner.len() - 1]
    } else {
        inner
    };

    if inner.is_empty() {
        return false;
    }

    inner.iter().all(|cell| {
        let c = cell.trim();
        !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' ')
    })
}

/// Parses a markdown table row into cells, trimming leading/trailing pipes and whitespace.
fn parse_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    // Strip leading and trailing |
    let inner = trimmed
        .strip_prefix('|')
        .unwrap_or(trimmed);
    let inner = inner
        .strip_suffix('|')
        .unwrap_or(inner);

    inner
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

/// Shrinks column widths proportionally to fit within available space,
/// ensuring each column gets at least 3 characters.
fn shrink_widths(natural: &[usize], available: usize) -> Vec<usize> {
    let natural_total: usize = natural.iter().sum();
    if natural_total == 0 {
        return natural.to_vec();
    }

    let min_col: usize = 3;
    let mut widths = Vec::with_capacity(natural.len());
    let mut used = 0;

    for (i, &w) in natural.iter().enumerate() {
        if i == natural.len() - 1 {
            // Last column gets the remainder
            widths.push(available.saturating_sub(used).max(min_col));
        } else {
            let share = (available as f64 * w as f64 / natural_total as f64).floor() as usize;
            let clamped = share.max(min_col);
            widths.push(clamped);
            used += clamped;
        }
    }

    widths
}

/// Distributes available width proportionally across columns,
/// ensuring each column gets at least its natural width.
fn distribute_widths(natural: &[usize], available: usize) -> Vec<usize> {
    let natural_total: usize = natural.iter().sum();
    if natural_total == 0 || natural_total >= available {
        return natural.to_vec();
    }

    let extra = available - natural_total;
    let mut widths: Vec<usize> = natural.to_vec();

    // Distribute extra space proportionally
    let mut distributed = 0;
    for (i, w) in widths.iter_mut().enumerate() {
        if i == natural.len() - 1 {
            // Last column gets the remainder
            *w += extra - distributed;
        } else {
            let share = (extra as f64 * natural[i] as f64 / natural_total as f64).round() as usize;
            *w += share;
            distributed += share;
        }
    }

    widths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_table_formatting() {
        let input = "| a | b |\n|---|---|\n| c | d |";
        let result = format_tables(input, 40);
        // Should contain formatted table with | separators
        assert!(result.contains('|'));
        // All lines should have same number of | characters
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_non_table_passthrough() {
        let input = "Hello world\nThis is not a table";
        let result = format_tables(input, 80);
        assert_eq!(result, "Hello world\nThis is not a table");
    }

    #[test]
    fn test_mixed_content() {
        let input = "# Heading\n\n| a | b |\n|---|---|\n| c | d |\n\nParagraph";
        let result = format_tables(input, 40);
        assert!(result.starts_with("# Heading"));
        assert!(result.ends_with("Paragraph"));
    }

    #[test]
    fn test_separator_detection() {
        assert!(is_separator_row("| --- | --- |"));
        assert!(is_separator_row("|---|---|"));
        assert!(is_separator_row("| :---: | ---: |"));
        assert!(!is_separator_row("| hello | world |"));
        assert!(!is_separator_row("no pipes here"));
    }

    #[test]
    fn test_parse_cells() {
        let cells = parse_cells("| hello | world |");
        assert_eq!(cells, vec!["hello", "world"]);
    }

    #[test]
    fn test_distribute_widths() {
        let natural = vec![5, 10, 5];
        let widths = distribute_widths(&natural, 40);
        assert_eq!(widths.iter().sum::<usize>(), 40);
        // Each column should be at least its natural width
        for (i, &w) in widths.iter().enumerate() {
            assert!(w >= natural[i]);
        }
    }

    #[test]
    fn test_uneven_columns() {
        let input = "| short | a much longer column |\n|---|---|\n| x | y |";
        let result = format_tables(input, 60);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
        // All data rows should have same length
        assert_eq!(lines[0].len(), lines[2].len());
    }

    #[test]
    fn test_shrink_widths() {
        let natural = vec![20, 30, 20];
        let widths = shrink_widths(&natural, 35);
        assert_eq!(widths.iter().sum::<usize>(), 35);
        // Each column should be at least the minimum (3)
        for &w in &widths {
            assert!(w >= 3);
        }
    }

    #[test]
    fn test_format_table_shrinks_to_narrow_terminal() {
        let input = "| Long Header One | Long Header Two |\n|---|---|\n| wide content aa | wide content bb |";
        let narrow = 30;
        let result = format_tables(input, narrow);
        for line in result.lines() {
            assert!(
                line.len() <= narrow,
                "Table line '{}' (len {}) exceeds terminal width {}",
                line,
                line.len(),
                narrow
            );
        }
    }

    // ─── hard_wrap tests ────────────────────────────────────────────

    #[test]
    fn test_hard_wrap_short_lines_unchanged() {
        let input = "short line\nanother short";
        assert_eq!(hard_wrap(input, 40), input);
    }

    #[test]
    fn test_hard_wrap_long_line_wraps() {
        let input = "this is a somewhat long line that should be wrapped at a reasonable boundary";
        let result = hard_wrap(input, 30);
        for line in result.lines() {
            assert!(
                line.len() <= 30,
                "Wrapped line '{}' (len {}) exceeds width 30",
                line, line.len()
            );
        }
        // Content should be preserved (joined with spaces matches original)
        let rejoined: String = result.lines().collect::<Vec<_>>().join(" ");
        assert_eq!(rejoined, input);
    }

    #[test]
    fn test_hard_wrap_preserves_code_fences() {
        let long_code = "x".repeat(80);
        let input = format!("```\n{}\n```", long_code);
        let result = hard_wrap(&input, 40);
        assert_eq!(result, input, "Code fences should not be wrapped");
    }

    #[test]
    fn test_hard_wrap_preserves_table_lines() {
        let input = "| a very long cell value here | another long cell |\n|---|---|\n| data | more |";
        let result = hard_wrap(input, 20);
        // Table lines should pass through unchanged
        assert_eq!(result, input);
    }

    #[test]
    fn test_hard_wrap_list_continuation_indent() {
        let input = "- this is a very long list item that should wrap with proper indentation";
        let result = hard_wrap(input, 40);
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines.len() > 1, "Should wrap into multiple lines");
        // Continuation lines should start with 2-space indent
        for line in &lines[1..] {
            assert!(
                line.starts_with("  "),
                "Continuation should be indented, got: '{}'", line
            );
        }
    }

    #[test]
    fn test_hard_wrap_blockquote_continuation() {
        let input = "> this is a long blockquote line that should wrap while preserving the quote marker";
        let result = hard_wrap(input, 40);
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines.len() > 1);
        for line in &lines[1..] {
            assert!(
                line.starts_with("> "),
                "Continuation should keep '> ' prefix, got: '{}'", line
            );
        }
    }
}
