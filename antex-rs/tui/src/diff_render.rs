//! Renders unified diffs with line numbers, gutter signs, and optional syntax
//! highlighting.
//!
//! Each `FileChange` variant (Add / Delete / Update) is rendered as a block of
//! diff lines, each prefixed by a right-aligned line number, a gutter sign
//! (`+` / `-` / ` `), and the content text.  When a recognized file extension
//! is present, the content text is syntax-highlighted using
//! [`crate::render::highlight`].
//!
//! **Terminal-palette styling:** add/delete content always carries ANSI green/red foregrounds.
//! Rich background colors may still be produced for internal previews and tests, but the terminal
//! output boundary removes computed backgrounds before cells reach the active screen or scrollback.
//! This keeps committed diffs readable across live terminal palette changes.
//!
//! **Syntax-theme scope backgrounds:** when the active syntax theme defines
//! background colors for `markup.inserted` / `markup.deleted` (or fallback
//! `diff.inserted` / `diff.deleted`) scopes, those colors override the
//! hardcoded palette for rich color levels.  ANSI-16 mode always uses
//! foreground-only styling regardless of theme scope backgrounds.
//!
//! **Highlighting strategy for `Update` diffs:** the renderer highlights each
//! hunk as a single concatenated block rather than line-by-line.  This
//! preserves syntect's parser state across consecutive lines within a hunk
//! (important for multi-line strings, block comments, etc.).  Cross-hunk state
//! is intentionally *not* preserved because hunks are visually separated and
//! re-synchronize at context boundaries anyway.
//!
//! **Wrapping:** long lines are hard-wrapped at the available column width.
//! Syntax-highlighted spans are split at character boundaries with styles
//! preserved across the split so that no color information is lost.

use diffy::Hunk;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line as RtLine;
use ratatui::text::Span as RtSpan;
use ratatui::widgets::Paragraph;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use unicode_width::UnicodeWidthChar;
#[path = "diff_render/lines.rs"]
mod lines;
pub(crate) use lines::calculate_add_remove_from_diff;
pub(crate) use lines::display_path_for;
pub(crate) use lines::line_number_width;
use lines::*;

/// Replacement for a tab character in rendered diff content.
const TAB_REPLACEMENT: &str = "    ";
/// Display width of a tab character in columns.
const TAB_WIDTH: usize = TAB_REPLACEMENT.len();

// -- Diff background palette --------------------------------------------------
//
// Dark-theme tints are subtle enough to avoid clashing with syntax colors.
// Light-theme values match GitHub's diff colors for familiarity.  The gutter
// (line-number column) uses slightly more saturated variants on light
// backgrounds so the numbers remain readable against the pastel line background.
// Truecolor palette.
const DARK_TC_ADD_LINE_BG_RGB: (u8, u8, u8) = (33, 58, 43); // #213A2B
const DARK_TC_DEL_LINE_BG_RGB: (u8, u8, u8) = (74, 34, 29); // #4A221D
const LIGHT_TC_ADD_LINE_BG_RGB: (u8, u8, u8) = (218, 251, 225); // #dafbe1
const LIGHT_TC_DEL_LINE_BG_RGB: (u8, u8, u8) = (255, 235, 233); // #ffebe9
const LIGHT_TC_ADD_NUM_BG_RGB: (u8, u8, u8) = (172, 238, 187); // #aceebb
const LIGHT_TC_DEL_NUM_BG_RGB: (u8, u8, u8) = (255, 206, 203); // #ffcecb
const LIGHT_TC_GUTTER_FG_RGB: (u8, u8, u8) = (31, 35, 40); // #1f2328

// 256-color palette.
const DARK_256_ADD_LINE_BG_IDX: u8 = 22;
const DARK_256_DEL_LINE_BG_IDX: u8 = 52;
const LIGHT_256_ADD_LINE_BG_IDX: u8 = 194;
const LIGHT_256_DEL_LINE_BG_IDX: u8 = 224;
const LIGHT_256_ADD_NUM_BG_IDX: u8 = 157;
const LIGHT_256_DEL_NUM_BG_IDX: u8 = 217;
const LIGHT_256_GUTTER_FG_IDX: u8 = 236;

