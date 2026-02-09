use std::sync::OnceLock;

use ratatui::{
    style::Style,
    text::{Line, Span},
};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::theme;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

/// Returns a shared reference to the default SyntaxSet, initializing if needed.
pub fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Returns a shared reference to the default ThemeSet, initializing if needed.
pub fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Spawns a background thread to warm up syntect statics.
/// Call early in main() so loading overlaps with other init work.
pub fn ensure_loaded() {
    std::thread::spawn(|| {
        syntax_set();
        theme_set();
    });
}

/// Non-blocking check: returns references only if both statics are already initialized.
pub fn try_get() -> Option<(&'static SyntaxSet, &'static ThemeSet)> {
    Some((SYNTAX_SET.get()?, THEME_SET.get()?))
}

/// Map common language aliases to tokens that syntect's default set recognizes.
fn resolve_lang<'a>(lang: &'a str) -> &'a str {
    match lang {
        "typescript" | "ts" => "javascript",
        "tsx" | "jsx" => "javascript",
        "sh" | "zsh" | "fish" => "bash",
        "yml" => "yaml",
        "dockerfile" => "bash",
        "makefile" => "bash",
        "jsonc" => "json",
        "cxx" | "cc" | "hpp" => "cpp",
        _ => lang,
    }
}

pub fn highlight_code(code: &str, lang: &str, width: usize) -> Vec<Line<'static>> {
    let ss = syntax_set();
    let syntax_theme = &theme_set().themes["base16-ocean.dark"];

    let syntax = if lang.is_empty() {
        ss.find_syntax_plain_text()
    } else {
        ss.find_syntax_by_token(lang)
            .or_else(|| ss.find_syntax_by_token(resolve_lang(lang)))
            .unwrap_or_else(|| ss.find_syntax_plain_text())
    };

    let mut highlighter = HighlightLines::new(syntax, syntax_theme);
    let mut code_lines: Vec<Line<'static>> = Vec::new();
    let border_style = Style::default().fg(theme::BORDER).bg(theme::CODE_BG);
    let bg_style = Style::default().bg(theme::CODE_BG);

    for line in LinesWithEndings::from(code) {
        let regions = match highlighter.highlight_line(line, ss) {
            Ok(r) => r,
            Err(_) => {
                let text = format!("  {}", line.trim_end_matches('\n'));
                let text_len = text.len();
                let mut spans = vec![Span::styled(text, Style::default().fg(theme::CODE).bg(theme::CODE_BG))];
                pad_to_width(&mut spans, text_len, width, bg_style);
                code_lines.push(Line::from(spans));
                continue;
            }
        };

        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled("  ", bg_style));
        let mut col = 2usize;

        for (style, content) in regions {
            let text = content.trim_end_matches('\n');
            if text.is_empty() {
                continue;
            }
            let fg = ratatui::style::Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
            let span_style = Style::default().fg(fg).bg(theme::CODE_BG);
            col += text.len();
            spans.push(Span::styled(text.to_string(), span_style));
        }

        pad_to_width(&mut spans, col, width, bg_style);
        code_lines.push(Line::from(spans));
    }

    if code_lines.is_empty() && !code.is_empty() {
        for code_line in code.lines() {
            let text = format!("  {}", code_line);
            let text_len = text.len();
            let mut spans = vec![Span::styled(text, Style::default().fg(theme::CODE).bg(theme::CODE_BG))];
            pad_to_width(&mut spans, text_len, width, bg_style);
            code_lines.push(Line::from(spans));
        }
    }

    // Wrap with top/bottom border chrome
    let mut lines: Vec<Line<'static>> = Vec::new();
    let inner_w = width.saturating_sub(2); // subtract ┌ and ┐

    // Top border: ┌─ language ─────...─┐
    let label = if lang.is_empty() { String::new() } else { format!(" {} ", lang) };
    let fill = inner_w.saturating_sub(1 + label.len()); // 1 for the ─ after ┌
    let top_border = format!("┌─{}{}┐", label, "─".repeat(fill));
    lines.push(Line::from(Span::styled(top_border, border_style)));

    lines.extend(code_lines);

    // Bottom border: └─────...─┘
    let bot_border = format!("└{}┘", "─".repeat(inner_w));
    lines.push(Line::from(Span::styled(bot_border, border_style)));

    lines
}

