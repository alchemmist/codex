use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VimModeStart {
    #[default]
    Normal,
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextElement {
    pub byte_range: ByteRange,
    placeholder: Option<String>,
}

impl TextElement {
    pub fn new(byte_range: ByteRange, placeholder: Option<String>) -> Self {
        Self {
            byte_range,
            placeholder,
        }
    }

    pub fn placeholder<'a>(&'a self, text: &'a str) -> Option<&'a str> {
        self.placeholder
            .as_deref()
            .or_else(|| text.get(self.byte_range.start..self.byte_range.end))
    }
}

#[cfg(test)]
pub(crate) const MAX_USER_INPUT_TEXT_CHARS: usize = 1 << 20;
