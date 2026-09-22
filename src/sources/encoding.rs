//! Encoding resolution and runtime defaults (design E-5).
//!
//! Priority, highest first:
//!   1. `--encoding` CLI flag
//!   2. encoding recorded on the source's last observation
//!   3. exact-file project config
//!   4. project default
//!   5. user-global default
//!   6. UTF-8

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::sources::SourceError;

/// Inputs to resolution. `None` means "this layer didn't specify".
#[derive(Debug, Default)]
pub struct EncodingChoice {
    pub cli: Option<String>,
    pub recorded: Option<String>,
    pub file_config: Option<String>,
    pub project_default: Option<String>,
    pub user_default: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct EncodingDefaults {
    pub file: Option<String>,
    pub project: Option<String>,
    pub user: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EncodingConfig {
    format: String,
    #[serde(default)]
    default_encoding: Option<String>,
    #[serde(default)]
    file: BTreeMap<String, FileEncoding>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileEncoding {
    encoding: String,
}

fn read_config(path: &Path) -> Result<Option<EncodingConfig>, SourceError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(SourceError::Io(error)),
    };
    let config: EncodingConfig = toml::from_str(&text).map_err(|error| {
        SourceError::Invalid(format!(
            "encoding configuration is invalid at {}: {error}",
            path.display()
        ))
    })?;
    if config.format != "omd.encoding/1" {
        return Err(SourceError::Invalid(format!(
            "unsupported encoding configuration format '{}' at {}",
            config.format,
            path.display()
        )));
    }
    Ok(Some(config))
}

/// Load optional exact-file, project, and user defaults without executing a
/// source. Project configuration lives in `<metadata-root>/omd.toml`; user
/// configuration lives in `<config-root>/omd.toml`.
pub fn load_defaults(
    config_cwd: &Path,
    metadata_root: &Path,
    logical_path: &str,
) -> Result<EncodingDefaults, SourceError> {
    let project = read_config(&metadata_root.join("omd.toml"))?;
    let user = read_config(&crate::sources::discovery::config_dir(config_cwd).join("omd.toml"))?;
    Ok(EncodingDefaults {
        file: project
            .as_ref()
            .and_then(|config| config.file.get(logical_path))
            .map(|config| config.encoding.clone()),
        project: project.and_then(|config| config.default_encoding),
        user: user.and_then(|config| config.default_encoding),
    })
}

/// Resolve and validate the runtime view selected for one tracked location.
/// Configuration is read before provider execution; malformed or applicable
/// unknown encodings never fall through to a lower-priority default.
pub fn resolve_runtime(
    cli: Option<&str>,
    recorded: Option<&str>,
    config_cwd: &Path,
    metadata_root: &Path,
    logical_path: &str,
) -> Result<String, SourceError> {
    let defaults = load_defaults(config_cwd, metadata_root, logical_path)?;
    let selected = resolve(&EncodingChoice {
        cli: cli.map(str::to_string),
        recorded: recorded.map(str::to_string),
        file_config: defaults.file,
        project_default: defaults.project,
        user_default: defaults.user,
    });
    validate(&selected)?;
    Ok(selected)
}

/// Reject unknown WHATWG labels before any source execution or publication.
pub fn validate(label: &str) -> Result<(), SourceError> {
    encoding_rs::Encoding::for_label(label.trim().as_bytes())
        .map(|_| ())
        .ok_or(SourceError::Encoding)
}

/// Resolve the effective encoding. Always returns a concrete name —
/// UTF-8 is the floor when nothing else chose.
pub fn resolve(c: &EncodingChoice) -> String {
    c.cli
        .as_ref()
        .or(c.recorded.as_ref())
        .or(c.file_config.as_ref())
        .or(c.project_default.as_ref())
        .or(c.user_default.as_ref())
        .cloned()
        .unwrap_or_else(|| "utf-8".to_string())
}
