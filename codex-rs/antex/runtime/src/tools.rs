use std::path::Path;
use std::path::PathBuf;

use antex_core::InteractionAnswer;
use antex_core::InteractionPrompt;
use antex_core::ToolCall;
use antex_core::ToolContext;
use antex_core::ToolDefinition;
use antex_core::ToolHost;
use antex_core::ToolOutcome;
use antex_core::ToolOutput;
use antex_core::ToolScope;
use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::json;

use crate::PermissionProfile;
use crate::Shell;
use crate::WorkspaceFiles;

pub struct LocalRuntime {
    workspace: PathBuf,
    profile: PermissionProfile,
    files: WorkspaceFiles,
    shell: Shell,
}

impl LocalRuntime {
    pub fn new(workspace: &Path, profile: PermissionProfile) -> std::io::Result<Self> {
        Ok(Self {
            workspace: workspace.canonicalize()?,
            profile,
            files: WorkspaceFiles::new(workspace, profile)?,
            shell: Shell::new(workspace, profile)?,
        })
    }

    pub fn with_bubblewrap(mut self, program: PathBuf) -> Self {
        self.shell = self.shell.with_bubblewrap(program);
        self
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    path: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    path: String,
    old_text: String,
    new_text: String,
}

#[derive(Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum ShellAccess {
    #[default]
    Sandboxed,
    RequestFull,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShellInput {
    command: String,
    #[serde(default)]
    access: ShellAccess,
}

impl ToolHost for LocalRuntime {
    fn definitions(&self, _scope: &ToolScope) -> Vec<ToolDefinition> {
        let mut definitions = vec![ToolDefinition {
            name: "read".into(),
            description:
                "Read a UTF-8 file with line numbers. Paths are relative to the workspace.".into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string","maxLength":4096},"offset":{"type":"integer","minimum":1},"limit":{"type":"integer","minimum":1,"maximum":1000}},"required":["path"],"additionalProperties":false}),
        }];
        if self.profile != PermissionProfile::ReadOnly {
            definitions.extend([
                ToolDefinition {name:"write".into(),description:"Atomically replace a file; its parent directory must already exist.".into(),parameters:json!({"type":"object","properties":{"path":{"type":"string","maxLength":4096},"content":{"type":"string"}},"required":["path","content"],"additionalProperties":false})},
                ToolDefinition {name:"edit".into(),description:"Replace exactly one occurrence of old_text, preserving all other bytes.".into(),parameters:json!({"type":"object","properties":{"path":{"type":"string","maxLength":4096},"old_text":{"type":"string","minLength":1},"new_text":{"type":"string"}},"required":["path","old_text","new_text"],"additionalProperties":false})},
            ]);
        }
        definitions.push(ToolDefinition {name:"shell".into(),description:"Run a POSIX shell command. Use shell for searching and listing. requestFull requires explicit approval outside the sandbox.".into(),parameters:json!({"type":"object","properties":{"command":{"type":"string","maxLength":7800},"access":{"enum":["sandboxed","requestFull"]}},"required":["command"],"additionalProperties":false})});
        definitions
    }

    fn execute(&self, call: ToolCall, context: ToolContext) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let result: Result<String, String> = async {
                if context.cancellation.is_cancelled() {
                    return Err("tool cancelled".into());
                }
                match call.name.as_str() {
                    "read" => {
                        let input: Read = serde_json::from_value(call.arguments)
                            .map_err(|error| error.to_string())?;
                        let bytes = self
                            .files
                            .read(&input.path)
                            .map_err(|error| error.to_string())?;
                        let text =
                            std::str::from_utf8(&bytes).map_err(|_| "read requires UTF-8 text")?;
                        let offset = input.offset.unwrap_or(1).max(1);
                        let limit = input.limit.unwrap_or(200).clamp(1, 1000);
                        let mut output = String::new();
                        for (index, line) in text.lines().enumerate().skip(offset - 1).take(limit) {
                            let number = index + 1;
                            output.push_str(&format!("{number}: {line}\n"));
                            if output.len() > antex_core::MAX_TEXT_BYTES {
                                break;
                            }
                        }
                        Ok(output)
                    }
                    "write" => {
                        let input: Write = serde_json::from_value(call.arguments)
                            .map_err(|error| error.to_string())?;
                        self.files
                            .write(&input.path, input.content.as_bytes())
                            .map_err(|error| error.to_string())?;
                        Ok(format!("Wrote {}", input.path))
                    }
                    "edit" => {
                        let input: Edit = serde_json::from_value(call.arguments)
                            .map_err(|error| error.to_string())?;
                        self.files
                            .edit(&input.path, &input.old_text, &input.new_text)
                            .map_err(|error| error.to_string())?;
                        Ok(format!("Edited {}", input.path))
                    }
                    "shell" => {
                        let input: ShellInput = serde_json::from_value(call.arguments)
                            .map_err(|error| error.to_string())?;
                        if self.profile == PermissionProfile::ReadOnly
                            && input.access == ShellAccess::RequestFull
                        {
                            return Err("read-only profile forbids unsandboxed execution".into());
                        }
                        let words = shlex::split(&input.command);
                        let force_push = words.as_ref().is_none_or(|words| {
                            words.iter().any(|word| word == "push")
                                && words.iter().any(|word| {
                                    word.starts_with("--force")
                                        || word == "--mirror"
                                        || word.starts_with('+')
                                        || (word.starts_with('-')
                                            && !word.starts_with("--")
                                            && word.contains('f'))
                                })
                        });
                        let opaque_full_script = self.profile == PermissionProfile::Full
                            && input.command.contains(['$', '`']);
                        let elevated = input.access == ShellAccess::RequestFull;
                        if elevated || force_push || opaque_full_script {
                            let action = format!(
                                "Approve this command once{}:\n{}",
                                if elevated { " outside the sandbox" } else { "" },
                                input.command
                            );
                            match context
                                .interact(InteractionPrompt::Approval { action })
                                .await
                                .map_err(|error| error.to_string())?
                            {
                                InteractionAnswer::AllowOnce => {}
                                InteractionAnswer::Deny | InteractionAnswer::Text(_) => {
                                    return Err("command denied".into());
                                }
                            }
                        }
                        let output = if elevated {
                            Shell::new(&self.workspace, PermissionProfile::Full)
                                .map_err(|error| error.to_string())?
                                .run(&input.command, context.cancellation.clone())
                                .await
                        } else {
                            self.shell
                                .run(&input.command, context.cancellation.clone())
                                .await
                        }
                        .map_err(|error| error.to_string())?;
                        let text = format!(
                            "exit: {:?}\n{}{}",
                            output.exit_code, output.stdout, output.stderr
                        );
                        if output.exit_code == Some(0) {
                            Ok(text)
                        } else {
                            Err(text)
                        }
                    }
                    _ => Err("tool is not registered".into()),
                }
            }
            .await;
            match result {
                Ok(text) => ToolOutput::new(call.id, ToolOutcome::Success, text),
                Err(text) => ToolOutput::new(
                    call.id,
                    if context.cancellation.is_cancelled() {
                        ToolOutcome::Cancelled
                    } else {
                        ToolOutcome::Failure
                    },
                    text,
                ),
            }
        })
    }
}
