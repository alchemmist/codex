//! Low-level markdown event renderer for the TUI transcript.
//!
//! This module consumes `pulldown-cmark` events and emits styled `ratatui`
//! lines, including table layout, width-aware wrapping, and local file-link
//! display. It is the final rendering stage used by higher-level helpers in
//! `markdown.rs`.
//!
//! Local file-link parsing and display policy live in [`local_links`].
//!
//! ## Table rendering pipeline
//!
//! When the parser emits `Tag::Table` .. `TagEnd::Table`, the writer
//! accumulates header and body rows into a `TableState`, then hands it to
//! `render_table_lines` which runs this pipeline:
//!
//! 1. **Filter spillover rows** -- heuristic extraction of rows that are
//!    artifacts of pulldown-cmark's lenient parsing.
//! 2. **Normalize column counts** -- pad or truncate so every row matches the
//!    alignment count.
//! 3. **Compute column widths** -- allocate widths with content-aware
//!    priority and iterative shrinking.
//! 4. **Choose presentation** -- render theme-accented row-separated columns
//!    while values remain scannable, otherwise transpose body rows
//!    into key/value records separated by muted rules.
//! 5. **Append spillover** -- extracted spillover rows rendered as plain text
//!    after the table.
//!
//! ## Width allocation
//!
//! Columns are classified as Narrative (long prose), TokenHeavy (paths, URLs,
//! or hashes), or Compact (short values such as counts and status labels).
//! Token-heavy columns give up excess width before narrative columns so an
//! oversized path does not collapse readable prose; compact values are
//! preserved last. When compact values split, token-heavy values collapse into
//! unusably short chunks, expansive cells form tall narrow strips across enough
//! body rows, or even 3-char-wide columns cannot fit, body rows render as
//! key/value records.

use crate::markdown_text_merge::DecodedTextMerge;
use crate::render::highlight::foreground_style_for_scopes;
use crate::render::highlight::highlight_code_to_lines;
use crate::render::line_utils::line_to_static;
use crate::style::table_separator_style;
use crate::terminal_hyperlinks::HyperlinkLine;
use crate::terminal_hyperlinks::annotate_web_urls_in_line;
use crate::terminal_hyperlinks::remap_wrapped_line;
use crate::terminal_hyperlinks::visible_lines;
use crate::terminal_hyperlinks::web_destination;
use crate::width::char_width;
use crate::width::display_width;
use crate::wrapping::RtOptions;
use crate::wrapping::adaptive_wrap_line;
use crate::wrapping::word_wrap_line;
use pulldown_cmark::Alignment;
use pulldown_cmark::CodeBlockKind;
use pulldown_cmark::CowStr;
use pulldown_cmark::Event;
use pulldown_cmark::HeadingLevel;
use pulldown_cmark::Options;
use pulldown_cmark::Parser;
use pulldown_cmark::Tag;
use pulldown_cmark::TagEnd;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;

#[path = "markdown_render/block_events.rs"]
mod block_events;
#[path = "markdown_render/file_citations.rs"]
mod file_citations;
#[path = "markdown_render/local_links.rs"]
mod local_links;
#[path = "markdown_render/output.rs"]
mod output;
#[path = "markdown_render/streaming.rs"]
mod streaming;
#[path = "markdown_render/table_events.rs"]
mod table_events;
#[path = "markdown_render/table_key_value.rs"]
mod table_key_value;
#[path = "markdown_render/table_layout.rs"]
mod table_layout;
#[path = "markdown_render/table_rows.rs"]
mod table_rows;
#[path = "markdown_render/web_links.rs"]
mod web_links;

use file_citations::FileCitations;
use local_links::is_local_path_like_link;
use local_links::render_local_link_target;
use local_links::should_render_local_link_label;
pub(crate) use streaming::StreamingMarkdownRender;
pub(crate) use streaming::render_streaming_markdown_lines_with_width_and_cwd;
pub(crate) use web_links::hide_web_link_destination;