use crate::color::is_light;
use crate::color::perceptual_distance;
use crate::diff_model::FileChange;
use crate::display_paths::get_git_repo_root;
use crate::display_paths::relativize_to_home;
use crate::render::Insets;
use crate::render::highlight::DiffScopeBackgroundRgbs;
use crate::render::highlight::diff_scope_background_rgbs;
use crate::render::highlight::exceeds_highlight_limits;
use crate::render::highlight::highlight_code_to_styled_spans;
use crate::render::line_utils::prefix_lines;
use crate::render::renderable::ColumnRenderable;
use crate::render::renderable::InsetRenderable;
use crate::render::renderable::Renderable;
use crate::terminal_detection::TerminalName;
use crate::terminal_detection::terminal_info;
use crate::terminal_palette::StdoutColorLevel;
use crate::terminal_palette::XTERM_COLORS;
use crate::terminal_palette::default_bg;
use crate::terminal_palette::indexed_color;
use crate::terminal_palette::rgb_color;
use crate::terminal_palette::stdout_color_level;

/// Classifies a diff line for gutter sign rendering and style selection.
///
/// `Insert` renders with a `+` sign and green text, `Delete` with `-` and red
/// text (plus dim overlay when syntax-highlighted), and `Context` with a space
/// and default styling.
#[derive(Clone, Copy)]
pub(crate) enum DiffLineType {
    Insert,
    Delete,
    Context,
}

/// Controls which color palette the diff renderer uses for backgrounds and
/// gutter styling.
///
/// Determined once per `render_change` call via [`diff_theme`], which probes
/// the terminal's queried background color.  When the background cannot be
/// determined (common in CI or piped output), `Dark` is used as the safe
/// default.
#[derive(Clone, Copy, Debug)]
enum DiffTheme {
    Dark,
    Light,
}

/// Palette depth the diff renderer will target.
///
/// This is the *renderer's own* notion of color depth, derived from — but not
/// identical to — the raw [`StdoutColorLevel`] reported by `supports-color`.
/// The indirection exists because some terminals (notably Windows Terminal)
/// advertise only ANSI-16 support while actually rendering truecolor sequences
/// correctly; [`diff_color_level_for_terminal`] promotes those cases so the
/// diff output uses the richer palette.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DiffColorLevel {
    TrueColor,
    Ansi256,
    Ansi16,
}

/// Subset of [`DiffColorLevel`] that supports tinted backgrounds.
///
/// ANSI-16 terminals render backgrounds with bold, saturated palette entries
/// that overpower syntax tokens.  This type encodes the invariant "we have
/// enough color depth for pastel tints" so that background-producing helpers
/// (`add_line_bg`, `del_line_bg`, `light_add_num_bg`, `light_del_num_bg`)
/// never need an unreachable ANSI-16 arm.
///
/// Construct via [`RichDiffColorLevel::from_diff_color_level`], which returns
/// `None` for ANSI-16 — callers branch on the `Option` and skip backgrounds
/// entirely when `None`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RichDiffColorLevel {
    TrueColor,
    Ansi256,
}

impl RichDiffColorLevel {
    /// Extract a rich level, returning `None` for ANSI-16.
    fn from_diff_color_level(level: DiffColorLevel) -> Option<Self> {
        match level {
            DiffColorLevel::TrueColor => Some(Self::TrueColor),
            DiffColorLevel::Ansi256 => Some(Self::Ansi256),
            DiffColorLevel::Ansi16 => None,
        }
    }
}

/// Pre-resolved background colors for insert and delete diff lines.
///
/// Computed once per `render_change` call from the active syntax theme's
/// scope backgrounds (via [`resolve_diff_backgrounds`]) and then threaded
/// through every style helper so individual lines never re-query the theme.
///
/// Both fields are `None` when the color level is ANSI-16 — callers fall
/// back to foreground-only styling in that case.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ResolvedDiffBackgrounds {
    add: Option<Color>,
    del: Option<Color>,
}

