use adk_core::{FileChange, PullInput, PushInput, pull_from_filesystem, push_from_filesystem};
use adk_io::{FileSystem, MemoryFileSystem};
use chrono::{DateTime, Utc};
use napi::bindgen_prelude::Uint8Array;
use napi::{Error, Result, Status};
use napi_derive::napi;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

type FileMap = BTreeMap<String, String>;

#[napi(object)]
pub struct NapiPullInput {
    pub root: String,
    pub files: FileMap,
    pub pull_projection_json: String,
    pub base_projection_json: Option<String>,
    pub force: Option<bool>,
}

#[napi(object)]
pub struct NapiFileChange {
    pub kind: String,
    pub path: String,
    pub content: Option<String>,
}

#[napi(object)]
pub struct NapiPullOutput {
    pub files: FileMap,
    pub changes: Vec<NapiFileChange>,
    pub conflicts: Vec<String>,
}

#[napi(object)]
pub struct NapiPushInput {
    pub root: String,
    pub files: FileMap,
    pub projection_json: String,
    pub last_known_sequence: i64,
    pub created_by: Option<String>,
    pub current_time: Option<String>,
    pub force: Option<bool>,
    pub skip_validation: Option<bool>,
}

#[napi(object)]
pub struct NapiPushOutput {
    pub success: bool,
    pub message: Option<String>,
    pub command_batch_bytes: Option<Uint8Array>,
}

#[napi]
pub fn pull(input: NapiPullInput) -> Result<NapiPullOutput> {
    let root = parse_root(&input.root)?;
    let fs = memory_filesystem_from_file_map(&root, input.files)?;
    let pull_projection = parse_projection(&input.pull_projection_json)?;
    let base_projection = input
        .base_projection_json
        .as_deref()
        .map(parse_projection)
        .transpose()?;
    let output = pull_from_filesystem(
        &fs,
        &root,
        PullInput {
            pull_projection,
            base_projection,
            force: input.force.unwrap_or(false),
        },
    )
    .map_err(core_error)?;

    Ok(NapiPullOutput {
        files: output.files,
        changes: output
            .changes
            .into_iter()
            .map(NapiFileChange::from)
            .collect(),
        conflicts: output.conflicts,
    })
}

#[napi]
pub fn push(input: NapiPushInput) -> Result<NapiPushOutput> {
    let root = parse_root(&input.root)?;
    let fs = memory_filesystem_from_file_map(&root, input.files)?;
    let projection = parse_projection(&input.projection_json)?;
    let current_time = input
        .current_time
        .as_deref()
        .map(parse_current_time)
        .transpose()?;
    let output = push_from_filesystem(
        &fs,
        &root,
        PushInput {
            projection,
            last_known_sequence: parse_last_known_sequence(input.last_known_sequence)?,
            created_by: input.created_by,
            current_time,
            force: input.force.unwrap_or(false),
            skip_validation: input.skip_validation.unwrap_or(false),
        },
    )
    .map_err(core_error)?;

    Ok(NapiPushOutput {
        success: output.success,
        message: output.message,
        command_batch_bytes: output.command_batch_bytes.map(Uint8Array::new),
    })
}

impl From<FileChange> for NapiFileChange {
    fn from(change: FileChange) -> Self {
        match change {
            FileChange::Write { path, content } => Self {
                kind: "write".to_string(),
                path,
                content: Some(content),
            },
            FileChange::Delete { path } => Self {
                kind: "delete".to_string(),
                path,
                content: None,
            },
        }
    }
}

fn parse_root(root: &str) -> Result<PathBuf> {
    if root.is_empty() {
        return Err(napi_error("INVALID_INPUT", "root must not be empty"));
    }
    Ok(PathBuf::from(root))
}

fn parse_projection(raw: &str) -> Result<serde_json::Value> {
    serde_json::from_str(raw).map_err(|error| {
        napi_error(
            "INVALID_PROJECTION",
            format!("invalid projection JSON: {error}"),
        )
    })
}

fn parse_current_time(raw: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| napi_error("INVALID_INPUT", format!("invalid currentTime: {error}")))
}

fn parse_last_known_sequence(value: i64) -> Result<u64> {
    value
        .try_into()
        .map_err(|_| napi_error("INVALID_INPUT", "lastKnownSequence must be non-negative"))
}

fn memory_filesystem_from_file_map(root: &Path, files: FileMap) -> Result<MemoryFileSystem> {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all(root).map_err(internal_error)?;
    for (path, content) in files {
        let path = validate_file_map_path(&path)?;
        if path == adk_core::STATUS_FILE {
            continue;
        }
        fs.write_string(&root.join(path), &content)
            .map_err(internal_error)?;
    }
    Ok(fs)
}

fn validate_file_map_path(path: &str) -> Result<&str> {
    if path.is_empty() {
        return Err(napi_error("INVALID_INPUT", "file paths must not be empty"));
    }
    if path.starts_with('/') || path.contains('\\') {
        return Err(napi_error(
            "INVALID_INPUT",
            format!("file path must be POSIX-style relative path: {path}"),
        ));
    }
    for component in Path::new(path).components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(napi_error(
                    "INVALID_INPUT",
                    format!("file path must not contain empty, '.', or '..' segments: {path}"),
                ));
            }
        }
    }
    Ok(path)
}

fn napi_error(code: &str, message: impl Into<String>) -> Error {
    Error::new(
        Status::GenericFailure,
        format!("{code}: {}", message.into()),
    )
}

fn internal_error(error: impl ToString) -> Error {
    napi_error("INTERNAL_ERROR", error.to_string())
}

fn core_error(error: adk_core::CoreError) -> Error {
    let code = match &error {
        adk_core::CoreError::Domain(_) => "INVALID_INPUT",
        adk_core::CoreError::Io(_) => "INTERNAL_ERROR",
        adk_core::CoreError::Json(_) => "INVALID_PROJECTION",
        adk_core::CoreError::CommandGeneration(_) => "COMMAND_GENERATION_FAILED",
    };
    napi_error(code, error.to_string())
}
