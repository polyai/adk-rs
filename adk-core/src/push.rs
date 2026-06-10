use crate::{
    CoreError, recursive_file_paths, validation, workspace::collect_local_resources_from_fs,
};
use adk_io::FileSystem;
use adk_protobuf::{Command, CommandBatch};
use adk_types::{Resource, ResourceMap};
use chrono::{DateTime, Utc};
use prost::Message;
use serde_json::Value as JsonValue;
use std::path::Path;

/// Inputs for planning a push command batch from an explicit filesystem root.
///
/// This is the pure filesystem-backed push contract used by embedded callers.
/// It assumes the caller already resolved `root` and supplied the remote
/// projection plus branch sequence; it does not read project status, discover a
/// root, call the network, write local files, or persist `_gen` metadata.
#[derive(Debug, Clone)]
pub struct PushInput {
    pub projection: JsonValue,
    pub last_known_sequence: u64,
    pub created_by: Option<String>,
    pub current_time: Option<DateTime<Utc>>,
    pub force: bool,
    pub skip_validation: bool,
}

/// Inputs for planning push commands from an already selected resource map.
#[derive(Debug, Clone)]
pub struct PushPlanInput {
    pub projection: JsonValue,
    pub last_known_sequence: u64,
    pub created_by: Option<String>,
    pub current_time: Option<DateTime<Utc>>,
}

/// Resource map containing only files selected by status-based change detection.
///
/// Plain `ResourceMap` values used by push planning are treated as full local
/// snapshots, where absence can mean "delete remotely". `ChangedResourceMap`
/// makes the changed-only case explicit: absence means "out of scope".
#[derive(Debug, Clone, Default)]
pub struct ChangedResourceMap {
    resources: ResourceMap,
}

impl ChangedResourceMap {
    pub fn new(resources: ResourceMap) -> Self {
        Self { resources }
    }

    pub fn as_resources(&self) -> &ResourceMap {
        &self.resources
    }

    pub fn as_resources_mut(&mut self) -> &mut ResourceMap {
        &mut self.resources
    }

    pub fn into_inner(self) -> ResourceMap {
        self.resources
    }

    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }
}

/// Planned push commands plus transport-ready protobuf bytes.
#[derive(Debug, Clone)]
pub struct PushCommandPlan {
    pub last_known_sequence: u64,
    pub commands: Vec<Command>,
    pub command_summaries: Vec<JsonValue>,
    pub command_batch_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
enum PushCommandScope {
    FullSnapshot,
    ChangedOnly,
}

/// Result of planning a push command batch without contacting Agent Studio.
///
/// `message` is set only for push-contract failures that should be reported to
/// a caller without throwing, such as conflicts, validation errors, or no
/// generated commands. Successful outputs contain protobuf `CommandBatch`
/// bytes ready for a transport layer to POST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushOutput {
    pub success: bool,
    pub message: Option<String>,
    pub command_batch_bytes: Option<Vec<u8>>,
}

/// Builds push commands and protobuf bytes from a caller-selected resource map.
///
/// This is the shared command-planning unit used by both service/CLI push and
/// embedded pure push wrappers. It performs no filesystem access, no transport,
/// and no status persistence.
pub fn plan_push_commands_from_resources(
    resources: &ResourceMap,
    input: PushPlanInput,
) -> Result<PushCommandPlan, CoreError> {
    build_push_command_plan(resources, input, PushCommandScope::FullSnapshot)
}

pub fn plan_push_commands_from_changed_resources(
    resources: &ChangedResourceMap,
    input: PushPlanInput,
) -> Result<PushCommandPlan, CoreError> {
    build_push_command_plan(
        resources.as_resources(),
        input,
        PushCommandScope::ChangedOnly,
    )
}

