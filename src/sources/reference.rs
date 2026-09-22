//! Closed source fields shared by CLI, source versions, and recovery bindings.
//!
//! No compound source string or URI parser exists here. Paths remain literal;
//! provider selection comes only from the explicit `file|command|git` kind.

use serde::{Deserialize, Serialize};

use super::SourceError;

/// One closed source descriptor. Machine-local roots are resolved from the
/// logical project alias at collection time and never persisted here.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceDescriptor {
    File {
        project: String,
        path: String,
    },
    Command {
        executable: String,
        args: Vec<String>,
    },
    Git {
        project: String,
        commit: String,
        path: String,
    },
}

impl SourceDescriptor {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::File { .. } => "file",
            Self::Command { .. } => "command",
            Self::Git { .. } => "git",
        }
    }

    pub fn validate(&self) -> Result<(), SourceError> {
        match self {
            Self::File { project, path } => {
                if project.is_empty() || path.is_empty() || path.contains('\0') {
                    return Err(SourceError::Invalid(
                        "file source requires non-empty project and path".into(),
                    ));
                }
            }
            Self::Command { executable, args } => {
                if executable.is_empty()
                    || executable.contains('\0')
                    || args.iter().any(|arg| arg.contains('\0'))
                {
                    return Err(SourceError::Invalid(
                        "command source executable/args contain an invalid empty or NUL value"
                            .into(),
                    ));
                }
            }
            Self::Git {
                project,
                commit,
                path,
            } => {
                if project.is_empty()
                    || path.is_empty()
                    || path.contains('\0')
                    || !matches!(commit.len(), 40 | 64)
                    || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(SourceError::Invalid(
                        "Git source requires project, literal path, and complete object id".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn validate_portable(&self) -> Result<(), SourceError> {
        self.validate()?;
        match self {
            Self::File { path, .. } => validate_shared_path(path, "file"),
            Self::Git { path, .. } => validate_shared_path(path, "Git"),
            Self::Command { .. } => Ok(()),
        }
    }
}

fn validate_shared_path(path: &str, kind: &str) -> Result<(), SourceError> {
    let value = std::path::Path::new(path);
    if value.is_absolute()
        || value.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::CurDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(SourceError::Invalid(format!(
            "{kind} source path must be normalized and project-relative"
        )));
    }
    Ok(())
}

/// Raw independent CLI fields. Validation closes incompatible combinations
/// before collection or metadata publication.
#[derive(Debug, Clone, Default)]
pub struct SourceFields {
    pub source_type: Option<String>,
    pub source_project: Option<String>,
    pub source_path: Option<String>,
    pub executable: Option<String>,
    pub args_json: Option<String>,
    pub git_commit: Option<String>,
    pub git_path: Option<String>,
}

impl SourceFields {
    pub fn any(&self) -> bool {
        self.source_type.is_some()
            || self.source_project.is_some()
            || self.source_path.is_some()
            || self.executable.is_some()
            || self.args_json.is_some()
            || self.git_commit.is_some()
            || self.git_path.is_some()
    }

    /// Validate fields into the one persisted descriptor.
    ///
    /// `default_file_path` makes a new source default to `file root:<path>`.
    /// `require_type` is used by replace: recovery replacement is always
    /// explicit and never guessed from a lone path.
    pub fn descriptor(
        &self,
        default_file_path: Option<&str>,
        require_type: bool,
    ) -> Result<Option<SourceDescriptor>, SourceError> {
        if !self.any() {
            if require_type {
                return Err(SourceError::Invalid(
                    "--source-type file|command|git is required".into(),
                ));
            }
            let Some(path) = default_file_path else {
                return Ok(None);
            };
            let descriptor = SourceDescriptor::File {
                project: "root".into(),
                path: path.to_string(),
            };
            descriptor.validate()?;
            return Ok(Some(descriptor));
        }
        if require_type && self.source_type.is_none() {
            return Err(SourceError::Invalid(
                "--source-type file|command|git is required".into(),
            ));
        }
        let kind = self.source_type.as_deref().unwrap_or("file");
        match kind {
            "file" => {
                self.reject_command_fields("file")?;
                self.reject_git_fields("file")?;
                let path = self
                    .source_path
                    .as_deref()
                    .or(default_file_path)
                    .ok_or_else(|| {
                        SourceError::Invalid("file source requires --source-path".into())
                    })?;
                if path.is_empty() {
                    return Err(SourceError::Invalid(
                        "file source path must not be empty".into(),
                    ));
                }
                let descriptor = SourceDescriptor::File {
                    project: self.source_project.clone().unwrap_or_else(|| "root".into()),
                    path: path.to_string(),
                };
                descriptor.validate()?;
                Ok(Some(descriptor))
            }
            "command" => {
                if self.source_project.is_some() || self.source_path.is_some() {
                    return Err(SourceError::Invalid(
                        "command source does not accept --source-project or --source-path".into(),
                    ));
                }
                self.reject_git_fields("command")?;
                let executable = self
                    .executable
                    .clone()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        SourceError::Invalid("command source requires --executable".into())
                    })?;
                let raw = self.args_json.as_deref().ok_or_else(|| {
                    SourceError::Invalid("command source requires --args-json".into())
                })?;
                let args = super::command::parse_args_json(raw)?;
                let descriptor = SourceDescriptor::Command { executable, args };
                descriptor.validate()?;
                Ok(Some(descriptor))
            }
            "git" => {
                if self.source_path.is_some() {
                    return Err(SourceError::Invalid(
                        "git source uses --git-path, not --source-path".into(),
                    ));
                }
                self.reject_command_fields("git")?;
                let project = self.source_project.clone().unwrap_or_else(|| "root".into());
                let commit = self.git_commit.clone().ok_or_else(|| {
                    SourceError::Invalid("git source requires --git-commit".into())
                })?;
                if !matches!(commit.len(), 40 | 64)
                    || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(SourceError::Invalid(
                        "--git-commit must be a complete 40- or 64-hex object id".into(),
                    ));
                }
                let path = self
                    .git_path
                    .clone()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| SourceError::Invalid("git source requires --git-path".into()))?;
                let descriptor = SourceDescriptor::Git {
                    project,
                    commit,
                    path,
                };
                descriptor.validate()?;
                Ok(Some(descriptor))
            }
            other => Err(SourceError::Invalid(format!(
                "unknown --source-type: {other}"
            ))),
        }
    }

    fn reject_command_fields(&self, kind: &str) -> Result<(), SourceError> {
        if self.executable.is_some() || self.args_json.is_some() {
            return Err(SourceError::Invalid(format!(
                "{kind} source does not accept --executable or --args-json"
            )));
        }
        Ok(())
    }

    fn reject_git_fields(&self, kind: &str) -> Result<(), SourceError> {
        if self.git_commit.is_some() || self.git_path.is_some() {
            return Err(SourceError::Invalid(format!(
                "{kind} source does not accept --git-commit or --git-path"
            )));
        }
        Ok(())
    }
}
