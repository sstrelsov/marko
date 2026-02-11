use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd, CodeBlockKind};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span, Text},
};

use crate::markdown::code_highlight;
use crate::markdown::math::latex_to_unicode;
use crate::markdown::style_ext::style_extensions;
use crate::theme;

/// Rendered markdown output with metadata for post-processing.
pub struct RenderedMarkdown {
    pub text: Text<'static>,
    /// Link URLs in document order.
    pub link_urls: Vec<String>,
    /// Image positions and URLs for inline rendering.
    pub image_infos: Vec<ImageInfo>,
}

/// Metadata for an image in the rendered output.
pub struct ImageInfo {
    pub url: String,
    pub start_line: usize,
    pub line_count: usize,
}

pub fn render_markdown(content: &str, width: usize) -> RenderedMarkdown {
    let options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_DEFINITION_LIST;
    let parser = Parser::new_ext(content, options);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut style_stack: Vec<Style> = vec![Style::default().fg(theme::FG)];
    let mut in_code_block = false;
    let mut code_block_lang = String::new();
    let mut code_block_content = String::new();
    let mut _in_heading = false;
    let mut _heading_level: u8 = 0;
    let mut blockquote_depth: usize = 0;

    // List stack: None = unordered, Some(counter) = ordered
    let mut list_stack: Vec<Option<u64>> = Vec::new();

    // Table state
    let mut in_table = false;
    let mut table_rows: Vec<Vec<Vec<Span<'static>>>> = Vec::new(); // rows of cells (each cell = Vec<Span>)
    let mut current_cell: Vec<Span<'static>> = Vec::new();
    let mut table_header_count: usize = 0;
    let mut _in_table_head = false;
    let mut table_alignments: Vec<Alignment> = Vec::new();

    // Footnote/definition list state
    let mut _in_footnote_def = false;
    let mut footnote_label = String::new();
    let mut _in_definition_title = false;
    let mut _in_definition_def = false;

    // Link/image URL tracking
    let mut link_url = String::new();
    let mut image_url = String::new();
    let mut link_urls: Vec<String> = Vec::new();
    let mut image_infos: Vec<ImageInfo> = Vec::new();

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    _in_heading = true;
                    _heading_level = level as u8;
                    // Extra spacing before headings (avoid doubles if previous line is blank)
                    flush_line(&mut lines, &mut current_spans);
                    let prev_blank = lines.last().map_or(true, |l| l.spans.is_empty() || l.spans.iter().all(|s| s.content.trim().is_empty() || s.content.trim() == "│"));
                    if _heading_level <= 1 {
                        // 2 blank lines before H1
                        if !prev_blank { push_blank_line(&mut lines, blockquote_depth); }
                        push_blank_line(&mut lines, blockquote_depth);
                    } else if _heading_level == 2 && !prev_blank {
                        // 1 blank line before H2
                        push_blank_line(&mut lines, blockquote_depth);
                    }
                    push_bq_prefix(&mut current_spans, blockquote_depth);
                    let prefix = "#".repeat(_heading_level as usize);
                    current_spans.push(Span::styled(
                        format!("{} ", prefix),
                        theme::heading_style(),
                    ));
                    style_stack.push(theme::heading_style());
                }
                Tag::Strong => {
                    let base = current_style(&style_stack);
                    style_stack.push(compose_style(base, theme::bold_style()));
                }
                Tag::Emphasis => {
                    let base = current_style(&style_stack);
                    style_stack.push(compose_style(base, theme::italic_style()));
                }
                Tag::Strikethrough => {
                    let base = current_style(&style_stack);
                    style_stack.push(compose_style(
                        base,
                        Style::default()
                            .fg(theme::FG)
                            .add_modifier(Modifier::CROSSED_OUT),
                    ));
                }
                Tag::Link { dest_url, .. } => {
                    let base = current_style(&style_stack);
                    style_stack.push(compose_style(base, theme::link_style()));
                    link_url = dest_url.to_string();
                }
                Tag::Image { dest_url, .. } => {
                    image_url = dest_url.to_string();
                    // Flush any pending content before the image box
                    flush_line(&mut lines, &mut current_spans);
                    style_stack.push(Style::default().fg(theme::FG));
                }
                Tag::CodeBlock(kind) => {
                    in_code_block = true;
                    code_block_content.clear();
                    code_block_lang = match kind {
                        CodeBlockKind::Fenced(lang) => lang.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                }
                Tag::BlockQuote(_) => {
                    blockquote_depth += 1;
                }
                Tag::List(start) => {
                    if !list_stack.is_empty() {
                        flush_line(&mut lines, &mut current_spans);
                    }
                    list_stack.push(start);
                }
                Tag::Item => {
                    flush_line(&mut lines, &mut current_spans);
                    push_bq_prefix(&mut current_spans, blockquote_depth);
                    let depth = list_stack.len();
                    let indent = "  ".repeat(depth.saturating_sub(1));
                    let bullet = if let Some(Some(ref mut counter)) = list_stack.last_mut() {
                        let n = *counter;
                        *counter = n + 1;
                        format!("{}{}. ", indent, n)
                    } else {
                        format!("{}• ", indent)
                    };
                    current_spans.push(Span::styled(
                        bullet,
                        Style::default().fg(theme::FG),
                    ));
                }
                Tag::Table(alignments) => {
                    in_table = true;
                    table_rows.clear();
                    table_header_count = 0;
                    table_alignments = alignments;
                }
                Tag::TableHead => {
                    _in_table_head = true;
                    table_rows.push(Vec::new());
                }
                Tag::TableRow => {
                    table_rows.push(Vec::new());
                }
                Tag::TableCell => {
                    current_cell.clear();
                }
                Tag::FootnoteDefinition(label) => {
                    _in_footnote_def = true;
                    footnote_label = label.to_string();
                    push_bq_prefix(&mut current_spans, blockquote_depth);
                    current_spans.push(Span::styled(
                        format!("[{}]: ", footnote_label),
                        Style::default().fg(theme::BORDER),
                    ));
                }
                Tag::DefinitionList => {}
                Tag::DefinitionListTitle => {
                    _in_definition_title = true;
                    style_stack.push(theme::bold_style());
                }
                Tag::DefinitionListDefinition => {
                    _in_definition_def = true;
                    push_bq_prefix(&mut current_spans, blockquote_depth);
                    current_spans.push(Span::styled(
                        ":  ".to_string(),
                        Style::default().fg(theme::BORDER),
                    ));
                }
                Tag::Paragraph => {}
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Heading(level) => {
                    let hlevel = level as u8;
                    _in_heading = false;
                    _heading_level = 0;
                    style_stack.pop();
                    flush_line(&mut lines, &mut current_spans);
                    // Add underline for H1 (heavy) and H2 (light)
                    let bq_w = blockquote_depth * 2;
                    if hlevel == 1 {
                        let rule = "━".repeat(width.saturating_sub(bq_w));
                        let mut spans: Vec<Span<'static>> = Vec::new();
                        if blockquote_depth > 0 {
                            spans.push(Span::styled("│ ".repeat(blockquote_depth), Style::default().fg(theme::QUOTE_BORDER)));
                        }
                        spans.push(Span::styled(rule, Style::default().fg(theme::HEADING)));
                        lines.push(Line::from(spans));
                    } else if hlevel == 2 {
                        let rule = "─".repeat(width.saturating_sub(bq_w));
                        let mut spans: Vec<Span<'static>> = Vec::new();
                        if blockquote_depth > 0 {
                            spans.push(Span::styled("│ ".repeat(blockquote_depth), Style::default().fg(theme::QUOTE_BORDER)));
                        }
                        spans.push(Span::styled(rule, Style::default().fg(theme::HEADING)));
                        lines.push(Line::from(spans));
                    }
                    push_blank_line(&mut lines, blockquote_depth);
                }
                TagEnd::Strong | TagEnd::Emphasis => {
                    style_stack.pop();
                }
                TagEnd::Strikethrough => {
                    style_stack.pop();
                }
                TagEnd::Link => {
                    style_stack.pop();
                    if !link_url.is_empty() {
                        // Append the URL in dimmed parentheses after the link text
                        current_spans.push(Span::styled(
                            format!(" ({})", link_url),
                            Style::default().fg(theme::LINE_NUMBER),
                        ));
                        link_urls.push(link_url.clone());
                        link_url.clear();
                    }
                }
                TagEnd::Image => {
                    style_stack.pop();
                    let img_start_line = lines.len();
                    // Collect alt text from any spans accumulated during Image
                    let alt_text: String = current_spans.drain(..).map(|s| s.content.to_string()).collect();
                    let alt_display = if alt_text.is_empty() { "Image".to_string() } else { alt_text };

                    // Extract filename from URL
                    let filename = image_url.rsplit('/').next().unwrap_or(&image_url).to_string();
                    let border_style = Style::default().fg(theme::BORDER);
                    let text_style = Style::default().fg(theme::FG).add_modifier(Modifier::ITALIC);
                    let dim_style = Style::default().fg(theme::LINE_NUMBER);

                    let inner_width = alt_display.len().max(filename.len()).max(6) + 2;
                    let top = format!("╭─{}─╮", "─".repeat(inner_width));
                    let bot = format!("╰─{}─╯", "─".repeat(inner_width));

                    let bq = |spans: &mut Vec<Span<'static>>| {
                        if blockquote_depth > 0 {
                            spans.push(Span::styled("│ ".repeat(blockquote_depth), border_style));
                        }
                    };

                    // Top border
                    let mut top_spans = Vec::new();
                    bq(&mut top_spans);
                    top_spans.push(Span::styled(top, border_style));
                    lines.push(Line::from(top_spans));

                    // Alt text line
                    let alt_pad = inner_width.saturating_sub(alt_display.len());
                    let mut alt_spans = Vec::new();
                    bq(&mut alt_spans);
                    alt_spans.push(Span::styled("│ ", border_style));
                    alt_spans.push(Span::styled(alt_display, text_style));
                    alt_spans.push(Span::styled(format!("{} │", " ".repeat(alt_pad)), border_style));
                    lines.push(Line::from(alt_spans));

                    // Filename line
                    let fn_pad = inner_width.saturating_sub(filename.len());
                    let mut fn_spans = Vec::new();
                    bq(&mut fn_spans);
                    fn_spans.push(Span::styled("│ ", border_style));
                    fn_spans.push(Span::styled(filename, dim_style));
                    fn_spans.push(Span::styled(format!("{} │", " ".repeat(fn_pad)), border_style));
                    lines.push(Line::from(fn_spans));

                    // Bottom border
                    let mut bot_spans = Vec::new();
                    bq(&mut bot_spans);
                    bot_spans.push(Span::styled(bot, border_style));
                    lines.push(Line::from(bot_spans));

                    // Reserve extra blank lines so the image overlay has room.
                    // The half-block renderer will overwrite these.
                    let target_height = 15usize;
                    let current_height = lines.len() - img_start_line;
                    for _ in current_height..target_height {
                        let mut blank = Vec::new();
                        bq(&mut blank);
                        lines.push(Line::from(blank));
                    }

                    image_infos.push(ImageInfo {
                        url: image_url.clone(),
                        start_line: img_start_line,
                        line_count: lines.len() - img_start_line,
                    });
                    image_url.clear();
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    let code_width = width.saturating_sub(blockquote_depth * 2);
                    let highlighted = code_highlight::highlight_code(
                        &code_block_content,
                        &code_block_lang,
                        code_width,
                    );
                    for line in highlighted {
                        if blockquote_depth > 0 {
                            let mut bq_spans = vec![Span::styled(
                                "│ ".repeat(blockquote_depth),
                                Style::default().fg(theme::QUOTE_BORDER),
                            )];
                            bq_spans.extend(line.spans);
                            lines.push(Line::from(bq_spans));
                        } else {
                            lines.push(line);
                        }
                    }
                    push_blank_line(&mut lines, blockquote_depth);
                    code_block_content.clear();
                    code_block_lang.clear();
                }
                TagEnd::BlockQuote(_) => {
                    blockquote_depth = blockquote_depth.saturating_sub(1);
                }
                TagEnd::List(_) => {
                    list_stack.pop();
                    if list_stack.is_empty() {
                        push_blank_line(&mut lines, blockquote_depth);
                    }
                }
                TagEnd::Item => {
                    flush_line(&mut lines, &mut current_spans);
                }
                TagEnd::Table => {
                    // Render accumulated table
                    render_table(&table_rows, table_header_count, &table_alignments, width, &mut lines, blockquote_depth);
                    in_table = false;
                    table_rows.clear();
                    table_alignments.clear();
                    push_blank_line(&mut lines, blockquote_depth);
                }
                TagEnd::TableHead => {
                    _in_table_head = false;
                    table_header_count = table_rows.len();
                }
                TagEnd::TableRow => {}
                TagEnd::TableCell => {
                    if let Some(row) = table_rows.last_mut() {
                        row.push(current_cell.drain(..).collect());
                    }
                }
                TagEnd::Paragraph => {
                    flush_line(&mut lines, &mut current_spans);
                    push_blank_line(&mut lines, blockquote_depth);
                }
                TagEnd::FootnoteDefinition => {
                    _in_footnote_def = false;
                    footnote_label.clear();
                    flush_line(&mut lines, &mut current_spans);
                }
                TagEnd::DefinitionList => {
                    lines.push(Line::from(""));
                }
                TagEnd::DefinitionListTitle => {
                    _in_definition_title = false;
                    style_stack.pop();
                    flush_line(&mut lines, &mut current_spans);
                }
                TagEnd::DefinitionListDefinition => {
                    _in_definition_def = false;
                    flush_line(&mut lines, &mut current_spans);
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    code_block_content.push_str(&text);
                } else if in_table {
                    let style = current_style(&style_stack);
                    current_cell.push(Span::styled(text.to_string(), style));
                } else {
                    push_bq_prefix(&mut current_spans, blockquote_depth);
                    let style = current_style(&style_stack);
                    let wrapped = word_wrap(&text, width, &current_spans);
                    if wrapped.len() <= 1 {
                        current_spans.extend(style_extensions(&text, style));
                    } else {
                        for (i, chunk) in wrapped.iter().enumerate() {
                            current_spans.extend(style_extensions(chunk, style));
                            if i < wrapped.len() - 1 {
                                flush_line(&mut lines, &mut current_spans);
                                push_bq_prefix(&mut current_spans, blockquote_depth);
                            }
                        }
                    }
                }
            }
            Event::Code(code) => {
                if in_table {
                    current_cell.push(Span::styled(
                        format!(" {} ", code),
                        theme::code_style(),
                    ));
                } else {
                    push_bq_prefix(&mut current_spans, blockquote_depth);
                    current_spans.push(Span::styled(
                        format!(" {} ", code),
                        theme::code_style(),
                    ));
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if !in_table {
                    flush_line(&mut lines, &mut current_spans);
                }
            }
            Event::FootnoteReference(label) => {
                push_bq_prefix(&mut current_spans, blockquote_depth);
                current_spans.push(Span::styled(
                    format!("[{}]", label),
                    theme::link_style(),
                ));
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked { "[x] " } else { "[ ] " };
                let style = if checked {
                    Style::default().fg(theme::SUCCESS)
                } else {
                    Style::default().fg(theme::FG)
                };
                current_spans.push(Span::styled(marker.to_string(), style));
            }
            Event::InlineMath(text) => {
                push_bq_prefix(&mut current_spans, blockquote_depth);
                let converted = latex_to_unicode(&text);
                current_spans.push(Span::styled(
                    converted,
                    Style::default().fg(theme::CODE).add_modifier(Modifier::ITALIC),
                ));
            }
            Event::DisplayMath(text) => {
                flush_line(&mut lines, &mut current_spans);
                let math_style = Style::default().fg(theme::CODE).add_modifier(Modifier::ITALIC);
                let converted = latex_to_unicode(&text);
                for math_line in converted.split('\n') {
                    let mut ml = Vec::new();
                    if blockquote_depth > 0 {
                        ml.push(Span::styled("│ ".repeat(blockquote_depth), Style::default().fg(theme::QUOTE_BORDER)));
                    }
                    ml.push(Span::styled(format!("  {}", math_line), math_style));
                    lines.push(Line::from(ml));
                }
                push_blank_line(&mut lines, blockquote_depth);
            }
            Event::Rule => {
                let bq_w = blockquote_depth * 2;
                let avail = width.saturating_sub(bq_w);
                let rule = if avail >= 3 {
                    format!("╶{}╴", "─".repeat(avail - 2))
                } else {
                    "─".repeat(avail)
                };
                let mut rule_spans: Vec<Span<'static>> = Vec::new();
                if blockquote_depth > 0 {
                    rule_spans.push(Span::styled(
                        "│ ".repeat(blockquote_depth),
                        Style::default().fg(theme::QUOTE_BORDER),
                    ));
                }
                rule_spans.push(Span::styled(rule, Style::default().fg(theme::BORDER)));
                lines.push(Line::from(rule_spans));
                push_blank_line(&mut lines, blockquote_depth);
            }
            _ => {}
        }
    }

    // Flush remaining spans
    if !current_spans.is_empty() {
        flush_line(&mut lines, &mut current_spans);
    }

    RenderedMarkdown {
        text: Text::from(lines),
        link_urls,
        image_infos,
    }
}

