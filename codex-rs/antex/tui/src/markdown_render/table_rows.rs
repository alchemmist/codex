use super::*;

impl<'a, 'policy, I> Writer<'a, 'policy, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    pub(super) fn render_table_separator(
        column_widths: &[usize],
        separator_char: char,
        style: Style,
    ) -> HyperlinkLine {
        let segment_char = separator_char.to_string();
        let gap = " ".repeat(TABLE_COLUMN_GAP);
        let text = column_widths
            .iter()
            .map(|width| segment_char.repeat(*width + (TABLE_CELL_PADDING * 2)))
            .collect::<Vec<_>>()
            .join(&gap);
        HyperlinkLine::new(Line::from(Span::styled(text, style)))
    }

    pub(super) fn render_table_row(
        &self,
        row: &[TableCell],
        column_widths: &[usize],
        alignments: &[Alignment],
        row_style: Style,
    ) -> Vec<HyperlinkLine> {
        let wrapped_cells: Vec<Vec<HyperlinkLine>> = row
            .iter()
            .zip(column_widths)
            .map(|(cell, width)| self.wrap_cell(cell, *width))
            .collect();
        let row_height = wrapped_cells.iter().map(Vec::len).max().unwrap_or(1);

        let mut out = Vec::with_capacity(row_height);
        for row_line in 0..row_height {
            let Some(last_visible_column) = wrapped_cells.iter().rposition(|lines| {
                lines
                    .get(row_line)
                    .is_some_and(|line| Self::line_display_width(&line.line) > 0)
            }) else {
                out.push(HyperlinkLine::new(Line::default().style(row_style)));
                continue;
            };
            let mut spans = Vec::new();
            for (column, width) in column_widths
                .iter()
                .enumerate()
                .take(last_visible_column + 1)
            {
                spans.push(Span::raw(" ".repeat(TABLE_CELL_PADDING)));
                let mut line = wrapped_cells[column]
                    .get(row_line)
                    .cloned()
                    .unwrap_or_default();
                let line_width = Self::line_display_width(&line.line);
                let remaining = width.saturating_sub(line_width);
                let (left_padding, right_padding) = match alignments[column] {
                    Alignment::Left | Alignment::None => (0, remaining),
                    Alignment::Center => (remaining / 2, remaining - (remaining / 2)),
                    Alignment::Right => (remaining, 0),
                };
                if left_padding > 0 {
                    spans.push(Span::raw(" ".repeat(left_padding)));
                }
                spans.append(&mut line.line.spans);
                let is_last_column = column == last_visible_column;
                if right_padding > 0 && !is_last_column {
                    spans.push(Span::raw(" ".repeat(right_padding)));
                }
                if !is_last_column {
                    spans.push(Span::raw(" ".repeat(TABLE_CELL_PADDING)));
                }
                if !is_last_column {
                    spans.push(Span::raw(" ".repeat(TABLE_COLUMN_GAP)));
                }
            }
            let mut out_line = HyperlinkLine::new(Line::from(spans).style(row_style));
            let mut column_start = 0usize;
            for (column, width) in column_widths
                .iter()
                .enumerate()
                .take(last_visible_column + 1)
            {
                column_start += TABLE_CELL_PADDING;
                if let Some(line) = wrapped_cells[column].get(row_line) {
                    let remaining = width.saturating_sub(Self::line_display_width(&line.line));
                    let left_padding = match alignments[column] {
                        Alignment::Left | Alignment::None => 0,
                        Alignment::Center => remaining / 2,
                        Alignment::Right => remaining,
                    };
                    out_line
                        .hyperlinks
                        .extend(line.hyperlinks.iter().cloned().map(|mut link| {
                            link.columns = link.columns.start + column_start + left_padding
                                ..link.columns.end + column_start + left_padding;
                            link
                        }));
                }
                column_start += *width + TABLE_CELL_PADDING + TABLE_COLUMN_GAP;
            }
            out.push(out_line);
        }
        out
    }

    /// Render a header-only table as raw pipe-delimited lines (`| A | B |`).
    ///
    /// Used when `compute_column_widths` returns `None` and there are no body
    /// records to transpose. Pipe characters inside cell content are escaped
    /// as `\|` so downstream parsers keep cell boundaries intact.
    pub(super) fn render_table_pipe_fallback(
        &self,
        header: &[TableCell],
        rows: &[Vec<TableCell>],
        alignments: &[Alignment],
    ) -> Vec<HyperlinkLine> {
        let mut out = Vec::new();
        out.push(Self::row_to_pipe_line(header));
        out.push(HyperlinkLine::new(Line::from(
            Self::alignments_to_pipe_delimiter(alignments),
        )));
        out.extend(rows.iter().map(|row| Self::row_to_pipe_line(row)));
        out
    }

    pub(super) fn row_to_pipe_line(row: &[TableCell]) -> HyperlinkLine {
        let mut out = HyperlinkLine::new(Line::default());
        out.push_span("|".into(), /*destination*/ None);
        for cell in row {
            out.push_span(" ".into(), /*destination*/ None);
            for (index, line) in cell.lines.iter().enumerate() {
                if index > 0 {
                    out.push_span(" ".into(), /*destination*/ None);
                }
                let text = line
                    .line
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>();
                let mut column = 0usize;
                let mut current_destination = None;
                let mut current_text = String::new();
                let flush = |out: &mut HyperlinkLine,
                             current_text: &mut String,
                             destination: Option<&str>| {
                    if !current_text.is_empty() {
                        out.push_span(Span::raw(std::mem::take(current_text)), destination);
                    }
                };
                for ch in text.chars() {
                    let destination = line
                        .hyperlinks
                        .iter()
                        .find(|link| link.columns.contains(&column))
                        .map(|link| link.destination.as_str());
                    if destination != current_destination {
                        flush(&mut out, &mut current_text, current_destination);
                        current_destination = destination;
                    }
                    if ch == '|' {
                        current_text.push_str("\\|");
                    } else {
                        current_text.push(ch);
                    }
                    column += char_width(ch);
                }
                flush(&mut out, &mut current_text, current_destination);
            }
            out.push_span(" |".into(), /*destination*/ None);
        }
        out
    }

    pub(super) fn alignments_to_pipe_delimiter(alignments: &[Alignment]) -> String {
        let mut out = String::new();
        out.push('|');
        for alignment in alignments {
            let segment = match alignment {
                Alignment::Left => ":---",
                Alignment::Center => ":---:",
                Alignment::Right => "---:",
                Alignment::None => "---",
            };
            out.push_str(segment);
            out.push('|');
        }
        out
    }

    /// Wrap a single table cell's content to `width`, preserving rich inline
    /// styling (bold, code, links) across wrapped lines.
    ///
    /// Each logical line within the cell (separated by hard breaks) is wrapped
    /// independently.  Empty cells produce a single blank line so the row grid
    /// stays aligned.
    pub(super) fn wrap_cell(&self, cell: &TableCell, width: usize) -> Vec<HyperlinkLine> {
        if cell.lines.is_empty() {
            return vec![HyperlinkLine::new(Line::default())];
        }
        let mut wrapped = Vec::new();
        for source_line in &cell.lines {
            let rendered =
                word_wrap_line(&source_line.line, RtOptions::new(width.max(/*other*/ 1)))
                    .into_iter()
                    .map(|line| line_to_static(&line))
                    .collect::<Vec<_>>();
            if rendered.is_empty() {
                wrapped.push(HyperlinkLine::new(Line::default()));
            } else {
                wrapped.extend(remap_wrapped_line(source_line, rendered));
            };
        }
        if wrapped.is_empty() {
            wrapped.push(HyperlinkLine::new(Line::default()));
        }
        wrapped
    }

    /// Detect rows that are artifacts of pulldown-cmark's lenient table parsing.
    ///
    /// pulldown-cmark accepts body rows without leading pipes, which can absorb a
    /// trailing paragraph as a single-cell row in a multi-column table. These
    /// "spillover" rows are extracted and rendered as plain text after the table
    /// grid so they don't appear as malformed table content.
    ///
    /// Heuristic: a row is spillover if its only non-empty cell is the first one
    /// AND (a single-cell row lacked table pipe syntax, the content looks like
    /// HTML, it's a label line followed by HTML content, or a trailing
    /// HTML-intro label line).
    pub(super) fn is_spillover_row(row: &TableBodyRow, next_row: Option<&TableBodyRow>) -> bool {
        let Some(first_text) = Self::first_non_empty_only_text(&row.cells) else {
            return false;
        };

        if row.cells.len() == 1 && !row.has_table_pipe_syntax {
            return true;
        }

        if Self::looks_like_html_content(&first_text) {
            return true;
        }

        // Keep common intro + html-block spillover together:
        // "HTML block:" followed by "<div ...>".
        if first_text.trim_end().ends_with(':') {
            if next_row
                .and_then(|row| Self::first_non_empty_only_text(&row.cells))
                .is_some_and(|text| Self::looks_like_html_content(&text))
            {
                return true;
            }

            // pulldown can end the table before the corresponding HTML block line.
            // In that case, treat trailing HTML-intro labels (e.g., "HTML block:")
            // as spillover while keeping explicit sparse labels in real tables.
            if next_row.is_none() && Self::looks_like_html_label_line(&first_text) {
                return true;
            }
        }

        false
    }

    pub(super) fn first_non_empty_only_text(row: &[TableCell]) -> Option<String> {
        let first = row.first()?.plain_text();
        if first.trim().is_empty() {
            return None;
        }
        let rest_empty = row[1..]
            .iter()
            .all(|cell| cell.plain_text().trim().is_empty());
        rest_empty.then_some(first)
    }

    pub(super) fn looks_like_html_content(text: &str) -> bool {
        let bytes = text.as_bytes();
        for (idx, &byte) in bytes.iter().enumerate() {
            if byte != b'<' {
                continue;
            }

            let mut tag_start = idx + 1;
            if tag_start < bytes.len() && (bytes[tag_start] == b'/' || bytes[tag_start] == b'!') {
                tag_start += 1;
            }

            if bytes.get(tag_start).is_some_and(u8::is_ascii_alphabetic)
                && bytes
                    .get(tag_start + 1..)
                    .is_some_and(|suffix| suffix.contains(&b'>'))
            {
                return true;
            }
        }
        false
    }

    pub(super) fn looks_like_html_label_line(text: &str) -> bool {
        let trimmed = text.trim();
        if !trimmed.ends_with(':') {
            return false;
        }
        let prefix = trimmed.trim_end_matches(':').trim();
        prefix
            .split_whitespace()
            .any(|word| word.eq_ignore_ascii_case("html"))
    }

    // Width-measurement helpers inlined — called per-cell during table column
    // width computation, which runs on every re-render.

    #[inline]
    pub(super) fn spans_display_width(spans: &[Span<'_>]) -> usize {
        spans
            .iter()
            .map(|span| display_width(span.content.as_ref()))
            .sum()
    }

    #[inline]
    pub(super) fn line_display_width(line: &Line<'_>) -> usize {
        Self::spans_display_width(&line.spans)
    }

    #[inline]
    pub(super) fn cell_display_width(cell: &TableCell) -> usize {
        cell.lines
            .iter()
            .map(|line| Self::line_display_width(&line.line))
            .max()
            .unwrap_or(0)
    }

    #[inline]
    pub(super) fn longest_token_width(text: &str) -> usize {
        text.split_whitespace()
            .map(display_width)
            .max()
            .unwrap_or(0)
    }
}
