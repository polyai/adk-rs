use adk_types::{
    BranchDescriptor, BranchMergeResult, DeploymentList, DiffMap, DomainError, ProjectConfig,
    PushResult, Resource, ResourceMap, StatusSummary,
};

pub mod discover;
mod python_syntax;

use adk_io::{compute_hash, diff_resources, parse_multi_resource_path};
use adk_platform_api::{ApiError, PlatformClient};
use anyhow::Result;
use base64::Engine;
pub use discover::discover_local_resources;
pub use discover::{DiscoveredResourceChanges, DiscoveredResourcePaths, TypedResourceLifecycle};
use globset::{Glob, GlobSetBuilder};
use python_syntax::validate_python_module;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;
use walkdir::WalkDir;

pub const PROJECT_CONFIG_FILE: &str = "project.yaml";
pub const STATUS_FILE: &str = "_gen/.agent_studio_config";
const MIGRATED_LEGACY_TOPIC_FILES: &str = "migrated_legacy_topic_files";

const PYTHON_GEN_TEMPLATE_FILES: &[(&str, &str)] = &[
    (
        "__init__.py",
        include_str!("../python-gen-template/__init__.py"),
    ),
    (
        "attachment.py",
        include_str!("../python-gen-template/attachment.py"),
    ),
    (
        "conv_utils.py",
        include_str!("../python-gen-template/conv_utils.py"),
    ),
    (
        "conversation.py",
        include_str!("../python-gen-template/conversation.py"),
    ),
    (
        "decorators.py",
        include_str!("../python-gen-template/decorators.py"),
    ),
    (
        "external_events.py",
        include_str!("../python-gen-template/external_events.py"),
    ),
    ("flow.py", include_str!("../python-gen-template/flow.py")),
    (
        "history.py",
        include_str!("../python-gen-template/history.py"),
    ),
    (
        "log_utils.py",
        include_str!("../python-gen-template/log_utils.py"),
    ),
    (
        "memory.py",
        include_str!("../python-gen-template/memory.py"),
    ),
    (
        "secret_vault.py",
        include_str!("../python-gen-template/secret_vault.py"),
    ),
    ("sms.py", include_str!("../python-gen-template/sms.py")),
    (
        "value_extraction.py",
        include_str!("../python-gen-template/value_extraction.py"),
    ),
];

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("{0}")]
    Domain(#[from] DomainError),
    #[error("{0}")]
    Api(#[from] ApiError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct AdkService {
    client: Box<dyn PlatformClient>,
}

pub struct PullOutcome {
    pub files_with_conflicts: Vec<String>,
    pub new_branch_name: Option<String>,
    pub new_branch_id: Option<String>,
}

impl AdkService {
    pub fn new(client: Box<dyn PlatformClient>) -> Self {
        Self { client }
    }

    pub fn init_project(
        &self,
        base_path: &Path,
        region: String,
        account_id: String,
        project_id: String,
    ) -> Result<ProjectConfig, CoreError> {
        self.init_project_with_name(base_path, region, account_id, project_id, None)
    }

    pub fn init_project_with_name(
        &self,
        base_path: &Path,
        region: String,
        account_id: String,
        project_id: String,
        project_name: Option<String>,
    ) -> Result<ProjectConfig, CoreError> {
        let root = base_path.join(&account_id).join(&project_id);
        fs::create_dir_all(&root)?;
        let config = ProjectConfig {
            region,
            account_id,
            project_id,
            project_name,
            branch_id: "main".to_string(),
        };
        let serialized =
            serde_yaml::to_string(&config).map_err(|e| DomainError::InvalidData(e.to_string()))?;
        fs::write(root.join(PROJECT_CONFIG_FILE), serialized)?;
        self.write_python_gen_package(&root)?;
        Ok(config)
    }

    pub fn load_project_config(&self, base_path: &Path) -> Result<ProjectConfig, CoreError> {
        let discovered = find_project_root(base_path)
            .ok_or_else(|| DomainError::ConfigNotFound(base_path.to_string_lossy().to_string()))?;
        let config_path = discovered.join(PROJECT_CONFIG_FILE);
        if config_path.exists() {
            let raw = fs::read_to_string(config_path)?;
            let config =
                serde_yaml::from_str(&raw).map_err(|e| DomainError::InvalidData(e.to_string()))?;
            self.run_and_persist_project_migrations(&discovered)?;
            return Ok(config);
        }

        let status_path = discovered.join(STATUS_FILE);
        if status_path.exists() {
            let encoded = fs::read(status_path)?;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|e| DomainError::InvalidData(e.to_string()))?;
            let json: serde_json::Value = serde_json::from_slice(&decoded)?;
            let config = ProjectConfig {
                region: json
                    .get("region")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                account_id: json
                    .get("account_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                project_id: json
                    .get("project_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                project_name: json
                    .get("project_name")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string),
                branch_id: json
                    .get("branch_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("main")
                    .to_string(),
            };
            self.run_and_persist_project_migrations(&discovered)?;
            return Ok(config);
        }

        Err(DomainError::ConfigNotFound(discovered.to_string_lossy().to_string()).into())
    }

    pub fn collect_local_resources(&self, root: &Path) -> Result<ResourceMap, CoreError> {
        let mut map = ResourceMap::new();
        let mut files = Vec::new();
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file() {
                files.push(entry.path().to_path_buf());
            }
        }
        files.sort();
        for file in files {
            let rel = file
                .strip_prefix(root)
                .unwrap_or(file.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            if rel == PROJECT_CONFIG_FILE || rel == STATUS_FILE || rel.starts_with("_gen/") {
                continue;
            }
            let content = fs::read_to_string(file.as_path()).unwrap_or_default();
            let payload = serde_json::json!({
                "content": local_resource_content(&rel, &content),
            });
            map.insert(
                rel.clone(),
                Resource {
                    resource_id: rel.clone(),
                    name: rel.clone(),
                    file_path: rel,
                    payload,
                },
            );
        }
        Ok(map)
    }

    /// Typed discovery matching Python `AgentStudioProject.discover_local_resources()`:
    /// logical paths per resource type, keyed by Python class name (`Topic`, `Entity`, ...).
    pub fn discover_local_resources(&self, root: &Path) -> indexmap::IndexMap<String, Vec<String>> {
        discover::discover_local_resources(root)
    }

    /// Typed parity helper matching Python `find_new_kept_deleted` semantics at path level.
    pub fn find_new_kept_deleted(
        &self,
        discovered_resources: &DiscoveredResourcePaths,
        existing_resources: &DiscoveredResourcePaths,
    ) -> DiscoveredResourceChanges {
        discover::find_new_kept_deleted(discovered_resources, existing_resources)
    }

    pub fn typed_resource_lifecycle(
        &self,
        root: &Path,
    ) -> Result<Vec<TypedResourceLifecycle>, CoreError> {
        let discovered = self.discover_local_resources(root);
        let existing_resource_ids = self.load_status_snapshot_resource_ids(root)?;
        Ok(discover::build_typed_resource_lifecycle(
            &discovered,
            &existing_resource_ids,
        ))
    }

    pub fn status(&self, root: &Path) -> Result<StatusSummary, CoreError> {
        let mut local = self.collect_local_resources(root)?;
        let remote = self.client.pull_resources()?;
        let mut summary = StatusSummary {
            conflict_detection_available: true,
            files_with_conflicts: self.detect_conflict_files(root)?,
            ..StatusSummary::default()
        };

        if let Some(existing_typed) = self.load_status_snapshot_discovered_resources(root)? {
            let discovered_typed = self.discover_local_resources(root);
            let typed_changes = self.find_new_kept_deleted(&discovered_typed, &existing_typed);
            summary.new_files =
                flatten_discovered_paths_by_type_order(&typed_changes.new_resources);
            summary.deleted_files =
                flatten_deleted_discovered_paths(&typed_changes.deleted_resources);

            if let Some(snapshot_hashes) = self.load_status_snapshot_file_hashes(root)? {
                summary.modified_files = compute_modified_files_against_snapshot(
                    root,
                    &typed_changes.kept_resources,
                    &snapshot_hashes,
                )?;
                let replacements = self.deleted_resource_reference_replacements(
                    root,
                    &typed_changes.deleted_resources,
                )?;
                if !replacements.is_empty() {
                    apply_reference_name_replacements(&mut local, &replacements);
                    summary.modified_files.extend(
                        compute_modified_files_against_snapshot_with_replacements(
                            root,
                            &typed_changes.kept_resources,
                            &snapshot_hashes,
                            &replacements,
                        )?,
                    );
                    stable_dedup(&mut summary.modified_files);
                }
            } else {
                for path in local.keys() {
                    if let (Some(l), Some(r)) = (local.get(path), remote.get(path))
                        && l.payload != r.payload
                    {
                        summary.modified_files.push(path.clone());
                    }
                }
            }
        } else {
            for path in local.keys() {
                if !remote.contains_key(path) {
                    summary.new_files.push(path.clone());
                }
            }
            for path in remote.keys() {
                if !local.contains_key(path) {
                    summary.deleted_files.push(path.clone());
                }
            }
            for path in local.keys() {
                if let (Some(l), Some(r)) = (local.get(path), remote.get(path))
                    && l.payload != r.payload
                {
                    summary.modified_files.push(path.clone());
                }
            }
        }
        Ok(summary)
    }

    pub fn diff(
        &self,
        root: &Path,
        files: &[String],
        before: Option<String>,
        after: Option<String>,
    ) -> Result<DiffMap, CoreError> {
        if before.is_some() || after.is_some() {
            let mut before_name = before.unwrap_or_default();
            let mut after_name = after.unwrap_or_default();
            if before_name.is_empty() {
                let client_env = if after_name == "pre-release" || after_name == "live" {
                    after_name.as_str()
                } else {
                    "sandbox"
                };
                let deployments = self.client.list_deployments(client_env)?;
                if let Some(active_hash) = deployments.active_deployment_hashes.get(&after_name) {
                    after_name = active_hash.clone();
                }
                if deployments.versions.is_empty() {
                    return Err(DomainError::InvalidData("No versions found.".to_string()).into());
                }
                let after_prefix = after_name
                    .chars()
                    .take(9)
                    .collect::<String>()
                    .to_lowercase();
                let version_idx = deployments.versions.iter().position(|version| {
                    version
                        .get("version_hash")
                        .or_else(|| version.get("versionHash"))
                        .or_else(|| version.get("hash"))
                        .and_then(Value::as_str)
                        .map(|v| {
                            v.chars().take(9).collect::<String>().to_lowercase() == after_prefix
                        })
                        .unwrap_or(false)
                });
                let Some(version_idx) = version_idx else {
                    return Err(DomainError::InvalidData(format!(
                        "Version hash '{after_name}' not found."
                    ))
                    .into());
                };
                if version_idx == deployments.versions.len() - 1 {
                    return Err(
                        DomainError::InvalidData("No previous version found.".to_string()).into(),
                    );
                }
                let previous = &deployments.versions[version_idx + 1];
                before_name = previous
                    .get("version_hash")
                    .or_else(|| previous.get("versionHash"))
                    .or_else(|| previous.get("hash"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .chars()
                    .take(9)
                    .collect::<String>();
            }
            if after_name.is_empty() {
                after_name = "local".to_string();
            }
            let mut before_state = self.resolve_named_state(root, &before_name)?;
            let mut after_state = self.resolve_named_state(root, &after_name)?;
            normalize_flow_resources_for_diff(&mut before_state, None);
            normalize_flow_resources_for_diff(&mut after_state, Some(&before_state));
            if before_name != "local" {
                normalize_function_references_in_rules(&mut before_state);
                normalize_function_references_in_rules(&mut after_state);
            }
            let mut diffs = diff_resources(&before_state, &after_state);
            if !files.is_empty() {
                let matcher = build_file_matcher(files)?;
                diffs.retain(|k, _| matcher.is_match(k));
            }
            return Ok(diffs);
        }
        let mut local = self.collect_local_resources(root)?;
        let remote = if let Some(resources) = self.load_replay_state_resources(root)? {
            resources
        } else {
            self.client.pull_resources()?
        };
        let mut changed_file_paths = None;
        let mut deleted_file_paths = HashSet::new();

        if let Some(existing_typed) = self.load_status_snapshot_discovered_resources(root)? {
            let discovered_typed = self.discover_local_resources(root);
            let typed_changes = self.find_new_kept_deleted(&discovered_typed, &existing_typed);
            deleted_file_paths = flatten_discovered_paths(&typed_changes.deleted_resources)
                .iter()
                .map(|p| parse_multi_resource_path(p).0)
                .collect();
            let replacements = self
                .deleted_resource_reference_replacements(root, &typed_changes.deleted_resources)?;
            if !replacements.is_empty() {
                apply_reference_name_replacements(&mut local, &replacements);
            }
            let mut changed_paths = flatten_discovered_paths(&typed_changes.new_resources)
                .into_iter()
                .chain(flatten_discovered_paths(&typed_changes.deleted_resources))
                .collect::<Vec<_>>();
            if let Some(snapshot_hashes) = self.load_status_snapshot_file_hashes(root)? {
                changed_paths.extend(compute_modified_files_against_snapshot(
                    root,
                    &typed_changes.kept_resources,
                    &snapshot_hashes,
                )?);
                changed_paths.extend(compute_modified_files_against_snapshot_with_replacements(
                    root,
                    &typed_changes.kept_resources,
                    &snapshot_hashes,
                    &replacements,
                )?);
            }

            if changed_paths.is_empty() {
                changed_file_paths = Some(HashSet::new());
            } else {
                changed_file_paths = Some(
                    changed_paths
                        .iter()
                        .map(|p| parse_multi_resource_path(p).0)
                        .collect(),
                );
            }
        }

        let mut remote = remote;
        normalize_flow_resources_for_diff(&mut remote, None);
        normalize_flow_resources_for_diff(&mut local, Some(&remote));
        let mut diffs = diff_resources(&remote, &local);

        if let Some(changed_file_paths) = changed_file_paths {
            if changed_file_paths.is_empty() {
                diffs.clear();
            } else {
                diffs.retain(|k, _| changed_file_paths.contains(k));
            }
        }

        if !files.is_empty() {
            let matcher = build_file_matcher(files)?;
            diffs.retain(|k, _| matcher.is_match(k));
        }
        absolutize_deleted_diff_keys(root, &mut diffs, &deleted_file_paths);
        Ok(diffs)
    }

    pub fn push(
        &self,
        root: &Path,
        force: bool,
        skip_validation: bool,
        dry_run: bool,
    ) -> Result<PushResult, CoreError> {
        self.push_with_options(root, force, skip_validation, dry_run, None, None)
    }

    pub fn push_with_options(
        &self,
        root: &Path,
        force: bool,
        skip_validation: bool,
        dry_run: bool,
        projection: Option<&Value>,
        actor: Option<&str>,
    ) -> Result<PushResult, CoreError> {
        if !force {
            let conflicted = self.detect_conflict_files(root)?;
            if !conflicted.is_empty() {
                let conflicts = conflicted.join("\n- ");
                return Ok(PushResult {
                    success: false,
                    message: format!(
                        "Merge conflicts detected in the following files:\n- {conflicts}\nPlease resolve the conflicts and try again."
                    ),
                    commands: vec![],
                });
            }
        }
        if !skip_validation {
            let validation_errors = self.validate_local_resources(root)?;
            if !validation_errors.is_empty() {
                return Ok(PushResult {
                    success: false,
                    message: format!(
                        "Validation errors detected:\n{}",
                        validation_errors.join("\n")
                    ),
                    commands: vec![],
                });
            }
        }
        let mut persistent_local = self.collect_local_resources(root)?;
        self.apply_deleted_reference_names(root, &mut persistent_local)?;
        let mut local = persistent_local.clone();
        self.add_discovered_variable_resources(root, &mut local);
        if dry_run {
            return Ok(self
                .client
                .preview_push_resources_with_options(&local, projection, actor)?);
        }
        let result = self
            .client
            .push_resources_with_options(&local, projection, actor)?;
        if result.success {
            self.save_replay_state_resources(root, &persistent_local)?;
            self.write_status_snapshot_from_resources(root, &persistent_local)?;
        }
        Ok(result)
    }

    fn add_discovered_variable_resources(&self, root: &Path, local: &mut ResourceMap) {
        let project_root = find_project_root(root).unwrap_or_else(|| root.to_path_buf());
        let discovered = self.discover_local_resources(&project_root);
        let Some(variables) = discovered.get("Variable") else {
            return;
        };
        for logical_path in variables {
            if local.contains_key(logical_path) {
                continue;
            }
            let Some(name) = logical_path.strip_prefix("variables/") else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            local.insert(
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

    pub fn push_main_to_new_branch(
        &self,
        root: &Path,
        branch_name: &str,
        force: bool,
        skip_validation: bool,
        actor: Option<&str>,
    ) -> Result<(ProjectConfig, PushResult), CoreError> {
        if !force {
            let conflicted = self.detect_conflict_files(root)?;
            if !conflicted.is_empty() {
                let conflicts = conflicted.join("\n- ");
                return Ok((
                    self.load_project_config(root)?,
                    PushResult {
                        success: false,
                        message: format!(
                            "Merge conflicts detected in the following files:\n- {conflicts}\nPlease resolve the conflicts and try again."
                        ),
                        commands: vec![],
                    },
                ));
            }
        }
        if !skip_validation {
            let validation_errors = self.validate_local_resources(root)?;
            if !validation_errors.is_empty() {
                return Ok((
                    self.load_project_config(root)?,
                    PushResult {
                        success: false,
                        message: format!(
                            "Validation errors detected:\n{}",
                            validation_errors.join("\n")
                        ),
                        commands: vec![],
                    },
                ));
            }
        }
        let mut persistent_local = self.collect_local_resources(root)?;
        self.apply_deleted_reference_names(root, &mut persistent_local)?;
        let mut local = persistent_local.clone();
        self.add_discovered_variable_resources(root, &mut local);
        let (branch_id, push_result) =
            self.client
                .push_main_resources_to_new_branch(branch_name, &local, actor)?;
        let mut cfg = self.load_project_config(root)?;
        if push_result.success {
            cfg.branch_id = branch_id;
            self.write_project_config(root, &cfg)?;
            self.save_replay_state_resources(root, &persistent_local)?;
            self.write_status_snapshot_from_resources(root, &persistent_local)?;
        }
        Ok((cfg, push_result))
    }

    pub fn pull(&self, root: &Path, force: bool) -> Result<Vec<String>, CoreError> {
        self.pull_with_format(root, force, false)
    }

    pub fn pull_with_format(
        &self,
        root: &Path,
        force: bool,
        format: bool,
    ) -> Result<Vec<String>, CoreError> {
        Ok(self
            .pull_detailed_with_format(root, force, format)?
            .files_with_conflicts)
    }

    pub fn pull_resource_map_with_format(
        &self,
        root: &Path,
        resources: ResourceMap,
        force: bool,
        format: bool,
    ) -> Result<Vec<String>, CoreError> {
        self.write_pulled_resources(root, resources, force, format)
    }

    pub fn pull_detailed_with_format(
        &self,
        root: &Path,
        force: bool,
        format: bool,
    ) -> Result<PullOutcome, CoreError> {
        let remote = if let Some(resources) = self.load_replay_state_resources(root)? {
            resources
        } else {
            let (resources, new_branch) = self.pull_resources_with_branch_reconciliation(root)?;
            let conflicts = self.write_pulled_resources(root, resources, force, format)?;
            return Ok(PullOutcome {
                files_with_conflicts: conflicts,
                new_branch_name: new_branch.as_ref().map(|branch| branch.name.clone()),
                new_branch_id: new_branch.map(|branch| branch.branch_id),
            });
        };
        let conflicts = self.write_pulled_resources(root, remote, force, format)?;
        Ok(PullOutcome {
            files_with_conflicts: conflicts,
            new_branch_name: None,
            new_branch_id: None,
        })
    }

    pub fn pull_projection_json(&self) -> Result<Value, CoreError> {
        Ok(self.client.pull_projection_json()?)
    }

    pub fn pull_projection_json_by_name(&self, name: &str) -> Result<Value, CoreError> {
        Ok(self.client.pull_projection_json_by_name(name)?)
    }

    pub fn pull_named(
        &self,
        root: &Path,
        name: &str,
        force: bool,
    ) -> Result<Vec<String>, CoreError> {
        self.pull_named_with_format(root, name, force, false)
    }

    pub fn pull_named_with_format(
        &self,
        root: &Path,
        name: &str,
        force: bool,
        format: bool,
    ) -> Result<Vec<String>, CoreError> {
        let remote = if let Some(resources) = self.load_replay_state_resources(root)? {
            resources
        } else {
            self.client.pull_resources_by_name(name)?
        };
        self.write_pulled_resources(root, remote, force, format)
    }

    fn pull_resources_with_branch_reconciliation(
        &self,
        root: &Path,
    ) -> Result<(ResourceMap, Option<BranchDescriptor>), CoreError> {
        let cfg = self.load_project_config(root)?;
        if cfg.branch_id == "main" {
            return Ok((self.client.pull_resources()?, None));
        }
        let branches = self.client.list_branches()?;
        if branches
            .iter()
            .any(|branch| branch.branch_id == cfg.branch_id || branch.name == cfg.branch_id)
        {
            return Ok((self.client.pull_resources()?, None));
        }
        let Some(branch) = branches
            .iter()
            .find(|branch| branch.name == "main" || branch.branch_id == "main")
            .or_else(|| branches.first())
            .cloned()
        else {
            return Ok((self.client.pull_resources()?, None));
        };
        let mut updated = cfg;
        updated.branch_id = branch.branch_id.clone();
        self.write_project_config(root, &updated)?;
        Ok((
            self.client.pull_resources_by_name(&branch.name)?,
            Some(branch),
        ))
    }

    fn write_pulled_resources(
        &self,
        root: &Path,
        remote: ResourceMap,
        force: bool,
        format: bool,
    ) -> Result<Vec<String>, CoreError> {
        let mut files_with_conflicts = Vec::new();
        let mut written_paths = Vec::new();
        let local_resources_before_force = force.then(|| self.discover_local_resources(root));
        let snapshot_hashes = if force {
            None
        } else {
            self.load_status_snapshot_file_hashes(root)?
        };
        for (path, resource) in &remote {
            let target = root.join(path);
            let content = resource
                .payload
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let file_content = resource_file_content(path, &content);
            if target.exists() && !force {
                let existing = fs::read_to_string(&target).unwrap_or_default();
                if existing.contains("<<<<<<<")
                    && existing.contains("=======")
                    && existing.contains(">>>>>>>")
                {
                    files_with_conflicts.push(target.to_string_lossy().to_string());
                    continue;
                }
                if let Some(snapshot_hash) =
                    snapshot_hashes.as_ref().and_then(|hashes| hashes.get(path))
                {
                    let local_changed = compute_hash(&existing) != *snapshot_hash;
                    let incoming_changed = compute_hash(&file_content) != *snapshot_hash;
                    if local_changed && !incoming_changed {
                        continue;
                    }
                    if local_changed && incoming_changed && existing != file_content {
                        let merged = format!(
                            "<<<<<<< local\n{existing}\n=======\n{file_content}\n>>>>>>> remote\n"
                        );
                        fs::write(&target, merged)?;
                        files_with_conflicts.push(target.to_string_lossy().to_string());
                        continue;
                    }
                }
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(target, file_content)?;
            written_paths.push(path.clone());
        }
        if let Some(local_resources) = local_resources_before_force.as_ref() {
            delete_local_only_resource_files(root, &remote, local_resources)?;
            delete_empty_subdirectories(&root.join("flows"))?;
        }
        if files_with_conflicts.is_empty() {
            let mut snapshot_resources = remote.clone();
            if format && !written_paths.is_empty() {
                self.format_local_resources(root, &written_paths, false)?;
                for path in &written_paths {
                    if let Some(resource) = snapshot_resources.get_mut(path)
                        && let Some(payload) = resource.payload.as_object_mut()
                    {
                        let formatted_content = fs::read_to_string(root.join(path))?;
                        payload.insert(
                            "content".to_string(),
                            Value::String(local_resource_content(path, &formatted_content)),
                        );
                    }
                }
            }
            self.save_replay_state_resources(root, &snapshot_resources)?;
            self.write_status_snapshot_from_resources(root, &snapshot_resources)?;
        }
        Ok(files_with_conflicts)
    }

    pub fn revert_changes(&self, root: &Path, files: &[String]) -> Result<Vec<String>, CoreError> {
        let remote = self.client.pull_resources()?;
        let all_files = files.is_empty();
        let selected: std::collections::HashSet<&str> = files.iter().map(String::as_str).collect();
        let mut reverted = Vec::new();
        for (path, resource) in remote {
            let target = root.join(&path);
            let target_abs = if target.is_absolute() {
                target.clone()
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(&target)
            };
            let target_abs_str = target_abs.to_string_lossy().to_string();
            if !all_files && !selected.contains(target_abs_str.as_str()) {
                continue;
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let content = resource
                .payload
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            fs::write(&target, resource_file_content(&path, &content))?;
            reverted.push(target_abs_str);
        }
        Ok(reverted)
    }

    pub fn list_deployments(&self, environment: &str) -> Result<DeploymentList, CoreError> {
        Ok(self.client.list_deployments(environment)?)
    }

    pub fn promote_deployment(
        &self,
        deployment_id: &str,
        target_env: &str,
        message: &str,
    ) -> Result<serde_json::Value, CoreError> {
        Ok(self
            .client
            .promote_deployment(deployment_id, target_env, message)?)
    }

    pub fn rollback_deployment(
        &self,
        deployment_id: &str,
        message: &str,
    ) -> Result<serde_json::Value, CoreError> {
        Ok(self.client.rollback_deployment(deployment_id, message)?)
    }

    pub fn create_chat_session(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, CoreError> {
        Ok(self.client.create_chat_session(payload)?)
    }

    pub fn send_chat_message(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, CoreError> {
        Ok(self.client.send_chat_message(payload)?)
    }

    pub fn end_chat_session(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, CoreError> {
        Ok(self.client.end_chat_session(payload)?)
    }

    pub fn conversation_url(
        &self,
        root: &Path,
        conversation_id: &str,
    ) -> Result<String, CoreError> {
        let cfg = self.load_project_config(root)?;
        let short_region = match cfg.region.as_str() {
            "uk-1" => "uk",
            "euw-1" => "eu",
            "us-1" => "us",
            other => other,
        };
        Ok(format!(
            "https://studio.{short_region}.poly.ai/{}/{}/conversations/{conversation_id}",
            cfg.account_id, cfg.project_id
        ))
    }

    pub fn current_branch(&self, root: &Path) -> Result<String, CoreError> {
        Ok(self.load_project_config(root)?.branch_id)
    }

    pub fn current_branch_name(&self, root: &Path) -> Result<String, CoreError> {
        let current_branch_id = self.current_branch(root)?;
        Ok(self
            .current_branch_name_optional(root)?
            .unwrap_or(current_branch_id))
    }

    pub fn current_branch_name_optional(&self, root: &Path) -> Result<Option<String>, CoreError> {
        let current_branch_id = self.current_branch(root)?;
        Ok(self
            .client
            .list_branches()?
            .into_iter()
            .find(|branch| {
                branch.branch_id == current_branch_id || branch.name == current_branch_id
            })
            .map(|branch| branch.name))
    }

    pub fn list_known_branches(&self, root: &Path) -> Result<Vec<String>, CoreError> {
        let _ = root;
        let names: Vec<String> = self
            .client
            .list_branches()?
            .into_iter()
            .map(|branch| branch.name)
            .collect();
        Ok(names)
    }

    pub fn list_branch_map(
        &self,
        root: &Path,
    ) -> Result<indexmap::IndexMap<String, String>, CoreError> {
        let _ = self.load_project_config(root)?;
        let mut branches = indexmap::IndexMap::new();
        for branch in self.client.list_branches()? {
            branches.insert(branch.name, branch.branch_id);
        }
        Ok(branches)
    }

    pub fn create_branch(
        &self,
        root: &Path,
        branch_name: &str,
    ) -> Result<ProjectConfig, CoreError> {
        let mut cfg = self.load_project_config(root)?;
        if cfg.branch_id != "main" {
            return Err(DomainError::InvalidData(
                "Branches can only be created from the main branch (sandbox).".to_string(),
            )
            .into());
        }
        let branch_id = self.client.create_branch(branch_name)?;
        cfg.branch_id = branch_id;
        self.write_project_config(root, &cfg)?;
        Ok(cfg)
    }

    pub fn set_branch(&self, root: &Path, branch_name: &str) -> Result<ProjectConfig, CoreError> {
        let mut cfg = self.load_project_config(root)?;
        cfg.branch_id = self
            .client
            .list_branches()?
            .into_iter()
            .find(|branch| branch.name == branch_name || branch.branch_id == branch_name)
            .map(|branch| branch.branch_id)
            .unwrap_or_else(|| branch_name.to_string());
        self.write_project_config(root, &cfg)?;
        Ok(cfg)
    }

    pub fn delete_branch(
        &self,
        root: &Path,
        branch_name: &str,
    ) -> Result<(bool, Option<String>), CoreError> {
        if branch_name == "main" {
            return Err(DomainError::InvalidData(format!(
                "Branch '{branch_name}' does not exist or cannot be deleted."
            ))
            .into());
        }
        let cfg = self.load_project_config(root)?;
        let branches = self.client.list_branches().map_err(|_| {
            DomainError::InvalidData(format!(
                "Branch '{branch_name}' does not exist or cannot be deleted."
            ))
        })?;
        let Some(branch_id) = branches
            .iter()
            .find(|branch| branch.name == branch_name)
            .map(|branch| branch.branch_id.clone())
        else {
            return Err(DomainError::InvalidData(format!(
                "Branch '{branch_name}' does not exist or cannot be deleted."
            ))
            .into());
        };
        self.client.delete_branch(&branch_id).map_err(|_| {
            DomainError::InvalidData(format!(
                "Branch '{branch_name}' does not exist or cannot be deleted."
            ))
        })?;
        let switched_to = if cfg.branch_id == branch_id || cfg.branch_id == branch_name {
            let mut updated_cfg = cfg;
            updated_cfg.branch_id = "main".to_string();
            self.write_project_config(root, &updated_cfg)?;
            Some("main".to_string())
        } else {
            None
        };
        Ok((true, switched_to))
    }

    pub fn merge_branch(
        &self,
        root: &Path,
        message: &str,
        conflict_resolutions: Option<Vec<serde_json::Value>>,
    ) -> Result<BranchMergeResult, CoreError> {
        let result = self.client.merge_branch(message, conflict_resolutions)?;
        if result.success {
            let mut cfg = self.load_project_config(root)?;
            cfg.branch_id = "main".to_string();
            self.write_project_config(root, &cfg)?;
            let local = self.collect_local_resources(root)?;
            self.write_status_snapshot_from_resources(root, &local)?;
        }
        Ok(result)
    }

    pub fn validate_local_resources(&self, root: &Path) -> Result<Vec<String>, CoreError> {
        let resources = self.collect_local_resources(root)?;
        let mut errors = Vec::new();
        for (path, resource) in &resources {
            let content = resource
                .payload
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if path.ends_with(".yaml") || path.ends_with(".yml") {
                let yaml = match serde_yaml::from_str::<serde_yaml::Value>(content) {
                    Ok(yaml) => yaml,
                    Err(e) => {
                        let _ = e;
                        return Err(
                            DomainError::InvalidData(resource_read_error(root, path)).into()
                        );
                    }
                };
                validate_semantic_resource(path, &yaml, &mut errors);
            } else if path.ends_with(".json")
                && let Err(e) = serde_json::from_str::<serde_json::Value>(content)
            {
                errors.push(format!("{path}: invalid json: {e}"));
            }
        }
        validate_python_function_resources(root, &resources)?;
        validate_flow_resources(root, &resources, &mut errors)?;
        Ok(errors)
    }

    pub fn format_local_resources(
        &self,
        root: &Path,
        files: &[String],
        check: bool,
    ) -> Result<Vec<String>, CoreError> {
        let resources = self.collect_local_resources(root)?;
        let file_patterns = normalize_format_file_patterns(root, files);
        let matcher = if files.is_empty() {
            None
        } else {
            Some(build_file_matcher(&file_patterns)?)
        };
        let mut changed_files = Vec::new();
        for (path, resource) in resources {
            if let Some(m) = &matcher
                && !m.is_match(&path)
            {
                continue;
            }
            let content = resource
                .payload
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let formatted = if path.ends_with(".yaml") || path.ends_with(".yml") {
                match serde_yaml::from_str::<serde_yaml::Value>(content) {
                    Ok(serde_yaml::Value::Null) | Err(_) => content.to_string(),
                    Ok(parsed) => serde_yaml::to_string(&parsed).map_err(|e| {
                        DomainError::InvalidData(format!("{path}: yaml error: {e}"))
                    })?,
                }
            } else if path.ends_with(".json") && !files.is_empty() {
                match serde_json::from_str::<serde_json::Value>(content) {
                    Ok(parsed) => {
                        let mut formatted = serde_json::to_string_pretty(&parsed).map_err(|e| {
                            DomainError::InvalidData(format!("{path}: json error: {e}"))
                        })?;
                        formatted.push('\n');
                        formatted
                    }
                    Err(_) => content.to_string(),
                }
            } else if path.ends_with(".py") {
                format_python_content(root.join(&path).as_path(), content)
            } else {
                continue;
            };
            if formatted != content {
                changed_files.push(path.clone());
                if !check {
                    fs::write(root.join(&path), resource_file_content(&path, &formatted))?;
                }
            }
        }
        self.order_formatted_files(root, changed_files)
    }

    fn order_formatted_files(
        &self,
        root: &Path,
        changed_files: Vec<String>,
    ) -> Result<Vec<String>, CoreError> {
        let mut remaining = changed_files.into_iter().collect::<BTreeSet<_>>();
        let Some(existing_typed) = self.load_status_snapshot_discovered_resources(root)? else {
            return Ok(remaining.into_iter().collect());
        };
        let discovered_typed = self.discover_local_resources(root);
        let typed_changes = self.find_new_kept_deleted(&discovered_typed, &existing_typed);
        let mut ordered = Vec::new();

        for paths_by_type in [&typed_changes.new_resources, &typed_changes.kept_resources] {
            for type_name in discover::ordered_type_names() {
                let Some(paths) = paths_by_type.get(*type_name) else {
                    continue;
                };
                for logical_path in paths {
                    let file_path = parse_multi_resource_path(logical_path).0;
                    if remaining.remove(&file_path) {
                        ordered.push(file_path);
                    }
                }
            }
        }

        ordered.extend(remaining);
        Ok(ordered)
    }

    fn load_status_snapshot_discovered_resources(
        &self,
        root: &Path,
    ) -> Result<Option<DiscoveredResourcePaths>, CoreError> {
        let status_path = root.join(STATUS_FILE);
        if !status_path.exists() {
            return Ok(None);
        }
        let encoded = fs::read(status_path)?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| DomainError::InvalidData(e.to_string()))?;
        let status_json: serde_json::Value = serde_json::from_slice(&decoded)?;

        let resources = match status_json.get("resources").and_then(|v| v.as_object()) {
            Some(r) => r,
            None => return Ok(Some(discover::empty_discovered_resource_paths())),
        };

        let mut discovered = discover::empty_discovered_resource_paths();
        for (resource_name, resource_entries) in resources {
            let Some(type_name) = discover::resource_name_to_type_name(resource_name) else {
                continue;
            };
            let Some(entries) = resource_entries.as_object() else {
                continue;
            };
            let mut paths = Vec::new();
            for resource_data in entries.values() {
                let Some(file_path) = resource_data.get("file_path").and_then(|v| v.as_str())
                else {
                    continue;
                };
                paths.push(file_path.replace('\\', "/"));
            }
            paths.sort();
            paths.dedup();
            discovered.insert(type_name.to_string(), paths);
        }
        Ok(Some(discovered))
    }

    fn resolve_named_state(&self, root: &Path, name: &str) -> Result<ResourceMap, CoreError> {
        if name == "local" {
            let mut resources = self.collect_local_resources(root)?;
            self.apply_status_resource_ids(root, &mut resources)?;
            return Ok(resources);
        }
        Ok(self.client.pull_resources_by_name(name)?)
    }

    fn apply_status_resource_ids(
        &self,
        root: &Path,
        resources: &mut ResourceMap,
    ) -> Result<(), CoreError> {
        let existing_resource_ids = self.load_status_snapshot_resource_ids(root)?;
        for resource in resources.values_mut() {
            let file_path = resource.file_path.replace('\\', "/");
            if let Some(resource_id) = existing_resource_ids.get(&file_path) {
                resource.resource_id = resource_id.clone();
            }
        }
        Ok(())
    }

    fn apply_deleted_reference_names(
        &self,
        root: &Path,
        resources: &mut ResourceMap,
    ) -> Result<(), CoreError> {
        let Some(existing_typed) = self.load_status_snapshot_discovered_resources(root)? else {
            return Ok(());
        };
        let discovered_typed = self.discover_local_resources(root);
        let typed_changes = self.find_new_kept_deleted(&discovered_typed, &existing_typed);
        let replacements =
            self.deleted_resource_reference_replacements(root, &typed_changes.deleted_resources)?;
        apply_reference_name_replacements(resources, &replacements);
        Ok(())
    }

    fn deleted_resource_reference_replacements(
        &self,
        root: &Path,
        deleted_resources: &DiscoveredResourcePaths,
    ) -> Result<Vec<ReferenceNameReplacement>, CoreError> {
        let existing_resource_ids = self.load_status_snapshot_resource_ids(root)?;
        let mut replacements = Vec::new();
        for (type_name, paths) in deleted_resources {
            let Some(prefix) = discover::type_name_to_resource_prefix(type_name) else {
                continue;
            };
            for logical_path in paths {
                let Some(resource_id) = existing_resource_ids.get(logical_path) else {
                    continue;
                };
                let name = reference_name_from_logical_path(logical_path);
                if !resource_id.is_empty() && !name.is_empty() && resource_id != &name {
                    replacements.push(ReferenceNameReplacement {
                        prefix: prefix.to_string(),
                        id: resource_id.clone(),
                        name,
                    });
                }
            }
        }
        Ok(replacements)
    }

    fn load_replay_state_resources(&self, root: &Path) -> Result<Option<ResourceMap>, CoreError> {
        let Some(path) = self.replay_state_path(root)? else {
            return Ok(None);
        };
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(path)?;
        Ok(Some(serde_json::from_str(&raw)?))
    }

    fn save_replay_state_resources(
        &self,
        root: &Path,
        resources: &ResourceMap,
    ) -> Result<(), CoreError> {
        let Some(path) = self.replay_state_path(root)? else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string(resources)?)?;
        Ok(())
    }

    fn replay_state_path(&self, root: &Path) -> Result<Option<PathBuf>, CoreError> {
        // Replay tests persist snapshots between recorded Python workflow steps.
        let Ok(state_dir) = env::var("POLY_ADK_REPLAY_STATE_DIR") else {
            return Ok(None);
        };
        let cfg = self.load_project_config(root)?;
        let branch = cfg
            .branch_id
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '_'
                }
            })
            .collect::<String>();
        Ok(Some(
            PathBuf::from(state_dir).join(format!("{branch}.json")),
        ))
    }

    fn write_status_snapshot_from_resources(
        &self,
        root: &Path,
        baseline: &ResourceMap,
    ) -> Result<(), CoreError> {
        let project_root = find_project_root(root).unwrap_or_else(|| root.to_path_buf());
        let baseline_file_paths: HashSet<String> = baseline
            .iter()
            .flat_map(|(path, resource)| [path.clone(), resource.file_path.clone()])
            .collect();
        let existing_resource_ids = self.load_status_snapshot_resource_ids(&project_root)?;
        let discovered = self.discover_local_resources(&project_root);
        let mut resources = serde_json::Map::new();
        let mut file_structure_metadata = BTreeMap::new();

        for (type_name, paths) in discovered {
            let Some(resource_name) = discover::type_name_to_resource_name(&type_name) else {
                continue;
            };
            let mut entries = serde_json::Map::new();
            for logical_path in paths {
                let (file_path, resource_suffix) = parse_multi_resource_path(&logical_path);
                if !baseline_file_paths.contains(&file_path) {
                    continue;
                }
                let status_resource_name = resource_suffix
                    .clone()
                    .or_else(|| {
                        baseline
                            .get(&file_path)
                            .map(|resource| resource.name.clone())
                    })
                    .unwrap_or_else(|| logical_path.clone());
                let resource_id = existing_resource_ids
                    .get(&logical_path)
                    .cloned()
                    .or_else(|| {
                        resource_suffix
                            .as_ref()
                            .map(|suffix| format!("{resource_name}:{}", suffix.replace('/', ":")))
                    })
                    .or_else(|| {
                        baseline
                            .get(&file_path)
                            .map(|resource| resource.resource_id.clone())
                    })
                    .unwrap_or_else(|| logical_path.clone());
                file_structure_metadata.insert(
                    file_path.clone(),
                    (
                        resource_name.to_string(),
                        resource_id.clone(),
                        status_resource_name,
                    ),
                );
                entries.insert(
                    resource_id.clone(),
                    serde_json::json!({
                        "resource_id": resource_id,
                        "name": logical_path,
                        "file_path": logical_path,
                    }),
                );
            }
            if !entries.is_empty() {
                resources.insert(
                    resource_name.to_string(),
                    serde_json::Value::Object(entries),
                );
            }
        }

        let mut file_structure_info = serde_json::Map::new();
        for (file_path, (resource_type, resource_id, resource_name)) in file_structure_metadata {
            let content = fs::read_to_string(project_root.join(&file_path)).unwrap_or_default();
            file_structure_info.insert(
                file_path.clone(),
                serde_json::json!({
                    "type": resource_type,
                    "resource_id": resource_id,
                    "resource_name": resource_name,
                    "hash": compute_hash(&content),
                }),
            );
        }

        let config = self.load_project_config(&project_root).ok();
        let branch_id = config
            .as_ref()
            .map(|cfg| cfg.branch_id.clone())
            .unwrap_or_else(|| "main".to_string());
        let migration_flags = self.run_and_persist_project_migrations(&project_root)?;
        let status = serde_json::json!({
            "region": config.as_ref().map(|cfg| cfg.region.clone()).unwrap_or_default(),
            "account_id": config.as_ref().map(|cfg| cfg.account_id.clone()).unwrap_or_default(),
            "project_id": config.as_ref().map(|cfg| cfg.project_id.clone()).unwrap_or_default(),
            "project_name": config.as_ref().and_then(|cfg| cfg.project_name.clone()),
            "resources": resources,
            "last_updated": chrono::Utc::now().to_rfc3339(),
            "file_structure_info": file_structure_info,
            "branch_id": branch_id,
            "migration_flags": migration_flags.into_iter().collect::<Vec<_>>(),
        });
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&status)?);
        let gen_dir = project_root.join("_gen");
        self.write_python_gen_package(&project_root)?;
        fs::write(gen_dir.join(".agent_studio_config"), encoded)?;
        Ok(())
    }

    fn write_python_gen_package(&self, project_root: &Path) -> Result<(), CoreError> {
        let gen_dir = project_root.join("_gen");
        fs::create_dir_all(&gen_dir)?;
        for entry in fs::read_dir(&gen_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|extension| extension == "pyi") {
                fs::remove_file(path)?;
            }
        }
        for (file_name, contents) in PYTHON_GEN_TEMPLATE_FILES {
            fs::write(gen_dir.join(file_name), contents)?;
        }
        Ok(())
    }

    fn run_and_persist_project_migrations(
        &self,
        project_root: &Path,
    ) -> Result<BTreeSet<String>, CoreError> {
        let mut status = self.load_status_snapshot_json(project_root)?;
        let mut migration_flags = migration_flags_from_status(&status);
        if !migration_flags.contains(MIGRATED_LEGACY_TOPIC_FILES) {
            let had_status_snapshot = project_root.join(STATUS_FILE).exists();
            let migrated_files = migrate_legacy_topic_files(project_root)?;
            migration_flags.insert(MIGRATED_LEGACY_TOPIC_FILES.to_string());
            if had_status_snapshot || migrated_files {
                status.insert(
                    "migration_flags".to_string(),
                    serde_json::Value::Array(
                        migration_flags
                            .iter()
                            .cloned()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                );
                self.write_python_gen_package(project_root)?;
                self.write_status_snapshot_json(project_root, &status)?;
            }
        }
        Ok(migration_flags)
    }

    fn load_status_snapshot_json(
        &self,
        project_root: &Path,
    ) -> Result<serde_json::Map<String, serde_json::Value>, CoreError> {
        let status_path = project_root.join(STATUS_FILE);
        if !status_path.exists() {
            return Ok(serde_json::Map::new());
        }
        let encoded = fs::read(status_path)?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| DomainError::InvalidData(e.to_string()))?;
        let value: serde_json::Value = serde_json::from_slice(&decoded)?;
        Ok(value.as_object().cloned().unwrap_or_default())
    }

    fn write_status_snapshot_json(
        &self,
        project_root: &Path,
        status: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), CoreError> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(status)?);
        let status_path = project_root.join(STATUS_FILE);
        if let Some(parent) = status_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(status_path, encoded)?;
        Ok(())
    }

    fn write_project_config(&self, root: &Path, cfg: &ProjectConfig) -> Result<(), CoreError> {
        let project_root = find_project_root(root)
            .ok_or_else(|| DomainError::ConfigNotFound(root.to_string_lossy().to_string()))?;
        let serialized =
            serde_yaml::to_string(cfg).map_err(|e| DomainError::InvalidData(e.to_string()))?;
        fs::write(project_root.join(PROJECT_CONFIG_FILE), serialized)?;
        Ok(())
    }

    fn detect_conflict_files(&self, root: &Path) -> Result<Vec<String>, CoreError> {
        let mut conflicts = Vec::new();
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let content = match fs::read_to_string(path) {
                Ok(content) => content,
                Err(_) => continue,
            };
            if content.contains("<<<<<<<")
                && content.contains("=======")
                && content.contains(">>>>>>>")
            {
                conflicts.push(path.to_string_lossy().to_string());
            }
        }
        Ok(conflicts)
    }

    fn load_status_snapshot_file_hashes(
        &self,
        root: &Path,
    ) -> Result<Option<indexmap::IndexMap<String, String>>, CoreError> {
        let status_path = root.join(STATUS_FILE);
        if !status_path.exists() {
            return Ok(None);
        }
        let encoded = fs::read(status_path)?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| DomainError::InvalidData(e.to_string()))?;
        let status_json: serde_json::Value = serde_json::from_slice(&decoded)?;
        let Some(file_structure_info) = status_json
            .get("file_structure_info")
            .and_then(|v| v.as_object())
        else {
            return Ok(None);
        };
        let mut out = indexmap::IndexMap::new();
        for (file_path, info) in file_structure_info {
            let Some(hash) = info.get("hash").and_then(|v| v.as_str()) else {
                continue;
            };
            out.insert(file_path.replace('\\', "/"), hash.to_string());
        }
        Ok(Some(out))
    }

    fn load_status_snapshot_resource_ids(
        &self,
        root: &Path,
    ) -> Result<indexmap::IndexMap<String, String>, CoreError> {
        let status_path = root.join(STATUS_FILE);
        if !status_path.exists() {
            return Ok(indexmap::IndexMap::new());
        }
        let encoded = fs::read(status_path)?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| DomainError::InvalidData(e.to_string()))?;
        let status_json: serde_json::Value = serde_json::from_slice(&decoded)?;
        let Some(resources) = status_json.get("resources").and_then(|v| v.as_object()) else {
            return Ok(indexmap::IndexMap::new());
        };
        let mut ids = indexmap::IndexMap::new();
        for entries in resources.values() {
            let Some(entries) = entries.as_object() else {
                continue;
            };
            for payload in entries.values() {
                let Some(path) = payload.get("file_path").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(id) = payload.get("resource_id").and_then(|v| v.as_str()) else {
                    continue;
                };
                ids.insert(path.replace('\\', "/"), id.to_string());
            }
        }
        Ok(ids)
    }
}

fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(PROJECT_CONFIG_FILE).exists() || current.join(STATUS_FILE).exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn migration_flags_from_status(
    status: &serde_json::Map<String, serde_json::Value>,
) -> BTreeSet<String> {
    status
        .get("migration_flags")
        .and_then(serde_json::Value::as_array)
        .map(|flags| {
            flags
                .iter()
                .filter_map(serde_json::Value::as_str)
                .filter(|flag| *flag == MIGRATED_LEGACY_TOPIC_FILES)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn migrate_legacy_topic_files(project_root: &Path) -> Result<bool, CoreError> {
    let topics_dir = project_root.join("topics");
    if !topics_dir.is_dir() {
        return Ok(false);
    }

    let mut migrated_topics: std::collections::BTreeMap<PathBuf, serde_yaml::Value> =
        std::collections::BTreeMap::new();
    let mut old_files = Vec::new();
    let mut old_dirs = BTreeSet::new();

    for entry in WalkDir::new(&topics_dir).into_iter().filter_map(Result::ok) {
        let topic_path = entry.path();
        if !topic_path.is_file() || !is_yaml_file(topic_path) {
            continue;
        }
        let raw = fs::read_to_string(topic_path)?;
        let Ok(parsed) = serde_yaml::from_str::<serde_yaml::Value>(&raw) else {
            continue;
        };
        let serde_yaml::Value::Mapping(existing) = parsed else {
            continue;
        };
        if yaml_mapping_contains_key(&existing, "name") {
            continue;
        }

        let rel_path = topic_path.strip_prefix(&topics_dir).unwrap_or(topic_path);
        let topic_name = rel_path
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");
        let clean_file_name = discover::clean_name(&topic_name, true);
        let clean_file_path = topics_dir.join(format!("{clean_file_name}.yaml"));
        if migrated_topics.contains_key(&clean_file_path) {
            return Err(DomainError::InvalidData(format!(
                "Can't migrate legacy topic files: multiple topics with the same file name after cleaning: {clean_file_name}"
            ))
            .into());
        }

        let mut updated = serde_yaml::Mapping::new();
        updated.insert(
            serde_yaml::Value::String("name".to_string()),
            serde_yaml::Value::String(topic_name),
        );
        for (key, value) in existing {
            updated.insert(key, value);
        }
        migrated_topics.insert(clean_file_path, serde_yaml::Value::Mapping(updated));
        old_files.push(topic_path.to_path_buf());
        if topic_path.parent() != Some(topics_dir.as_path())
            && let Some(parent) = topic_path.parent()
        {
            old_dirs.insert(parent.to_path_buf());
        }
    }

    for (path, content) in &migrated_topics {
        let serialized =
            serde_yaml::to_string(content).map_err(|e| DomainError::InvalidData(e.to_string()))?;
        fs::write(path, serialized)?;
    }
    for old_file in old_files {
        if !migrated_topics.contains_key(&old_file) {
            fs::remove_file(old_file)?;
        }
    }
    let mut old_dirs = old_dirs.into_iter().collect::<Vec<_>>();
    old_dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for old_dir in old_dirs {
        if old_dir.is_dir() && fs::read_dir(&old_dir)?.next().is_none() {
            fs::remove_dir(old_dir)?;
        }
    }
    Ok(!migrated_topics.is_empty())
}

fn yaml_mapping_contains_key(mapping: &serde_yaml::Mapping, key: &str) -> bool {
    mapping
        .keys()
        .any(|candidate| candidate.as_str() == Some(key))
}

fn is_yaml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "yaml" | "yml"))
}

