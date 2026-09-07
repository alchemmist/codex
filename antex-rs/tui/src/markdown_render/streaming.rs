//! Streaming markdown rendering.

use super::DecodedTextMerge;
use super::FileCitations;
use super::HyperlinkLine;
use super::Options;
use super::Parser;
use super::Writer;
use std::path::Path;

pub(crate) struct StreamingMarkdownRender {
    pub(crate) lines: Vec<HyperlinkLine>,
}

pub(crate) fn render_streaming_markdown_lines_with_width_and_cwd(
    input: &str,
    width: Option<usize>,
    cwd: Option<&Path>,
    is_hidden_link_destination: &dyn Fn(&str) -> bool,
) -> StreamingMarkdownRender {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    let citations = FileCitations::new(input, options);
    let parser = Parser::new_ext(&citations.markdown, options);
    let parser = DecodedTextMerge::new(citations.events(parser, cwd));
    let mut writer = Writer::new(input, parser, width, cwd, is_hidden_link_destination);
    writer.run();
    StreamingMarkdownRender { lines: writer.text }
}