/// Pad a span list with trailing spaces so the line fills `width` with `bg_style`.
fn pad_to_width(spans: &mut Vec<Span<'static>>, current_cols: usize, width: usize, style: Style) {
    if current_cols < width {
        spans.push(Span::styled(" ".repeat(width - current_cols), style));
    }
}

/// Represents a code fence region in the editor buffer.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeFenceRegion {
    /// Line index of the opening ``` delimiter (0-based).
    pub start_line: usize,
    /// Line index of the closing ``` delimiter (0-based), or last line if unclosed.
    pub end_line: usize,
    /// Language string from the opening fence (e.g. "rust", "python").
    pub language: String,
}

/// Scans editor lines for ``` delimiters and returns code fence regions.
pub fn find_code_fence_regions(lines: &[String]) -> Vec<CodeFenceRegion> {
    let mut regions = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if trimmed.starts_with("```") {
            let lang = trimmed[3..].trim().to_string();
            let start = i;
            i += 1;

            // Find the closing fence
            let mut end = lines.len() - 1; // default: unclosed
            while i < lines.len() {
                let t = lines[i].trim_start();
                if t.starts_with("```") && t[3..].trim().is_empty() {
                    end = i;
                    i += 1;
                    break;
                }
                i += 1;
            }

            regions.push(CodeFenceRegion {
                start_line: start,
                end_line: end,
                language: lang,
            });
        } else {
            i += 1;
        }
    }

    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_typescript_has_colored_spans() {
        let code = "const a = 5;\n";
        let lines = highlight_code(code, "typescript", 80);
        let has_keyword_color = lines.iter().any(|line| {
            line.spans.iter().any(|s| {
                s.content.as_ref() == "const"
                    && matches!(s.style.fg, Some(ratatui::style::Color::Rgb(r, g, b)) if !(r == g && g == b))
            })
        });
        assert!(has_keyword_color, "TypeScript 'const' should be syntax-highlighted via JS fallback");
    }

    #[test]
    fn test_highlight_rust_has_colored_spans() {
        let code = "fn main() {\n    println!(\"hello\");\n}\n";
        let lines = highlight_code(code, "rust", 80);
        let has_colored_fg = lines.iter().any(|line| {
            line.spans.iter().any(|s| {
                matches!(s.style.fg, Some(ratatui::style::Color::Rgb(r, g, b)) if !(r == g && g == b))
            })
        });
        assert!(has_colored_fg, "Rust code should have syntax-colored spans");
    }

    #[test]
    fn test_resolve_lang_aliases() {
        assert_eq!(resolve_lang("typescript"), "javascript");
        assert_eq!(resolve_lang("ts"), "javascript");
        assert_eq!(resolve_lang("tsx"), "javascript");
        assert_eq!(resolve_lang("sh"), "bash");
        assert_eq!(resolve_lang("yml"), "yaml");
        assert_eq!(resolve_lang("rust"), "rust");
    }

    #[test]
    fn test_find_code_fence_regions_simple() {
        let lines: Vec<String> = vec![
            "# Hello".to_string(),
            "```rust".to_string(),
            "fn main() {}".to_string(),
            "```".to_string(),
            "done".to_string(),
        ];
        let regions = find_code_fence_regions(&lines);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start_line, 1);
        assert_eq!(regions[0].end_line, 3);
        assert_eq!(regions[0].language, "rust");
    }

    #[test]
    fn test_find_code_fence_regions_multiple() {
        let lines: Vec<String> = vec![
            "```python".to_string(),
            "print('hi')".to_string(),
            "```".to_string(),
            "text".to_string(),
            "```js".to_string(),
            "console.log('hi')".to_string(),
            "```".to_string(),
        ];
        let regions = find_code_fence_regions(&lines);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].language, "python");
        assert_eq!(regions[1].language, "js");
    }

    #[test]
    fn test_find_code_fence_regions_unclosed() {
        let lines: Vec<String> = vec![
            "```rust".to_string(),
            "fn main() {}".to_string(),
        ];
        let regions = find_code_fence_regions(&lines);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start_line, 0);
        assert_eq!(regions[0].end_line, 1); // defaults to last line
    }

    #[test]
    fn test_find_code_fence_regions_no_lang() {
        let lines: Vec<String> = vec![
            "```".to_string(),
            "plain code".to_string(),
            "```".to_string(),
        ];
        let regions = find_code_fence_regions(&lines);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].language, "");
    }
}