fn delete_local_only_resource_files(
    root: &Path,
    remote: &ResourceMap,
    local_resources: &DiscoveredResourcePaths,
) -> Result<(), CoreError> {
    let remote_file_paths: HashSet<String> = remote
        .iter()
        .flat_map(|(path, resource)| [path.clone(), resource.file_path.clone()])
        .map(|path| parse_multi_resource_path(&path).0)
        .collect();
    let mut local_only_files: Vec<String> = flatten_discovered_paths(local_resources)
        .into_iter()
        .map(|path| parse_multi_resource_path(&path).0)
        .filter(|path| !remote_file_paths.contains(path))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    local_only_files.sort_by_key(|path| {
        std::cmp::Reverse((Path::new(path).components().count(), path.clone()))
    });

    for rel_path in local_only_files {
        let path = root.join(rel_path);
        if path.is_file() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn delete_empty_subdirectories(dir: &Path) -> Result<(), CoreError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in WalkDir::new(dir)
        .contents_first(true)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_dir() && fs::read_dir(path)?.next().is_none() {
            fs::remove_dir(path)?;
        }
    }
    Ok(())
}

fn normalize_format_file_patterns(root: &Path, files: &[String]) -> Vec<String> {
    files
        .iter()
        .map(|file| {
            let path = Path::new(file);
            let rel = if path.is_absolute() {
                path.strip_prefix(root).unwrap_or(path).to_path_buf()
            } else {
                path.to_path_buf()
            };
            rel.to_string_lossy().replace('\\', "/")
        })
        .collect()
}

