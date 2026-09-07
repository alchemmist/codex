use std::path::PathBuf;

pub enum ImageSource {
    File(PathBuf),
    Rgba {
        width: u32,
        height: u32,
        bytes: Vec<u8>,
    },
}

pub fn parse_image_path(text: &str) -> Option<PathBuf> {
    let text = text.trim();
    if text.len() > 4096 || text.contains(['\n', '\r', '\0']) {
        return None;
    }
    let mut parts = shlex::split(text)?;
    if parts.len() != 1 {
        return None;
    }
    let path = parts.pop()?;
    if path.starts_with("file://") {
        return url::Url::parse(&path).ok()?.to_file_path().ok();
    }
    if let Some(relative) = path.strip_prefix("~/") {
        return Some(PathBuf::from(std::env::var_os("HOME")?).join(relative));
    }
    Some(path.into())
}

pub(crate) fn pasted_image_path(text: &str) -> Option<PathBuf> {
    let path = parse_image_path(text)?;
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp").then_some(path)
}

pub(crate) fn image_source() -> Result<ImageSource, String> {
    if std::env::var_os("SSH_TTY").is_some() || std::env::var_os("SSH_CONNECTION").is_some() {
        return Err("Image paste over SSH requires a transferred file; use /image <path>.".into());
    }
    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| format!("Clipboard unavailable: {error}"))?;
    if let Ok(files) = clipboard.get().file_list()
        && let Some(path) = files.into_iter().take(16).find(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(
                        extension.to_ascii_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "gif" | "webp"
                    )
                })
        })
    {
        return Ok(ImageSource::File(path));
    }
    let image = clipboard
        .get_image()
        .map_err(|error| format!("No image on clipboard: {error}"))?;
    let width = u32::try_from(image.width).map_err(|_| "Clipboard image is too wide.")?;
    let height = u32::try_from(image.height).map_err(|_| "Clipboard image is too tall.")?;
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || image.bytes.len() > 128 * 1024 * 1024
        || u64::from(width) * u64::from(height) * 4 != image.bytes.len() as u64
    {
        return Err("Clipboard image exceeds supported dimensions or byte limits.".into());
    }
    Ok(ImageSource::Rgba {
        width,
        height,
        bytes: image.bytes.into_owned(),
    })
}

pub(crate) fn is_probably_wsl() -> bool {
    std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::env::var_os("WSL_INTEROP").is_some()
        || std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .is_ok_and(|release| release.to_ascii_lowercase().contains("microsoft"))
}

#[cfg(test)]
#[path = "clipboard_path_tests.rs"]
mod tests;