/// Precomputed render state for diff line styling.
///
/// This bundles the terminal-derived theme and color depth plus theme-resolved
/// diff backgrounds so callers rendering many lines can compute once per render
/// pass and reuse it across all line calls.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DiffRenderStyleContext {
    theme: DiffTheme,
    color_level: DiffColorLevel,
    diff_backgrounds: ResolvedDiffBackgrounds,
}

/// Resolve diff backgrounds for production rendering.
///
/// Queries the active syntax theme for `markup.inserted` / `markup.deleted`
/// (and `diff.*` fallbacks), then delegates to [`resolve_diff_backgrounds_for`].
fn resolve_diff_backgrounds(
    theme: DiffTheme,
    color_level: DiffColorLevel,
) -> ResolvedDiffBackgrounds {
    resolve_diff_backgrounds_for(theme, color_level, diff_scope_background_rgbs())
}

/// Snapshot the current terminal environment into a reusable style context.
///
/// Queries `diff_theme`, `diff_color_level`, and the active syntax theme's
/// scope backgrounds once, bundling them into a [`DiffRenderStyleContext`]
/// that callers thread through every line-rendering call in a single pass.
///
/// Call this at the top of each render frame — not per line — so the diff
/// palette stays consistent within a frame even if the user swaps themes
/// mid-render (theme picker live preview).
pub(crate) fn current_diff_render_style_context() -> DiffRenderStyleContext {
    let theme = diff_theme();
    let color_level = diff_color_level();
    let diff_backgrounds = resolve_diff_backgrounds(theme, color_level);
    DiffRenderStyleContext {
        theme,
        color_level,
        diff_backgrounds,
    }
}

/// Core background-resolution logic, kept pure for testability.
///
/// Starts from the hardcoded fallback palette and then overrides with theme
/// scope backgrounds when both (a) the color level is rich enough and (b) the
/// theme defines a matching scope.  This means the fallback palette is always
/// the baseline and theme scopes are strictly additive.
fn resolve_diff_backgrounds_for(
    theme: DiffTheme,
    color_level: DiffColorLevel,
    scope_backgrounds: DiffScopeBackgroundRgbs,
) -> ResolvedDiffBackgrounds {
    let mut resolved = fallback_diff_backgrounds(theme, color_level);
    let Some(level) = RichDiffColorLevel::from_diff_color_level(color_level) else {
        return resolved;
    };

    if let Some(rgb) = scope_backgrounds.inserted {
        resolved.add = Some(color_from_rgb_for_level(rgb, level));
    }
    if let Some(rgb) = scope_backgrounds.deleted {
        resolved.del = Some(color_from_rgb_for_level(rgb, level));
    }
    resolved
}

/// Hardcoded palette backgrounds, used when the syntax theme provides no
/// diff-specific scope backgrounds.  Returns empty backgrounds for ANSI-16.
fn fallback_diff_backgrounds(
    theme: DiffTheme,
    color_level: DiffColorLevel,
) -> ResolvedDiffBackgrounds {
    match RichDiffColorLevel::from_diff_color_level(color_level) {
        Some(level) => ResolvedDiffBackgrounds {
            add: Some(add_line_bg(theme, level)),
            del: Some(del_line_bg(theme, level)),
        },
        None => ResolvedDiffBackgrounds::default(),
    }
}

/// Convert an RGB triple to the appropriate ratatui `Color` for the given
/// rich color level — passthrough for truecolor, quantized for ANSI-256.
fn color_from_rgb_for_level(rgb: (u8, u8, u8), color_level: RichDiffColorLevel) -> Color {
    match color_level {
        RichDiffColorLevel::TrueColor => rgb_color(rgb),
        RichDiffColorLevel::Ansi256 => quantize_rgb_to_ansi256(rgb),
    }
}

