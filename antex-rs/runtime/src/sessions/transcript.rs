use super::*;

pub struct TranscriptPage {
    pub text: String,
    pub next_cursor: Option<Uuid>,
}

impl Session {
    pub fn transcript_page(&mut self, cursor: Option<Uuid>) -> io::Result<TranscriptPage> {
        let start = cursor.unwrap_or(self.head);
        let mut probe = Some(self.head);
        while probe.is_some_and(|id| id != start) {
            probe = self.records[&probe.unwrap()].parent;
        }
        if probe.is_none() {
            return Err(io::Error::other(
                "transcript cursor is not on the active branch",
            ));
        }
        let mut next = Some(start);
        let mut items = Vec::new();
        let mut bytes = 0;
        let mut read_bytes = 0;
        for _ in 0..4096 {
            let Some(id) = next else {
                break;
            };
            let index = &self.records[&id];
            if !matches!(
                index.kind,
                Kind::User | Kind::Assistant | Kind::ToolCall | Kind::ToolResult
            ) {
                next = index.parent;
                continue;
            }
            let (record, length) = self.read_record(id)?;
            let role = match record.kind {
                Kind::User => "You",
                Kind::Assistant => "Antex",
                Kind::ToolCall => "Tool call",
                Kind::ToolResult => "Tool result",
                _ => unreachable!(),
            };
            let mut content = match record.kind {
                Kind::ToolResult => record.payload["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                Kind::ToolCall => format!(
                    "```json\n{}\n```",
                    serde_json::to_string_pretty(&record.payload)?
                ),
                _ => String::new(),
            };
            if let Some(blocks) = record.payload["content"].as_array() {
                for block in blocks.iter().take(64) {
                    match block["type"].as_str() {
                        Some("text") => {
                            content.push_str(block["text"].as_str().unwrap_or_default())
                        }
                        Some("reasoning") => {
                            content.push_str("\n[Reasoning summary]\n");
                            content.push_str(block["text"].as_str().unwrap_or_default());
                        }
                        Some("image") => content.push_str("\n[Image attachment]\n"),
                        _ => {}
                    }
                }
            }
            if content.len() > 48 * 1024 {
                let mut end = 48 * 1024;
                while !content.is_char_boundary(end) {
                    end -= 1;
                }
                content.truncate(end);
                content.push_str(
                    "\n[Long message truncated in this view; original remains in the session]",
                );
            }
            let item = format!("## {role} · {id}\n\n{content}\n\n");
            if bytes + item.len() > 64 * 1024 && !items.is_empty() {
                break;
            }
            bytes += item.len();
            read_bytes += length;
            items.push(item);
            next = record.parent_id;
            if items.len() >= 64 || read_bytes >= 8 * 1024 * 1024 {
                break;
            }
        }
        items.reverse();
        Ok(TranscriptPage {
            text: items.concat(),
            next_cursor: next,
        })
    }
}
