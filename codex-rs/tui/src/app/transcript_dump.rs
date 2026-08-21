use std::fmt::Write as _;
use std::sync::Arc;

use pulldown_cmark::Event;
use pulldown_cmark::HeadingLevel;
use pulldown_cmark::Options;
use pulldown_cmark::Parser;
use pulldown_cmark::Tag;
use pulldown_cmark::TagEnd;

use super::App;
use super::transcript_export::TranscriptBlock;
use super::transcript_export::TranscriptBlockKind;
use super::transcript_export::load_export_transcript;
use super::transcript_export::transcript_blocks;
use super::transcript_export::write_transcript;
use crate::app_server_session::AppServerSession;
use crate::history_cell::HistoryCell;
use crate::thread_transcript::RawReasoningVisibility;

impl App {
    pub(super) async fn dump_transcript_html(
        &mut self,
        app_server: &mut AppServerSession,
    ) -> Result<(), String> {
        let thread_id = self
            .chat_widget
            .thread_id()
            .ok_or_else(|| "No active conversation to dump.".to_string())?;
        let visibility = if self.config.show_raw_agent_reasoning {
            RawReasoningVisibility::Visible
        } else {
            RawReasoningVisibility::Hidden
        };
        let cells = load_export_transcript(
            app_server,
            thread_id,
            visibility,
            Some(&self.config),
            self.transcript_cells.clone(),
        )
        .await?;
        let exported_at = chrono::Utc::now();
        let html = render_html_transcript(&cells, &thread_id.to_string(), exported_at)?;
        let short_id = thread_id.to_string().chars().take(8).collect::<String>();
        let filename = format!(
            "codex-conversation-{short_id}-{}.html",
            exported_at.format("%Y%m%d-%H%M%S")
        );
        let cwd = if self.app_server_target.uses_remote_workspace() {
            self.launch_cwd.as_path()
        } else {
            self.chat_widget.config_ref().cwd.as_path()
        };
        let path = write_transcript(cwd, filename.as_ref(), &html)?;
        self.chat_widget.add_info_message(
            format!("Saved HTML conversation to {}", path.display()),
            /*hint*/ None,
        );
        Ok(())
    }
}

