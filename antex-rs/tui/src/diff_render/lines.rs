use super::*;

/// Format a path for display relative to the current working directory when
/// possible, keeping output stable in jj/no-`.git` workspaces (e.g. image
/// tool calls should show `example.png` instead of an absolute path).
pub(crate) fn display_path_for(path: &Path, cwd: &Path) -> String {
    if path.is_relative() {
        return path.display().to_string();
    }

    if let Ok(stripped) = path.strip_prefix(cwd) {
        return stripped.display().to_string();
    }

    let path_in_same_repo = match (get_git_repo_root(cwd), get_git_repo_root(path)) {
        (Some(cwd_repo), Some(path_repo)) => cwd_repo == path_repo,
        _ => false,
    };
    let chosen = if path_in_same_repo {
        pathdiff::diff_paths(path, cwd).unwrap_or_else(|| path.to_path_buf())
    } else {
        relativize_to_home(path)
            .map(|p| PathBuf::from_iter([Path::new("~"), p.as_path()]))
            .unwrap_or_else(|| path.to_path_buf())
    };
    chosen.display().to_string()
}

pub(crate) fn calculate_add_remove_from_diff(diff: &str) -> (usize, usize) {
    if let Ok(patch) = diffy::Patch::from_str(diff) {
        patch
            .hunks()
            .iter()
            .flat_map(Hunk::lines)
            .fold((0, 0), |(a, d), l| match l {
                diffy::Line::Insert(_) => (a + 1, d),
                diffy::Line::Delete(_) => (a, d + 1),
                diffy::Line::Context(_) => (a, d),
            })
    } else {
        // For unparsable diffs, return 0 for both counts.
        (0, 0)
    }
}

/// Render a single plain-text (non-syntax-highlighted) diff line, wrapped to
/// `width` columns, using a pre-computed [`DiffRenderStyleContext`].
///
/// This is the convenience entry point used by the theme picker preview and
/// any caller that does not have syntax spans.  Delegates to the inner
/// rendering core with `syntax_spans = None`.
pub(crate) fn push_wrapped_diff_line_with_style_context(
    line_number: usize,
    kind: DiffLineType,
    text: &str,
    width: usize,
    line_number_width: usize,
    style_context: DiffRenderStyleContext,
) -> Vec<RtLine<'static>> {
    push_wrapped_diff_line_inner_with_theme_and_color_level(
        line_number,
        kind,
        text,
        width,
        line_number_width,
        /*syntax_spans*/ None,
        style_context.theme,
        style_context.color_level,
        style_context.diff_backgrounds,
    )
}