/// Renders accumulated table rows into styled lines with box-drawing borders.
fn render_table(
    rows: &[Vec<Vec<Span<'static>>>],
    header_count: usize,
    alignments: &[Alignment],
    width: usize,
    lines: &mut Vec<Line<'static>>,
    bq_depth: usize,
) {
    if rows.is_empty() {
        return;
    }

    // Determine number of columns
    let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if num_cols == 0 {
        return;
    }

    let bq_prefix_width = bq_depth * 2;
    let effective_width = width.saturating_sub(bq_prefix_width);

    // Calculate column widths from cell content
    let mut col_widths: Vec<usize> = vec![0; num_cols];
    for row in rows {
        for (j, cell) in row.iter().enumerate() {
            if j < num_cols {
                let cell_width: usize = cell.iter().map(|s| s.width()).sum();
                col_widths[j] = col_widths[j].max(cell_width);
            }
        }
    }

    // Ensure minimum width of 3
    for w in &mut col_widths {
        if *w < 3 {
            *w = 3;
        }
    }

    // Calculate available width and scale columns
    // Layout: │ col1 │ col2 │  → (num_cols+1) borders + 2 spaces per col
    let border_chars = num_cols + 1 + num_cols * 2;
    let available = effective_width.saturating_sub(border_chars);
    let natural_total: usize = col_widths.iter().sum();
    if available > natural_total {
        // Expand columns proportionally to fill available space
        let extra = available - natural_total;
        let mut distributed = 0;
        for i in 0..col_widths.len() {
            if i == col_widths.len() - 1 {
                col_widths[i] += extra - distributed;
            } else {
                let share = if natural_total > 0 {
                    (extra as f64 * col_widths[i] as f64 / natural_total as f64).round() as usize
                } else {
                    extra / col_widths.len()
                };
                col_widths[i] += share;
                distributed += share;
            }
        }
    } else if natural_total > available && available > 0 {
        // Shrink columns proportionally to fit within available space
        let min_col: usize = 3;
        let mut shrunk = 0;
        for i in 0..col_widths.len() {
            if i == col_widths.len() - 1 {
                col_widths[i] = available.saturating_sub(shrunk).max(min_col);
            } else {
                let share = (available as f64 * col_widths[i] as f64 / natural_total as f64).floor() as usize;
                col_widths[i] = share.max(min_col);
                shrunk += col_widths[i];
            }
        }
    }

    let border_style = Style::default().fg(theme::BORDER);

    // Render each row
    for (i, row) in rows.iter().enumerate() {
        let mut spans: Vec<Span<'static>> = Vec::new();
        if bq_depth > 0 {
            spans.push(Span::styled("│ ".repeat(bq_depth), border_style));
        }
        spans.push(Span::styled("│ ".to_string(), border_style));

        for j in 0..num_cols {
            let cell = row.get(j);
            let max_w = col_widths[j];
            let cell_width: usize = cell.map_or(0, |c| c.iter().map(|s| s.width()).sum());
            let pad = max_w.saturating_sub(cell_width);
            let align = alignments.get(j).copied().unwrap_or(Alignment::None);
            let pad_style = Style::default().fg(theme::FG);

            // Left padding for right/center alignment
            match align {
                Alignment::Right => {
                    spans.push(Span::styled(" ".repeat(pad), pad_style));
                }
                Alignment::Center => {
                    let left_pad = pad / 2;
                    spans.push(Span::styled(" ".repeat(left_pad), pad_style));
                }
                _ => {}
            }

            if let Some(cell_spans) = cell {
                if cell_width <= max_w {
                    for s in cell_spans {
                        spans.push(s.clone());
                    }
                } else {
                    // Truncate cell content to fit column width
                    let mut remaining = max_w;
                    for s in cell_spans {
                        let sw = s.width();
                        if sw <= remaining {
                            spans.push(s.clone());
                            remaining -= sw;
                        } else if remaining > 0 {
                            let truncated: String = s.content.chars().take(remaining).collect();
                            spans.push(Span::styled(truncated, s.style));
                            remaining = 0;
                        }
                    }
                }
            }

            // Right padding for left/none/center alignment
            match align {
                Alignment::Right => {}
                Alignment::Center => {
                    let right_pad = pad - pad / 2;
                    spans.push(Span::styled(" ".repeat(right_pad), pad_style));
                }
                _ => {
                    spans.push(Span::styled(" ".repeat(pad), pad_style));
                }
            }

            if j < num_cols - 1 {
                spans.push(Span::styled(" │ ".to_string(), border_style));
            } else {
                spans.push(Span::styled(" │".to_string(), border_style));
            }
        }

        lines.push(Line::from(spans));

        // Add separator line after header
        if i + 1 == header_count {
            let mut sep_spans: Vec<Span<'static>> = Vec::new();
            if bq_depth > 0 {
                sep_spans.push(Span::styled("│ ".repeat(bq_depth), border_style));
            }
            sep_spans.push(Span::styled("├".to_string(), border_style));
            for j in 0..num_cols {
                sep_spans.push(Span::styled(
                    "─".repeat(col_widths[j] + 2),
                    border_style,
                ));
                if j < num_cols - 1 {
                    sep_spans.push(Span::styled("┼".to_string(), border_style));
                } else {
                    sep_spans.push(Span::styled("┤".to_string(), border_style));
                }
            }
            lines.push(Line::from(sep_spans));
        }
    }
}

fn flush_line(lines: &mut Vec<Line<'static>>, spans: &mut Vec<Span<'static>>) {
    if !spans.is_empty() {
        lines.push(Line::from(spans.drain(..).collect::<Vec<_>>()));
    }
}

fn current_style(stack: &[Style]) -> Style {
    stack.last().copied().unwrap_or(Style::default().fg(theme::FG))
}

fn word_wrap(text: &str, max_width: usize, existing_spans: &[Span]) -> Vec<String> {
    let current_col: usize = existing_spans.iter().map(|s| s.width()).sum();
    let remaining = max_width.saturating_sub(current_col);

    if text.len() <= remaining {
        return vec![text.to_string()];
    }

    let mut result = Vec::new();
    let mut current = String::new();
    let mut col = current_col;

    for word in text.split_inclusive(' ') {
        if col + word.len() > max_width && !current.is_empty() {
            result.push(current.clone());
            current.clear();
            col = 0;
        }
        current.push_str(word);
        col += word.len();
    }

    if !current.is_empty() {
        result.push(current);
    }

    if result.is_empty() {
        vec![text.to_string()]
    } else {
        result
    }
}

/// Compose two styles: overlay's colors win, but modifiers accumulate.
fn compose_style(base: Style, overlay: Style) -> Style {
    let mut result = overlay;
    result.add_modifier |= base.add_modifier;
    result
}

/// Push blockquote `│ ` prefix to spans if at start of a new line (spans empty).
fn push_bq_prefix(spans: &mut Vec<Span<'static>>, depth: usize) {
    if depth > 0 && spans.is_empty() {
        spans.push(Span::styled(
            "│ ".repeat(depth),
            Style::default().fg(theme::QUOTE_BORDER),
        ));
    }
}

/// Push a blank line, with blockquote prefix if inside a blockquote.
fn push_blank_line(lines: &mut Vec<Line<'static>>, bq_depth: usize) {
    if bq_depth > 0 {
        lines.push(Line::from(Span::styled(
            "│ ".repeat(bq_depth),
            Style::default().fg(theme::QUOTE_BORDER),
        )));
    } else {
        lines.push(Line::from(""));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;

    #[test]
    fn test_render_heading() {
        let text = render_markdown("# Hello", 80).text;
        assert!(!text.lines.is_empty());
        let has_heading = text.lines.iter().any(|line| {
            let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            content.contains("# Hello")
        });
        assert!(has_heading, "Should contain '# Hello' heading");
    }

    #[test]
    fn test_render_bold() {
        let text = render_markdown("**bold**", 80).text;
        assert!(!text.lines.is_empty());
        let first_line = &text.lines[0];
        let has_bold = first_line.spans.iter().any(|s| {
            s.style.add_modifier.contains(Modifier::BOLD) && s.content.contains("bold")
        });
        assert!(has_bold);
    }

    #[test]
    fn test_render_italic() {
        let text = render_markdown("*italic*", 80).text;
        assert!(!text.lines.is_empty());
        let first_line = &text.lines[0];
        let has_italic = first_line.spans.iter().any(|s| {
            s.style.add_modifier.contains(Modifier::ITALIC) && s.content.contains("italic")
        });
        assert!(has_italic);
    }

    #[test]
    fn test_render_inline_code() {
        let text = render_markdown("`code`", 80).text;
        assert!(!text.lines.is_empty());
        let first_line = &text.lines[0];
        let has_code = first_line.spans.iter().any(|s| {
            s.style.fg == Some(theme::CODE) && s.content.contains("code")
        });
        assert!(has_code);
    }

    #[test]
    fn test_render_rule() {
        let text = render_markdown("---", 80).text;
        let has_rule = text.lines.iter().any(|line| {
            line.spans.iter().any(|s| s.content.contains("─"))
        });
        assert!(has_rule);
    }

    #[test]
    fn test_render_list() {
        let text = render_markdown("- item one\n- item two", 80).text;
        let has_bullet = text.lines.iter().any(|line| {
            line.spans.iter().any(|s| s.content.contains("•"))
        });
        assert!(has_bullet);
    }

    #[test]
    fn test_render_ordered_list() {
        let text = render_markdown("1. first\n2. second\n3. third", 80).text;
        let all_text: String = text.lines.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains("1."), "Should contain '1.' but got: {}", all_text);
        assert!(all_text.contains("2."), "Should contain '2.' but got: {}", all_text);
        assert!(all_text.contains("3."), "Should contain '3.' but got: {}", all_text);
    }

    #[test]
    fn test_render_ordered_list_no_bullet() {
        let text = render_markdown("1. first\n2. second", 80).text;
        let all_text: String = text.lines.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
            .collect::<Vec<_>>()
            .join("");
        // Ordered lists should not use bullet character
        assert!(!all_text.contains("•"), "Ordered list should not contain '•' but got: {}", all_text);
    }

    #[test]
    fn test_render_table() {
        let text = render_markdown("| A | B |\n|---|---|\n| 1 | 2 |", 40).text;
        let all_text: String = text.lines.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains("│"), "Table should contain │ borders but got: {}", all_text);
        assert!(all_text.contains("A"), "Table should contain cell 'A'");
        assert!(all_text.contains("B"), "Table should contain cell 'B'");
        assert!(all_text.contains("1"), "Table should contain cell '1'");
        assert!(all_text.contains("2"), "Table should contain cell '2'");
    }

    #[test]
    fn test_render_table_separator() {
        let text = render_markdown("| A | B |\n|---|---|\n| 1 | 2 |", 40).text;
        let all_text: String = text.lines.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains("─"), "Table should have ─ separator");
        assert!(all_text.contains("├"), "Table should have ├ corner");
    }

    #[test]
    fn test_render_table_shrinks_to_fit_narrow_width() {
        // Table with wide cells rendered in a narrow width should not exceed that width
        let md = "| Long Header One | Long Header Two |\n|---|---|\n| cell content a | cell content b |";
        let narrow_width = 30;
        let text = render_markdown(md, narrow_width).text;
        for line in &text.lines {
            let line_width: usize = line.spans.iter().map(|s| s.width()).sum();
            assert!(
                line_width <= narrow_width,
                "Table line width {} exceeds available width {}: {:?}",
                line_width,
                narrow_width,
                line.spans.iter().map(|s| s.content.as_ref()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn test_render_rule_fills_width() {
        let text = render_markdown("---", 50).text;
        let rule_line = text.lines.iter().find(|line| {
            line.spans.iter().any(|s| s.content.contains("─"))
        });
        assert!(rule_line.is_some(), "Should have a rule line");
        let rule_content: String = rule_line.unwrap().spans.iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(rule_content.chars().count(), 50, "Rule should fill available width");
    }

    #[test]
    fn test_render_strikethrough() {
        let text = render_markdown("~~struck~~", 80).text;
        assert!(!text.lines.is_empty());
        let has_strikethrough = text.lines.iter().any(|line| {
            line.spans.iter().any(|s| {
                s.style.add_modifier.contains(Modifier::CROSSED_OUT) && s.content.contains("struck")
            })
        });
        assert!(has_strikethrough, "Should render strikethrough text");
    }
}
