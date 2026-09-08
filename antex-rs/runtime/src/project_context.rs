use std::io;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

use antex_core::ContextFragment;
use antex_core::ContextHook;
use antex_core::ContextKind;
use antex_core::ContextualUserFragment;
use antex_core::MAX_TEXT_BYTES;
use antex_core::Message;
use antex_core::ProviderError;
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use cap_std::fs::OpenOptions;
use futures::future::BoxFuture;

const MAX_CONTEXT_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub struct ProjectContext {
    fragments: Vec<ContextFragment>,
    pub read_roots: Vec<PathBuf>,
    pub warnings: Vec<String>,
    bytes: usize,
}

impl ProjectContext {
    pub fn load(home: &Path, workspace: &Path) -> io::Result<Self> {
        let home = home.canonicalize()?;
        let workspace = workspace.canonicalize()?;
        let home_directory = Dir::open_ambient_dir(&home, ambient_authority())?;
        let workspace_directory = Dir::open_ambient_dir(&workspace, ambient_authority())?;
        let mut context = Self {
            fragments: Vec::new(),
            read_roots: Vec::new(),
            warnings: Vec::new(),
            bytes: 0,
        };
        context.append(
            ContextKind::System,
            "Antex",
            include_str!("system_prompt.md"),
        )?;
        context.append(ContextKind::System, "Execution environment", &format!(
            "Current working directory: {}\nResolve relative file paths against this directory.",
            serde_json::to_string(&workspace)?
        ))?;
        if let Some(text) = read_file(&home_directory, Path::new("AGENTS.md"))? {
            context.append(ContextKind::Project, "Global AGENTS.md", &text)?;
        }
        let mut ancestors = Vec::new();
        for directory in workspace.ancestors().take(32) {
            ancestors.push(directory);
            if directory.join(".git").exists() {
                break;
            }
        }
        if ancestors.len() == 32
            && ancestors.last().is_some_and(|directory| {
                directory.parent().is_some() && !directory.join(".git").exists()
            })
        {
            return Err(io::Error::other(
                "workspace ancestry exceeds its context discovery budget",
            ));
        }
        for directory in ancestors.into_iter().rev() {
            let root = Dir::open_ambient_dir(directory, ambient_authority())?;
            if let Some(text) = read_file(&root, Path::new("AGENTS.md"))? {
                context.append(
                    ContextKind::Project,
                    &format!(
                        "Project instructions: {}",
                        directory.join("AGENTS.md").display()
                    ),
                    &text,
                )?;
            }
        }
        let mut catalog = String::new();
        for (root, relative, absolute) in [
            (&home_directory, Path::new("skills"), home.join("skills")),
            (
                &workspace_directory,
                Path::new(".antex/skills"),
                workspace.join(".antex/skills"),
            ),
        ] {
            let metadata = match root.symlink_metadata(relative) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            if !metadata.is_dir() {
                context
                    .warnings
                    .push("ignored non-directory or symlinked skills root".into());
                continue;
            }
            let skills = root.open_dir(relative)?;
            if !absolute.starts_with(&workspace) {
                context.read_roots.push(absolute.canonicalize()?);
            }
            let mut entries = skills.entries()?.take(65).collect::<io::Result<Vec<_>>>()?;
            entries.sort_by_key(cap_std::fs::DirEntry::file_name);
            if entries.len() > 64 {
                context
                    .warnings
                    .push("skill catalog is limited to 64 entries per root".into());
            }
            for entry in entries.into_iter().take(64) {
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let name = entry.file_name();
                let Some(name) = name.to_str().filter(|name| name.len() <= 64) else {
                    continue;
                };
                let path = Path::new(name).join("SKILL.md");
                let Some(text) = read_file(&skills, &path)? else {
                    continue;
                };
                let mut lines = text.lines();
                let mut description = String::new();
                while let Some(line) = lines.next() {
                    if let Some(value) = line.strip_prefix("description:") {
                        let value = value.trim().trim_matches(['\'', '"']);
                        if matches!(value, ">" | "|" | ">-" | "|-") {
                            for line in lines {
                                if !line.starts_with([' ', '\t']) {
                                    break;
                                }
                                description.push_str(line.trim());
                                description.push(' ');
                                if description.len() > 640 {
                                    break;
                                }
                            }
                        } else {
                            description.push_str(value);
                        }
                        break;
                    }
                }
                let description = description.trim().chars().take(160).collect::<String>();
                let line = format!(
                    "- {name}: {description} ({})\n",
                    absolute.join(path).display()
                );
                if catalog.len() + line.len() > MAX_TEXT_BYTES - 128 {
                    context
                        .warnings
                        .push("additional skill metadata omitted from the bounded catalog".into());
                    break;
                }
                catalog.push_str(&line);
            }
        }
        if !catalog.is_empty() {
            context.append(
                ContextKind::Skill,
                "Available skills — read SKILL.md before acting",
                &catalog,
            )?;
        }
        Ok(context)
    }

    fn append(&mut self, kind: ContextKind, source: &str, mut text: &str) -> io::Result<()> {
        let prefix = format!("{source}\n");
        let limit = MAX_TEXT_BYTES
            .checked_sub(prefix.len())
            .filter(|limit| *limit > 0)
            .ok_or_else(|| io::Error::other("instruction source name is too long"))?;
        while !text.is_empty() {
            let mut end = limit.min(text.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            if end == 0 {
                return Err(io::Error::other(
                    "instruction fragment cannot fit its byte budget",
                ));
            }
            let fragment = format!("{prefix}{}", &text[..end]);
            self.bytes += fragment.len();
            if self.bytes > MAX_CONTEXT_BYTES {
                return Err(io::Error::other(
                    "project instructions exceed the total context budget",
                ));
            }
            self.fragments
                .push(ContextFragment::new(kind, fragment).map_err(io::Error::other)?);
            text = &text[end..];
        }
        Ok(())
    }
}

impl ContextHook for ProjectContext {
    fn prepare<'a>(
        &'a self,
        history: &'a [Message],
    ) -> BoxFuture<'a, Result<antex_core::PreparedContext, ProviderError>> {
        Box::pin(async move {
            Ok(self
                .fragments
                .iter()
                .map(ContextualUserFragment::to_message)
                .chain(history.iter().cloned())
                .collect::<Vec<_>>()
                .into())
        })
    }
}

fn read_file(root: &Dir, path: &Path) -> io::Result<Option<String>> {
    let metadata = match root.symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() {
        return Err(io::Error::other(
            "instruction source must be a regular file, not a symlink",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = root.open_with(path, &options)?;
    let mut text = String::new();
    file.take((MAX_CONTEXT_BYTES + 1) as u64)
        .read_to_string(&mut text)?;
    if text.len() > MAX_CONTEXT_BYTES {
        return Err(io::Error::other(
            "instruction source exceeds its byte budget",
        ));
    }
    Ok(Some(text))
}
