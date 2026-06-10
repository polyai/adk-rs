use crate::{
    CoreError, flatten_discovered_paths, is_generated_metadata_path, recursive_file_paths,
};
use adk_io::{FileSystem, parse_multi_resource_path};
use adk_types::ResourceMap;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Inputs for applying an explicit projection pull to an injected filesystem.
///
/// This is the pure pull contract for embedded callers. It does not fetch
/// remote state, discover project roots, read or write status snapshots, or
/// persist `_gen` metadata. When a base projection is supplied, it is used as
/// the three-way conflict baseline for ADK-owned files.
#[derive(Debug, Clone)]
pub struct PullInput {
    pub pull_projection: JsonValue,
    pub base_projection: Option<JsonValue>,
    pub force: bool,
}

/// Inputs for applying already materialized resources to an injected filesystem.
///
/// Most embedded callers should use `PullInput`; this lower-level form exists
/// so native service code that already fetched or replayed a `ResourceMap` can
/// reuse the same filesystem application logic without going back through JSON.
#[derive(Debug, Clone)]
pub struct PullResourceMapInput {
    pub pull_resources: ResourceMap,
    pub base_resources: Option<ResourceMap>,
    pub force: bool,
    pub delete_local_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChange {
    Write { path: String, content: String },
    Delete { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullOutput {
    pub files: BTreeMap<String, String>,
    pub changes: Vec<FileChange>,
    pub conflicts: Vec<String>,
}

/// Materializes a pull projection into the supplied filesystem.
///
/// Existing files are overwritten only when they are unchanged from the
/// optional base projection, already match the pull projection, or `force` is
/// set. Without a base projection, differing existing ADK files are reported as
/// conflicts unless forced because the caller has not supplied enough
/// information to distinguish user edits from previously materialized content.
pub fn pull_from_filesystem<Fs: FileSystem>(
    fs: &Fs,
    root: &Path,
    input: PullInput,
) -> Result<PullOutput, CoreError> {
    let pull_resources = adk_resources::projection_to_resource_map(&input.pull_projection)?;
    let base_resources = input
        .base_projection
        .as_ref()
        .map(adk_resources::projection_to_resource_map)
        .transpose()?;
    pull_resource_map_from_filesystem(
        fs,
        root,
        PullResourceMapInput {
            pull_resources,
            base_resources,
            force: input.force,
            delete_local_only: input.force,
        },
    )
}

/// Applies already materialized resources to the supplied filesystem.
pub fn pull_resource_map_from_filesystem<Fs: FileSystem>(
    fs: &Fs,
    root: &Path,
    input: PullResourceMapInput,
) -> Result<PullOutput, CoreError> {
    let pull_files = materialized_resource_files(&input.pull_resources);
    let base_files = input
        .base_resources
        .as_ref()
        .map(materialized_resource_files);
    let mut changes = Vec::new();
    let mut conflicts = Vec::new();

    for (path, pull_content) in &pull_files {
        let target = root.join(path);
        let existing = if fs.is_file(&target) {
            Some(fs.read_to_string(&target)?)
        } else {
            None
        };
        let base_content = base_files.as_ref().and_then(|files| files.get(path));

        let should_write = match existing.as_deref() {
            None => true,
            Some(existing) if existing == pull_content => false,
            Some(_) if input.force => true,
            Some(existing) if has_conflict_markers(existing) => {
                conflicts.push(path.clone());
                false
            }
            Some(existing) => match base_content {
                Some(base_content) if existing == base_content => true,
                Some(base_content) if pull_content == base_content => false,
                Some(_) => {
                    conflicts.push(path.clone());
                    false
                }
                None => {
                    conflicts.push(path.clone());
                    false
                }
            },
        };

        if should_write {
            write_file(fs, &target, pull_content)?;
            changes.push(FileChange::Write {
                path: path.clone(),
                content: pull_content.clone(),
            });
        }
    }

    for path in deleted_paths(
        fs,
        root,
        &pull_files,
        base_files.as_ref(),
        input.delete_local_only,
    ) {
        let target = root.join(&path);
        if !fs.is_file(&target) {
            continue;
        }
        let existing = fs.read_to_string(&target)?;
        let base_content = base_files.as_ref().and_then(|files| files.get(&path));
        let should_delete = if input.force {
            true
        } else {
            base_content.is_some_and(|base_content| existing == *base_content)
        };
        if should_delete {
            fs.remove_file(&target)?;
            changes.push(FileChange::Delete { path });
        } else if !conflicts.contains(&path) {
            conflicts.push(path);
        }
    }

    conflicts.sort();
    conflicts.dedup();

    Ok(PullOutput {
        files: snapshot_text_files(fs, root)?,
        changes,
        conflicts,
    })
}

#[cfg(test)]
fn materialized_projection_files(
    projection: &JsonValue,
) -> Result<BTreeMap<String, String>, CoreError> {
    let resources = adk_resources::projection_to_resource_map(projection)?;
    Ok(materialized_resource_files(&resources))
}

fn materialized_resource_files(resources: &ResourceMap) -> BTreeMap<String, String> {
    resources
        .iter()
        .map(|(path, resource)| {
            let content = resource
                .payload
                .get("content")
                .and_then(JsonValue::as_str)
                .unwrap_or_default();
            (
                path.clone(),
                adk_resources::resource_file_content(path, content),
            )
        })
        .collect()
}

fn deleted_paths<Fs: FileSystem>(
    fs: &Fs,
    root: &Path,
    pull_files: &BTreeMap<String, String>,
    base_files: Option<&BTreeMap<String, String>>,
    delete_local_only: bool,
) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    if let Some(base_files) = base_files {
        paths.extend(
            base_files
                .keys()
                .filter(|path| !pull_files.contains_key(*path))
                .cloned(),
        );
    }
    if delete_local_only {
        let discovered = adk_resources::discover_local_resources(fs, root);
        paths.extend(
            flatten_discovered_paths(&discovered)
                .into_iter()
                .map(|path| parse_multi_resource_path(&path).0)
                .filter(|path| !pull_files.contains_key(path)),
        );
    }
    paths
}

fn snapshot_text_files<Fs: FileSystem>(
    fs: &Fs,
    root: &Path,
) -> Result<BTreeMap<String, String>, CoreError> {
    let mut files = BTreeMap::new();
    for path in recursive_file_paths(fs, root)? {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        if is_generated_metadata_path(&rel) {
            continue;
        }
        files.insert(rel, fs.read_to_string(&path)?);
    }
    Ok(files)
}

fn write_file<Fs: FileSystem>(fs: &Fs, path: &Path, content: &str) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        fs.create_dir_all(parent)?;
    }
    fs.write_string(path, content)?;
    Ok(())
}