/// Find the closest ANSI-256 color (indices 16–255) to `target` using
/// perceptual distance.
///
/// Skips the first 16 entries (system colors) because their actual RGB
/// values depend on the user's terminal configuration and are unreliable
/// for distance calculations.
fn quantize_rgb_to_ansi256(target: (u8, u8, u8)) -> Color {
    let best_index = XTERM_COLORS
        .iter()
        .enumerate()
        .skip(16)
        .min_by(|(_, a), (_, b)| {
            perceptual_distance(**a, target).total_cmp(&perceptual_distance(**b, target))
        })
        .map(|(index, _)| index as u8);
    match best_index {
        Some(index) => indexed_color(index),
        None => indexed_color(DARK_256_ADD_LINE_BG_IDX),
    }
}

pub struct DiffSummary {
    changes: HashMap<PathBuf, FileChange>,
    cwd: PathBuf,
}

impl DiffSummary {
    pub(crate) fn new(changes: HashMap<PathBuf, FileChange>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            changes,
            cwd: cwd.into(),
        }
    }
}

impl Renderable for FileChange {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let mut lines = vec![];
        render_change(self, &mut lines, area.width as usize, /*lang*/ None);
        Paragraph::new(lines).render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        let mut lines = vec![];
        render_change(self, &mut lines, width as usize, /*lang*/ None);
        lines.len() as u16
    }
}

impl From<DiffSummary> for Box<dyn Renderable> {
    fn from(val: DiffSummary) -> Self {
        let mut rows: Vec<Box<dyn Renderable>> = vec![];
        let mut changes: Vec<_> = val.changes.into_iter().collect();
        changes.sort_by(|left, right| left.0.cmp(&right.0));

        for (i, (path, change)) in changes.into_iter().enumerate() {
            if i > 0 {
                rows.push(Box::new(RtLine::from("")));
            }
            let (added, removed) = line_counts(&change);
            let mut path = RtLine::from(display_path_for(&path, val.cwd.as_path()));
            path.push_span(" ");
            path.extend(render_line_count_summary(added, removed));
            rows.push(Box::new(path));
            rows.push(Box::new(RtLine::from("")));
            rows.push(Box::new(InsetRenderable::new(
                Box::new(change) as Box<dyn Renderable>,
                Insets::tlbr(
                    /*top*/ 0, /*left*/ 2, /*bottom*/ 0, /*right*/ 0,
                ),
            )));
        }

        Box::new(ColumnRenderable::with(rows))
    }
}

pub(crate) fn create_diff_summary(
    changes: &HashMap<PathBuf, FileChange>,
    cwd: &Path,
    wrap_cols: usize,
) -> Vec<RtLine<'static>> {
    create_diff_summary_for(changes, cwd, wrap_cols, DiffStage::Applied)
}

#[derive(Clone, Copy)]
pub(crate) enum DiffStage {
    Applied,
    Proposed,
}

pub(crate) fn create_diff_summary_for(
    changes: &HashMap<PathBuf, FileChange>,
    cwd: &Path,
    wrap_cols: usize,
    stage: DiffStage,
) -> Vec<RtLine<'static>> {
    let rows = collect_rows(changes);
    render_changes_block(rows, wrap_cols, cwd, stage)
}

// Shared row for per-file presentation
struct Row<'a> {
    path: &'a Path,
    move_path: Option<&'a Path>,
    added: usize,
    removed: usize,
    change: &'a FileChange,
}

fn collect_rows(changes: &HashMap<PathBuf, FileChange>) -> Vec<Row<'_>> {
    let mut rows = Vec::with_capacity(changes.len());
    for (path, change) in changes.iter() {
        let (added, removed) = line_counts(change);
        let move_path = match change {
            FileChange::Update {
                move_path: Some(new),
                ..
            } => Some(new.as_path()),
            _ => None,
        };
        rows.push(Row {
            path: path.as_path(),
            move_path,
            added,
            removed,
            change,
        });
    }
    rows.sort_by(|left, right| left.path.cmp(right.path));
    rows
}

fn line_counts(change: &FileChange) -> (usize, usize) {
    match change {
        FileChange::Add { content } => (content.lines().count(), 0),
        FileChange::Delete { content } => (0, content.lines().count()),
        FileChange::Update { unified_diff, .. } => calculate_add_remove_from_diff(unified_diff),
    }
}

