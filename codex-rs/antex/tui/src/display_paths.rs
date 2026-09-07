use std::path::Path;
use std::path::PathBuf;

pub(crate) fn get_git_repo_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(Path::to_path_buf)
}

pub(crate) fn relativize_to_home(path: &Path) -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    path.strip_prefix(home).ok().map(Path::to_path_buf)
}