fn has_conflict_markers(content: &str) -> bool {
    content.contains("<<<<<<<") && content.contains("=======") && content.contains(">>>>>>>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_io::MemoryFileSystem;

    fn topic_projection(name: &str, content: &str) -> JsonValue {
        serde_json::json!({
            "knowledgeBase": {
                "topics": {
                    "entities": {
                        format!("TOPIC-{name}"): {
                            "name": name,
                            "isActive": true,
                            "actions": "",
                            "content": content,
                            "exampleQueries": []
                        }
                    }
                }
            }
        })
    }

    fn topic_content(name: &str, content: &str) -> String {
        materialized_projection_files(&topic_projection(name, content))
            .expect("projection files")
            .remove(&format!("topics/{name}.yaml"))
            .expect("topic file")
    }

    #[test]
    fn pull_from_filesystem_writes_projection_and_preserves_unrelated_files() {
        let fs = MemoryFileSystem::new();
        let root = Path::new("project");
        fs.write_string(&root.join("README.md"), "notes\n")
            .expect("write unrelated file");
        fs.write_string(&root.join(crate::STATUS_FILE), "ignored")
            .expect("write status file");
        fs.write_string(&root.join("_gen/decorators.py"), "generated\n")
            .expect("write generated helper");

        let output = pull_from_filesystem(
            &fs,
            root,
            PullInput {
                pull_projection: topic_projection("billing", "Remote"),
                base_projection: None,
                force: false,
            },
        )
        .expect("pull");

        assert!(output.conflicts.is_empty());
        assert_eq!(
            fs.read_to_string(&root.join("README.md"))
                .expect("read unrelated"),
            "notes\n"
        );
        assert_eq!(
            output.files.get("topics/billing.yaml"),
            Some(&topic_content("billing", "Remote"))
        );
        assert_eq!(output.files.get("README.md"), Some(&"notes\n".to_string()));
        assert!(!output.files.contains_key(crate::STATUS_FILE));
        assert!(!output.files.contains_key("_gen/decorators.py"));
        assert!(fs.exists(&root.join("_gen/decorators.py")));
        assert_eq!(
            output.changes,
            vec![FileChange::Write {
                path: "topics/billing.yaml".to_string(),
                content: topic_content("billing", "Remote"),
            }]
        );
    }

    #[test]
    fn pull_from_filesystem_uses_base_projection_as_conflict_baseline() {
        let fs = MemoryFileSystem::new();
        let root = Path::new("project");
        fs.write_string(
            &root.join("topics/billing.yaml"),
            &topic_content("billing", "Base"),
        )
        .expect("write base topic");

        let clean = pull_from_filesystem(
            &fs,
            root,
            PullInput {
                pull_projection: topic_projection("billing", "Remote"),
                base_projection: Some(topic_projection("billing", "Base")),
                force: false,
            },
        )
        .expect("clean pull");
        assert!(clean.conflicts.is_empty());
        assert_eq!(
            fs.read_to_string(&root.join("topics/billing.yaml"))
                .expect("read remote topic"),
            topic_content("billing", "Remote")
        );

        fs.write_string(
            &root.join("topics/billing.yaml"),
            &topic_content("billing", "Local"),
        )
        .expect("write local edit");
        let conflicted = pull_from_filesystem(
            &fs,
            root,
            PullInput {
                pull_projection: topic_projection("billing", "Remote 2"),
                base_projection: Some(topic_projection("billing", "Remote")),
                force: false,
            },
        )
        .expect("conflicted pull");

        assert_eq!(conflicted.conflicts, vec!["topics/billing.yaml"]);
        assert_eq!(
            fs.read_to_string(&root.join("topics/billing.yaml"))
                .expect("read local topic"),
            topic_content("billing", "Local")
        );
        assert!(conflicted.changes.is_empty());
    }

    #[test]
    fn pull_from_filesystem_deletes_base_resources_when_clean_or_forced() {
        let fs = MemoryFileSystem::new();
        let root = Path::new("project");
        fs.write_string(&root.join("topics/old.yaml"), &topic_content("old", "Base"))
            .expect("write old topic");

        let output = pull_from_filesystem(
            &fs,
            root,
            PullInput {
                pull_projection: serde_json::json!({}),
                base_projection: Some(topic_projection("old", "Base")),
                force: false,
            },
        )
        .expect("pull deletion");

        assert!(output.conflicts.is_empty());
        assert!(!fs.exists(&root.join("topics/old.yaml")));
        assert_eq!(
            output.changes,
            vec![FileChange::Delete {
                path: "topics/old.yaml".to_string(),
            }]
        );

        fs.write_string(
            &root.join("topics/old.yaml"),
            &topic_content("old", "Local"),
        )
        .expect("write local old topic");
        let conflicted = pull_from_filesystem(
            &fs,
            root,
            PullInput {
                pull_projection: serde_json::json!({}),
                base_projection: Some(topic_projection("old", "Base")),
                force: false,
            },
        )
        .expect("conflicted deletion");
        assert_eq!(conflicted.conflicts, vec!["topics/old.yaml"]);
        assert!(fs.exists(&root.join("topics/old.yaml")));

        let forced = pull_from_filesystem(
            &fs,
            root,
            PullInput {
                pull_projection: serde_json::json!({}),
                base_projection: Some(topic_projection("old", "Base")),
                force: true,
            },
        )
        .expect("forced deletion");
        assert!(forced.conflicts.is_empty());
        assert!(!fs.exists(&root.join("topics/old.yaml")));
    }

    #[test]
    fn pull_from_filesystem_without_base_is_conservative_unless_forced() {
        let fs = MemoryFileSystem::new();
        let root = Path::new("project");
        fs.write_string(
            &root.join("topics/billing.yaml"),
            &topic_content("billing", "Local"),
        )
        .expect("write local topic");

        let output = pull_from_filesystem(
            &fs,
            root,
            PullInput {
                pull_projection: topic_projection("billing", "Remote"),
                base_projection: None,
                force: false,
            },
        )
        .expect("conservative pull");
        assert_eq!(output.conflicts, vec!["topics/billing.yaml"]);
        assert_eq!(
            fs.read_to_string(&root.join("topics/billing.yaml"))
                .expect("read local topic"),
            topic_content("billing", "Local")
        );

        let forced = pull_from_filesystem(
            &fs,
            root,
            PullInput {
                pull_projection: topic_projection("billing", "Remote"),
                base_projection: None,
                force: true,
            },
        )
        .expect("forced pull");
        assert!(forced.conflicts.is_empty());
        assert_eq!(
            fs.read_to_string(&root.join("topics/billing.yaml"))
                .expect("read remote topic"),
            topic_content("billing", "Remote")
        );
    }

    #[test]
    fn pull_from_filesystem_force_deletes_local_adk_files_without_base() {
        let fs = MemoryFileSystem::new();
        let root = Path::new("project");
        fs.write_string(
            &root.join("topics/local_only.yaml"),
            &topic_content("local_only", "Local"),
        )
        .expect("write local topic");
        fs.write_string(&root.join("notes.txt"), "keep me\n")
            .expect("write unrelated file");

        let output = pull_from_filesystem(
            &fs,
            root,
            PullInput {
                pull_projection: serde_json::json!({}),
                base_projection: None,
                force: true,
            },
        )
        .expect("forced pull");

        assert!(output.conflicts.is_empty());
        assert!(!fs.exists(&root.join("topics/local_only.yaml")));
        assert_eq!(
            fs.read_to_string(&root.join("notes.txt"))
                .expect("read unrelated file"),
            "keep me\n"
        );
        assert_eq!(
            output.changes,
            vec![FileChange::Delete {
                path: "topics/local_only.yaml".to_string(),
            }]
        );
        assert_eq!(
            output.files.get("notes.txt"),
            Some(&"keep me\n".to_string())
        );
    }
}