const TABLE_COLUMN_GAP: usize = 2;
const TABLE_CELL_PADDING: usize = 1;
const TABLE_HEADER_SEPARATOR_CHAR: char = '━';
const TABLE_BODY_SEPARATOR_CHAR: char = '─';

struct MarkdownStyles {
    h1: Style,
    h2: Style,
    h3: Style,
    h4: Style,
    h5: Style,
    h6: Style,
    code: Style,
    emphasis: Style,
    strong: Style,
    strikethrough: Style,
    ordered_list_marker: Style,
    unordered_list_marker: Style,
    link: Style,
    blockquote: Style,
}

impl Default for MarkdownStyles {
    fn default() -> Self {
        Self {
            h1: Style::new().bold().underlined(),
            h2: Style::new().bold(),
            h3: Style::new().bold().italic(),
            h4: Style::new().italic(),
            h5: Style::new().italic(),
            h6: Style::new().italic(),
            code: Style::new().cyan(),
            emphasis: Style::new().italic(),
            strong: Style::new().bold(),
            strikethrough: Style::new().crossed_out(),
            ordered_list_marker: Style::new().light_blue(),
            unordered_list_marker: Style::new(),
            link: Style::new().cyan().underlined(),
            blockquote: Style::new().green(),
        }
    }
}

#[derive(Clone, Debug)]
struct IndentContext {
    prefix: Vec<Span<'static>>,
    marker: Option<Vec<Span<'static>>>,
    is_list: bool,
}

impl IndentContext {
    fn new(prefix: Vec<Span<'static>>, marker: Option<Vec<Span<'static>>>, is_list: bool) -> Self {
        Self {
            prefix,
            marker,
            is_list,
        }
    }
}

/// Styled content of a single cell in the table being parsed.
///
/// A cell can contain multiple lines (hard breaks inside the cell) and rich inline spans (bold,
/// code, links).  The `plain_text()` projection is used for column-width measurement; the styled
/// `lines` are used for final rendering.
#[derive(Clone, Debug, Default)]
struct TableCell {
    lines: Vec<HyperlinkLine>,
}

// TableCell mutators inlined — called per-span during table event parsing.
impl TableCell {
    #[inline]
    fn ensure_line(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(HyperlinkLine::new(Line::default()));
        }
    }

    fn push_annotated(&mut self, mut appended: HyperlinkLine) {
        self.ensure_line();
        if let Some(line) = self.lines.last_mut() {
            let shift = line.width();
            line.line.spans.append(&mut appended.line.spans);
            line.hyperlinks
                .extend(appended.hyperlinks.into_iter().map(|mut link| {
                    link.columns = link.columns.start + shift..link.columns.end + shift;
                    link
                }));
        }
    }

    #[inline]
    fn hard_break(&mut self) {
        self.lines.push(HyperlinkLine::new(Line::default()));
    }

    fn plain_text(&self) -> String {
        use std::fmt::Write;
        let mut buf = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            if i > 0 {
                buf.push(' ');
            }
            for span in &line.line.spans {
                let _ = write!(buf, "{}", span.content);
            }
        }
        buf
    }
}

/// Accumulates pulldown-cmark table events into a structured representation.
///
/// `TableState` is created on `Tag::Table` and consumed on `TagEnd::Table`. Between those events,
/// the Writer delegates cell content (text, code, html, breaks) into the `current_cell`, which is
/// flushed into `current_row` on `TagEnd::TableCell`, then into `header`/`rows` on row/head end
/// events.
#[derive(Debug)]
struct TableBodyRow {
    cells: Vec<TableCell>,
    has_table_pipe_syntax: bool,
}

#[derive(Debug)]
struct TableState {
    alignments: Vec<Alignment>,
    header: Option<Vec<TableCell>>,
    rows: Vec<TableBodyRow>,
    current_row: Option<Vec<TableCell>>,
    current_row_has_table_pipe_syntax: bool,
    current_cell: Option<TableCell>,
    in_header: bool,
}

