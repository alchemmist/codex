pub(crate) use codex_utils_string::normalize_markdown_hash_location_suffix;

pub(crate) fn file_uri_path(uri: &str) -> Option<String> {
    codex_utils_path_uri::PathUri::parse(uri)
        .ok()
        .map(|path| path.inferred_native_path_string())
}
