/// Result of analyzing a line for Enter-key continuation.
#[derive(Debug, PartialEq)]
pub enum Continuation {
    /// Insert a new line with the given prefix (e.g. "- ", "3. ", "> ").
    Continue(String),
    /// The line is an empty list item or quote — clear it (exit list/quote mode).
    ClearLine,
    /// No special continuation — delegate to normal newline.
    None,
}

/// Analyzes the current line to decide what should happen when Enter is pressed
/// at the end of the line.
///
/// Returns a `Continuation` describing whether to continue a list/quote,
/// clear an empty item, or do nothing special.
pub fn analyze_line_for_continuation(line: &str) -> Continuation {
    // Extract leading whitespace
    let indent = &line[..line.len() - line.trim_start().len()];
    let trimmed = line.trim_start();

    // Empty blockquote: "> " with nothing after
    if trimmed == ">" || trimmed == "> " {
        return Continuation::ClearLine;
    }

    // Blockquote continuation
    if let Some(rest) = trimmed.strip_prefix("> ") {
        if !rest.is_empty() {
            return Continuation::Continue(format!("{}> ", indent));
        }
    }
    // Also handle ">" without space followed by text
    if let Some(rest) = trimmed.strip_prefix('>') {
        if rest.starts_with(' ') {
            // Already handled above
        } else if !rest.is_empty() {
            return Continuation::Continue(format!("{}> ", indent));
        }
    }

    // Task list: "- [ ] " or "- [x] "
    if trimmed == "- [ ] " || trimmed == "- [ ]" || trimmed == "- [x] " || trimmed == "- [x]" {
        return Continuation::ClearLine;
    }
    if let Some(rest) = trimmed.strip_prefix("- [ ] ") {
        if !rest.is_empty() {
            return Continuation::Continue(format!("{}- [ ] ", indent));
        }
    }
    if let Some(rest) = trimmed.strip_prefix("- [x] ") {
        if !rest.is_empty() {
            return Continuation::Continue(format!("{}- [ ] ", indent));
        }
    }

    // Unordered list markers: -, *, +
    for marker in &["- ", "* ", "+ "] {
        // Empty item: just the marker
        if trimmed == marker.trim() || trimmed == *marker {
            return Continuation::ClearLine;
        }
        if let Some(rest) = trimmed.strip_prefix(marker) {
            if !rest.is_empty() {
                return Continuation::Continue(format!("{}{}", indent, marker));
            }
        }
    }

    // Ordered list: "N. "
    if let Some(dot_pos) = trimmed.find(". ") {
        let num_part = &trimmed[..dot_pos];
        if let Ok(n) = num_part.parse::<u64>() {
            let after = &trimmed[dot_pos + 2..];
            if after.is_empty() {
                return Continuation::ClearLine;
            }
            return Continuation::Continue(format!("{}{}. ", indent, n + 1));
        }
    }
    // Also handle "N." at end (empty ordered item)
    if trimmed.ends_with('.') {
        let num_part = &trimmed[..trimmed.len() - 1];
        if num_part.parse::<u64>().is_ok() {
            return Continuation::ClearLine;
        }
    }

    Continuation::None
}

/// Determines the closing character for an auto-close pair.
/// Returns None if the character shouldn't be auto-closed.
pub fn auto_close_pair(ch: char) -> Option<char> {
    match ch {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '`' => Some('`'),
        '"' => Some('"'),
        '\'' => Some('\''),
        _ => None,
    }
}

/// Returns true if backtick auto-pairing should be skipped.
/// Skip when the previous character is also a backtick (code fence typing).
pub fn should_skip_backtick_pair(prev_char: Option<char>) -> bool {
    prev_char == Some('`')
}