fn build_file_matcher(patterns: &[String]) -> Result<globset::GlobSet, CoreError> {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        let glob = Glob::new(p).map_err(|e| DomainError::InvalidData(e.to_string()))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| DomainError::InvalidData(e.to_string()).into())
}

fn validate_flow_resources(
    root: &Path,
    resources: &ResourceMap,
    errors: &mut Vec<String>,
) -> Result<(), CoreError> {
    let flow_steps = flow_validation_step_names(resources);
    let entity_ids = flow_validation_entity_ids(resources);

    let mut step_paths = resources
        .keys()
        .filter(|path| {
            path.starts_with("flows/") && path.contains("/steps/") && path.ends_with(".yaml")
        })
        .cloned()
        .collect::<Vec<_>>();
    step_paths.sort();
    for path in step_paths {
        let Some(yaml) = resource_yaml_content(resources, &path) else {
            continue;
        };
        validate_flow_step_resource(&path, &yaml, &flow_steps, &entity_ids, errors);
    }

    let mut function_step_paths = resources
        .keys()
        .filter(|path| {
            path.starts_with("flows/") && path.contains("/function_steps/") && path.ends_with(".py")
        })
        .cloned()
        .collect::<Vec<_>>();
    function_step_paths.sort();
    for path in function_step_paths {
        let Some(content) = resource_content(resources, &path) else {
            continue;
        };
        validate_flow_function_step_resource(root, &path, content, errors)?;
    }

    let mut transition_function_paths = resources
        .keys()
        .filter(|path| {
            path.starts_with("flows/") && path.contains("/functions/") && path.ends_with(".py")
        })
        .cloned()
        .collect::<Vec<_>>();
    transition_function_paths.sort();
    for path in transition_function_paths {
        let Some(content) = resource_content(resources, &path) else {
            continue;
        };
        validate_flow_transition_function_resource(root, &path, content, errors)?;
    }

    let mut config_paths = resources
        .keys()
        .filter(|path| path.starts_with("flows/") && path.ends_with("/flow_config.yaml"))
        .cloned()
        .collect::<Vec<_>>();
    config_paths.sort();
    for path in config_paths {
        let Some(yaml) = resource_yaml_content(resources, &path) else {
            continue;
        };
        validate_flow_config_resource(&path, &yaml, &flow_steps, errors);
    }
    Ok(())
}