/// Render a syntax-highlighted diff line, wrapped to `width` columns, using
/// a pre-computed [`DiffRenderStyleContext`].
///
/// Like [`push_wrapped_diff_line_with_style_context`] but overlays
/// `syntax_spans` (from [`highlight_code_to_styled_spans`]) onto the diff
/// coloring.  Delete lines receive a `DIM` modifier so syntax colors do not
/// overpower the removal cue.
pub(crate) fn push_wrapped_diff_line_with_syntax_and_style_context(
    line_number: usize,
    kind: DiffLineType,
    text: &str,
    width: usize,
    line_number_width: usize,
    syntax_spans: &[RtSpan<'static>],
    style_context: DiffRenderStyleContext,
) -> Vec<RtLine<'static>> {
    push_wrapped_diff_line_inner_with_theme_and_color_level(
        line_number,
        kind,
        text,
        width,
        line_number_width,
        Some(syntax_spans),
        style_context.theme,
        style_context.color_level,
        style_context.diff_backgrounds,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_wrapped_diff_line_inner_with_theme_and_color_level(
    line_number: usize,
    kind: DiffLineType,
    text: &str,
    width: usize,
    line_number_width: usize,
    syntax_spans: Option<&[RtSpan<'static>]>,
    theme: DiffTheme,
    color_level: DiffColorLevel,
    diff_backgrounds: ResolvedDiffBackgrounds,
) -> Vec<RtLine<'static>> {
    let ln_str = line_number.to_string();

    // Reserve a fixed number of spaces (equal to the widest line number plus a
    // trailing spacer) so the sign column stays aligned across the diff block.
    let gutter_width = line_number_width.max(1);
    let prefix_cols = gutter_width + 1;

    let (sign_char, sign_style, content_style) = match kind {
        DiffLineType::Insert => (
            '+',
            style_sign_add(theme, color_level, diff_backgrounds),
            style_add(theme, color_level, diff_backgrounds),
        ),
        DiffLineType::Delete => (
            '-',
            style_sign_del(theme, color_level, diff_backgrounds),
            style_del(theme, color_level, diff_backgrounds),
        ),
        DiffLineType::Context => (' ', style_context(), style_context()),
    };

    let line_bg = style_line_bg_for(kind, diff_backgrounds);
    let gutter_style = style_gutter_for(kind, theme, color_level);

    // When we have syntax spans, compose them with the diff style for a richer
    // view. The sign character keeps the diff color; content gets syntax colors
    // with an overlay modifier for delete lines (dim).
    if let Some(syn_spans) = syntax_spans {
        let gutter = format!("{ln_str:>gutter_width$} ");
        let sign = format!("{sign_char}");
        let styled: Vec<RtSpan<'static>> = syn_spans
            .iter()
            .map(|sp| {
                let style = if matches!(kind, DiffLineType::Delete) {
                    sp.style.add_modifier(Modifier::DIM)
                } else {
                    sp.style
                };
                RtSpan::styled(sp.content.clone().into_owned(), style)
            })
            .collect();

        // Determine how many display columns remain for content after the
        // gutter and sign character.
        let available_content_cols = width.saturating_sub(prefix_cols + 1).max(1);

        // Wrap the styled content spans to fit within the available columns.
        let wrapped_chunks = wrap_styled_spans(&styled, available_content_cols);

        let mut lines: Vec<RtLine<'static>> = Vec::new();
        for (i, chunk) in wrapped_chunks.into_iter().enumerate() {
            let mut row_spans: Vec<RtSpan<'static>> = Vec::new();
            if i == 0 {
                // First line: gutter + sign + content
                row_spans.push(RtSpan::styled(gutter.clone(), gutter_style));
                row_spans.push(RtSpan::styled(sign.clone(), sign_style));
            } else {
                // Continuation: empty gutter + two-space indent (matches
                // the plain-text wrapping continuation style).
                let cont_gutter = format!("{:gutter_width$}  ", "");
                row_spans.push(RtSpan::styled(cont_gutter, gutter_style));
            }
            row_spans.extend(chunk);
            lines.push(RtLine::from(row_spans).style(line_bg));
        }
        return lines;
    }

    let available_content_cols = width.saturating_sub(prefix_cols + 1).max(1);
    let styled = vec![RtSpan::styled(text.to_string(), content_style)];
    let wrapped_chunks = wrap_styled_spans(&styled, available_content_cols);

    let mut lines: Vec<RtLine<'static>> = Vec::new();
    for (i, chunk) in wrapped_chunks.into_iter().enumerate() {
        let mut row_spans: Vec<RtSpan<'static>> = Vec::new();
        if i == 0 {
            let gutter = format!("{ln_str:>gutter_width$} ");
            let sign = format!("{sign_char}");
            row_spans.push(RtSpan::styled(gutter, gutter_style));
            row_spans.push(RtSpan::styled(sign, sign_style));
        } else {
            let cont_gutter = format!("{:gutter_width$}  ", "");
            row_spans.push(RtSpan::styled(cont_gutter, gutter_style));
        }
        row_spans.extend(chunk);
        lines.push(RtLine::from(row_spans).style(line_bg));
    }

    lines
}

/// Split styled spans into chunks that fit within `max_cols` display columns.
///
/// Returns one `Vec<RtSpan>` per output line.  Styles are preserved across
/// split boundaries so that wrapping never loses syntax coloring.
///
/// The algorithm walks characters using their Unicode display width (with tabs
/// expanded to [`TAB_WIDTH`] columns).  When a character would overflow the
/// current line, the accumulated text is flushed and a new line begins.  A
/// single character wider than the remaining space forces a line break *before*
/// the character so that progress is always made (avoiding infinite loops on
/// CJK characters or tabs at the end of a line).
pub(super) fn wrap_styled_spans(
    spans: &[RtSpan<'static>],
    max_cols: usize,
) -> Vec<Vec<RtSpan<'static>>> {
    let mut result: Vec<Vec<RtSpan<'static>>> = Vec::new();
    let mut current_line: Vec<RtSpan<'static>> = Vec::new();
    let mut col: usize = 0;

    for span in spans {
        let style = span.style;
        let text = span.content.as_ref();
        let mut remaining = text;

        while !remaining.is_empty() {
            // Accumulate characters until we fill the line.
            let mut byte_end = 0;
            let mut chars_col = 0;

            for ch in remaining.chars() {
                // Tabs have no Unicode width; treat them as TAB_WIDTH columns.
                let w = ch.width().unwrap_or(if ch == '\t' { TAB_WIDTH } else { 0 });
                if col + chars_col + w > max_cols {
                    // Adding this character would exceed the line width.
                    // Break here; if this is the first character in `remaining`
                    // we will flush/start a new line in the `byte_end == 0`
                    // branch below before consuming it.
                    break;
                }
                byte_end += ch.len_utf8();
                chars_col += w;
            }

            if byte_end == 0 {
                // Single character wider than remaining space — force onto a
                // new line so we make progress.
                if !current_line.is_empty() {
                    result.push(std::mem::take(&mut current_line));
                }
                // Take at least one character to avoid an infinite loop.
                let Some(ch) = remaining.chars().next() else {
                    break;
                };
                let ch_len = ch.len_utf8();
                current_line.push(RtSpan::styled(
                    remaining[..ch_len].replace('\t', TAB_REPLACEMENT),
                    style,
                ));
                // Use fallback width 1 (not 0) so this branch always advances
                // even if `ch` has unknown/zero display width.
                col = ch.width().unwrap_or(if ch == '\t' { TAB_WIDTH } else { 1 });
                remaining = &remaining[ch_len..];
                continue;
            }

            let (chunk, rest) = remaining.split_at(byte_end);
            current_line.push(RtSpan::styled(chunk.replace('\t', TAB_REPLACEMENT), style));
            col += chars_col;
            remaining = rest;

            // If we exactly filled or exceeded the line, start a new one.
            // Do not gate on !remaining.is_empty() — the next span in the
            // outer loop may still have content that must start on a fresh line.
            if col >= max_cols {
                result.push(std::mem::take(&mut current_line));
                col = 0;
            }
        }
    }

    // Push the last line (always at least one, even if empty).
    if !current_line.is_empty() || result.is_empty() {
        result.push(current_line);
    }

    result
}

pub(crate) fn line_number_width(max_line_number: usize) -> usize {
    if max_line_number == 0 {
        1
    } else {
        max_line_number.to_string().len()
    }
}

/// Testable helper: picks `DiffTheme` from an explicit background sample.
pub(super) fn diff_theme_for_bg(bg: Option<(u8, u8, u8)>) -> DiffTheme {
    if let Some(rgb) = bg
        && is_light(rgb)
    {
        return DiffTheme::Light;
    }
    DiffTheme::Dark
}

/// Probe the terminal's background and return the appropriate diff palette.
pub(super) fn diff_theme() -> DiffTheme {
    diff_theme_for_bg(default_bg())
}

/// Return the [`DiffColorLevel`] for the current terminal session.
///
/// This is the environment-reading adapter: it samples runtime signals
/// (`supports-color` level, terminal name, `WT_SESSION`, and `FORCE_COLOR`)
/// and forwards them to [`diff_color_level_for_terminal`].
///
/// Keeping env reads in this thin wrapper lets
/// [`diff_color_level_for_terminal`] stay pure and easy to unit test.
pub(super) fn diff_color_level() -> DiffColorLevel {
    diff_color_level_for_terminal(
        stdout_color_level(),
        terminal_info().name,
        std::env::var_os("WT_SESSION").is_some(),
        has_force_color_override(),
    )
}

/// Returns whether `FORCE_COLOR` is explicitly set.
pub(super) fn has_force_color_override() -> bool {
    std::env::var_os("FORCE_COLOR").is_some()
}

/// Map a raw [`StdoutColorLevel`] to a [`DiffColorLevel`] using
/// Windows Terminal-specific truecolor promotion rules.
///
/// This helper is intentionally pure (no env access) so tests can validate
/// the policy table by passing explicit inputs.
///
/// Windows Terminal fully supports 24-bit color but the `supports-color`
/// crate often reports only ANSI-16 there because no `COLORTERM` variable
/// is set.  We detect Windows Terminal two ways — via `terminal_name`
/// (parsed from `WT_SESSION` / `TERM_PROGRAM` by `terminal_info()`) and
/// via the raw `has_wt_session` flag.
///
/// These signals are intentionally not equivalent: `terminal_name` is a
/// derived classification with `TERM_PROGRAM` precedence, so `WT_SESSION`
/// can be present while `terminal_name` is not `WindowsTerminal`.
///
/// When `WT_SESSION` is present, we promote to truecolor unconditionally
/// unless `FORCE_COLOR` is set. This keeps Windows Terminal rendering rich
/// by default while preserving explicit `FORCE_COLOR` user intent.
///
/// Outside `WT_SESSION`, only ANSI-16 is promoted for identified
/// `WindowsTerminal` sessions; `Unknown` stays conservative.
pub(super) fn diff_color_level_for_terminal(
    stdout_level: StdoutColorLevel,
    terminal_name: TerminalName,
    has_wt_session: bool,
    has_force_color_override: bool,
) -> DiffColorLevel {
    if has_wt_session && !has_force_color_override {
        return DiffColorLevel::TrueColor;
    }

    let base = match stdout_level {
        StdoutColorLevel::TrueColor => DiffColorLevel::TrueColor,
        StdoutColorLevel::Ansi256 => DiffColorLevel::Ansi256,
        StdoutColorLevel::Ansi16 | StdoutColorLevel::Unknown => DiffColorLevel::Ansi16,
    };

    // Outside `WT_SESSION`, keep the existing Windows Terminal promotion for
    // ANSI-16 sessions that likely support truecolor.
    if stdout_level == StdoutColorLevel::Ansi16
        && terminal_name == TerminalName::WindowsTerminal
        && !has_force_color_override
    {
        DiffColorLevel::TrueColor
    } else {
        base
    }
}

// -- Style helpers ------------------------------------------------------------
//
// Each diff line is composed of three visual regions, styled independently:
//
//   ┌──────────┬──────┬──────────────────────────────────────────┐
//   │  gutter  │ sign │              content                     │
//   │ (line #) │ +/-  │  (plain or syntax-highlighted text)      │
//   └──────────┴──────┴──────────────────────────────────────────┘
//
// A fourth, full-width layer — `line_bg` — is applied via `RtLine::style()`
// so that the background tint extends from the leftmost column to the right
// edge of the terminal, including any padding beyond the content.
//
// On dark terminals, the sign and content share one style (colored fg + tinted
// bg), and the gutter is simply dimmed.  On light terminals, sign and content
// are split: the sign gets only a colored foreground (no bg, so the line bg
// shows through), while content relies on the line bg alone; the gutter gets
// an opaque, more-saturated background so line numbers stay readable against
// the pastel line tint.

/// Full-width background applied to the `RtLine` itself (not individual spans).
/// Context lines intentionally leave the background unset so the terminal
/// default shows through.
pub(super) fn style_line_bg_for(
    kind: DiffLineType,
    diff_backgrounds: ResolvedDiffBackgrounds,
) -> Style {
    match kind {
        DiffLineType::Insert => diff_backgrounds
            .add
            .map_or_else(Style::default, |bg| Style::default().bg(bg)),
        DiffLineType::Delete => diff_backgrounds
            .del
            .map_or_else(Style::default, |bg| Style::default().bg(bg)),
        DiffLineType::Context => Style::default(),
    }
}

pub(super) fn style_context() -> Style {
    Style::default()
}

pub(super) fn add_line_bg(theme: DiffTheme, color_level: RichDiffColorLevel) -> Color {
    match (theme, color_level) {
        (DiffTheme::Dark, RichDiffColorLevel::TrueColor) => rgb_color(DARK_TC_ADD_LINE_BG_RGB),
        (DiffTheme::Dark, RichDiffColorLevel::Ansi256) => indexed_color(DARK_256_ADD_LINE_BG_IDX),
        (DiffTheme::Light, RichDiffColorLevel::TrueColor) => rgb_color(LIGHT_TC_ADD_LINE_BG_RGB),
        (DiffTheme::Light, RichDiffColorLevel::Ansi256) => indexed_color(LIGHT_256_ADD_LINE_BG_IDX),
    }
}

pub(super) fn del_line_bg(theme: DiffTheme, color_level: RichDiffColorLevel) -> Color {
    match (theme, color_level) {
        (DiffTheme::Dark, RichDiffColorLevel::TrueColor) => rgb_color(DARK_TC_DEL_LINE_BG_RGB),
        (DiffTheme::Dark, RichDiffColorLevel::Ansi256) => indexed_color(DARK_256_DEL_LINE_BG_IDX),
        (DiffTheme::Light, RichDiffColorLevel::TrueColor) => rgb_color(LIGHT_TC_DEL_LINE_BG_RGB),
        (DiffTheme::Light, RichDiffColorLevel::Ansi256) => indexed_color(LIGHT_256_DEL_LINE_BG_IDX),
    }
}

pub(super) fn light_gutter_fg(color_level: DiffColorLevel) -> Color {
    match color_level {
        DiffColorLevel::TrueColor => rgb_color(LIGHT_TC_GUTTER_FG_RGB),
        DiffColorLevel::Ansi256 => indexed_color(LIGHT_256_GUTTER_FG_IDX),
        DiffColorLevel::Ansi16 => Color::Black,
    }
}

pub(super) fn light_add_num_bg(color_level: RichDiffColorLevel) -> Color {
    match color_level {
        RichDiffColorLevel::TrueColor => rgb_color(LIGHT_TC_ADD_NUM_BG_RGB),
        RichDiffColorLevel::Ansi256 => indexed_color(LIGHT_256_ADD_NUM_BG_IDX),
    }
}

pub(super) fn light_del_num_bg(color_level: RichDiffColorLevel) -> Color {
    match color_level {
        RichDiffColorLevel::TrueColor => rgb_color(LIGHT_TC_DEL_NUM_BG_RGB),
        RichDiffColorLevel::Ansi256 => indexed_color(LIGHT_256_DEL_NUM_BG_IDX),
    }
}

/// Line-number gutter style.  On light backgrounds the gutter has an opaque
/// tinted background so numbers contrast against the pastel line fill.  On
/// dark backgrounds a simple `DIM` modifier is sufficient.
pub(super) fn style_gutter_for(
    kind: DiffLineType,
    theme: DiffTheme,
    color_level: DiffColorLevel,
) -> Style {
    match (
        theme,
        kind,
        RichDiffColorLevel::from_diff_color_level(color_level),
    ) {
        (DiffTheme::Light, DiffLineType::Insert, None) => {
            Style::default().fg(light_gutter_fg(color_level))
        }
        (DiffTheme::Light, DiffLineType::Delete, None) => {
            Style::default().fg(light_gutter_fg(color_level))
        }
        (DiffTheme::Light, DiffLineType::Insert, Some(level)) => Style::default()
            .fg(light_gutter_fg(color_level))
            .bg(light_add_num_bg(level)),
        (DiffTheme::Light, DiffLineType::Delete, Some(level)) => Style::default()
            .fg(light_gutter_fg(color_level))
            .bg(light_del_num_bg(level)),
        _ => style_gutter_dim(),
    }
}

/// Sign character (`+`) for insert lines.  On dark terminals it inherits the
/// full content style (green fg + tinted bg).  On light terminals it uses only
/// a green foreground and lets the line-level bg show through.
pub(super) fn style_sign_add(
    theme: DiffTheme,
    color_level: DiffColorLevel,
    diff_backgrounds: ResolvedDiffBackgrounds,
) -> Style {
    match theme {
        DiffTheme::Light => Style::default().fg(Color::Green),
        DiffTheme::Dark => style_add(theme, color_level, diff_backgrounds),
    }
}

/// Sign character (`-`) for delete lines.  Mirror of [`style_sign_add`].
pub(super) fn style_sign_del(
    theme: DiffTheme,
    color_level: DiffColorLevel,
    diff_backgrounds: ResolvedDiffBackgrounds,
) -> Style {
    match theme {
        DiffTheme::Light => Style::default().fg(Color::Red),
        DiffTheme::Dark => style_del(theme, color_level, diff_backgrounds),
    }
}

/// Content style for insert lines (plain, non-syntax-highlighted text).
///
/// Always uses an ANSI green foreground. On rich levels it also carries the pre-resolved background
/// for internal previews; the terminal output boundary removes that computed background.
///
/// When no background is resolved (e.g. a theme that defines no diff
/// scopes and the fallback palette is somehow empty), the style degrades
/// to foreground-only so the line is still legible.
pub(super) fn style_add(
    _theme: DiffTheme,
    color_level: DiffColorLevel,
    diff_backgrounds: ResolvedDiffBackgrounds,
) -> Style {
    let style = Style::default().fg(Color::Green);
    match (color_level, diff_backgrounds.add) {
        (DiffColorLevel::TrueColor, Some(bg)) | (DiffColorLevel::Ansi256, Some(bg)) => style.bg(bg),
        (DiffColorLevel::TrueColor, None)
        | (DiffColorLevel::Ansi256, None)
        | (DiffColorLevel::Ansi16, _) => style,
    }
}

/// Content style for delete lines (plain, non-syntax-highlighted text).
///
/// Mirror of [`style_add`] with red foreground and the delete-side
/// resolved background.
pub(super) fn style_del(
    _theme: DiffTheme,
    color_level: DiffColorLevel,
    diff_backgrounds: ResolvedDiffBackgrounds,
) -> Style {
    let style = Style::default().fg(Color::Red);
    match (color_level, diff_backgrounds.del) {
        (DiffColorLevel::TrueColor, Some(bg)) | (DiffColorLevel::Ansi256, Some(bg)) => style.bg(bg),
        (DiffColorLevel::TrueColor, None)
        | (DiffColorLevel::Ansi256, None)
        | (DiffColorLevel::Ansi16, _) => style,
    }
}

pub(super) fn style_gutter_dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}
