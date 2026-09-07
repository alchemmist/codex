//! Markdown-to-ratatui rendering entry points.
//!
//! This module provides the public API surface that the rest of the TUI uses
//! to turn markdown source into `Vec<Line<'static>>`.  Two variants exist:
//!
//! - [`append_markdown`] -- general-purpose, used for plan blocks and history
//!   cells that already hold pre-processed markdown (no fence unwrapping).
//! - [`append_markdown_agent`] -- for agent responses.  Runs
//!   [`unwrap_markdown_fences`] first so that `` ```md ``/`` ```markdown ``
//!   fences containing tables are stripped and `pulldown-cmark` sees raw
//!   table syntax instead of fenced code.
//!
//! ## Why fence unwrapping exists
//!
//! LLM agents frequently wrap tables in `` ```markdown `` fences, treating
//! them as code.  Without unwrapping, `pulldown-cmark` parses those lines
//! as a fenced code block and renders them as monospace code rather than a
//! structured table.  The unwrapper is intentionally conservative: it
//! buffers the entire fence body before deciding, only unwraps fences whose
//! info string is `md` or `markdown` AND whose body contains a
//! header+delimiter pair, and degrades gracefully on unclosed fences.
use pulldown_cmark::CodeBlockKind;
use pulldown_cmark::Event;
use pulldown_cmark::Tag;
use pulldown_cmark::TagEnd;
use ratatui::text::Line;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use crate::inline_visualization::InlineVisualizationContext;
use crate::inline_visualization::rewrite_inline_visualizations;
pub(crate) use crate::markdown_normalize::unwrap_markdown_fences;
use crate::terminal_hyperlinks::HyperlinkLine;

/// Render markdown source to styled ratatui lines and append them to `lines`.
///
/// Callers that already know the session working directory should pass it here so streamed and
/// non-streamed rendering show the same relative path text even if the process cwd differs.
pub(crate) fn append_markdown(
    markdown_source: &str,
    width: Option<usize>,
    cwd: Option<&Path>,
    lines: &mut Vec<Line<'static>>,
) {
    let rendered = crate::markdown_render::render_markdown_text_with_width_and_cwd(
        markdown_source,
        width,
        cwd,
    );
    crate::render::line_utils::push_owned_lines(&rendered.lines, lines);
}

/// Render an agent message to styled ratatui lines.
///
/// Before rendering, the source is passed through [`unwrap_markdown_fences`] so that tables
/// wrapped in `` ```md `` fences are rendered as native tables rather than code blocks.
/// Non-markdown fences (e.g. `rust`, `sh`) are left
/// intact.
#[cfg(test)]
pub(crate) fn append_markdown_agent(
    markdown_source: &str,
    width: Option<usize>,
    lines: &mut Vec<Line<'static>>,
) {
    let normalized = unwrap_markdown_fences(markdown_source);
    let rendered = crate::markdown_render::render_markdown_text_with_width_and_cwd(
        &normalized,
        width,
        /*cwd*/ None,
    );
    crate::render::line_utils::push_owned_lines(&rendered.lines, lines);
}

pub(crate) fn render_markdown_agent_with_links_and_cwd(
    markdown_source: &str,
    width: Option<usize>,
    cwd: Option<&Path>,
) -> Vec<HyperlinkLine> {
    render_markdown_agent_with_links_cwd_and_visualizations(
        markdown_source,
        width,
        cwd,
        /*inline_visualization_context*/ None,
    )
}

pub(crate) fn render_markdown_agent_with_links_cwd_and_visualizations(
    markdown_source: &str,
    width: Option<usize>,
    cwd: Option<&Path>,
    inline_visualization_context: Option<&InlineVisualizationContext>,
) -> Vec<HyperlinkLine> {
    let rewritten = rewrite_inline_visualizations(markdown_source, inline_visualization_context);
    let normalized = unwrap_markdown_fences(&rewritten.markdown);
    let is_hidden_link_destination = |destination: &str| {
        rewritten.trusted_file_links.contains_key(destination)
            || crate::markdown_render::hide_web_link_destination(destination)
    };
    let mut lines = crate::markdown_render::render_streaming_markdown_lines_with_width_and_cwd(
        &normalized,
        width,
        cwd,
        &is_hidden_link_destination,
    )
    .lines;
    for hyperlink in lines.iter_mut().flat_map(|line| &mut line.hyperlinks) {
        if let Some(link) = rewritten.trusted_file_links.get(&hyperlink.destination) {
            hyperlink.retarget_to_trusted_file(&link.destination);
        }
    }
    lines
}