/// Returns true if quote auto-pairing should be skipped.
/// Skip when the previous character is alphanumeric (contractions like don't).
pub fn should_skip_quote_pair(ch: char, prev_char: Option<char>) -> bool {
    if ch == '\'' || ch == '"' {
        if let Some(prev) = prev_char {
            return prev.is_alphanumeric();
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Continuation tests ─────────────────────────────────────────

    #[test]
    fn test_unordered_dash_continuation() {
        assert_eq!(
            analyze_line_for_continuation("- item text"),
            Continuation::Continue("- ".to_string())
        );
    }

    #[test]
    fn test_unordered_star_continuation() {
        assert_eq!(
            analyze_line_for_continuation("* item text"),
            Continuation::Continue("* ".to_string())
        );
    }

    #[test]
    fn test_unordered_plus_continuation() {
        assert_eq!(
            analyze_line_for_continuation("+ item text"),
            Continuation::Continue("+ ".to_string())
        );
    }

    #[test]
    fn test_ordered_list_continuation() {
        assert_eq!(
            analyze_line_for_continuation("3. item text"),
            Continuation::Continue("4. ".to_string())
        );
    }

    #[test]
    fn test_ordered_list_increment() {
        assert_eq!(
            analyze_line_for_continuation("10. something"),
            Continuation::Continue("11. ".to_string())
        );
    }

    #[test]
    fn test_task_list_continuation() {
        assert_eq!(
            analyze_line_for_continuation("- [ ] task"),
            Continuation::Continue("- [ ] ".to_string())
        );
    }

    #[test]
    fn test_checked_task_continues_unchecked() {
        assert_eq!(
            analyze_line_for_continuation("- [x] done task"),
            Continuation::Continue("- [ ] ".to_string())
        );
    }

    #[test]
    fn test_blockquote_continuation() {
        assert_eq!(
            analyze_line_for_continuation("> quote text"),
            Continuation::Continue("> ".to_string())
        );
    }

    #[test]
    fn test_empty_dash_item_clears() {
        assert_eq!(
            analyze_line_for_continuation("- "),
            Continuation::ClearLine
        );
    }

    #[test]
    fn test_empty_star_item_clears() {
        assert_eq!(
            analyze_line_for_continuation("* "),
            Continuation::ClearLine
        );
    }

    #[test]
    fn test_empty_plus_item_clears() {
        assert_eq!(
            analyze_line_for_continuation("+ "),
            Continuation::ClearLine
        );
    }

    #[test]
    fn test_empty_blockquote_clears() {
        assert_eq!(
            analyze_line_for_continuation("> "),
            Continuation::ClearLine
        );
    }

    #[test]
    fn test_empty_ordered_clears() {
        assert_eq!(
            analyze_line_for_continuation("1. "),
            Continuation::ClearLine
        );
    }

    #[test]
    fn test_plain_text_no_continuation() {
        assert_eq!(
            analyze_line_for_continuation("just some text"),
            Continuation::None
        );
    }

    #[test]
    fn test_indented_list_preserves_indent() {
        assert_eq!(
            analyze_line_for_continuation("  - nested item"),
            Continuation::Continue("  - ".to_string())
        );
    }

    #[test]
    fn test_indented_ordered_list_preserves_indent() {
        assert_eq!(
            analyze_line_for_continuation("    1. nested ordered"),
            Continuation::Continue("    2. ".to_string())
        );
    }

    #[test]
    fn test_empty_line_no_continuation() {
        assert_eq!(
            analyze_line_for_continuation(""),
            Continuation::None
        );
    }

    // ─── Auto-close pair tests ──────────────────────────────────────

    #[test]
    fn test_auto_close_pairs() {
        assert_eq!(auto_close_pair('('), Some(')'));
        assert_eq!(auto_close_pair('['), Some(']'));
        assert_eq!(auto_close_pair('{'), Some('}'));
        assert_eq!(auto_close_pair('`'), Some('`'));
        assert_eq!(auto_close_pair('"'), Some('"'));
        assert_eq!(auto_close_pair('\''), Some('\''));
        assert_eq!(auto_close_pair('a'), None);
    }

    #[test]
    fn test_skip_backtick_after_backtick() {
        assert!(should_skip_backtick_pair(Some('`')));
        assert!(!should_skip_backtick_pair(Some('a')));
        assert!(!should_skip_backtick_pair(None));
    }

    #[test]
    fn test_skip_quote_after_alphanumeric() {
        assert!(should_skip_quote_pair('\'', Some('n'))); // don't
        assert!(should_skip_quote_pair('"', Some('a')));
        assert!(!should_skip_quote_pair('\'', Some(' ')));
        assert!(!should_skip_quote_pair('\'', None));
        assert!(!should_skip_quote_pair('(', Some('a'))); // not a quote char
    }
}