fn build_push_command_plan(
    resources: &ResourceMap,
    input: PushPlanInput,
    scope: PushCommandScope,
) -> Result<PushCommandPlan, CoreError> {
    let timestamp = input.current_time.map(timestamp_from_datetime);
    let commands = match scope {
        PushCommandScope::FullSnapshot => adk_resources::try_build_push_commands_with_metadata(
            resources,
            &input.projection,
            input.created_by.as_deref(),
            timestamp,
        )?,
        PushCommandScope::ChangedOnly => {
            adk_resources::try_build_push_commands_for_changed_resources_with_metadata(
                resources,
                &input.projection,
                input.created_by.as_deref(),
                timestamp,
            )?
        }
    };
    let command_summaries = commands
        .iter()
        .map(adk_resources::command_to_json_summary)
        .collect();
    let command_batch_bytes = CommandBatch {
        last_known_sequence: input.last_known_sequence,
        commands: commands.clone(),
    }
    .encode_to_vec();
    Ok(PushCommandPlan {
        last_known_sequence: input.last_known_sequence,
        commands,
        command_summaries,
        command_batch_bytes,
    })
}

/// Builds a protobuf command batch from local files and a caller-supplied projection.
///
/// The workflow mirrors the command-planning portion of CLI push while keeping
/// side effects outside the core helper. The caller provides the filesystem,
/// root, remote projection, sequence number, and optional metadata timestamp.
pub fn push_from_filesystem<Fs: FileSystem>(
    fs: &Fs,
    root: &Path,
    input: PushInput,
) -> Result<PushOutput, CoreError> {
    if !input.force {
        let conflicted = detect_conflict_files(fs, root)?;
        if !conflicted.is_empty() {
            let conflicts = conflicted.join("\n- ");
            return Ok(PushOutput {
                success: false,
                message: Some(format!(
                    "Merge conflicts detected in the following files:\n- {conflicts}\nPlease resolve the conflicts and try again."
                )),
                command_batch_bytes: None,
            });
        }
    }

    let mut resources = collect_local_resources_from_fs(fs, root)?;
    if !input.skip_validation {
        let validation_errors = validation::validate_local_resources(root, &resources)?;
        if !validation_errors.is_empty() {
            return Ok(PushOutput {
                success: false,
                message: Some(format!(
                    "Validation errors detected:\n{}",
                    validation_errors.join("\n")
                )),
                command_batch_bytes: None,
            });
        }
    }

    add_discovered_variable_resources_from_fs(fs, root, &mut resources);
    let plan = plan_push_commands_from_resources(
        &resources,
        PushPlanInput {
            projection: input.projection,
            last_known_sequence: input.last_known_sequence,
            created_by: input.created_by,
            current_time: input.current_time,
        },
    )?;
    if plan.commands.is_empty() {
        return Ok(PushOutput {
            success: false,
            message: Some("No changes detected".to_string()),
            command_batch_bytes: None,
        });
    }

    Ok(PushOutput {
        success: true,
        message: None,
        command_batch_bytes: Some(plan.command_batch_bytes),
    })
}

pub(crate) fn add_discovered_variable_resources_from_fs<Fs: FileSystem>(
    fs: &Fs,
    root: &Path,
    resources: &mut ResourceMap,
) {
    let discovered = adk_resources::discover_local_resources(fs, root);
    let Some(variables) = discovered.get("Variable") else {
        return;
    };
    for logical_path in variables {
        if resources.contains_key(logical_path) {
            continue;
        }
        let Some(name) = logical_path.strip_prefix("variables/") else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        resources.insert(
            logical_path.clone(),
            Resource {
                resource_id: "local".to_string(),
                name: name.to_string(),
                file_path: logical_path.clone(),
                payload: serde_json::json!({ "content": "" }),
            },
        );
    }
}

