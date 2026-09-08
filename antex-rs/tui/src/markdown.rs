use std::path::Path;

pub(crate) use crate::markdown_normalize::unwrap_markdown_fences;
use crate::terminal_hyperlinks::HyperlinkLine;

pub(crate) fn render_markdown_agent_with_links_and_cwd(
    source: &str,
    width: Option<usize>,
    cwd: Option<&Path>,
) -> Vec<HyperlinkLine> {
    let normalized = unwrap_markdown_fences(source);
    crate::markdown_render::render_streaming_markdown_lines_with_width_and_cwd(
        &normalized,
        width,
        cwd,
        &crate::markdown_render::hide_web_link_destination,
    )
    .lines
}