fn validate_python_function_resources(
    root: &Path,
    resources: &ResourceMap,
) -> Result<(), CoreError> {
    let mut paths = resources
        .keys()
        .filter(|path| is_python_function_resource(path))
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let Some(content) = resource_content(resources, &path) else {
            continue;
        };
        validate_python_resource_syntax(root, &path, content)?;
        validate_function_parameter_decorators(root, &path, content)?;
    }
    Ok(())
}

fn validate_function_parameter_decorators(
    root: &Path,
    path: &str,
    content: &str,
) -> Result<(), CoreError> {
    let function_name = reference_name_from_logical_path(path);
    let Some(parameters) = function_signature_parameters(content, &function_name) else {
        return Ok(());
    };
    for parameter_name in function_parameter_decorator_names(content) {
        let Some(annotation) = parameters.get(&parameter_name) else {
            return Err(DomainError::InvalidData(resource_read_error_with_detail(
                root,
                path,
                &format!(
                    "Parameter '{parameter_name}' has no type annotation. Supported types: str, int, float, bool."
                ),
            ))
            .into());
        };
        let Some(annotation) = annotation else {
            return Err(DomainError::InvalidData(resource_read_error_with_detail(
                root,
                path,
                &format!(
                    "Parameter '{parameter_name}' has no type annotation. Supported types: str, int, float, bool."
                ),
            ))
            .into());
        };
        if !matches!(annotation.as_str(), "str" | "int" | "float" | "bool") {
            return Err(DomainError::InvalidData(resource_read_error_with_detail(
                root,
                path,
                &format!(
                    "Parameter '{parameter_name}' has an unsupported type annotation. Supported types: str, int, float, bool."
                ),
            ))
            .into());
        }
    }
    Ok(())
}

#[derive(Debug)]
struct FunctionSignatureParameter {
    name: String,
    annotation: Option<String>,
}

fn function_signature_parameter_list(
    content: &str,
    function_name: &str,
) -> Option<Vec<FunctionSignatureParameter>> {
    let prefix = format!("def {function_name}(");
    let signature = content
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(&prefix))?;
    let open = signature.find('(')?;
    let close = signature[open + 1..].find(')')?;
    let params = &signature[open + 1..open + 1 + close];
    Some(
        params
            .split(',')
            .map(str::trim)
            .filter(|param| !param.is_empty())
            .filter_map(|param| {
                let before_default = param.split('=').next().unwrap_or_default().trim();
                let (name, annotation) = before_default
                    .split_once(':')
                    .map(|(name, annotation)| {
                        (name.trim().to_string(), Some(annotation.trim().to_string()))
                    })
                    .unwrap_or_else(|| (before_default.to_string(), None));
                (!name.is_empty()).then_some(FunctionSignatureParameter { name, annotation })
            })
            .collect(),
    )
}

fn function_signature_parameters(
    content: &str,
    function_name: &str,
) -> Option<HashMap<String, Option<String>>> {
    Some(
        function_signature_parameter_list(content, function_name)?
            .into_iter()
            .map(|parameter| (parameter.name, parameter.annotation))
            .collect(),
    )
}

fn function_parameter_decorator_names(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("@func_parameter(")?;
            let args = parse_python_string_args(rest.strip_suffix(')').unwrap_or(rest));
            args.first().cloned()
        })
        .collect()
}

fn parse_python_string_args(value: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for ch in value.chars() {
        if let Some(active_quote) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                current.push(ch);
            }
            ',' => {
                args.push(parse_python_string_literal(current.trim()));
                current.clear();
            }
            other => current.push(other),
        }
    }
    if !current.trim().is_empty() {
        args.push(parse_python_string_literal(current.trim()));
    }
    args
}

fn parse_python_string_literal(value: &str) -> String {
    let mut chars = value.chars();
    let Some(quote @ ('\'' | '"')) = chars.next() else {
        return value.to_string();
    };
    let mut out = String::new();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            match ch {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                other => out.push(other),
            }
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            break;
        } else {
            out.push(ch);
        }
    }
    out
}

fn is_python_function_resource(path: &str) -> bool {
    ((path.starts_with("functions/") && path.ends_with(".py"))
        || (path.starts_with("flows/") && path.contains("/functions/") && path.ends_with(".py")))
        && !path.contains("/function_steps/")
}

