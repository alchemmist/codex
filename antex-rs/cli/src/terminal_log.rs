use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use uuid::Uuid;

const MAX_LOG_BYTES: usize = 8 * 1024 * 1024;

pub(crate) struct TmuxLog {
    path: PathBuf,
    file: File,
    pane: String,
    program: PathBuf,
    bytes: usize,
}

impl TmuxLog {
    pub(crate) fn start(home: &Path, session_id: Uuid) -> io::Result<Self> {
        Self::start_with_program(home, session_id, Path::new("tmux"))
    }

    fn start_with_program(home: &Path, session_id: Uuid, program: &Path) -> io::Result<Self> {
        let directory = home.join("tmp");
        fs::create_dir_all(&directory)?;
        let path = directory.join(format!("antex-tmux-{session_id}-{}.log", Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path)?;
        let path_text = path
            .to_str()
            .ok_or_else(|| io::Error::other("tmux log path is not UTF-8"))?;
        let viewer = format!("exec tail -f -- {}", shell_quote(path_text));
        let output = Command::new(program)
            .args([
                "new-window",
                "-d",
                "-P",
                "-F",
                "#{pane_id}",
                "-n",
                "antex-log",
                &viewer,
            ])
            .output()?;
        if !output.status.success() {
            let _ = fs::remove_file(&path);
            return Err(io::Error::other(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        let pane = String::from_utf8(output.stdout)
            .map_err(|_| io::Error::other("tmux returned a non-UTF-8 pane identifier"))?
            .trim()
            .to_owned();
        if !pane.starts_with('%') || pane.len() > 64 {
            let _ = fs::remove_file(&path);
            return Err(io::Error::other("tmux returned an invalid pane identifier"));
        }
        Ok(Self {
            path,
            file,
            pane,
            program: program.to_owned(),
            bytes: 0,
        })
    }

    pub(crate) fn append(&mut self, text: &str) -> io::Result<()> {
        let bytes = text.as_bytes();
        let added = bytes.len().saturating_add(1);
        if self.bytes.saturating_add(added) > MAX_LOG_BYTES {
            return Err(io::Error::other("tmux command log reached its 8 MiB limit"));
        }
        self.file.write_all(bytes)?;
        self.file.write_all(b"\n")?;
        self.file.flush()?;
        self.bytes += added;
        Ok(())
    }
}

impl Drop for TmuxLog {
    fn drop(&mut self) {
        let _ = Command::new(&self.program)
            .args(["kill-window", "-t", &self.pane])
            .status();
        let _ = fs::remove_file(&self.path);
    }
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[cfg(test)]
#[path = "terminal_log_tests.rs"]
mod tests;
