use super::*;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedDraft {
    version: u32,
    text: String,
    elements: Vec<TextElement>,
    images: Vec<SavedImage>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedImage {
    marker: String,
    media_type: String,
    data: String,
}

impl Draft {
    fn encode(&self) -> Result<Value, String> {
        let images = self
            .images
            .iter()
            .map(|(marker, content)| match content {
                Content::Image { media_type, data } => Ok(
                    json!({"marker":marker,"media_type":media_type,"data":STANDARD.encode(data)}),
                ),
                Content::Text(_) | Content::Reasoning(_) | Content::Continuation { .. } => {
                    Err("invalid stash attachment".to_string())
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let value = json!({"version":1,"text":self.text,"elements":self.elements,"images":images});
        if value.to_string().len() > MAX_STASH_BYTES {
            return Err("Prompt stash is too large to persist safely.".into());
        }
        Ok(value)
    }

    fn decode(value: Value) -> Result<Self, String> {
        if value.to_string().len() > MAX_STASH_BYTES {
            return Err("Prompt stash exceeds its byte budget.".into());
        }
        let saved: SavedDraft =
            serde_json::from_value(value).map_err(|_| "Invalid prompt stash format.")?;
        if saved.version != 1
            || saved.text.len() > MAX_DRAFT_BYTES
            || saved.elements.len() > 64
            || saved.images.len() > 4
        {
            return Err("Invalid prompt stash limits.".into());
        }
        let mut end = 0;
        for element in &saved.elements {
            let range = element.byte_range;
            if range.start < end
                || range.start >= range.end
                || saved.text.get(range.start..range.end).is_none()
            {
                return Err("Invalid prompt stash element range.".into());
            }
            end = range.end;
        }
        let mut images = Vec::new();
        for image in saved.images {
            let number = image
                .marker
                .strip_prefix("[Image ")
                .and_then(|marker| marker.strip_suffix(']'))
                .and_then(|number| number.parse::<u64>().ok())
                .filter(|number| *number > 0 && *number < u64::MAX)
                .ok_or("Invalid stash image marker.")?;
            if image.marker != format!("[Image {number}]")
                || images.iter().any(|(marker, _)| marker == &image.marker)
                || !matches!(
                    image.media_type.as_str(),
                    "image/png" | "image/jpeg" | "image/gif" | "image/webp"
                )
                || image.data.len() > 4 * 1024 * 1024
                || !saved.elements.iter().any(|element| {
                    saved
                        .text
                        .get(element.byte_range.start..element.byte_range.end)
                        == Some(image.marker.as_str())
                        && element.placeholder(&saved.text) == Some(image.marker.as_str())
                })
            {
                return Err("Invalid stash image metadata.".into());
            }
            let data = STANDARD
                .decode(image.data)
                .map_err(|_| "Invalid stash image encoding.")?;
            if data.len() > 3 * 1024 * 1024 {
                return Err("Stash image exceeds its byte budget.".into());
            }
            images.push((
                image.marker,
                Content::Image {
                    media_type: image.media_type,
                    data: data.into(),
                },
            ));
        }
        Ok(Self {
            text: saved.text,
            elements: saved.elements,
            images,
        })
    }
}

impl Composer {
    pub(crate) fn load_stash(&mut self, value: Option<Value>) -> Result<(), String> {
        let stash = value
            .filter(|value| !value.is_null())
            .map(Draft::decode)
            .transpose()?;
        if let Some(stash) = &stash {
            for (marker, _) in &stash.images {
                let number = marker
                    .trim_start_matches("[Image ")
                    .trim_end_matches(']')
                    .parse::<u64>()
                    .map_err(|_| "Invalid stash image marker.")?;
                self.next_image = self.next_image.max(number + 1);
            }
        }
        self.stashed = stash;
        Ok(())
    }

    pub(crate) fn toggle_stash(
        &mut self,
        persist: impl FnOnce(&Value) -> Result<(), String>,
    ) -> Result<(), String> {
        let current = self.draft();
        if let Some(stashed) = &self.stashed {
            if current.text.len() + stashed.text.len() > MAX_DRAFT_BYTES
                || current.images.len() + stashed.images.len() > 4
                || current.elements.len() + stashed.elements.len() > 64
                || current.size() + stashed.size() > MAX_STASH_BYTES
            {
                return Err("Restoring the stash would exceed the draft limits; shorten the current draft first.".into());
            }
            let mut merged = current;
            let offset = merged.text.len();
            merged.text.push_str(&stashed.text);
            merged
                .elements
                .extend(stashed.elements.iter().map(|element| {
                    TextElement::new(
                        crate::editor_types::ByteRange {
                            start: element.byte_range.start + offset,
                            end: element.byte_range.end + offset,
                        },
                        element.placeholder(&stashed.text).map(str::to_owned),
                    )
                }));
            merged.images.extend(stashed.images.iter().cloned());
            persist(&Value::Null)?;
            self.editor
                .set_text_with_elements(&merged.text, &merged.elements);
            self.images = merged.images;
            self.stashed = None;
            self.state = TextAreaState::default();
        } else if !current.text.is_empty() {
            persist(&current.encode()?)?;
            self.stashed = Some(current);
            self.accept_submission();
        }
        Ok(())
    }

    pub(crate) fn has_stash(&self) -> bool {
        self.stashed.is_some()
    }
}