/// Render an agent message and collect the block metadata needed for incremental rendering.
///
/// Block offsets are mapped back to `markdown_source` after Markdown table fences are unwrapped.
/// If a normalized boundary cannot be expressed as a raw-source suffix, it is discarded so the
/// transformed block remains mutable.
pub(crate) fn render_streaming_markdown_agent_with_links_and_cwd(
    markdown_source: &str,
    width: Option<usize>,
    cwd: Option<&Path>,
) -> crate::markdown_render::StreamingMarkdownRender {
    let normalized = unwrap_markdown_fences(markdown_source);
    let mut rendered = crate::markdown_render::render_streaming_markdown_lines_with_width_and_cwd(
        &normalized,
        width,
        cwd,
        &crate::markdown_render::hide_web_link_destination,
    );
    if normalized != markdown_source {
        // Fence unwrapping removes opening/closing lines. A normalized tail that is still a raw
        // suffix necessarily begins after those removed lines, so its boundary can safely be
        // mapped back to the raw source; otherwise leave the transformed block mutable.
        rendered.last_top_level_block_start = rendered
            .last_top_level_block_start
            .and_then(|boundary| markdown_source.strip_suffix(&normalized[boundary..]))
            .map(str::len);
    }
    rendered
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CopyTarget {
    Code {
        language: Option<String>,
        content: Arc<str>,
    },
    Quote(Arc<str>),
}

pub(crate) fn extract_copy_targets(markdown_source: &str) -> Vec<CopyTarget> {
    let mut targets = Vec::new();
    let mut quote: Option<(Range<usize>, usize, bool)> = None;
    let mut quote_depth = 0usize;
    let mut in_code_block = false;
    let mut code = None;

    for (event, range) in pulldown_cmark::Parser::new(markdown_source).into_offset_iter() {
        match event {
            Event::Start(Tag::BlockQuote) => {
                if quote_depth == 0 {
                    quote = Some((range, targets.len(), false));
                }
                quote_depth += 1;
            }
            Event::End(TagEnd::BlockQuote) => {
                quote_depth = quote_depth.saturating_sub(1);
                if quote_depth == 0 {
                    quote = None;
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                if let CodeBlockKind::Fenced(info) = kind {
                    let language = info
                        .split([',', ' ', '\t'])
                        .next()
                        .filter(|language| !language.is_empty())
                        .map(str::to_owned);
                    code = Some((language, String::new()));
                }
            }
            Event::Text(text) | Event::Code(text) => {
                if in_code_block {
                    if let Some((_, content)) = &mut code {
                        if text.starts_with('\n')
                            && range.start > 0
                            && markdown_source.as_bytes()[range.start - 1] == b'\r'
                        {
                            content.push('\r');
                        }
                        content.push_str(&text);
                    }
                } else if let Some((span, position, emitted)) = &mut quote
                    && !*emitted
                    && !crate::git_action_directives::strip_line_directives(&text)
                        .0
                        .trim()
                        .is_empty()
                {
                    let content = markdown_source[span.clone()]
                        .split_inclusive('\n')
                        .map(|line| {
                            line.trim_start_matches(' ')
                                .strip_prefix('>')
                                .map_or(line, |line| line.strip_prefix(' ').unwrap_or(line))
                        })
                        .collect::<String>();
                    targets.insert(*position, CopyTarget::Quote(content.into()));
                    *emitted = true;
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                if let Some((language, content)) = code.take() {
                    targets.push(CopyTarget::Code {
                        language,
                        content: content.into(),
                    });
                }
            }
            _ => {}
        }
    }

    targets
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use ratatui::text::Line;

    #[test]
    fn copy_targets_preserve_fenced_code_language_and_exact_content() {
        let markdown =
            "```python title=example\nprint('hi')\n```\n\n    ignored()\n\n~~~\nplain()\n~~~";
        assert_eq!(
            extract_copy_targets(markdown),
            vec![
                CopyTarget::Code {
                    language: Some("python".to_string()),
                    content: Arc::from("print('hi')\n"),
                },
                CopyTarget::Code {
                    language: None,
                    content: Arc::from("plain()\n"),
                },
            ]
        );
        assert_eq!(
            extract_copy_targets("```powershell\r\nGet-Item .\r\nWrite-Output ok\r\n```\r\n"),
            vec![CopyTarget::Code {
                language: Some("powershell".to_string()),
                content: Arc::from("Get-Item .\r\nWrite-Output ok\r\n"),
            }]
        );
    }

    #[test]
    fn copy_targets_preserve_nested_quote_markdown_and_source_order() {
        let markdown = "> outer **bold**\n> > inner *quote*\n> ```sh\n> nested()\n> ```\n";
        assert_eq!(
            extract_copy_targets(markdown),
            vec![
                CopyTarget::Quote(Arc::from(
                    "outer **bold**\n> inner *quote*\n```sh\nnested()\n```\n"
                )),
                CopyTarget::Code {
                    language: Some("sh".to_string()),
                    content: Arc::from("nested()\n"),
                },
            ]
        );
    }

    #[test]
    fn copy_targets_exclude_blockquotes_without_prose() {
        let markdown = concat!(
            "> > ::git-stage{cwd=\"/repo\"}\n\n",
            "> ::git-stage{cwd=\"/repo\"}\n> ```sh\n> code()\n> ```\n\n",
            "> quoted prose\n"
        );
        assert_eq!(
            extract_copy_targets(markdown),
            vec![
                CopyTarget::Code {
                    language: Some("sh".to_string()),
                    content: Arc::from("code()\n"),
                },
                CopyTarget::Quote(Arc::from("quoted prose\n")),
            ]
        );
    }

    fn lines_to_strings(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.clone())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn citations_render_as_plain_text() {
        let src = "Before 【F:/x.rs†L1】\nAfter 【F:/x.rs†L3】\n";
        let mut out = Vec::new();
        append_markdown(src, /*width*/ None, /*cwd*/ None, &mut out);
        let rendered = lines_to_strings(&out);
        assert_eq!(
            rendered,
            vec![
                "Before 【F:/x.rs†L1】".to_string(),
                "After 【F:/x.rs†L3】".to_string()
            ]
        );
    }

    #[test]
    fn indented_code_blocks_preserve_leading_whitespace() {
        // Basic sanity: indented code with surrounding blank lines should produce the indented line.
        let src = "Before\n\n    code 1\n\nAfter\n";
        let mut out = Vec::new();
        append_markdown(src, /*width*/ None, /*cwd*/ None, &mut out);
        let lines = lines_to_strings(&out);
        assert_eq!(lines, vec!["Before", "", "    code 1", "", "After"]);
    }

    #[test]
    fn append_markdown_preserves_full_text_line() {
        let src = "Hi! How can I help with codex-rs today? Want me to explore the repo, run tests, or work on a specific change?\n";
        let mut out = Vec::new();
        append_markdown(src, /*width*/ None, /*cwd*/ None, &mut out);
        assert_eq!(
            out.len(),
            1,
            "expected a single rendered line for plain text"
        );
        let rendered: String = out
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.clone())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(
            rendered,
            "Hi! How can I help with codex-rs today? Want me to explore the repo, run tests, or work on a specific change?"
        );
    }

    #[test]
    fn append_markdown_matches_tui_markdown_for_ordered_item() {
        let mut out = Vec::new();
        append_markdown(
            "1. Tight item\n",
            /*width*/ None,
            /*cwd*/ None,
            &mut out,
        );
        let lines = lines_to_strings(&out);
        assert_eq!(lines, vec!["1. Tight item".to_string()]);
    }

    #[test]
    fn append_markdown_keeps_ordered_list_line_unsplit_in_context() {
        let src = "Loose vs. tight list items:\n1. Tight item\n";
        let mut out = Vec::new();
        append_markdown(src, /*width*/ None, /*cwd*/ None, &mut out);

        let lines = lines_to_strings(&out);

        // Expect to find the ordered list line rendered as a single line,
        // not split into a marker-only line followed by the text.
        assert!(
            lines.iter().any(|s| s == "1. Tight item"),
            "expected '1. Tight item' rendered as a single line; got: {lines:?}"
        );
        assert!(
            !lines
                .windows(2)
                .any(|w| w[0].trim_end() == "1." && w[1] == "Tight item"),
            "did not expect a split into ['1.', 'Tight item']; got: {lines:?}"
        );
    }

    #[test]
    fn append_markdown_agent_unwraps_markdown_fences_for_table_rendering() {
        let src = "```markdown\n| A | B |\n|---|---|\n| 1 | 2 |\n```\n";
        let mut out = Vec::new();
        append_markdown_agent(src, /*width*/ None, &mut out);
        let rendered = lines_to_strings(&out);
        assert!(rendered.iter().any(|line| line.contains('━')));
        assert!(rendered.iter().any(|line| line.contains(" 1      2")));
    }

    #[test]
    fn append_markdown_agent_unwraps_markdown_fences_for_no_outer_table_rendering() {
        let src = "```md\nCol A | Col B | Col C\n--- | --- | ---\nx | y | z\n10 | 20 | 30\n```\n";
        let mut out = Vec::new();
        append_markdown_agent(src, /*width*/ None, &mut out);
        let rendered = lines_to_strings(&out);
        assert!(rendered.iter().any(|line| line.contains('━')));
        assert!(
            rendered
                .iter()
                .any(|line| line.contains(" Col A    Col B    Col C"))
        );
        assert!(
            !rendered
                .iter()
                .any(|line| line.trim() == "Col A | Col B | Col C")
        );
    }

    #[test]
    fn append_markdown_agent_unwraps_markdown_fences_for_two_column_no_outer_table() {
        let src = "```md\nA | B\n--- | ---\nleft | right\n```\n";
        let mut out = Vec::new();
        append_markdown_agent(src, /*width*/ None, &mut out);
        let rendered = lines_to_strings(&out);
        assert!(rendered.iter().any(|line| line.contains('━')));
        assert!(rendered.iter().any(|line| line.contains(" left    right")));
        assert!(!rendered.iter().any(|line| line.trim() == "A | B"));
    }

    #[test]
    fn append_markdown_agent_unwraps_markdown_fences_for_single_column_table() {
        let src = "```md\n| Only |\n|---|\n| value |\n```\n";
        let mut out = Vec::new();
        append_markdown_agent(src, /*width*/ None, &mut out);
        let rendered = lines_to_strings(&out);
        assert!(rendered.iter().any(|line| line.contains('━')));
        assert!(!rendered.iter().any(|line| line.trim() == "| Only |"));
    }

    #[test]
    fn append_markdown_agent_keeps_non_markdown_fences_as_code() {
        let src = "```rust\n| A | B |\n|---|---|\n| 1 | 2 |\n```\n";
        let mut out = Vec::new();
        append_markdown_agent(src, /*width*/ None, &mut out);
        let rendered = lines_to_strings(&out);
        assert_eq!(
            rendered,
            vec![
                "| A | B |".to_string(),
                "|---|---|".to_string(),
                "| 1 | 2 |".to_string(),
            ]
        );
    }

    #[test]
    fn append_markdown_agent_unwraps_blockquoted_markdown_fence_table() {
        let src = "> ```markdown\n> | A | B |\n> |---|---|\n> | 1 | 2 |\n> ```\n";
        let rendered = unwrap_markdown_fences(src);
        assert!(
            !rendered.contains("```"),
            "expected markdown fence markers to be removed: {rendered:?}"
        );
    }

    #[test]
    fn append_markdown_agent_keeps_non_blockquoted_markdown_fence_with_blockquote_table_example() {
        let src = "```markdown\n> | A | B |\n> |---|---|\n> | 1 | 2 |\n```\n";
        let normalized = unwrap_markdown_fences(src);
        assert_eq!(normalized, src);
    }

    #[test]
    fn append_markdown_agent_keeps_markdown_fence_when_content_is_not_table() {
        let src = "```markdown\n**bold**\n```\n";
        let mut out = Vec::new();
        append_markdown_agent(src, /*width*/ None, &mut out);
        let rendered = lines_to_strings(&out);
        assert_eq!(rendered, vec!["**bold**".to_string()]);
    }

    #[test]
    fn unwrap_markdown_fences_repro_keeps_fence_without_header_delimiter_pair() {
        let src = "```markdown\n| A | B |\nnot a delimiter row\n| --- | --- |\n# Heading\n```\n";
        let normalized = unwrap_markdown_fences(src);
        assert_eq!(normalized, src);
    }

    #[test]
    fn append_markdown_agent_keeps_markdown_fence_with_blank_line_between_header_and_delimiter() {
        let src = "```markdown\n| A | B |\n\n|---|---|\n| 1 | 2 |\n```\n";
        let rendered = unwrap_markdown_fences(src);
        assert_eq!(rendered, src);
    }
}