fn render_line_count_summary(added: usize, removed: usize) -> Vec<RtSpan<'static>> {
    let mut spans = Vec::new();
    spans.push("(".into());
    spans.push(format!("+{added}").green());
    spans.push(" ".into());
    spans.push(format!("-{removed}").red());
    spans.push(")".into());
    spans
}

fn render_changes_block(
    rows: Vec<Row<'_>>,
    wrap_cols: usize,
    cwd: &Path,
    stage: DiffStage,
) -> Vec<RtLine<'static>> {
    let mut out: Vec<RtLine<'static>> = Vec::new();

    let render_path = |row: &Row<'_>| -> Vec<RtSpan<'static>> {
        let mut spans = Vec::new();
        spans.push(display_path_for(row.path, cwd).into());
        if let Some(move_path) = row.move_path {
            spans.push(format!(" → {}", display_path_for(move_path, cwd)).into());
        }
        spans
    };

    // Header
    let total_added: usize = rows.iter().map(|r| r.added).sum();
    let total_removed: usize = rows.iter().map(|r| r.removed).sum();
    let file_count = rows.len();
    let noun = if file_count == 1 { "file" } else { "files" };
    let mut header_spans: Vec<RtSpan<'static>> = vec!["• ".dim()];
    if let [row] = &rows[..] {
        let verb = match (stage, row.change) {
            (DiffStage::Applied, FileChange::Add { .. }) => "Added",
            (DiffStage::Applied, FileChange::Delete { .. }) => "Deleted",
            (DiffStage::Applied, FileChange::Update { .. }) => "Edited",
            (DiffStage::Proposed, FileChange::Add { .. }) => "Write",
            (DiffStage::Proposed, FileChange::Delete { .. }) => "Delete",
            (DiffStage::Proposed, FileChange::Update { .. }) => "Edit",
        };
        header_spans.push(verb.bold());
        header_spans.push(" ".into());
        header_spans.extend(render_path(row));
        header_spans.push(" ".into());
        header_spans.extend(render_line_count_summary(row.added, row.removed));
    } else {
        header_spans.push(
            match stage {
                DiffStage::Applied => "Edited",
                DiffStage::Proposed => "Proposed changes to",
            }
            .bold(),
        );
        header_spans.push(format!(" {file_count} {noun} ").into());
        header_spans.extend(render_line_count_summary(total_added, total_removed));
    }
    out.push(RtLine::from(header_spans));

    for (idx, r) in rows.into_iter().enumerate() {
        // Insert a blank separator between file chunks (except before the first)
        if idx > 0 {
            out.push("".into());
        }
        // File header line (skip when single-file header already shows the name)
        let skip_file_header = file_count == 1;
        if !skip_file_header {
            let mut header: Vec<RtSpan<'static>> = Vec::new();
            header.push("  └ ".dim());
            header.extend(render_path(&r));
            header.push(" ".into());
            header.extend(render_line_count_summary(r.added, r.removed));
            out.push(RtLine::from(header));
        }

        // For renames, use the destination extension for highlighting — the
        // diff content reflects the new file, not the old one.
        let lang_path = r.move_path.unwrap_or(r.path);
        let lang = detect_lang_for_path(lang_path);
        let mut lines = vec![];
        let prefix = "    ";
        let content_width = wrap_cols.saturating_sub(prefix.len());
        render_change(r.change, &mut lines, content_width, lang.as_deref());
        out.extend(prefix_lines(lines, prefix.into(), prefix.into()));
    }

    out
}

/// Detect the programming language for a file path by its extension.
/// Returns the raw extension string for `normalize_lang` / `find_syntax`
/// to resolve downstream.
fn detect_lang_for_path(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?;
    Some(ext.to_string())
}