#[derive(Debug, Default)]
struct FlowValidationNames {
    by_flow: HashMap<String, BTreeSet<String>>,
}

impl FlowValidationNames {
    fn contains(&self, flow_name: &str, step_name: &str) -> bool {
        self.by_flow
            .get(flow_name)
            .is_some_and(|steps| steps.contains(step_name))
    }
}

fn flow_validation_step_names(resources: &ResourceMap) -> FlowValidationNames {
    let mut names = FlowValidationNames::default();
    for path in resources.keys() {
        let Some(flow_name) = flow_name_from_resource_path(path) else {
            continue;
        };
        if path.starts_with("flows/") && path.contains("/steps/") && path.ends_with(".yaml") {
            if let Some(stem) = path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".yaml"))
            {
                names
                    .by_flow
                    .entry(flow_name.to_string())
                    .or_default()
                    .insert(stem.to_string());
            }
        } else if path.starts_with("flows/")
            && path.contains("/function_steps/")
            && path.ends_with(".py")
            && let Some(stem) = path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".py"))
        {
            names
                .by_flow
                .entry(flow_name.to_string())
                .or_default()
                .insert(stem.to_string());
        }
    }
    names
}

fn flow_validation_entity_ids(resources: &ResourceMap) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let Some(yaml) = resource_yaml_content(resources, "config/entities.yaml") else {
        return ids;
    };
    let Some(items) = yaml
        .get("entities")
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return ids;
    };
    for item in items {
        let Some(name) = item.get("name").and_then(serde_yaml::Value::as_str) else {
            continue;
        };
        ids.insert(format!("ENTITY-{name}"));
        ids.insert(name.to_string());
    }
    ids
}

fn validate_flow_config_resource(
    path: &str,
    yaml: &serde_yaml::Value,
    flow_steps: &FlowValidationNames,
    errors: &mut Vec<String>,
) {
    let flow_name = flow_name_from_resource_path(path).unwrap_or_default();
    let start_step = yaml
        .get("start_step")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if start_step.is_empty() {
        errors.push(format!(
            "Validation error in {path}: Start step cannot be empty."
        ));
        return;
    }
    if !flow_steps.contains(flow_name, start_step) {
        errors.push(format!(
            "Validation error in {path}: Start step '{start_step}' not found."
        ));
        return;
    }
    let description = yaml
        .get("description")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if description.is_empty() {
        errors.push(format!(
            "Validation error in {path}: Description cannot be empty."
        ));
    } else if description != description.trim() {
        errors.push(format!(
            "Validation error in {path}: Description cannot contain leading or trailing whitespace."
        ));
    }
}

fn validate_flow_step_resource(
    path: &str,
    yaml: &serde_yaml::Value,
    flow_steps: &FlowValidationNames,
    entity_ids: &BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    let flow_name = flow_name_from_resource_path(path).unwrap_or_default();
    let name = yaml
        .get("name")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if name.is_empty() {
        errors.push(format!("Validation error in {path}: Name cannot be empty."));
        return;
    }
    if !valid_flow_step_name(name) {
        errors.push(format!(
            "Validation error in {path}: Name must contain only letters (including accented), numbers, and _ & , / . -"
        ));
        return;
    }
    let prompt = yaml
        .get("prompt")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if prompt.trim().is_empty() {
        errors.push(format!(
            "Validation error in {path}: Prompt cannot be empty."
        ));
        return;
    }
    let step_type = yaml
        .get("step_type")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if !matches!(
        step_type,
        "advanced_step" | "default_step" | "function_step"
    ) {
        errors.push(format!(
            "Validation error in {path}: Invalid step type: {step_type}. Valid types: ['advanced_step', 'default_step', 'function_step']"
        ));
        return;
    }
    let function_references = prompt_function_references(prompt);
    if step_type == "default_step" && !function_references.is_empty() {
        errors.push(format!(
            "Validation error in {path}: Default steps cannot reference functions. Found function references: [{}]",
            python_string_list(&function_references)
        ));
        return;
    }
    if step_type == "default_step"
        && let Some(conditions) = yaml
            .get("conditions")
            .and_then(serde_yaml::Value::as_sequence)
    {
        for condition in conditions {
            validate_flow_condition(path, condition, flow_name, flow_steps, entity_ids, errors);
        }
    }
}

fn validate_flow_condition(
    path: &str,
    condition: &serde_yaml::Value,
    flow_name: &str,
    flow_steps: &FlowValidationNames,
    entity_ids: &BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    let name = condition
        .get("name")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if name.is_empty() {
        errors.push(format!(
            "Validation error in {path}: Condition '{name}': Condition name cannot be empty."
        ));
        return;
    }
    let condition_type = condition
        .get("condition_type")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("exit_flow_condition");
    if condition_type != "exit_flow_condition" {
        let child_step = condition
            .get("child_step")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or_default();
        if !flow_steps.contains(flow_name, child_step) {
            errors.push(format!(
                "Validation error in {path}: Condition '{name}': Step '{child_step}' not found"
            ));
            return;
        }
    }
    let missing_entities = condition
        .get("required_entities")
        .and_then(serde_yaml::Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(serde_yaml::Value::as_str)
        .filter(|entity| !entity_ids.contains(*entity))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if !missing_entities.is_empty() {
        errors.push(format!(
            "Validation error in {path}: Condition '{name}': Required entities not found: {{{}}}",
            python_string_set(&missing_entities)
        ));
        return;
    }
    let description = condition
        .get("description")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if !description.is_empty() && description != description.trim() {
        errors.push(format!(
            "Validation error in {path}: Condition '{name}': Description cannot contain leading or trailing whitespace."
        ));
    }
}

fn validate_flow_function_step_resource(
    root: &Path,
    path: &str,
    content: &str,
    errors: &mut Vec<String>,
) -> Result<(), CoreError> {
    validate_flow_scoped_function_resource(root, path, content, errors, false)
}

fn validate_flow_transition_function_resource(
    root: &Path,
    path: &str,
    content: &str,
    errors: &mut Vec<String>,
) -> Result<(), CoreError> {
    validate_flow_scoped_function_resource(root, path, content, errors, true)
}

fn validate_flow_scoped_function_resource(
    root: &Path,
    path: &str,
    content: &str,
    errors: &mut Vec<String>,
    allow_user_parameters: bool,
) -> Result<(), CoreError> {
    validate_python_resource_syntax(root, path, content)?;
    validate_function_parameter_decorators(root, path, content)?;
    let Some(file_name) = path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".py"))
    else {
        return Ok(());
    };
    let expected_typed = format!("def {file_name}(conv: Conversation, flow: Flow)");
    let valid_signature = if allow_user_parameters {
        flow_scoped_signature_has_receiver_prefix(content, file_name)
    } else {
        content.contains(&expected_typed)
            || content.contains(&format!("def {file_name}(conv, flow)"))
    };
    if !valid_signature {
        errors.push(format!(
            "Validation error in {path}: Function definition '{expected_typed}' not found in code."
        ));
    }
    Ok(())
}

fn flow_scoped_signature_has_receiver_prefix(content: &str, function_name: &str) -> bool {
    let Some(parameters) = function_signature_parameter_list(content, function_name) else {
        return false;
    };
    let Some(conv) = parameters.first() else {
        return false;
    };
    let Some(flow) = parameters.get(1) else {
        return false;
    };
    conv.name == "conv"
        && conv
            .annotation
            .as_deref()
            .is_none_or(|annotation| annotation == "Conversation")
        && flow.name == "flow"
        && flow
            .annotation
            .as_deref()
            .is_none_or(|annotation| annotation == "Flow")
}

fn validate_python_resource_syntax(
    root: &Path,
    path: &str,
    content: &str,
) -> Result<(), CoreError> {
    if let Err(error) = validate_python_module(content) {
        return Err(DomainError::InvalidData(resource_read_error_with_detail(
            root,
            path,
            &error.to_string(),
        ))
        .into());
    }
    Ok(())
}

fn resource_content<'a>(resources: &'a ResourceMap, path: &str) -> Option<&'a str> {
    resources.get(path)?.payload.get("content")?.as_str()
}

fn resource_yaml_content(resources: &ResourceMap, path: &str) -> Option<serde_yaml::Value> {
    serde_yaml::from_str(resource_content(resources, path)?).ok()
}

fn flow_name_from_resource_path(path: &str) -> Option<&str> {
    let mut parts = path.split('/');
    (parts.next()? == "flows").then_some(())?;
    parts.next()
}

fn valid_flow_step_name(name: &str) -> bool {
    name.chars()
        .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | ' ' | '&' | ',' | '/' | '.' | '-'))
}

fn prompt_function_references(prompt: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut rest = prompt;
    while let Some(index) = rest.find("{{f") {
        rest = &rest[index + 3..];
        let Some(prefix_end) = rest.find(':') else {
            continue;
        };
        let prefix = &rest[..prefix_end];
        if prefix != "n" && prefix != "t" {
            continue;
        }
        let tail = &rest[prefix_end + 1..];
        let Some(end) = tail.find("}}") else {
            break;
        };
        let name = tail[..end].trim();
        if !name.is_empty() {
            refs.push(name.to_string());
        }
        rest = &tail[end + 2..];
    }
    refs
}

fn python_string_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn python_string_set(values: &[String]) -> String {
    let mut values = values.to_vec();
    values.sort();
    python_string_list(&values)
}

fn validate_semantic_resource(path: &str, yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    match path {
        "config/api_integrations.yaml" => validate_api_integrations(yaml, errors),
        "config/entities.yaml" => validate_entities(yaml, errors),
        "config/handoffs.yaml" => {
            validate_named_sequence(path, yaml, "handoffs", "handoff", errors)
        }
        "config/sms_templates.yaml" => {
            validate_named_sequence(path, yaml, "sms_templates", "SMS template", errors)
        }
        "config/variant_attributes.yaml" => validate_variant_defaults(yaml, errors),
        "voice/speech_recognition/transcript_corrections.yaml" => {
            validate_transcript_corrections(yaml, errors)
        }
        _ if path.starts_with("topics/") => validate_topic(path, yaml, errors),
        _ => {}
    }
}

fn validate_api_integrations(yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    validate_named_sequence(
        "config/api_integrations.yaml",
        yaml,
        "api_integrations",
        "API integration",
        errors,
    );
    let Some(items) = yaml
        .get("api_integrations")
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return;
    };
    for item in items {
        let Some(raw_name) = item.get("name").and_then(serde_yaml::Value::as_str) else {
            continue;
        };
        let name = discover::clean_name(raw_name, false);
        if !is_python_function_name(&name) {
            errors.push(format!(
                "Validation error in config/api_integrations.yaml/api_integrations/{name}: API integration name '{name}' must follow Python function naming convention (lowercase letters, numbers, and underscores only, starting with letter or underscore)."
            ));
        }
    }
}

fn validate_entities(yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    validate_named_sequence("config/entities.yaml", yaml, "entities", "entity", errors);
    let Some(items) = yaml
        .get("entities")
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return;
    };
    let allowed = [
        "numeric",
        "alphanumeric",
        "enum",
        "date",
        "phone_number",
        "time",
        "address",
        "free_text",
        "name_config",
    ];
    for item in items {
        let name = item
            .get("name")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("<missing>");
        let Some(entity_type) = item.get("entity_type").and_then(serde_yaml::Value::as_str) else {
            errors.push(format!(
                "Validation error in config/entities.yaml/entities/{name}: entity_type is required."
            ));
            continue;
        };
        if !allowed.contains(&entity_type) {
            errors.push(format!(
                "Validation error in config/entities.yaml/entities/{name}: unsupported entity_type '{entity_type}'."
            ));
        }
    }
}