fn detect_conflict_files<Fs: FileSystem>(fs: &Fs, root: &Path) -> Result<Vec<String>, CoreError> {
    let mut conflicts = Vec::new();
    for path in recursive_file_paths(fs, root)? {
        let content = match fs.read_to_string(&path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        if content.contains("<<<<<<<") && content.contains("=======") && content.contains(">>>>>>>")
        {
            conflicts.push(path.to_string_lossy().to_string());
        }
    }
    Ok(conflicts)
}

fn timestamp_from_datetime(value: DateTime<Utc>) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: value.timestamp(),
        nanos: value.timestamp_subsec_nanos() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_io::MemoryFileSystem;
    use adk_protobuf::CommandBatch;
    use prost::Message;
    use std::collections::BTreeSet;

    fn write_topic(fs: &MemoryFileSystem, root: &Path, name: &str, content: &str) {
        fs.write_string(
            &root.join(format!("topics/{name}.yaml")),
            &format!(
                "name: {name}\nenabled: true\nactions: \"\"\ncontent: \"{content}\"\nexample_queries: []\n"
            ),
        )
        .expect("write topic");
    }

    fn push_input(projection: JsonValue) -> PushInput {
        PushInput {
            projection,
            last_known_sequence: 42,
            created_by: Some("tester@example.com".to_string()),
            current_time: Some(
                DateTime::parse_from_rfc3339("2026-06-10T12:34:56.123Z")
                    .expect("timestamp")
                    .with_timezone(&Utc),
            ),
            force: false,
            skip_validation: false,
        }
    }

    #[test]
    fn push_from_filesystem_encodes_command_batch_with_supplied_metadata() {
        let fs = MemoryFileSystem::new();
        let root = Path::new("workspace/project");
        write_topic(&fs, root, "sample", "hello");

        let output = push_from_filesystem(&fs, root, push_input(serde_json::json!({})))
            .expect("push output");

        assert!(output.success);
        assert_eq!(output.message, None);
        let bytes = output.command_batch_bytes.expect("command batch bytes");
        let batch = CommandBatch::decode(bytes.as_slice()).expect("decode batch");
        assert_eq!(batch.last_known_sequence, 42);
        assert_eq!(batch.commands.len(), 1);
        assert_eq!(batch.commands[0].r#type, "create_topic");
        let metadata = batch.commands[0].metadata.as_ref().expect("metadata");
        assert_eq!(metadata.created_by, "tester@example.com");
        assert_eq!(
            metadata.created_at,
            Some(prost_types::Timestamp {
                seconds: 1_781_094_896,
                nanos: 123_000_000,
            })
        );
    }

    #[test]
    fn push_from_filesystem_reports_no_changes_without_batch_bytes() {
        let fs = MemoryFileSystem::new();
        let root = Path::new("workspace/project");
        write_topic(&fs, root, "sample", "hello");
        let projection = serde_json::json!({
            "knowledgeBase": {
                "topics": {
                    "entities": {
                        "topic-1": {
                            "name": "sample",
                            "content": "hello",
                            "actions": "",
                            "exampleQueries": { "queries": [] },
                            "isActive": true
                        }
                    }
                }
            }
        });

        let output = push_from_filesystem(&fs, root, push_input(projection)).expect("push output");

        assert!(!output.success);
        assert_eq!(output.message.as_deref(), Some("No changes detected"));
        assert_eq!(output.command_batch_bytes, None);
    }

    #[test]
    fn push_from_filesystem_blocks_conflicts_unless_forced() {
        let fs = MemoryFileSystem::new();
        let root = Path::new("workspace/project");
        fs.write_string(
            &root.join("topics/sample.yaml"),
            "# <<<<<<< ours\nname: sample\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n# =======\n# >>>>>>> theirs\n",
        )
        .expect("write conflict");

        let blocked = push_from_filesystem(&fs, root, push_input(serde_json::json!({})))
            .expect("blocked push");
        assert!(!blocked.success);
        assert!(
            blocked
                .message
                .as_deref()
                .unwrap_or_default()
                .contains("Merge conflicts detected")
        );

        let mut forced_input = push_input(serde_json::json!({}));
        forced_input.force = true;
        forced_input.skip_validation = true;
        let forced = push_from_filesystem(&fs, root, forced_input).expect("forced push");
        assert!(forced.success);
        assert!(forced.command_batch_bytes.is_some());
    }

    #[test]
    fn discovered_variable_resources_are_included_in_push_batch() {
        let fs = MemoryFileSystem::new();
        let root = Path::new("workspace/project");
        fs.write_string(
            &root.join("functions/remember.py"),
            "def remember(conv):\n    conv.state.account_id = '123'\n",
        )
        .expect("write function");

        let output = push_from_filesystem(&fs, root, push_input(serde_json::json!({})))
            .expect("push output");

        let bytes = output.command_batch_bytes.expect("command batch bytes");
        let batch = CommandBatch::decode(bytes.as_slice()).expect("decode batch");
        let command_types = batch
            .commands
            .iter()
            .map(|command| command.r#type.as_str())
            .collect::<BTreeSet<_>>();
        assert!(command_types.contains("create_function"));
        assert!(command_types.contains("variable_create"));
    }
}
