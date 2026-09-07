use super::*;

impl<'a, 'policy, I> Writer<'a, 'policy, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    pub(super) fn start_paragraph(&mut self) {
        if self.in_table_cell() {
            return;
        }
        if self.needs_newline {
            self.push_blank_line();
        }
        self.push_line(Line::default());
        self.needs_newline = false;
        self.in_paragraph = true;
    }

    pub(super) fn end_paragraph(&mut self) {
        if self.in_table_cell() {
            return;
        }
        self.needs_newline = true;
        self.in_paragraph = false;
        self.pending_marker_line = false;
    }

    pub(super) fn start_heading(&mut self, level: HeadingLevel) {
        if self.in_table_cell() {
            return;
        }
        if self.needs_newline {
            self.push_line(Line::default());
            self.needs_newline = false;
        }
        let heading_style = match level {
            HeadingLevel::H1 => self.styles.h1,
            HeadingLevel::H2 => self.styles.h2,
            HeadingLevel::H3 => self.styles.h3,
            HeadingLevel::H4 => self.styles.h4,
            HeadingLevel::H5 => self.styles.h5,
            HeadingLevel::H6 => self.styles.h6,
        };
        let content = format!("{} ", "#".repeat(level as usize));
        self.push_line(Line::from(vec![Span::styled(content, heading_style)]));
        self.push_inline_style(heading_style);
        self.needs_newline = false;
    }

    pub(super) fn end_heading(&mut self) {
        if self.in_table_cell() {
            return;
        }
        self.needs_newline = true;
        self.pop_inline_style();
    }

    pub(super) fn start_blockquote(&mut self) {
        if self.in_table_cell() {
            return;
        }
        if self.needs_newline {
            self.push_blank_line();
            self.needs_newline = false;
        }
        self.indent_stack.push(IndentContext::new(
            vec![Span::from("> ")],
            /*marker*/ None,
            /*is_list*/ false,
        ));
    }

    pub(super) fn end_blockquote(&mut self) {
        if self.in_table_cell() {
            return;
        }
        self.indent_stack.pop();
        self.needs_newline = true;
    }

    pub(super) fn text(&mut self, text: CowStr<'a>) {
        if self.collecting_local_link_label() {
            let style = self.inline_styles.last().copied().unwrap_or_default();
            for (index, line) in text.lines().enumerate() {
                if index > 0 {
                    self.push_local_link_label_break();
                }
                self.push_local_link_label_span(Span::styled(line.to_string(), style));
            }
            return;
        }
        self.line_ends_with_local_link_target = false;
        if self.in_table_cell() {
            self.push_text_to_table_cell(&text);
            return;
        }

        if self.pending_marker_line {
            self.push_line(Line::default());
        }
        self.pending_marker_line = false;

        // When inside a fenced code block with a known language, accumulate
        // text into the buffer for batch highlighting in end_codeblock().
        // Append verbatim — pulldown-cmark text events already contain the
        // original line breaks, so inserting separators would double them.
        if self.in_code_block && self.code_block_lang.is_some() {
            self.code_block_buffer.push_str(&text);
            return;
        }

        if self.in_code_block && !self.needs_newline {
            let has_content = self
                .current_line_content
                .as_ref()
                .map(|line| !line.line.spans.is_empty())
                .unwrap_or_else(|| {
                    self.text
                        .last()
                        .map(|line| !line.line.spans.is_empty())
                        .unwrap_or(false)
                });
            if has_content {
                self.push_line(Line::default());
            }
        }
        for (i, line) in text.lines().enumerate() {
            if self.needs_newline {
                self.push_line(Line::default());
                self.needs_newline = false;
            }
            if i > 0 {
                self.push_line(Line::default());
            }
            let content = line.to_string();
            let style = self.inline_styles.last().copied().unwrap_or_default();
            self.push_text_spans(&content, style);
        }
        self.needs_newline = false;
    }

    pub(super) fn code(&mut self, code: CowStr<'a>) {
        if self.collecting_local_link_label() {
            self.push_local_link_label_span(Span::from(code.into_string()).style(self.styles.code));
            return;
        }
        self.line_ends_with_local_link_target = false;
        if self.in_table_cell() {
            self.push_span_to_table_cell(Span::from(code.into_string()).style(self.styles.code));
            return;
        }

        if self.pending_marker_line {
            self.push_line(Line::default());
            self.pending_marker_line = false;
        }
        let span = Span::from(code.into_string()).style(self.styles.code);
        self.push_span(span);
    }

    pub(super) fn html(&mut self, html: CowStr<'a>, inline: bool) {
        if self.collecting_local_link_label() {
            let style = self.inline_styles.last().copied().unwrap_or_default();
            for (index, line) in html.lines().enumerate() {
                if index > 0 {
                    self.push_local_link_label_break();
                }
                self.push_local_link_label_span(Span::styled(line.to_string(), style));
            }
            if !inline {
                self.push_local_link_label_break();
            }
            return;
        }
        self.line_ends_with_local_link_target = false;
        if self.in_table_cell() {
            let style = self.inline_styles.last().copied().unwrap_or_default();
            for (i, line) in html.lines().enumerate() {
                if i > 0 {
                    self.push_table_cell_hard_break();
                }
                self.push_span_to_table_cell(Span::styled(line.to_string(), style));
            }
            if !inline {
                self.push_table_cell_hard_break();
            }
            return;
        }
        self.pending_marker_line = false;
        for (i, line) in html.lines().enumerate() {
            if self.needs_newline {
                self.push_line(Line::default());
                self.needs_newline = false;
            }
            if i > 0 {
                self.push_line(Line::default());
            }
            let style = self.inline_styles.last().copied().unwrap_or_default();
            self.push_span(Span::styled(line.to_string(), style));
        }
        self.needs_newline = !inline;
    }

    pub(super) fn hard_break(&mut self) {
        if self.collecting_local_link_label() {
            self.push_local_link_label_break();
            return;
        }
        self.line_ends_with_local_link_target = false;
        if self.in_table_cell() {
            self.push_table_cell_hard_break();
            return;
        }
        self.push_line(Line::default());
    }

    pub(super) fn soft_break(&mut self) {
        if self.collecting_local_link_label() {
            self.push_local_link_label_break();
            return;
        }
        if self.in_table_cell() {
            let style = self.inline_styles.last().copied().unwrap_or_default();
            self.push_span_to_table_cell(Span::styled(" ".to_string(), style));
            return;
        }
        if self.line_ends_with_local_link_target {
            self.pending_local_link_soft_break = true;
            self.line_ends_with_local_link_target = false;
            return;
        }
        self.line_ends_with_local_link_target = false;
        self.push_line(Line::default());
    }

    pub(super) fn start_list(&mut self, index: Option<u64>) {
        if self.list_indices.is_empty() && self.needs_newline {
            self.push_line(Line::default());
        }
        self.list_indices.push(index);
        self.list_needs_blank_before_next_item.push(false);
    }

    pub(super) fn end_list(&mut self) {
        self.list_indices.pop();
        self.list_needs_blank_before_next_item.pop();
        self.needs_newline = true;
    }

    pub(super) fn start_item(&mut self) {
        if self
            .list_needs_blank_before_next_item
            .last_mut()
            .map(std::mem::take)
            .unwrap_or(false)
        {
            self.push_blank_line();
        }
        self.flush_current_line();
        self.list_item_start_line_counts.push(self.text.len());
        self.pending_marker_line = true;
        let depth = self.list_indices.len();
        let is_ordered = self
            .list_indices
            .last()
            .map(Option::is_some)
            .unwrap_or(false);
        let width = depth * 4 - 3;
        let marker = if let Some(last_index) = self.list_indices.last_mut() {
            match last_index {
                None => Some(vec![Span::styled(
                    " ".repeat(width - 1) + "- ",
                    self.styles.unordered_list_marker,
                )]),
                Some(index) => {
                    *index += 1;
                    Some(vec![Span::styled(
                        format!("{:width$}. ", *index - 1),
                        self.styles.ordered_list_marker,
                    )])
                }
            }
        } else {
            None
        };
        let indent_prefix = if depth == 0 {
            Vec::new()
        } else {
            let indent_len = if is_ordered { width + 2 } else { width + 1 };
            vec![Span::from(" ".repeat(indent_len))]
        };
        self.indent_stack.push(IndentContext::new(
            indent_prefix,
            marker,
            /*is_list*/ true,
        ));
        self.needs_newline = false;
    }

    pub(super) fn start_codeblock(&mut self, lang: Option<String>, indent: Option<Span<'static>>) {
        self.flush_current_line();
        if !self.text.is_empty() {
            self.push_blank_line();
        }
        self.in_code_block = true;

        // Extract the language token from the info string.  CommonMark info
        // strings can contain metadata after the language, separated by commas,
        // spaces, or other delimiters (e.g. "rust,no_run", "rust title=demo").
        // Take only the first token so the syntax lookup succeeds.
        let lang = lang
            .as_deref()
            .and_then(|s| s.split([',', ' ', '\t']).next())
            .filter(|s| !s.is_empty())
            .map(std::string::ToString::to_string);
        self.code_block_lang = lang;
        self.code_block_buffer.clear();

        self.indent_stack.push(IndentContext::new(
            vec![indent.unwrap_or_default()],
            /*marker*/ None,
            /*is_list*/ false,
        ));
        self.needs_newline = true;
    }

    pub(super) fn end_codeblock(&mut self) {
        // If we buffered code for a known language, syntax-highlight it now.
        if let Some(lang) = self.code_block_lang.take() {
            let code = std::mem::take(&mut self.code_block_buffer);
            if !code.is_empty() {
                let highlighted = highlight_code_to_lines(&code, &lang);
                for hl_line in highlighted {
                    self.push_line(Line::default());
                    for span in hl_line.spans {
                        self.push_span(span);
                    }
                }
            }
        }

        self.needs_newline = true;
        self.in_code_block = false;
        self.indent_stack.pop();
    }
}
