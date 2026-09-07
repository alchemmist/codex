use super::*;

impl<'a, 'policy, I> Writer<'a, 'policy, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    /// Convert a completed `TableState` into styled table `Line`s.
    ///
    /// Pipeline: filter spillover rows -> normalize column counts -> compute
    /// column widths -> render aligned rows or key/value records when values
    /// systemically lose token readability or expansive cells become tall
    /// narrow strips. Spillover rows are appended as plain text after the
    /// table.
    ///
    /// Falls back to key/value records when body rows cannot fit in the aligned
    /// grid; header-only tables retain raw pipe output because they contain no
    /// records to transpose.
    pub(super) fn render_table_lines(&self, mut table_state: TableState) -> RenderedTableLines {
        let column_count = table_state.alignments.len();
        if column_count == 0 {
            return RenderedTableLines {
                table_lines: Vec::new(),
                table_lines_prewrapped: true,
                spillover_lines: Vec::new(),
            };
        }

        let mut spillover_rows: Vec<TableCell> = Vec::with_capacity(4);
        let mut rows: Vec<Vec<TableCell>> = Vec::with_capacity(table_state.rows.len());
        for (row_idx, row) in table_state.rows.iter().enumerate() {
            let next_row = table_state.rows.get(row_idx + 1);
            // pulldown-cmark accepts body rows without pipes, which can turn a following paragraph
            // into a one-cell table row. For multi-column tables, treat those as spillover text
            // rendered after the table.
            if column_count > 1 && Self::is_spillover_row(row, next_row) {
                if let Some(cell) = row.cells.first().cloned() {
                    spillover_rows.push(cell);
                }
            } else {
                rows.push(row.cells.clone());
            }
        }

        let mut header = table_state
            .header
            .take()
            .unwrap_or_else(|| vec![TableCell::default(); column_count]);
        Self::normalize_row(&mut header, column_count);
        for row in &mut rows {
            Self::normalize_row(row, column_count);
        }

        let metrics = Self::collect_table_column_metrics(&header, &rows, column_count);
        let available_width = self.available_table_width(column_count);
        let widths = Self::compute_column_widths(&metrics, available_width);
        let spillover_lines: Vec<HyperlinkLine> = spillover_rows
            .into_iter()
            .flat_map(|spillover| spillover.lines)
            .collect();
        let header_style =
            foreground_style_for_scopes(&["entity.name.type", "support.type", "variable"])
                .unwrap_or(self.styles.strong)
                .bold();
        let separator_style = table_separator_style();

        let Some(column_widths) = widths else {
            if !rows.is_empty() {
                return RenderedTableLines {
                    table_lines: table_key_value::render_records(
                        &header,
                        &rows,
                        &metrics,
                        self.available_record_width(),
                        header_style,
                        separator_style,
                    ),
                    table_lines_prewrapped: true,
                    spillover_lines,
                };
            }
            return RenderedTableLines {
                table_lines: self.render_table_pipe_fallback(
                    &header,
                    &rows,
                    &table_state.alignments,
                ),
                table_lines_prewrapped: false,
                spillover_lines,
            };
        };

        if table_key_value::should_render_records(&rows, &column_widths, &metrics) {
            return RenderedTableLines {
                table_lines: table_key_value::render_records(
                    &header,
                    &rows,
                    &metrics,
                    self.available_record_width(),
                    header_style,
                    separator_style,
                ),
                table_lines_prewrapped: true,
                spillover_lines,
            };
        }

        let mut out = Vec::with_capacity(2 + rows.len() * 2);
        out.extend(self.render_table_row(
            &header,
            &column_widths,
            &table_state.alignments,
            header_style,
        ));
        out.push(Self::render_table_separator(
            &column_widths,
            TABLE_HEADER_SEPARATOR_CHAR,
            separator_style,
        ));
        for (row_idx, row) in rows.iter().enumerate() {
            out.extend(self.render_table_row(
                row,
                &column_widths,
                &table_state.alignments,
                Style::default(),
            ));
            if row_idx + 1 < rows.len() {
                out.push(Self::render_table_separator(
                    &column_widths,
                    TABLE_BODY_SEPARATOR_CHAR,
                    separator_style,
                ));
            }
        }
        RenderedTableLines {
            table_lines: out,
            table_lines_prewrapped: true,
            spillover_lines,
        }
    }

    pub(super) fn normalize_row(row: &mut Vec<TableCell>, column_count: usize) {
        row.truncate(column_count);
        row.resize(column_count, TableCell::default());
    }

    /// Subtract horizontal gutters and per-cell padding from the content budget.
    pub(super) fn available_table_width(&self, column_count: usize) -> Option<usize> {
        self.wrap_width.map(|wrap_width| {
            let prefix_width =
                Self::spans_display_width(&self.prefix_spans(self.pending_marker_line));
            let reserved = prefix_width
                + (column_count.saturating_sub(1) * TABLE_COLUMN_GAP)
                + (column_count * TABLE_CELL_PADDING * 2);
            wrap_width.saturating_sub(reserved)
        })
    }

    /// Return the full content budget for record fallback rendering.
    pub(super) fn available_record_width(&self) -> Option<usize> {
        self.wrap_width.map(|wrap_width| {
            let prefix_width =
                Self::spans_display_width(&self.prefix_spans(self.pending_marker_line));
            wrap_width.saturating_sub(prefix_width)
        })
    }

    /// Allocate column widths for aligned, row-separated table rendering.
    ///
    /// Each column starts at its natural (max cell content) width, then columns
    /// are shrunk by priority until the total fits within `available_width`.
    /// Token-heavy columns surrender excess width before narrative prose; compact
    /// columns are preserved last. Returns `None` when even the minimum width
    /// (3 chars per column) cannot fit.
    pub(super) fn compute_column_widths(
        metrics: &[TableColumnMetrics],
        available_width: Option<usize>,
    ) -> Option<Vec<usize>> {
        let min_column_width = 3usize;
        let mut widths: Vec<usize> = metrics
            .iter()
            .map(|col| col.max_width.max(min_column_width))
            .collect();

        let Some(max_width) = available_width else {
            return Some(widths);
        };
        let minimum_total = metrics.len() * min_column_width;
        if max_width < minimum_total {
            return None;
        }

        let mut floors: Vec<usize> = metrics
            .iter()
            .map(|col| Self::preferred_column_floor(col, min_column_width))
            .collect();
        let floor_total: usize = floors.iter().sum();
        if floor_total > max_width {
            let minimums = vec![min_column_width; floors.len()];
            Self::shrink_columns(&mut floors, &minimums, metrics, floor_total - max_width);
        }

        let total_width: usize = widths.iter().sum();
        if total_width > max_width {
            let remaining =
                Self::shrink_columns(&mut widths, &floors, metrics, total_width - max_width);
            if remaining > 0 {
                return None;
            }
        }

        Some(widths)
    }

    pub(super) fn collect_table_column_metrics(
        header: &[TableCell],
        rows: &[Vec<TableCell>],
        column_count: usize,
    ) -> Vec<TableColumnMetrics> {
        let mut metrics = Vec::with_capacity(column_count);
        for column in 0..column_count {
            let header_cell = &header[column];
            let header_plain = header_cell.plain_text();
            let header_token_width = Self::longest_token_width(&header_plain);
            let mut max_width = Self::cell_display_width(header_cell);
            let mut body_token_width = 0usize;
            let mut body_token_count = 0usize;
            let mut long_body_token_count = 0usize;
            let mut total_words = 0usize;
            let mut total_cells = 0usize;
            let mut total_cell_width = 0usize;

            for row in rows {
                let cell = &row[column];
                max_width = max_width.max(Self::cell_display_width(cell));
                let plain = cell.plain_text();
                let mut word_count = 0usize;
                for token in plain.split_whitespace() {
                    let token_width = display_width(token);
                    body_token_width = body_token_width.max(token_width);
                    long_body_token_count += usize::from(token_width >= 20);
                    word_count += 1;
                }
                if word_count > 0 {
                    body_token_count += word_count;
                    total_words += word_count;
                    total_cells += 1;
                    total_cell_width += display_width(&plain);
                }
            }

            let avg_words_per_cell = if total_cells == 0 {
                header_plain.split_whitespace().count() as f64
            } else {
                total_words as f64 / total_cells as f64
            };
            let avg_cell_width = if total_cells == 0 {
                display_width(&header_plain) as f64
            } else {
                total_cell_width as f64 / total_cells as f64
            };
            let kind = if long_body_token_count > 0
                && long_body_token_count >= body_token_count.saturating_sub(long_body_token_count)
            {
                TableColumnKind::TokenHeavy
            } else if avg_words_per_cell >= 4.0 || avg_cell_width >= 28.0 {
                TableColumnKind::Narrative
            } else {
                TableColumnKind::Compact
            };

            metrics.push(TableColumnMetrics {
                max_width,
                header_token_width,
                body_token_width,
                kind,
            });
        }

        metrics
    }

    /// Compute the preferred minimum width for a column before the shrink loop
    /// starts reducing it further.
    ///
    /// Narrative and token-heavy columns retain a readable 16-cell soft floor.
    /// Compact columns floor at the larger of the header and body token widths
    /// (body capped at 16). The result is clamped to `[min_column_width, max_width]`.
    pub(super) fn preferred_column_floor(
        metrics: &TableColumnMetrics,
        min_column_width: usize,
    ) -> usize {
        let token_target = match metrics.kind {
            TableColumnKind::Narrative | TableColumnKind::TokenHeavy => 16,
            TableColumnKind::Compact => metrics
                .header_token_width
                .max(metrics.body_token_width.min(16)),
        };
        token_target.max(min_column_width).min(metrics.max_width)
    }

    /// Shrink columns in priority order, balancing slack within each priority.
    ///
    /// Priority: TokenHeavy columns are shrunk before Narrative, then Compact.
    /// Within the same kind, columns with the most slack above their floor are
    /// shrunk first so similarly-shaped columns stay balanced. This produces the
    /// same result as repeatedly shrinking one display cell, without repeatedly
    /// scanning every column for long tokens.
    pub(super) fn shrink_columns(
        widths: &mut [usize],
        floors: &[usize],
        metrics: &[TableColumnMetrics],
        mut amount: usize,
    ) -> usize {
        for kind in [
            TableColumnKind::TokenHeavy,
            TableColumnKind::Narrative,
            TableColumnKind::Compact,
        ] {
            let slack_total = widths
                .iter()
                .enumerate()
                .filter(|(idx, _)| metrics[*idx].kind == kind)
                .map(|(idx, width)| width.saturating_sub(floors[idx]))
                .sum::<usize>();
            let to_remove = amount.min(slack_total);
            if to_remove == 0 {
                continue;
            }

            let mut low = 0usize;
            let mut high = widths
                .iter()
                .enumerate()
                .filter(|(idx, _)| metrics[*idx].kind == kind)
                .map(|(idx, width)| width.saturating_sub(floors[idx]))
                .max()
                .unwrap_or(/*default*/ 0);
            while low < high {
                let cap = low + (high - low) / 2;
                let removed = widths
                    .iter()
                    .enumerate()
                    .filter(|(idx, _)| metrics[*idx].kind == kind)
                    .map(|(idx, width)| width.saturating_sub(floors[idx]).saturating_sub(cap))
                    .sum::<usize>();
                if removed > to_remove {
                    low = cap + 1;
                } else {
                    high = cap;
                }
            }

            let cap = low;
            let mut removed = 0usize;
            for (idx, width) in widths.iter_mut().enumerate() {
                if metrics[idx].kind != kind {
                    continue;
                }
                let reduction = width.saturating_sub(floors[idx]).saturating_sub(cap);
                *width -= reduction;
                removed += reduction;
            }

            let mut remainder = to_remove - removed;
            for (idx, width) in widths.iter_mut().enumerate() {
                if remainder == 0 {
                    break;
                }
                if metrics[idx].kind == kind && width.saturating_sub(floors[idx]) == cap {
                    *width -= 1;
                    remainder -= 1;
                }
            }

            amount -= to_remove;
            if amount == 0 {
                break;
            }
        }

        amount
    }
}