fn validate_variant_defaults(yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    let Some(variants) = yaml
        .get("variants")
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return;
    };
    validate_duplicate_names(
        "config/variant_attributes.yaml",
        "variants",
        "variant",
        variants,
        errors,
    );
    let default_names = variants
        .iter()
        .filter(|variant| {
            variant
                .get("is_default")
                .and_then(serde_yaml::Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|variant| variant.get("name").and_then(serde_yaml::Value::as_str))
        .collect::<Vec<_>>();
    if default_names.len() != 1 {
        let names = default_names
            .iter()
            .map(|name| format!("'{name}'"))
            .collect::<Vec<_>>()
            .join(", ");
        errors.push(format!(
            "Validation error: Multiple or zero default variants detected: [{names}]. One variant must be set as default."
        ));
    }
}

fn validate_topic(path: &str, yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    if yaml
        .get("name")
        .and_then(serde_yaml::Value::as_str)
        .is_none_or(str::is_empty)
    {
        errors.push(format!(
            "Validation error in {path}: topic name is required."
        ));
    }
}

fn validate_named_sequence(
    path: &str,
    yaml: &serde_yaml::Value,
    key: &str,
    label: &str,
    errors: &mut Vec<String>,
) {
    let Some(items) = yaml.get(key).and_then(serde_yaml::Value::as_sequence) else {
        return;
    };
    for (idx, item) in items.iter().enumerate() {
        if item
            .get("name")
            .and_then(serde_yaml::Value::as_str)
            .is_none_or(str::is_empty)
        {
            errors.push(format!(
                "Validation error in {path}/{key}/{idx}: {label} name is required."
            ));
        }
    }
    validate_duplicate_names(path, key, label, items, errors);
}

fn validate_duplicate_names(
    path: &str,
    key: &str,
    label: &str,
    items: &[serde_yaml::Value],
    errors: &mut Vec<String>,
) {
    let mut seen = std::collections::BTreeSet::new();
    let mut duplicates = std::collections::BTreeSet::new();
    for item in items {
        let Some(name) = item.get("name").and_then(serde_yaml::Value::as_str) else {
            continue;
        };
        if !seen.insert(name.to_string()) {
            duplicates.insert(name.to_string());
        }
    }
    for name in duplicates {
        errors.push(format!(
            "Validation error in {path}/{key}/{name}: duplicate {label} name '{name}'."
        ));
    }
}

fn validate_transcript_corrections(yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    let Some(corrections) = yaml
        .get("corrections")
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return;
    };
    for correction in corrections {
        let Some(raw_name) = correction.get("name").and_then(serde_yaml::Value::as_str) else {
            continue;
        };
        let regular_expression_count = correction
            .get("regular_expressions")
            .and_then(serde_yaml::Value::as_sequence)
            .map(Vec::len)
            .unwrap_or(0);
        if regular_expression_count == 0 {
            let name = discover::clean_name(raw_name, false);
            errors.push(format!(
                "Validation error in voice/speech_recognition/transcript_corrections.yaml/corrections/{name}: At least one regular expression rule is required"
            ));
        }
    }
}

fn is_python_function_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_lowercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_lowercase() || ch.is_ascii_digit())
}

fn format_python_content(filename: &Path, content: &str) -> String {
    let fallback = || ensure_trailing_newline(content);
    let filename = filename.to_string_lossy().to_string();
    let Ok(mut child) = Command::new("ruff")
        .args(["format", "--stdin-filename", &filename, "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return fallback();
    };
    let Some(stdin) = child.stdin.as_mut() else {
        return fallback();
    };
    if stdin.write_all(content.as_bytes()).is_err() {
        return fallback();
    }
    let Ok(output) = child.wait_with_output() else {
        return fallback();
    };
    if !output.status.success() {
        return fallback();
    }
    String::from_utf8(output.stdout).unwrap_or_else(|_| fallback())
}

fn ensure_trailing_newline(content: &str) -> String {
    if content.ends_with('\n') {
        content.to_string()
    } else {
        format!("{content}\n")
    }
}

fn flatten_discovered_paths(paths: &DiscoveredResourcePaths) -> Vec<String> {
    let mut out: Vec<String> = paths.values().flat_map(|v| v.iter().cloned()).collect();
    out.sort();
    out
}

fn flatten_discovered_paths_by_type_order(paths: &DiscoveredResourcePaths) -> Vec<String> {
    let mut out = Vec::new();
    for type_name in discover::ordered_type_names() {
        if let Some(type_paths) = paths.get(*type_name) {
            out.extend(type_paths.iter().cloned());
        }
    }
    let known_types = discover::ordered_type_names()
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let mut remaining = paths
        .iter()
        .filter(|(type_name, _)| !known_types.contains(type_name.as_str()))
        .flat_map(|(_, type_paths)| type_paths.iter().cloned())
        .collect::<Vec<_>>();
    remaining.sort();
    out.extend(remaining);
    out
}

fn ordered_discovered_paths_for_files(
    paths: &DiscoveredResourcePaths,
    file_paths: &HashSet<String>,
) -> Vec<String> {
    flatten_discovered_paths_by_type_order(paths)
        .into_iter()
        .filter(|logical_path| file_paths.contains(&parse_multi_resource_path(logical_path).0))
        .collect()
}

fn flatten_deleted_discovered_paths(paths: &DiscoveredResourcePaths) -> Vec<String> {
    let mut entries = Vec::new();
    for (type_name, logical_paths) in paths {
        for path in logical_paths {
            entries.push((deleted_status_type_rank(type_name), path.clone()));
        }
    }
    entries.sort_by(|(left_rank, left_path), (right_rank, right_path)| {
        left_rank
            .cmp(right_rank)
            .then_with(|| left_path.cmp(right_path))
    });
    entries.into_iter().map(|(_, path)| path).collect()
}

fn deleted_status_type_rank(type_name: &str) -> usize {
    match type_name {
        "VoiceStylePrompt" => 0,
        "SettingsPersonality" => 1,
        "VoiceSafetyFilters" => 2,
        "SettingsRole" => 3,
        "GeneralSafetyFilters" => 4,
        "VoiceDisclaimerMessage" => 5,
        "VoiceGreeting" => 6,
        "AsrSettings" => 7,
        "Entity" => 8,
        "PhraseFilter" => 9,
        "Handoff" => 10,
        "SMSTemplate" => 11,
        other => discover::ordered_type_names()
            .iter()
            .position(|name| *name == other)
            .map(|position| position + 100)
            .unwrap_or(usize::MAX),
    }
}

fn stable_dedup(items: &mut Vec<String>) {
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.clone()));
}

fn normalize_flow_resources_for_diff(resources: &mut ResourceMap, reference: Option<&ResourceMap>) {
    let step_ids = reference
        .map(flow_step_ids_by_folder_and_name)
        .unwrap_or_default();
    for (path, resource) in resources.iter_mut() {
        let Some(content) = resource
            .payload
            .get("content")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
        else {
            continue;
        };
        let normalized = if path.starts_with("flows/") && path.ends_with("/flow_config.yaml") {
            canonical_flow_config_for_diff(path, &content, &step_ids)
        } else if path.starts_with("flows/") && path.contains("/steps/") && path.ends_with(".yaml")
        {
            canonical_flow_step_for_diff(&content)
        } else if path.starts_with("flows/")
            && path.contains("/function_steps/")
            && path.ends_with(".py")
        {
            Some(strip_generated_flow_function_imports(&content))
        } else {
            None
        };
        if let Some(normalized) = normalized {
            resource.payload = serde_json::json!({ "content": normalized });
        }
    }
}

fn flow_step_ids_by_folder_and_name(resources: &ResourceMap) -> HashMap<(String, String), String> {
    resources
        .iter()
        .filter_map(|(path, resource)| {
            if !(path.starts_with("flows/") && path.contains("/steps/") && path.ends_with(".yaml"))
            {
                return None;
            }
            let folder = path.split('/').nth(1)?.to_string();
            let content = resource.payload.get("content")?.as_str()?;
            let yaml = serde_yaml::from_str::<serde_yaml::Value>(content).ok()?;
            let name = yaml.get("name")?.as_str()?.to_string();
            Some(((folder, name), resource.resource_id.clone()))
        })
        .collect()
}

fn canonical_flow_config_for_diff(
    path: &str,
    content: &str,
    step_ids: &HashMap<(String, String), String>,
) -> Option<String> {
    let yaml = serde_yaml::from_str::<serde_yaml::Value>(content).ok()?;
    let name = yaml
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let description = yaml
        .get("description")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let mut start_step = yaml
        .get("start_step")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    if !start_step.starts_with("STEP-")
        && let Some(folder) = path.split('/').nth(1)
        && let Some(id) = step_ids.get(&(folder.to_string(), start_step.clone()))
    {
        start_step = id.clone();
    }
    Some(format!(
        "name: {name}\ndescription: {description}\nstart_step: {start_step}\n"
    ))
}

fn canonical_flow_step_for_diff(content: &str) -> Option<String> {
    let yaml = serde_yaml::from_str::<serde_yaml::Value>(content).ok()?;
    let step_type = yaml
        .get("step_type")
        .and_then(|value| value.as_str())
        .unwrap_or("advanced_step");
    if step_type == "default_step" {
        Some(canonical_default_step_for_diff(&yaml))
    } else {
        Some(canonical_advanced_step_for_diff(&yaml))
    }
}

fn canonical_advanced_step_for_diff(yaml: &serde_yaml::Value) -> String {
    let name = yaml_string_value(yaml, "name");
    let prompt = yaml_string_value(yaml, "prompt");
    let asr = yaml.get("asr_biasing").or_else(|| yaml.get("asrBiasing"));
    let dtmf = yaml.get("dtmf_config").or_else(|| yaml.get("dtmfConfig"));
    let mut out = String::new();
    out.push_str("step_type: advanced_step\n");
    out.push_str(&format!("name: {name}\n"));
    out.push_str("asr_biasing:\n");
    for (key, value) in [
        (
            "is_enabled",
            yaml_bool_value(asr, &["is_enabled", "isEnabled"], false),
        ),
        (
            "alphanumeric",
            yaml_bool_value(asr, &["alphanumeric"], false),
        ),
        (
            "name_spelling",
            yaml_bool_value(asr, &["name_spelling", "nameSpelling"], false),
        ),
        ("numeric", yaml_bool_value(asr, &["numeric"], false)),
        (
            "party_size",
            yaml_bool_value(asr, &["party_size", "partySize"], false),
        ),
        (
            "precise_date",
            yaml_bool_value(asr, &["precise_date", "preciseDate"], false),
        ),
        (
            "relative_date",
            yaml_bool_value(asr, &["relative_date", "relativeDate"], false),
        ),
        (
            "single_number",
            yaml_bool_value(asr, &["single_number", "singleNumber"], false),
        ),
        ("time", yaml_bool_value(asr, &["time"], false)),
        ("yes_no", yaml_bool_value(asr, &["yes_no", "yesNo"], false)),
        ("address", yaml_bool_value(asr, &["address"], false)),
    ] {
        out.push_str(&format!("  {key}: {value}\n"));
    }
    let keywords = yaml_string_sequence(asr.and_then(|value| {
        value
            .get("custom_keywords")
            .or_else(|| value.get("customKeywords"))
    }));
    if keywords.is_empty() {
        out.push_str("  custom_keywords: []\n");
    } else {
        out.push_str("  custom_keywords:\n");
        for keyword in keywords {
            out.push_str(&format!("  - {keyword}\n"));
        }
    }
    out.push_str("dtmf_config:\n");
    out.push_str(&format!(
        "  is_enabled: {}\n",
        yaml_bool_value(dtmf, &["is_enabled", "isEnabled"], false)
    ));
    out.push_str(&format!(
        "  inter_digit_timeout: {}\n",
        yaml_i64_value(dtmf, &["inter_digit_timeout", "interDigitTimeout"], 0)
    ));
    out.push_str(&format!(
        "  max_digits: {}\n",
        yaml_i64_value(dtmf, &["max_digits", "maxDigits"], 0)
    ));
    out.push_str(&format!(
        "  end_key: '{}'\n",
        yaml_string_value_from(dtmf, &["end_key", "endKey"])
    ));
    out.push_str(&format!(
        "  collect_while_agent_speaking: {}\n",
        yaml_bool_value(
            dtmf,
            &["collect_while_agent_speaking", "collectWhileAgentSpeaking"],
            false
        )
    ));
    out.push_str(&format!(
        "  is_pii: {}\n",
        yaml_bool_value(dtmf, &["is_pii", "isPii"], false)
    ));
    out.push_str(&format!("prompt: {prompt}\n"));
    out
}