impl TableState {
    fn new(alignments: Vec<Alignment>) -> Self {
        Self {
            alignments,
            header: None,
            rows: Vec::new(),
            current_row: None,
            current_row_has_table_pipe_syntax: false,
            current_cell: None,
            in_header: false,
        }
    }
}

/// Rendered table output split by wrapping behavior.
///
/// `table_lines` are prewrapped aligned rows or key/value records, except
/// header-only tables may retain pipe fallback rows for normal wrapping.
/// `spillover_lines` are prose rows extracted from parser artifacts and should
/// be routed through normal wrapping.
struct RenderedTableLines {
    table_lines: Vec<HyperlinkLine>,
    table_lines_prewrapped: bool,
    spillover_lines: Vec<HyperlinkLine>,
}

/// Classification of a table column for width-allocation priority.
///
/// Token-heavy columns such as paths and URLs are allowed to wrap before prose becomes unreadable.
/// Compact columns such as counts or status words resist wrapping so their values stay scannable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableColumnKind {
    /// Long-form prose content (>= 4 avg words/cell or >= 28 avg char width).
    Narrative,
    /// Content dominated by long tokens, such as paths, URLs, and hashes.
    TokenHeavy,
    /// Short values, such as counts and status labels, that should resist wrapping.
    Compact,
}

/// Per-column statistics used to drive the width-allocation algorithm.
///
/// Collected in a single pass over the header and body rows before any
/// shrinking decisions are made.
#[derive(Clone, Debug)]
struct TableColumnMetrics {
    /// Widest cell content (display width) across header and all body rows.
    max_width: usize,
    /// Display width of the longest whitespace-delimited token in the header.
    header_token_width: usize,
    /// Display width of the longest whitespace-delimited token across body rows.
    body_token_width: usize,
    /// Classification derived from body token density and average cell content.
    kind: TableColumnKind,
}

/// Render markdown with default wrapping behavior.
///
/// Use this when the caller does not have a concrete render width yet (for
/// example, snapshot tests or contexts that intentionally defer wrapping). If
/// a viewport width is known, prefer [`render_markdown_text_with_width`] so
/// table fallback and line wrapping decisions match the visible terminal.
pub fn render_markdown_text(input: &str) -> Text<'static> {
    render_markdown_text_with_width(input, /*width*/ None)
}

/// Render markdown constrained to a known terminal width.
///
/// The renderer preserves columnar table structure while values remain
/// scannable and falls back to key/value records when body rows cannot fit
/// readably. Passing `None` keeps intrinsic line widths and disables
/// width-driven wrapping in the markdown writer. Local file links render
/// relative to the current process working directory.
pub(crate) fn render_markdown_text_with_width(input: &str, width: Option<usize>) -> Text<'static> {
    let cwd = std::env::current_dir().ok();
    render_markdown_text_with_width_and_cwd(input, width, cwd.as_deref())
}

/// Render markdown with an explicit working directory for local file links.
///
/// The `cwd` parameter controls how absolute local targets are shortened before display. Passing
/// the session cwd keeps full renders, history cells, and streamed deltas visually aligned even
/// when rendering happens away from the process cwd.
pub(crate) fn render_markdown_text_with_width_and_cwd(
    input: &str,
    width: Option<usize>,
    cwd: Option<&Path>,
) -> Text<'static> {
    Text::from(visible_lines(render_markdown_lines_with_width_and_cwd(
        input, width, cwd,
    )))
}

/// Keep destinations visible by default, including for callers that discard hyperlink metadata.
/// Semantic output paths supply their hidden-destination policy explicitly.
pub(crate) fn render_markdown_lines_with_width_and_cwd(
    input: &str,
    width: Option<usize>,
    cwd: Option<&Path>,
) -> Vec<HyperlinkLine> {
    render_markdown_lines_with_width_cwd_and_hidden_link_destinations(
        input,
        width,
        cwd,
        &never_hide_link_destination,
    )
}

fn never_hide_link_destination(_: &str) -> bool {
    false
}