fn render_html_transcript(
    cells: &[Arc<dyn HistoryCell>],
    thread_id: &str,
    exported_at: chrono::DateTime<chrono::Utc>,
) -> Result<String, String> {
    let blocks = transcript_blocks(cells)?;
    let mut body = String::new();
    let mut activities = Vec::new();
    for block in blocks {
        match block.kind {
            TranscriptBlockKind::User => {
                push_activities(&mut body, &mut activities);
                push_message(&mut body, "you", "You", &block.markdown);
            }
            TranscriptBlockKind::Assistant => {
                push_activities(&mut body, &mut activities);
                push_message(&mut body, "codex", "Codex", &block.markdown);
            }
            TranscriptBlockKind::Plan => {
                push_activities(&mut body, &mut activities);
                push_message(&mut body, "codex", "Codex · plan", &block.markdown);
            }
            TranscriptBlockKind::Reasoning | TranscriptBlockKind::Activity => {
                activities.push(block);
            }
        }
    }
    push_activities(&mut body, &mut activities);
    let thread_id = escape_html(thread_id);
    let exported_at = exported_at.format("%Y-%m-%d %H:%M UTC");
    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<title>Codex conversation</title>
<style>
:root {{ --paper:#f8f7f3; --ink:#171717; --muted:#686868; --line:#c8c6bf; --soft:#eeece6; }}
@media (prefers-color-scheme:dark) {{ :root {{ --paper:#111; --ink:#e8e6df; --muted:#aaa69c; --line:#3b3934; --soft:#1b1a18; }} }}
* {{ box-sizing:border-box; }}
body {{ margin:0; background:var(--paper); color:var(--ink); font:19px/1.62 Georgia,"Times New Roman",serif; }}
main {{ width:min(760px,calc(100% - 36px)); margin:0 auto; padding:72px 0 120px; }}
header.page {{ padding-bottom:28px; border-bottom:1px solid var(--ink); }}
h1 {{ margin:0 0 8px; font-size:clamp(2rem,7vw,3.4rem); font-weight:400; letter-spacing:-.04em; line-height:1; }}
.meta,.role,summary {{ font:12px/1.4 ui-monospace,SFMono-Regular,Menlo,monospace; letter-spacing:.08em; text-transform:uppercase; }}
.meta {{ color:var(--muted); overflow-wrap:anywhere; }}
.message {{ padding:34px 0 38px; border-bottom:1px solid var(--line); }}
.message.you {{ border-left:3px solid var(--ink); padding-left:22px; }}
.role {{ margin-bottom:13px; color:var(--muted); }}
.content > :first-child {{ margin-top:0; }} .content > :last-child {{ margin-bottom:0; }}
p,ul,ol,pre,blockquote,table {{ margin:0 0 1em; }}
h2,h3,h4 {{ margin:1.5em 0 .5em; line-height:1.2; }}
pre {{ padding:16px 18px; overflow:auto; background:var(--soft); border:1px solid var(--line); font:13px/1.55 ui-monospace,SFMono-Regular,Menlo,monospace; }}
code {{ font: .82em ui-monospace,SFMono-Regular,Menlo,monospace; }}
:not(pre)>code {{ padding:.12em .3em; background:var(--soft); }}
blockquote {{ margin-left:0; padding-left:18px; border-left:1px solid var(--ink); color:var(--muted); }}
table {{ width:100%; border-collapse:collapse; font-size:.9em; }} th,td {{ padding:8px; border-bottom:1px solid var(--line); text-align:left; }}
.actions {{ padding:14px 0; border-bottom:1px solid var(--line); }}
summary {{ cursor:pointer; color:var(--muted); list-style:none; }} summary::-webkit-details-marker {{ display:none; }}
summary::before {{ content:"+ "; }} details[open] summary::before {{ content:"− "; }}
.action {{ margin-top:12px; padding:14px 16px; background:var(--soft); border-left:1px solid var(--line); }}
.action-label {{ margin-bottom:8px; color:var(--muted); font:11px/1.4 ui-monospace,SFMono-Regular,Menlo,monospace; text-transform:uppercase; }}
.action pre {{ margin:0; padding:0; border:0; background:transparent; white-space:pre-wrap; overflow-wrap:anywhere; }}
@media (max-width:600px) {{ main {{ padding-top:38px; }} body {{ font-size:17px; }} .message.you {{ padding-left:15px; }} }}
</style>
</head>
<body>
<main>
<header class="page"><h1>Codex conversation</h1><div class="meta">{thread_id} · {exported_at}</div></header>
{body}</main>
</body>
</html>
"#
    ))
}

fn push_message(body: &mut String, class: &str, role: &str, markdown: &str) {
    let mut rendered = String::new();
    render_safe_markdown(markdown, &mut rendered);
    let _ = writeln!(
        body,
        "<section class=\"message {class}\"><div class=\"role\">{role}</div><div class=\"content\">{rendered}</div></section>"
    );
}

fn push_activities(body: &mut String, activities: &mut Vec<TranscriptBlock>) {
    if activities.is_empty() {
        return;
    }
    let count = activities.len();
    let _ = writeln!(
        body,
        "<details class=\"actions\"><summary>{count} {}</summary>",
        if count == 1 { "action" } else { "actions" }
    );
    for activity in activities.drain(..) {
        let label = match activity.kind {
            TranscriptBlockKind::Reasoning => "Reasoning",
            TranscriptBlockKind::Activity => "Action",
            TranscriptBlockKind::User
            | TranscriptBlockKind::Assistant
            | TranscriptBlockKind::Plan => continue,
        };
        let _ = writeln!(
            body,
            "<div class=\"action\"><div class=\"action-label\">{label}</div><pre>{}</pre></div>",
            escape_html(&activity.markdown)
        );
    }
    body.push_str("</details>\n");
}

fn render_safe_markdown(markdown: &str, output: &mut String) {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(tag) => push_start_tag(output, tag),
            Event::End(tag) => push_end_tag(output, tag),
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                output.push_str(&escape_html(&text));
            }
            Event::Code(code) => {
                let _ = write!(output, "<code>{}</code>", escape_html(&code));
            }
            Event::FootnoteReference(label) => {
                let _ = write!(output, "<sup>[{}]</sup>", escape_html(&label));
            }
            Event::SoftBreak => output.push('\n'),
            Event::HardBreak => output.push_str("<br>\n"),
            Event::Rule => output.push_str("<hr>\n"),
            Event::TaskListMarker(checked) => {
                output.push_str(if checked { "☒ " } else { "☐ " });
            }
        }
    }
}

fn push_start_tag(output: &mut String, tag: Tag<'_>) {
    match tag {
        Tag::Paragraph => output.push_str("<p>"),
        Tag::Heading { level, .. } => {
            let level = heading_number(level);
            let _ = write!(output, "<h{level}>");
        }
        Tag::BlockQuote => output.push_str("<blockquote>"),
        Tag::CodeBlock(_) => output.push_str("<pre><code>"),
        Tag::HtmlBlock | Tag::Link { .. } | Tag::Image { .. } | Tag::MetadataBlock(_) => {}
        Tag::List(Some(start)) => {
            let _ = write!(output, "<ol start=\"{start}\">");
        }
        Tag::List(None) => output.push_str("<ul>"),
        Tag::Item => output.push_str("<li>"),
        Tag::FootnoteDefinition(label) => {
            let _ = write!(
                output,
                "<aside class=\"footnote\"><sup>{}</sup> ",
                escape_html(&label)
            );
        }
        Tag::Table(_) => output.push_str("<table>"),
        Tag::TableHead => output.push_str("<thead>"),
        Tag::TableRow => output.push_str("<tr>"),
        Tag::TableCell => output.push_str("<td>"),
        Tag::Emphasis => output.push_str("<em>"),
        Tag::Strong => output.push_str("<strong>"),
        Tag::Strikethrough => output.push_str("<del>"),
    }
}

fn push_end_tag(output: &mut String, tag: TagEnd) {
    match tag {
        TagEnd::Paragraph => output.push_str("</p>\n"),
        TagEnd::Heading(level) => {
            let level = heading_number(level);
            let _ = writeln!(output, "</h{level}>");
        }
        TagEnd::BlockQuote => output.push_str("</blockquote>\n"),
        TagEnd::CodeBlock => output.push_str("</code></pre>\n"),
        TagEnd::HtmlBlock | TagEnd::Link | TagEnd::Image | TagEnd::MetadataBlock(_) => {}
        TagEnd::List(true) => output.push_str("</ol>\n"),
        TagEnd::List(false) => output.push_str("</ul>\n"),
        TagEnd::Item => output.push_str("</li>\n"),
        TagEnd::FootnoteDefinition => output.push_str("</aside>\n"),
        TagEnd::Table => output.push_str("</table>\n"),
        TagEnd::TableHead => output.push_str("</thead>"),
        TagEnd::TableRow => output.push_str("</tr>\n"),
        TagEnd::TableCell => output.push_str("</td>"),
        TagEnd::Emphasis => output.push_str("</em>"),
        TagEnd::Strong => output.push_str("</strong>"),
        TagEnd::Strikethrough => output.push_str("</del>"),
    }
}

fn heading_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn escape_html(text: &str) -> String {
    text.chars().fold(
        String::with_capacity(text.len()),
        |mut escaped, character| {
            match character {
                '&' => escaped.push_str("&amp;"),
                '<' => escaped.push_str("&lt;"),
                '>' => escaped.push_str("&gt;"),
                '"' => escaped.push_str("&quot;"),
                '\'' => escaped.push_str("&#39;"),
                _ => escaped.push(character),
            }
            escaped
        },
    )
}

#[cfg(test)]
#[path = "transcript_dump_tests.rs"]
mod tests;
