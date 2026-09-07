use std::io;

pub(crate) fn replace_unique(source: &str, old: &str, new: &str) -> io::Result<String> {
    if old.is_empty() {
        return Err(io::Error::other("edit requires nonempty original text"));
    }
    let normalized = normalize(source);
    let old = normalize(old);
    let start = normalized
        .find(&old)
        .ok_or_else(|| io::Error::other("original text was not found"))?;
    if normalized.rfind(&old) != Some(start) {
        return Err(io::Error::other(
            "original text is ambiguous; include more context",
        ));
    }
    let end = source_offset(source, start + old.len());
    let start = source_offset(source, start);
    let ending = source
        .bytes()
        .position(|byte| matches!(byte, b'\r' | b'\n'))
        .map(|index| {
            if source.as_bytes()[index] == b'\n' {
                "\n"
            } else if source.as_bytes().get(index + 1) == Some(&b'\n') {
                "\r\n"
            } else {
                "\r"
            }
        })
        .unwrap_or("\n");
    let new = normalize(new).replace('\n', ending);
    let mut result = String::with_capacity(source.len() + new.len());
    result.push_str(&source[..start]);
    result.push_str(&new);
    result.push_str(&source[end..]);
    Ok(result)
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn source_offset(source: &str, normalized_offset: usize) -> usize {
    let mut offset = 0;
    let mut normalized = 0;
    while normalized < normalized_offset {
        offset += if source.as_bytes()[offset] == b'\r'
            && source.as_bytes().get(offset + 1) == Some(&b'\n')
        {
            2
        } else {
            1
        };
        normalized += 1;
    }
    offset
}
