use super::*;

impl<'a, 'policy, I> Writer<'a, 'policy, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    pub(super) fn push_inline_style(&mut self, style: Style) {
        let current = self.inline_styles.last().copied().unwrap_or_default();
        let merged = current.patch(style);
        self.inline_styles.push(merged);
    }

    pub(super) fn pop_inline_style(&mut self) {
        self.inline_styles.pop();
    }

    pub(super) fn style_link_label(&mut self, mut span: Span<'static>) -> Span<'static> {
        if let Some(link) = self.link.as_mut()
            && web_destination(&link.destination).is_some()
        {
            link.has_visible_label |=
                !span.content.trim().is_empty() && display_width(&span.content) > 0;
            span.style = span.style.patch(self.styles.link);
        }
        span
    }

    pub(super) fn push_link(&mut self, dest_url: String) {
        let style_label = (self.is_hidden_link_destination)(&dest_url);
        if style_label {
            self.push_inline_style(self.styles.link);
        }
        let show_destination = !style_label && should_render_link_destination(&dest_url);
        self.link = Some(LinkState {
            show_destination,
            style_label,
            has_visible_label: false,
            local_target_display: if is_local_path_like_link(&dest_url) {
                render_local_link_target(&dest_url, self.cwd.as_deref())
            } else {
                None
            },
            local_label_spans: Vec::new(),
            destination: dest_url,
        });
    }

    pub(super) fn pop_link(&mut self) {
        if let Some(link) = self.link.take() {
            if link.style_label {
                self.pop_inline_style();
            }
            if link.show_destination
                || (!link.has_visible_label && web_destination(&link.destination).is_some())
            {
                // Link destinations are rendered as " (url)" suffixes. When parsing table cells,
                // append the suffix into the active cell buffer rather than the outer paragraph
                // line to avoid detached url lines.
                if self.in_table_cell() {
                    self.push_span_to_table_cell(" (".into());
                    let mut destination = HyperlinkLine::new(Line::default());
                    destination.push_span(
                        Span::styled(link.destination.clone(), self.styles.link),
                        web_destination(&link.destination).as_deref(),
                    );
                    if let Some(table_state) = self.table_state.as_mut()
                        && let Some(cell) = table_state.current_cell.as_mut()
                    {
                        cell.push_annotated(destination);
                    }
                    self.push_span_to_table_cell(")".into());
                } else {
                    self.push_span(" (".into());
                    let mut destination = HyperlinkLine::new(Line::default());
                    destination.push_span(
                        Span::styled(link.destination.clone(), self.styles.link),
                        web_destination(&link.destination).as_deref(),
                    );
                    self.push_annotated(destination);
                    self.push_span(")".into());
                }
            } else if let Some(local_target_display) = link.local_target_display {
                let local_label_text = link
                    .local_label_spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>();
                let show_label =
                    should_render_local_link_label(&local_label_text, &link.destination);
                let style = self
                    .inline_styles
                    .last()
                    .copied()
                    .unwrap_or_default()
                    .patch(self.styles.code);
                let span = Span::styled(local_target_display, style);
                if self.in_table_cell() {
                    if show_label {
                        for label_span in link.local_label_spans {
                            self.push_span_to_table_cell(label_span);
                        }
                        self.push_span_to_table_cell(" (".into());
                    }
                    self.push_span_to_table_cell(span);
                    if show_label {
                        self.push_span_to_table_cell(")".into());
                    }
                } else {
                    if self.pending_marker_line {
                        self.push_line(Line::default());
                    }
                    if show_label {
                        for label_span in link.local_label_spans {
                            self.push_span(label_span);
                        }
                        self.push_span(" (".into());
                    }
                    self.push_span(span);
                    if show_label {
                        self.push_span(")".into());
                    }
                    self.line_ends_with_local_link_target = true;
                }
            }
        }
    }

    pub(super) fn collecting_local_link_label(&self) -> bool {
        self.link
            .as_ref()
            .and_then(|link| link.local_target_display.as_ref())
            .is_some()
    }

    pub(super) fn push_local_link_label_span(&mut self, span: Span<'static>) {
        if let Some(link) = self.link.as_mut() {
            link.local_label_spans.push(span);
        }
    }

    pub(super) fn push_local_link_label_break(&mut self) {
        let needs_space = self
            .link
            .as_ref()
            .and_then(|link| link.local_label_spans.last())
            .and_then(|span| span.content.chars().last())
            .is_some_and(|character| !character.is_whitespace());
        if needs_space {
            self.push_local_link_label_span(" ".into());
        }
    }

    pub(super) fn flush_current_line(&mut self) {
        if let Some(mut line) = self.current_line_content.take() {
            let style = self.current_line_style;
            // NB we don't wrap code in code blocks, in order to preserve whitespace for copy/paste.
            if !self.current_line_in_code_block
                && let Some(width) = self.wrap_width
            {
                let opts = RtOptions::new(width)
                    .initial_indent(self.current_initial_indent.clone().into())
                    .subsequent_indent(self.current_subsequent_indent.clone().into());
                let wrapped = adaptive_wrap_line(&line.line, opts)
                    .into_iter()
                    .map(|wrapped| line_to_static(&wrapped))
                    .collect();
                for wrapped in remap_wrapped_line(&line, wrapped) {
                    self.push_output_line(wrapped.style(style));
                }
            } else {
                let mut spans = self.current_initial_indent.clone();
                let shift = Self::spans_display_width(&spans);
                spans.append(&mut line.line.spans);
                for hyperlink in &mut line.hyperlinks {
                    hyperlink.columns =
                        hyperlink.columns.start + shift..hyperlink.columns.end + shift;
                }
                line.line = Line::from_iter(spans);
                self.push_output_line(line.style(style));
            }
            self.current_initial_indent.clear();
            self.current_subsequent_indent.clear();
            self.current_line_in_code_block = false;
            self.line_ends_with_local_link_target = false;
        }
    }

    /// Push a line that has already been laid out at the correct width, skipping
    /// word wrapping.
    ///
    /// Table lines are pre-formatted with exact column widths and separators.
    /// Passing them through `word_wrap_line` would break the layout at
    /// arbitrary positions. This method prepends the indent/blockquote prefix
    /// and pushes directly to `self.text`.
    pub(super) fn is_blockquote_active(&self) -> bool {
        self.indent_stack
            .iter()
            .any(|ctx| ctx.prefix.iter().any(|p| p.content.contains('>')))
    }

    pub(super) fn push_prewrapped_line(
        &mut self,
        mut line: HyperlinkLine,
        pending_marker_line: bool,
    ) {
        self.flush_current_line();
        let blockquote_active = self.is_blockquote_active();
        let style = if blockquote_active {
            self.styles.blockquote.patch(line.line.style)
        } else {
            line.line.style
        };

        let mut spans = self.prefix_spans(pending_marker_line);
        let shift = Self::spans_display_width(&spans);
        spans.append(&mut line.line.spans);
        for hyperlink in &mut line.hyperlinks {
            hyperlink.columns = hyperlink.columns.start + shift..hyperlink.columns.end + shift;
        }
        line.line = Line::from(spans);
        self.push_output_line(line.style(style));
    }

    pub(super) fn push_line(&mut self, line: Line<'static>) {
        self.flush_current_line();
        let blockquote_active = self.is_blockquote_active();
        let style = if blockquote_active {
            self.styles.blockquote
        } else {
            line.style
        };
        let was_pending = self.pending_marker_line;

        self.current_initial_indent = self.prefix_spans(was_pending);
        self.current_subsequent_indent = self.prefix_spans(/*pending_marker_line*/ false);
        self.current_line_style = style;
        self.current_line_content = Some(HyperlinkLine::new(line));
        self.current_line_in_code_block = self.in_code_block;
        self.line_ends_with_local_link_target = false;

        self.pending_marker_line = false;
    }

    pub(super) fn push_hyperlink_line(&mut self, line: HyperlinkLine) {
        let hyperlinks = line.hyperlinks;
        self.push_line(line.line);
        if let Some(current) = self.current_line_content.as_mut() {
            current.hyperlinks = hyperlinks;
        }
    }

    pub(super) fn push_span(&mut self, span: Span<'static>) {
        let span = self.style_link_label(span);
        if self.current_line_content.is_none() {
            self.push_line(Line::default());
        }
        if let Some(line) = self.current_line_content.as_mut() {
            line.push_span(
                span,
                self.link.as_ref().map(|link| link.destination.as_str()),
            );
        }
    }

    pub(super) fn push_annotated(&mut self, mut appended: HyperlinkLine) {
        if self.current_line_content.is_none() {
            self.push_line(Line::default());
        }
        if let Some(line) = self.current_line_content.as_mut() {
            let shift = line.width();
            line.line.spans.append(&mut appended.line.spans);
            line.hyperlinks
                .extend(appended.hyperlinks.into_iter().map(|mut link| {
                    link.columns = link.columns.start + shift..link.columns.end + shift;
                    link
                }));
        }
    }

    pub(super) fn push_text_spans(&mut self, text: &str, style: Style) {
        let span = self.style_link_label(Span::styled(text.to_string(), style));
        let destination = self
            .link
            .as_ref()
            .and_then(|link| web_destination(&link.destination));
        let annotated = if let Some(destination) = destination {
            let mut annotated = HyperlinkLine::new(Line::default());
            annotated.push_span(span, Some(&destination));
            annotated
        } else if self.link.is_some() || self.in_code_block {
            HyperlinkLine::new(Line::from(span))
        } else {
            annotate_web_urls_in_line(Line::from(span))
        };
        self.push_annotated(annotated);
    }

    pub(super) fn push_blank_line(&mut self) {
        self.flush_current_line();
        if self.indent_stack.iter().all(|ctx| ctx.is_list) {
            self.push_output_line(HyperlinkLine::new(Line::default()));
        } else {
            self.push_line(Line::default());
            self.flush_current_line();
        }
    }

    pub(super) fn push_output_line(&mut self, line: HyperlinkLine) {
        self.text.push(line);
    }

    pub(super) fn prefix_spans(&self, pending_marker_line: bool) -> Vec<Span<'static>> {
        let mut prefix: Vec<Span<'static>> = Vec::new();
        let last_marker_index = if pending_marker_line {
            self.indent_stack
                .iter()
                .enumerate()
                .rev()
                .find_map(|(i, ctx)| if ctx.marker.is_some() { Some(i) } else { None })
        } else {
            None
        };
        let last_list_index = self.indent_stack.iter().rposition(|ctx| ctx.is_list);

        for (i, ctx) in self.indent_stack.iter().enumerate() {
            if pending_marker_line {
                if Some(i) == last_marker_index
                    && let Some(marker) = &ctx.marker
                {
                    prefix.extend(marker.iter().cloned());
                    continue;
                }
                if ctx.is_list && last_marker_index.is_some_and(|idx| idx > i) {
                    continue;
                }
            } else if ctx.is_list && Some(i) != last_list_index {
                continue;
            }
            prefix.extend(ctx.prefix.iter().cloned());
        }

        prefix
    }
}
