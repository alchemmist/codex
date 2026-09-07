use super::*;

impl TextArea {
    pub(crate) fn render_ref_masked(
        &self,
        area: Rect,
        buf: &mut Buffer,
        state: &mut TextAreaState,
        mask_char: char,
    ) {
        let lines = self.wrapped_lines(area.width);
        let scroll = self.effective_scroll(area, &lines, state.scroll);
        state.scroll = scroll;

        let start = scroll as usize;
        let end = (scroll + area.height).min(lines.len() as u16) as usize;
        self.render_lines_masked(area, buf, &lines, start..end, mask_char);
    }

    /// Render the textarea with `base_style` plus additional render-only highlight ranges.
    ///
    /// Highlight ranges are byte ranges in `self.text`. They affect only the buffer rendering and
    /// do not mutate the editable text, cursor, element metadata, or wrapping cache.
    pub(crate) fn render_ref_styled_with_highlights(
        &self,
        area: Rect,
        buf: &mut Buffer,
        state: &mut TextAreaState,
        base_style: Style,
        highlights: &[(Range<usize>, Style)],
    ) {
        let lines = self.wrapped_lines(area.width);
        let scroll = self.effective_scroll(area, &lines, state.scroll);
        state.scroll = scroll;

        let start = scroll as usize;
        let end = (scroll + area.height).min(lines.len() as u16) as usize;
        self.render_lines(area, buf, &lines, start..end, base_style, highlights);
    }

    /// Renders visible text and styled overlays without writing outside the textarea viewport.
    pub(super) fn render_lines(
        &self,
        area: Rect,
        buf: &mut Buffer,
        lines: &[Range<usize>],
        range: std::ops::Range<usize>,
        base_style: Style,
        highlights: &[(Range<usize>, Style)],
    ) {
        let element_style = base_style.fg(Color::Cyan);
        for (row, idx) in range.clone().enumerate() {
            let r = &lines[idx];
            let y = area.y + row as u16;
            let visible = wrapping::visible_prefix(&self.text[r.start..r.end - 1], area.width);
            let line_range = r.start..r.start + visible.len();
            buf.set_style(Rect::new(area.x, y, area.width, 1), base_style);
            // Draw base line with the provided style.
            buf.set_stringn(
                area.x,
                y,
                text_for_display(visible),
                usize::from(area.width),
                base_style,
            );

            // Apply search highlights last so they remain visible over styled elements.
            let visual_ranges = self.vim_visual_ranges();
            let visual_style = Style::default().bg(Color::DarkGray);
            let overlays = self
                .elements
                .iter()
                .map(|element| (&element.range, element_style))
                .chain(highlights.iter().map(|(range, style)| (range, *style)))
                .chain(visual_ranges.iter().map(|range| (range, visual_style)));
            for (overlay_range, style) in overlays {
                let overlap_start = overlay_range.start.max(line_range.start);
                let overlap_end = overlay_range.end.min(line_range.end);
                if overlap_start >= overlap_end {
                    continue;
                }
                let styled = &self.text[overlap_start..overlap_end];
                let x_off = display_width(&self.text[line_range.start..overlap_start]);
                if x_off >= usize::from(area.width) {
                    continue;
                }
                let x_off = x_off as u16;
                buf.set_stringn(
                    area.x + x_off,
                    y,
                    text_for_display(styled),
                    usize::from(area.width.saturating_sub(x_off)),
                    style,
                );
            }
        }
        if let Some(wrap_cache) = self.wrap_cache.borrow().as_ref() {
            wrap_cache
                .hyperlinks
                .get_or_init(|| hyperlinks::HyperlinkCache::new(&self.text, lines))
                .mark(buf, area, &self.text, lines, range);
        }
    }

    /// Renders width-preserving mask glyphs without writing outside the textarea viewport.
    pub(super) fn render_lines_masked(
        &self,
        area: Rect,
        buf: &mut Buffer,
        lines: &[Range<usize>],
        range: std::ops::Range<usize>,
        mask_char: char,
    ) {
        for (row, idx) in range.enumerate() {
            let r = &lines[idx];
            let y = area.y + row as u16;
            let visible = wrapping::visible_prefix(&self.text[r.start..r.end - 1], area.width);
            let masked = visible
                .graphemes(/*is_extended*/ true)
                .flat_map(|grapheme| std::iter::repeat_n(mask_char, display_width(grapheme)))
                .collect::<String>();
            buf.set_stringn(
                area.x,
                y,
                &masked,
                usize::from(area.width),
                Style::default(),
            );
        }
    }
}