pub(crate) fn render_markdown_lines_with_width_cwd_and_hidden_link_destinations(
    input: &str,
    width: Option<usize>,
    cwd: Option<&Path>,
    is_hidden_link_destination: &dyn Fn(&str) -> bool,
) -> Vec<HyperlinkLine> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    let parser = DecodedTextMerge::new(Parser::new_ext(input, options).into_offset_iter());
    let mut w = Writer::new(input, parser, width, cwd, is_hidden_link_destination);
    w.run();
    w.text
}

#[derive(Clone, Debug)]
struct LinkState {
    destination: String,
    show_destination: bool,
    style_label: bool,
    has_visible_label: bool,
    /// Pre-rendered display text for local file links.
    ///
    /// When this is present, label spans are buffered until the link closes so path-like labels
    /// can collapse to this canonical target without losing descriptive labels.
    local_target_display: Option<String>,
    local_label_spans: Vec<Span<'static>>,
}

fn should_render_link_destination(dest_url: &str) -> bool {
    !is_local_path_like_link(dest_url)
}

/// Stateful pulldown-cmark event consumer that builds styled `ratatui` output.
///
/// Tracks inline style nesting, indent/blockquote context, list numbering,
/// and an optional `TableState` for accumulating table events.  The
/// `wrap_width` field enables width-aware line wrapping and table column
/// allocation; when `None`, lines keep their intrinsic width.
struct Writer<'a, 'policy, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    input: &'a str,
    iter: I,
    text: Vec<HyperlinkLine>,
    styles: MarkdownStyles,
    inline_styles: Vec<Style>,
    indent_stack: Vec<IndentContext>,
    list_indices: Vec<Option<u64>>,
    list_needs_blank_before_next_item: Vec<bool>,
    list_item_start_line_counts: Vec<usize>,
    link: Option<LinkState>,
    needs_newline: bool,
    pending_marker_line: bool,
    in_paragraph: bool,
    in_code_block: bool,
    code_block_lang: Option<String>,
    code_block_buffer: String,
    wrap_width: Option<usize>,
    cwd: Option<PathBuf>,
    is_hidden_link_destination: &'policy dyn Fn(&str) -> bool,
    line_ends_with_local_link_target: bool,
    pending_local_link_soft_break: bool,
    current_line_content: Option<HyperlinkLine>,
    current_initial_indent: Vec<Span<'static>>,
    current_subsequent_indent: Vec<Span<'static>>,
    current_line_style: Style,
    current_line_in_code_block: bool,
    table_state: Option<TableState>,
}

