use super::*;

impl<'a, 'policy, I> Writer<'a, 'policy, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    pub(super) fn start_table(&mut self, alignments: Vec<Alignment>) {
        self.flush_current_line();
        if self.needs_newline {
            self.push_blank_line();
            self.needs_newline = false;
        }
        self.table_state = Some(TableState::new(alignments));
    }

    pub(super) fn end_table(&mut self) {
        let Some(table_state) = self.table_state.take() else {
            return;
        };

        let RenderedTableLines {
            table_lines,
            table_lines_prewrapped,
            spillover_lines,
        } = self.render_table_lines(table_state);
        let mut pending_marker_line = self.pending_marker_line;
        for line in table_lines {
            if table_lines_prewrapped {
                self.push_prewrapped_line(line, pending_marker_line);
            } else {
                self.push_hyperlink_line(line);
                self.flush_current_line();
            }
            pending_marker_line = false;
        }
        self.pending_marker_line = false;
        for spillover_line in spillover_lines {
            self.push_hyperlink_line(spillover_line);
            self.flush_current_line();
        }
        self.needs_newline = true;
    }

    pub(super) fn start_table_head(&mut self) {
        if let Some(table_state) = self.table_state.as_mut() {
            table_state.in_header = true;
            table_state.current_row = Some(Vec::new());
        }
    }

    pub(super) fn end_table_head(&mut self) {
        let Some(table_state) = self.table_state.as_mut() else {
            return;
        };
        if let Some(current_cell) = table_state.current_cell.take() {
            table_state
                .current_row
                .get_or_insert_with(Vec::new)
                .push(current_cell);
        }
        if let Some(row) = table_state.current_row.take() {
            table_state.header = Some(row);
        }
        table_state.in_header = false;
    }

    pub(super) fn start_table_row(&mut self, source_range: Range<usize>) {
        let has_table_pipe_syntax = self.has_table_row_boundary_pipe(source_range);
        if let Some(table_state) = self.table_state.as_mut() {
            table_state.current_row = Some(Vec::new());
            table_state.current_row_has_table_pipe_syntax = has_table_pipe_syntax;
        }
    }

    pub(super) fn has_table_row_boundary_pipe(&self, source_range: Range<usize>) -> bool {
        let Some(source) = self.input.get(source_range) else {
            return false;
        };
        let source = source.trim();
        source.starts_with('|') || source.ends_with('|')
    }

    pub(super) fn end_table_row(&mut self) {
        let Some(table_state) = self.table_state.as_mut() else {
            return;
        };

        if let Some(current_cell) = table_state.current_cell.take() {
            table_state
                .current_row
                .get_or_insert_with(Vec::new)
                .push(current_cell);
        }

        let Some(row) = table_state.current_row.take() else {
            return;
        };

        if table_state.in_header {
            table_state.header = Some(row);
        } else {
            table_state.rows.push(TableBodyRow {
                cells: row,
                has_table_pipe_syntax: table_state.current_row_has_table_pipe_syntax,
            });
        }
        table_state.current_row_has_table_pipe_syntax = false;
    }

    pub(super) fn start_table_cell(&mut self) {
        if let Some(table_state) = self.table_state.as_mut() {
            table_state.current_cell = Some(TableCell::default());
        }
    }

    pub(super) fn end_table_cell(&mut self) {
        let Some(table_state) = self.table_state.as_mut() else {
            return;
        };

        if let Some(cell) = table_state.current_cell.take() {
            table_state
                .current_row
                .get_or_insert_with(Vec::new)
                .push(cell);
        }
    }

    pub(super) fn in_table_cell(&self) -> bool {
        self.table_state
            .as_ref()
            .and_then(|table_state| table_state.current_cell.as_ref())
            .is_some()
    }

    pub(super) fn push_span_to_table_cell(&mut self, span: Span<'static>) {
        let span = self.style_link_label(span);
        let mut annotated = HyperlinkLine::new(Line::default());
        annotated.push_span(
            span,
            self.link.as_ref().map(|link| link.destination.as_str()),
        );
        if let Some(table_state) = self.table_state.as_mut()
            && let Some(cell) = table_state.current_cell.as_mut()
        {
            cell.push_annotated(annotated);
        }
    }

    pub(super) fn push_table_cell_hard_break(&mut self) {
        if let Some(table_state) = self.table_state.as_mut()
            && let Some(cell) = table_state.current_cell.as_mut()
        {
            cell.hard_break();
        }
    }

    pub(super) fn push_text_to_table_cell(&mut self, text: &str) {
        let style = self.inline_styles.last().copied().unwrap_or_default();
        for (i, line) in text.lines().enumerate() {
            if i > 0 {
                self.push_table_cell_hard_break();
            }
            self.push_text_spans_to_table_cell(line, style);
        }
    }

    pub(super) fn push_text_spans_to_table_cell(&mut self, text: &str, style: Style) {
        let span = self.style_link_label(Span::styled(text.to_string(), style));
        let destination = self
            .link
            .as_ref()
            .and_then(|link| web_destination(&link.destination));
        let mut annotated = if let Some(destination) = destination {
            let mut annotated = HyperlinkLine::new(Line::default());
            annotated.push_span(span, Some(&destination));
            annotated
        } else if self.link.is_some() || self.in_code_block {
            HyperlinkLine::new(Line::from(span))
        } else {
            annotate_web_urls_in_line(Line::from(span))
        };
        if let Some(table_state) = self.table_state.as_mut()
            && let Some(cell) = table_state.current_cell.as_mut()
        {
            cell.push_annotated(std::mem::take(&mut annotated));
        }
    }
}