fn render_change(
    change: &FileChange,
    out: &mut Vec<RtLine<'static>>,
    width: usize,
    lang: Option<&str>,
) {
    let style_context = current_diff_render_style_context();
    match change {
        FileChange::Add { content } => {
            // Pre-highlight the entire file content as a whole.
            let syntax_lines = lang.and_then(|l| highlight_code_to_styled_spans(content, l));
            let line_number_width = line_number_width(content.lines().count());
            for (i, raw) in content.lines().enumerate() {
                let syn = syntax_lines.as_ref().and_then(|sl| sl.get(i));
                if let Some(spans) = syn {
                    out.extend(push_wrapped_diff_line_inner_with_theme_and_color_level(
                        i + 1,
                        DiffLineType::Insert,
                        raw,
                        width,
                        line_number_width,
                        Some(spans),
                        style_context.theme,
                        style_context.color_level,
                        style_context.diff_backgrounds,
                    ));
                } else {
                    out.extend(push_wrapped_diff_line_inner_with_theme_and_color_level(
                        i + 1,
                        DiffLineType::Insert,
                        raw,
                        width,
                        line_number_width,
                        /*syntax_spans*/ None,
                        style_context.theme,
                        style_context.color_level,
                        style_context.diff_backgrounds,
                    ));
                }
            }
        }
        FileChange::Delete { content } => {
            let syntax_lines = lang.and_then(|l| highlight_code_to_styled_spans(content, l));
            let line_number_width = line_number_width(content.lines().count());
            for (i, raw) in content.lines().enumerate() {
                let syn = syntax_lines.as_ref().and_then(|sl| sl.get(i));
                if let Some(spans) = syn {
                    out.extend(push_wrapped_diff_line_inner_with_theme_and_color_level(
                        i + 1,
                        DiffLineType::Delete,
                        raw,
                        width,
                        line_number_width,
                        Some(spans),
                        style_context.theme,
                        style_context.color_level,
                        style_context.diff_backgrounds,
                    ));
                } else {
                    out.extend(push_wrapped_diff_line_inner_with_theme_and_color_level(
                        i + 1,
                        DiffLineType::Delete,
                        raw,
                        width,
                        line_number_width,
                        /*syntax_spans*/ None,
                        style_context.theme,
                        style_context.color_level,
                        style_context.diff_backgrounds,
                    ));
                }
            }
        }
        FileChange::Update { unified_diff, .. } => {
            if let Ok(patch) = diffy::Patch::from_str(unified_diff) {
                let mut max_line_number = 0;
                let mut total_diff_bytes: usize = 0;
                let mut total_diff_lines: usize = 0;
                for h in patch.hunks() {
                    let mut old_ln = h.old_range().start();
                    let mut new_ln = h.new_range().start();
                    for l in h.lines() {
                        let text = match l {
                            diffy::Line::Insert(t)
                            | diffy::Line::Delete(t)
                            | diffy::Line::Context(t) => t,
                        };
                        total_diff_bytes += text.len();
                        total_diff_lines += 1;
                        match l {
                            diffy::Line::Insert(_) => {
                                max_line_number = max_line_number.max(new_ln);
                                new_ln += 1;
                            }
                            diffy::Line::Delete(_) => {
                                max_line_number = max_line_number.max(old_ln);
                                old_ln += 1;
                            }
                            diffy::Line::Context(_) => {
                                max_line_number = max_line_number.max(new_ln);
                                old_ln += 1;
                                new_ln += 1;
                            }
                        }
                    }
                }

                // Skip per-line syntax highlighting when the patch is too
                // large — avoids thousands of parser initializations that
                // would stall rendering on big diffs.
                let diff_lang = if exceeds_highlight_limits(total_diff_bytes, total_diff_lines) {
                    None
                } else {
                    lang
                };

                let line_number_width = line_number_width(max_line_number);
                let mut is_first_hunk = true;
                for h in patch.hunks() {
                    if !is_first_hunk {
                        let spacer = format!("{:width$} ", "", width = line_number_width.max(1));
                        let spacer_span = RtSpan::styled(
                            spacer,
                            style_gutter_for(
                                DiffLineType::Context,
                                style_context.theme,
                                style_context.color_level,
                            ),
                        );
                        out.push(RtLine::from(vec![spacer_span, "⋮".dim()]));
                    }
                    is_first_hunk = false;

                    // Highlight each hunk as a single block so syntect parser
                    // state is preserved across consecutive lines.
                    let hunk_syntax_lines = diff_lang.and_then(|language| {
                        let hunk_text: String = h
                            .lines()
                            .iter()
                            .map(|line| match line {
                                diffy::Line::Insert(text)
                                | diffy::Line::Delete(text)
                                | diffy::Line::Context(text) => *text,
                            })
                            .collect();
                        let syntax_lines = highlight_code_to_styled_spans(&hunk_text, language)?;
                        (syntax_lines.len() == h.lines().len()).then_some(syntax_lines)
                    });

                    let mut old_ln = h.old_range().start();
                    let mut new_ln = h.new_range().start();
                    for (line_idx, l) in h.lines().iter().enumerate() {
                        let syntax_spans = hunk_syntax_lines
                            .as_ref()
                            .and_then(|syntax_lines| syntax_lines.get(line_idx));
                        match l {
                            diffy::Line::Insert(text) => {
                                let s = text.trim_end_matches('\n');
                                if let Some(syn) = syntax_spans {
                                    out.extend(
                                        push_wrapped_diff_line_inner_with_theme_and_color_level(
                                            new_ln,
                                            DiffLineType::Insert,
                                            s,
                                            width,
                                            line_number_width,
                                            Some(syn),
                                            style_context.theme,
                                            style_context.color_level,
                                            style_context.diff_backgrounds,
                                        ),
                                    );
                                } else {
                                    out.extend(
                                        push_wrapped_diff_line_inner_with_theme_and_color_level(
                                            new_ln,
                                            DiffLineType::Insert,
                                            s,
                                            width,
                                            line_number_width,
                                            /*syntax_spans*/ None,
                                            style_context.theme,
                                            style_context.color_level,
                                            style_context.diff_backgrounds,
                                        ),
                                    );
                                }
                                new_ln += 1;
                            }
                            diffy::Line::Delete(text) => {
                                let s = text.trim_end_matches('\n');
                                if let Some(syn) = syntax_spans {
                                    out.extend(
                                        push_wrapped_diff_line_inner_with_theme_and_color_level(
                                            old_ln,
                                            DiffLineType::Delete,
                                            s,
                                            width,
                                            line_number_width,
                                            Some(syn),
                                            style_context.theme,
                                            style_context.color_level,
                                            style_context.diff_backgrounds,
                                        ),
                                    );
                                } else {
                                    out.extend(
                                        push_wrapped_diff_line_inner_with_theme_and_color_level(
                                            old_ln,
                                            DiffLineType::Delete,
                                            s,
                                            width,
                                            line_number_width,
                                            /*syntax_spans*/ None,
                                            style_context.theme,
                                            style_context.color_level,
                                            style_context.diff_backgrounds,
                                        ),
                                    );
                                }
                                old_ln += 1;
                            }
                            diffy::Line::Context(text) => {
                                let s = text.trim_end_matches('\n');
                                if let Some(syn) = syntax_spans {
                                    out.extend(
                                        push_wrapped_diff_line_inner_with_theme_and_color_level(
                                            new_ln,
                                            DiffLineType::Context,
                                            s,
                                            width,
                                            line_number_width,
                                            Some(syn),
                                            style_context.theme,
                                            style_context.color_level,
                                            style_context.diff_backgrounds,
                                        ),
                                    );
                                } else {
                                    out.extend(
                                        push_wrapped_diff_line_inner_with_theme_and_color_level(
                                            new_ln,
                                            DiffLineType::Context,
                                            s,
                                            width,
                                            line_number_width,
                                            /*syntax_spans*/ None,
                                            style_context.theme,
                                            style_context.color_level,
                                            style_context.diff_backgrounds,
                                        ),
                                    );
                                }
                                old_ln += 1;
                                new_ln += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "diff_render_tests.rs"]
mod tests;