impl<'a, 'policy, I> Writer<'a, 'policy, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    fn new(
        input: &'a str,
        iter: I,
        wrap_width: Option<usize>,
        cwd: Option<&Path>,
        is_hidden_link_destination: &'policy dyn Fn(&str) -> bool,
    ) -> Self {
        Self {
            input,
            iter,
            text: Vec::new(),
            styles: MarkdownStyles::default(),
            inline_styles: Vec::new(),
            indent_stack: Vec::new(),
            list_indices: Vec::new(),
            list_needs_blank_before_next_item: Vec::new(),
            list_item_start_line_counts: Vec::new(),
            link: None,
            needs_newline: false,
            pending_marker_line: false,
            in_paragraph: false,
            in_code_block: false,
            code_block_lang: None,
            code_block_buffer: String::new(),
            wrap_width,
            cwd: cwd.map(Path::to_path_buf),
            is_hidden_link_destination,
            line_ends_with_local_link_target: false,
            pending_local_link_soft_break: false,
            current_line_content: None,
            current_initial_indent: Vec::new(),
            current_subsequent_indent: Vec::new(),
            current_line_style: Style::default(),
            current_line_in_code_block: false,
            table_state: None,
        }
    }

    fn run(&mut self) {
        while let Some((ev, range)) = self.iter.next() {
            self.handle_event(ev, range);
        }
        self.flush_current_line();
    }

    fn handle_event(&mut self, event: Event<'a>, range: Range<usize>) {
        self.prepare_for_event(&event);
        match event {
            Event::Start(tag) => self.start_tag(tag, range),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.text(text),
            Event::Code(code) => self.code(code),
            Event::SoftBreak => self.soft_break(),
            Event::HardBreak => self.hard_break(),
            Event::Rule => {
                self.flush_current_line();
                if !self.text.is_empty() {
                    self.push_blank_line();
                }
                self.push_line(Line::from("———"));
                self.needs_newline = true;
            }
            Event::Html(html) => self.html(html, /*inline*/ false),
            Event::InlineHtml(html) => self.html(html, /*inline*/ true),
            Event::FootnoteReference(_) => {}
            Event::TaskListMarker(_) => {}
        }
    }

    fn prepare_for_event(&mut self, event: &Event<'a>) {
        if !self.pending_local_link_soft_break {
            return;
        }

        // Local file links render from the destination at `TagEnd::Link`, so a Markdown soft break
        // immediately before a descriptive `: ...` should stay inline instead of splitting the
        // list item across two lines.
        if matches!(event, Event::Text(text) if text.trim_start().starts_with(':')) {
            self.pending_local_link_soft_break = false;
            return;
        }

        self.pending_local_link_soft_break = false;
        self.push_line(Line::default());
    }

    fn start_tag(&mut self, tag: Tag<'a>, range: Range<usize>) {
        match tag {
            Tag::Paragraph => self.start_paragraph(),
            Tag::Heading { level, .. } => self.start_heading(level),
            Tag::BlockQuote => self.start_blockquote(),
            Tag::CodeBlock(kind) => {
                let indent = match kind {
                    CodeBlockKind::Fenced(_) => None,
                    CodeBlockKind::Indented => Some(Span::from(" ".repeat(4))),
                };
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => Some(lang.to_string()),
                    CodeBlockKind::Indented => None,
                };
                self.start_codeblock(lang, indent)
            }
            Tag::List(start) => self.start_list(start),
            Tag::Item => self.start_item(),
            Tag::Emphasis => self.push_inline_style(self.styles.emphasis),
            Tag::Strong => self.push_inline_style(self.styles.strong),
            Tag::Strikethrough => self.push_inline_style(self.styles.strikethrough),
            Tag::Link { dest_url, .. } => self.push_link(dest_url.to_string()),
            Tag::Table(alignments) => self.start_table(alignments),
            Tag::TableHead => self.start_table_head(),
            Tag::TableRow => self.start_table_row(range),
            Tag::TableCell => self.start_table_cell(),
            Tag::HtmlBlock
            | Tag::FootnoteDefinition(_)
            | Tag::Image { .. }
            | Tag::MetadataBlock(_) => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.end_paragraph(),
            TagEnd::Heading(_) => self.end_heading(),
            TagEnd::BlockQuote => self.end_blockquote(),
            TagEnd::CodeBlock => self.end_codeblock(),
            TagEnd::List(_) => self.end_list(),
            TagEnd::Item => {
                self.flush_current_line();
                let start_line_count = self.list_item_start_line_counts.pop().unwrap_or_default();
                if self.text.len().saturating_sub(start_line_count) > 1
                    && let Some(needs_blank) = self.list_needs_blank_before_next_item.last_mut()
                {
                    *needs_blank = true;
                }
                self.indent_stack.pop();
                self.pending_marker_line = false;
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => self.pop_inline_style(),
            TagEnd::Link => self.pop_link(),
            TagEnd::Table => self.end_table(),
            TagEnd::TableHead => self.end_table_head(),
            TagEnd::TableRow => self.end_table_row(),
            TagEnd::TableCell => self.end_table_cell(),
            TagEnd::HtmlBlock
            | TagEnd::FootnoteDefinition
            | TagEnd::Image
            | TagEnd::MetadataBlock(_) => {}
        }
    }
}

#[cfg(test)]
mod markdown_render_tests {
    include!("markdown_render_tests.rs");
}

#[cfg(test)]
#[path = "markdown_render_unit_tests.rs"]
mod tests;
