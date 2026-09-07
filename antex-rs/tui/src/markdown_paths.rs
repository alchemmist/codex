use url::Url;

pub(crate) fn file_uri_path(uri: &str) -> Option<String> {
    let url = Url::parse(uri).ok()?;
    if url.scheme() != "file" || url.query().is_some() || url.fragment().is_some() {
        return None;
    }
    let path = urlencoding::decode(url.path()).ok()?;
    if path.contains('\0') {
        return None;
    }
    if let Some(host) = url.host_str().filter(|host| *host != "localhost") {
        return Some(format!("//{host}{path}"));
    }
    if matches!(path.as_bytes(), [b'/', drive, b':', b'/', ..] if drive.is_ascii_alphabetic()) {
        return Some(path[1..].to_owned());
    }
    Some(path.into_owned())
}

pub(crate) fn normalize_markdown_hash_location_suffix(suffix: &str) -> Option<String> {
    let fragment = suffix.strip_prefix('#')?;
    let (start, end) = match fragment.split_once('-') {
        Some((start, end)) => (start, Some(end)),
        None => (fragment, None),
    };
    let point = |point: &str| -> Option<String> {
        let point = point.strip_prefix('L')?;
        let (line, column) = match point.split_once('C') {
            Some((line, column)) => (line, Some(column)),
            None => (point, None),
        };
        Some(match column {
            Some(column) => format!("{line}:{column}"),
            None => line.into(),
        })
    };
    let mut normalized = format!(":{}", point(start)?);
    if let Some(end) = end {
        normalized.push_str(&format!("-{}", point(end)?));
    }
    Some(normalized)
}