fn canonical_default_step_for_diff(yaml: &serde_yaml::Value) -> String {
    let name = yaml_string_value(yaml, "name");
    let prompt = yaml_string_value(yaml, "prompt");
    let mut out = String::new();
    out.push_str("step_type: default_step\n");
    out.push_str(&format!("name: {name}\n"));
    out.push_str("conditions:\n");
    if let Some(conditions) = yaml.get("conditions").and_then(|value| value.as_sequence()) {
        for condition in conditions {
            out.push_str(&format!(
                "- name: {}\n",
                yaml_string_value(condition, "name")
            ));
            out.push_str(&format!(
                "  condition_type: {}\n",
                yaml_string_value(condition, "condition_type")
            ));
            out.push_str(&format!(
                "  description: {}\n",
                yaml_string_value(condition, "description")
            ));
            let required = yaml_string_sequence(condition.get("required_entities"));
            if required.is_empty() {
                out.push_str("  required_entities: []\n");
            } else {
                out.push_str("  required_entities:\n");
                for entity in required {
                    out.push_str(&format!("  - {entity}\n"));
                }
            }
        }
    }
    let extracted = yaml_string_sequence(yaml.get("extracted_entities"));
    if extracted.is_empty() {
        out.push_str("extracted_entities: []\n");
    } else {
        out.push_str("extracted_entities:\n");
        for entity in extracted {
            out.push_str(&format!("- {entity}\n"));
        }
    }
    out.push_str(&format!("prompt: {prompt}\n"));
    out
}

fn strip_generated_flow_function_imports(content: &str) -> String {
    let mut lines = content.lines().collect::<Vec<_>>();
    while lines
        .first()
        .is_some_and(|line| line.trim().is_empty() || line.starts_with("from _gen import"))
    {
        lines.remove(0);
    }
    format!("{}\n", lines.join("\n"))
        .trim_end_matches('\n')
        .to_string()
}

const FUNCTION_HEADER: &str = "from _gen import *  # <AUTO GENERATED>\n";
const LEGACY_FUNCTION_HEADER: &str = "from imports import *  # <AUTO GENERATED>\n";

fn local_resource_content(path: &str, content: &str) -> String {
    if is_python_function_like_path(path) {
        raw_function_content(content)
    } else {
        content.to_string()
    }
}

fn resource_file_content(path: &str, content: &str) -> String {
    if is_python_function_like_path(path) {
        pretty_function_content(content)
    } else {
        content.to_string()
    }
}

fn is_python_function_like_path(path: &str) -> bool {
    path.ends_with(".py")
        && ((path.starts_with("functions/"))
            || (path.starts_with("flows/")
                && (path.contains("/functions/") || path.contains("/function_steps/"))))
}

fn raw_function_content(content: &str) -> String {
    content
        .replace(FUNCTION_HEADER, "")
        .replace(LEGACY_FUNCTION_HEADER, "")
        .trim_start_matches('\n')
        .to_string()
}

fn pretty_function_content(content: &str) -> String {
    if content.contains(FUNCTION_HEADER) || content.contains(LEGACY_FUNCTION_HEADER) {
        return content.to_string();
    }

    let content = content.trim_start_matches('\n');
    if let Some(docstring_end) = module_docstring_end(content) {
        let before_docstring = &content[..docstring_end];
        let after_docstring = content[docstring_end..].trim_start_matches('\n');
        if after_docstring.starts_with("from ") || after_docstring.starts_with("import ") {
            format!("{before_docstring}\n{FUNCTION_HEADER}{after_docstring}")
        } else {
            format!("{before_docstring}\n{FUNCTION_HEADER}\n{after_docstring}")
        }
    } else if content.starts_with("from ") || content.starts_with("import ") {
        format!("{FUNCTION_HEADER}{content}")
    } else {
        format!("{FUNCTION_HEADER}\n\n{content}")
    }
}

fn module_docstring_end(content: &str) -> Option<usize> {
    let quote = if content.starts_with("\"\"\"") {
        "\"\"\""
    } else if content.starts_with("'''") {
        "'''"
    } else {
        return None;
    };
    content[quote.len()..]
        .find(quote)
        .map(|index| quote.len() + index + quote.len())
}

fn yaml_string_value(yaml: &serde_yaml::Value, key: &str) -> String {
    yaml.get(key)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

fn yaml_string_value_from(yaml: Option<&serde_yaml::Value>, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| {
            yaml.and_then(|value| value.get(*key))
                .and_then(|value| value.as_str())
        })
        .unwrap_or_default()
        .to_string()
}

fn yaml_bool_value(yaml: Option<&serde_yaml::Value>, keys: &[&str], default: bool) -> bool {
    keys.iter()
        .find_map(|key| {
            yaml.and_then(|value| value.get(*key))
                .and_then(|value| value.as_bool())
        })
        .unwrap_or(default)
}

fn yaml_i64_value(yaml: Option<&serde_yaml::Value>, keys: &[&str], default: i64) -> i64 {
    keys.iter()
        .find_map(|key| {
            yaml.and_then(|value| value.get(*key))
                .and_then(|value| value.as_i64())
        })
        .unwrap_or(default)
}

fn yaml_string_sequence(yaml: Option<&serde_yaml::Value>) -> Vec<String> {
    yaml.and_then(|value| value.as_sequence())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .map(ToString::to_string)
        .collect()
}

fn normalize_function_references_in_rules(resources: &mut ResourceMap) {
    let replacements = resources
        .values()
        .filter(|resource| {
            resource.file_path.starts_with("functions/") && resource.file_path.ends_with(".py")
        })
        .map(|resource| {
            let name = resource
                .file_path
                .split('/')
                .next_back()
                .unwrap_or(&resource.name)
                .trim_end_matches(".py")
                .to_string();
            (resource.resource_id.clone(), name)
        })
        .filter(|(id, name)| !id.is_empty() && id != name)
        .collect::<Vec<_>>();
    if replacements.is_empty() {
        return;
    }
    let Some(rules) = resources.get_mut("agent_settings/rules.txt") else {
        return;
    };
    let Some(content) = rules
        .payload
        .get("content")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
    else {
        return;
    };
    let mut normalized = content;
    for (id, name) in replacements {
        normalized = normalized.replace(&format!("{{{{fn:{id}}}}}"), &format!("{{{{fn:{name}}}}}"));
    }
    rules.payload["content"] = serde_json::Value::String(normalized);
}

#[derive(Debug, Clone)]
struct ReferenceNameReplacement {
    prefix: String,
    id: String,
    name: String,
}

fn apply_reference_name_replacements(
    resources: &mut ResourceMap,
    replacements: &[ReferenceNameReplacement],
) {
    if replacements.is_empty() {
        return;
    }
    for resource in resources.values_mut() {
        let Some(content) = resource
            .payload
            .get("content")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
        else {
            continue;
        };
        let normalized = replace_reference_ids_with_names(&content, replacements);
        if normalized != content {
            resource.payload["content"] = serde_json::Value::String(normalized);
        }
    }
}

fn replace_reference_ids_with_names(
    content: &str,
    replacements: &[ReferenceNameReplacement],
) -> String {
    let mut normalized = content.to_string();
    for replacement in replacements {
        normalized = normalized.replace(
            &format!("{{{{{}:{}}}}}", replacement.prefix, replacement.id),
            &format!("{{{{{}:{}}}}}", replacement.prefix, replacement.name),
        );
    }
    normalized
}

fn reference_name_from_logical_path(logical_path: &str) -> String {
    let (_, resource_suffix) = parse_multi_resource_path(logical_path);
    let source = resource_suffix.as_deref().unwrap_or(logical_path);
    let leaf = source.rsplit('/').next().unwrap_or(source);
    Path::new(leaf)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(leaf)
        .to_string()
}

fn resource_read_error(root: &Path, path: &str) -> String {
    let abs_path = root.join(path).to_string_lossy().to_string();
    resource_read_error_with_detail(root, path, &format!("Error loading YAML file: {abs_path}"))
}

fn resource_read_error_with_detail(root: &Path, path: &str, detail: &str) -> String {
    let abs_path = root.join(path).to_string_lossy().to_string();
    let resource_name = Path::new(path)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    format!("Error reading resource {resource_name} at {abs_path}: {detail}")
}

fn absolutize_deleted_diff_keys(
    root: &Path,
    diffs: &mut DiffMap,
    deleted_file_paths: &HashSet<String>,
) {
    if deleted_file_paths.is_empty() {
        return;
    }
    let mut replacements = Vec::new();
    for path in deleted_file_paths {
        if let Some(diff) = diffs.shift_remove(path) {
            replacements.push((root.join(path).to_string_lossy().to_string(), diff));
        }
    }
    for (path, diff) in replacements {
        diffs.insert(path, diff);
    }
}

fn compute_modified_files_against_snapshot(
    root: &Path,
    kept_resources: &DiscoveredResourcePaths,
    snapshot_hashes: &indexmap::IndexMap<String, String>,
) -> Result<Vec<String>, CoreError> {
    let kept_file_paths: HashSet<String> = flatten_discovered_paths(kept_resources)
        .into_iter()
        .map(|p| parse_multi_resource_path(&p).0)
        .collect();
    let mut modified_file_paths = HashSet::new();
    for rel_path in kept_file_paths {
        let Some(expected_hash) = snapshot_hashes.get(&rel_path) else {
            continue;
        };
        let current_path = root.join(&rel_path);
        let current_content = fs::read_to_string(&current_path).unwrap_or_default();
        let current_hash = compute_hash(&current_content);
        if &current_hash != expected_hash {
            modified_file_paths.insert(rel_path);
        }
    }
    Ok(ordered_discovered_paths_for_files(
        kept_resources,
        &modified_file_paths,
    ))
}

fn compute_modified_files_against_snapshot_with_replacements(
    root: &Path,
    kept_resources: &DiscoveredResourcePaths,
    snapshot_hashes: &indexmap::IndexMap<String, String>,
    replacements: &[ReferenceNameReplacement],
) -> Result<Vec<String>, CoreError> {
    if replacements.is_empty() {
        return Ok(Vec::new());
    }
    let kept_file_paths: HashSet<String> = flatten_discovered_paths(kept_resources)
        .into_iter()
        .map(|p| parse_multi_resource_path(&p).0)
        .collect();
    let mut modified_file_paths = HashSet::new();
    for rel_path in kept_file_paths {
        let Some(expected_hash) = snapshot_hashes.get(&rel_path) else {
            continue;
        };
        let current_path = root.join(&rel_path);
        let current_content = fs::read_to_string(&current_path).unwrap_or_default();
        let normalized_content = replace_reference_ids_with_names(&current_content, replacements);
        if normalized_content == current_content {
            continue;
        }
        let current_hash = compute_hash(&normalized_content);
        if &current_hash != expected_hash {
            modified_file_paths.insert(rel_path);
        }
    }
    Ok(ordered_discovered_paths_for_files(
        kept_resources,
        &modified_file_paths,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_file_content_is_pretty_on_disk_and_raw_in_resources() {
        let raw = "@func_description('Looks up a customer.')\ndef lookup(conv: Conversation):\n    return None\n";
        let pretty = resource_file_content("functions/lookup.py", raw);

        assert!(pretty.starts_with("from _gen import *  # <AUTO GENERATED>\n\n\n"));
        assert_eq!(local_resource_content("functions/lookup.py", &pretty), raw);
    }

    #[test]
    fn function_header_is_inserted_after_module_docstring() {
        let raw =
            "\"\"\"Helpers.\"\"\"\nimport json\n\ndef lookup(conv):\n    return json.dumps({})\n";
        let pretty = resource_file_content("functions/lookup.py", raw);

        assert!(pretty.starts_with(
            "\"\"\"Helpers.\"\"\"\nfrom _gen import *  # <AUTO GENERATED>\nimport json\n"
        ));
        assert_eq!(local_resource_content("functions/lookup.py", &pretty), raw);
    }
}
