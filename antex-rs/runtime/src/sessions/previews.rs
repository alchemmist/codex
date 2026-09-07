use super::*;

#[derive(Debug, PartialEq, Eq)]
pub struct SessionPreview {
    pub id: Uuid,
    pub text: String,
}

impl SessionStore {
    pub fn previews(&self, limit: usize) -> io::Result<Vec<SessionPreview>> {
        let ids = self.list()?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let directory =
            Dir::open_ambient_dir(&self.home, ambient_authority())?.open_dir(&self.relative)?;
        let mut previews = Vec::new();
        for id in ids.into_iter().take(limit.min(256)) {
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use cap_std::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            let Ok(file) = directory.open_with(format!("{id}.jsonl"), &options) else {
                continue;
            };
            let file = file.into_std();
            let metadata = file.metadata()?;
            if !metadata.is_file() {
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.mode() & 0o077 != 0 || metadata.uid() != unsafe { libc::geteuid() } {
                    continue;
                }
            }
            let mut bytes = Vec::new();
            file.take(64 * 1024).read_to_end(&mut bytes)?;
            let mut text = "No short preview available".to_owned();
            for line in bytes.split(|byte| *byte == b'\n').take(64) {
                let Ok(record) = serde_json::from_slice::<Record>(line) else {
                    break;
                };
                if record.schema_version != 1 || record.session_id != id {
                    break;
                }
                if record.kind == Kind::User {
                    if let Ok(Message::User(input)) = session_codec::decode(&record.payload) {
                        let preview = input
                            .content
                            .iter()
                            .filter_map(|content| match content {
                                antex_core::Content::Text(text) => Some(text.as_str()),
                                _ => None,
                            })
                            .flat_map(str::chars)
                            .take(120)
                            .collect::<String>();
                        if !preview.is_empty() {
                            text = preview.split_whitespace().collect::<Vec<_>>().join(" ");
                        }
                    }
                    break;
                }
            }
            previews.push(SessionPreview { id, text });
        }
        Ok(previews)
    }
}
