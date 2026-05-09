use adk_domain::{
    BranchMergeResult, DeploymentList, DiffMap, DomainError, ProjectConfig, PushResult, Resource,
    ResourceMap, StatusSummary,
};

pub mod discover;

use adk_io::{compute_hash, diff_resources, parse_multi_resource_path};
use adk_platform_api::{ApiError, PlatformClient};
use anyhow::Result;
use base64::Engine;
pub use discover::discover_local_resources;
pub use discover::{DiscoveredResourceChanges, DiscoveredResourcePaths, TypedResourceLifecycle};
use globset::{Glob, GlobSetBuilder};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

pub const PROJECT_CONFIG_FILE: &str = "project.yaml";
pub const STATUS_FILE: &str = "_gen/.agent_studio_config";

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
        let root = base_path.join(&account_id).join(&project_id);
        fs::create_dir_all(&root)?;
        let config = ProjectConfig {
            region,
            account_id,
            project_id,
            project_name: None,
            branch_id: "main".to_string(),
        };
        let serialized =
            serde_yaml::to_string(&config).map_err(|e| DomainError::InvalidData(e.to_string()))?;
        fs::write(root.join(PROJECT_CONFIG_FILE), serialized)?;
        Ok(config)
    }

    pub fn load_project_config(&self, base_path: &Path) -> Result<ProjectConfig, CoreError> {
        let discovered = find_project_root(base_path)
            .ok_or_else(|| DomainError::ConfigNotFound(base_path.to_string_lossy().to_string()))?;
        let config_path = discovered.join(PROJECT_CONFIG_FILE);
        if config_path.exists() {
            let raw = fs::read_to_string(config_path)?;
            return serde_yaml::from_str(&raw)
                .map_err(|e| DomainError::InvalidData(e.to_string()).into());
        }

        let status_path = discovered.join(STATUS_FILE);
        if status_path.exists() {
            let encoded = fs::read(status_path)?;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|e| DomainError::InvalidData(e.to_string()))?;
            let json: serde_json::Value = serde_json::from_slice(&decoded)?;
            return Ok(ProjectConfig {
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
            });
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
            let payload = serde_json::json!({
                "content": fs::read_to_string(file.as_path()).unwrap_or_default(),
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
        let mut summary = StatusSummary::default();
        summary.conflict_detection_available = true;
        summary.files_with_conflicts = self.detect_conflict_files(root)?;

        if let Some(existing_typed) = self.load_status_snapshot_discovered_resources(root)? {
            let discovered_typed = self.discover_local_resources(root);
            let typed_changes = self.find_new_kept_deleted(&discovered_typed, &existing_typed);
            summary.new_files = flatten_discovered_paths(&typed_changes.new_resources);
            summary.deleted_files = flatten_discovered_paths(&typed_changes.deleted_resources);

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
                    summary.modified_files.sort();
                    summary.modified_files.dedup();
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
                let current_branch = self.load_project_config(root).ok().map(|cfg| cfg.branch_id);
                if before_name == "main" {
                    if current_branch
                        .as_deref()
                        .is_some_and(|branch_id| branch_id != "main")
                    {
                        after_name = "local".to_string();
                    } else {
                        return Err(DomainError::InvalidData(
                            "Failed to compute diffs.".to_string(),
                        )
                        .into());
                    }
                } else {
                    after_name = "local".to_string();
                }
            }
            let mut before_state = self.resolve_named_state(root, &before_name)?;
            let mut after_state = self.resolve_named_state(root, &after_name)?;
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
        let remote = self.client.pull_resources()?;
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
                return Err(DomainError::InvalidData(format!(
                    "validation failed: {}",
                    validation_errors.join("; ")
                ))
                .into());
            }
        }
        let mut local = self.collect_local_resources(root)?;
        self.apply_deleted_reference_names(root, &mut local)?;
        if dry_run {
            return Ok(self
                .client
                .preview_push_resources_with_options(&local, projection, actor)?);
        }
        let result = self
            .client
            .push_resources_with_options(&local, projection, actor)?;
        if result.success {
            self.save_replay_state_resources(root, &local)?;
            self.write_status_snapshot_from_resources(root, &local)?;
        }
        Ok(result)
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
                return Err(DomainError::InvalidData(format!(
                    "validation failed: {}",
                    validation_errors.join("; ")
                ))
                .into());
            }
        }
        let mut local = self.collect_local_resources(root)?;
        self.apply_deleted_reference_names(root, &mut local)?;
        let (branch_id, push_result) =
            self.client
                .push_main_resources_to_new_branch(branch_name, &local, actor)?;
        let mut cfg = self.load_project_config(root)?;
        if push_result.success {
            cfg.branch_id = branch_id;
            self.write_project_config(root, &cfg)?;
            self.save_replay_state_resources(root, &local)?;
            self.write_status_snapshot_from_resources(root, &local)?;
        }
        Ok((cfg, push_result))
    }

    pub fn pull(&self, root: &Path, force: bool) -> Result<Vec<String>, CoreError> {
        let remote = if let Some(resources) = self.load_replay_state_resources(root)? {
            resources
        } else {
            self.client.pull_resources()?
        };
        self.write_pulled_resources(root, remote, force)
    }

    pub fn pull_projection_json(&self) -> Result<Value, CoreError> {
        Ok(self.client.pull_projection_json()?)
    }

    pub fn pull_named(
        &self,
        root: &Path,
        name: &str,
        force: bool,
    ) -> Result<Vec<String>, CoreError> {
        let remote = if let Some(resources) = self.load_replay_state_resources(root)? {
            resources
        } else {
            self.client.pull_resources_by_name(name)?
        };
        self.write_pulled_resources(root, remote, force)
    }

    fn write_pulled_resources(
        &self,
        root: &Path,
        remote: ResourceMap,
        force: bool,
    ) -> Result<Vec<String>, CoreError> {
        let mut files_with_conflicts = Vec::new();
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
                    let incoming_changed = compute_hash(&content) != *snapshot_hash;
                    if local_changed && !incoming_changed {
                        continue;
                    }
                    if local_changed && incoming_changed && existing != content {
                        let merged = format!(
                            "<<<<<<< local\n{existing}\n=======\n{content}\n>>>>>>> remote\n"
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
            fs::write(target, content)?;
        }
        if files_with_conflicts.is_empty() {
            self.write_status_snapshot_from_resources(root, &remote)?;
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
            fs::write(&target, content)?;
            reverted.push(target_abs_str);
        }
        Ok(reverted)
    }

    pub fn list_deployments(&self, environment: &str) -> Result<DeploymentList, CoreError> {
        Ok(self.client.list_deployments(environment)?)
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
            .client
            .list_branches()?
            .into_iter()
            .find(|branch| {
                branch.branch_id == current_branch_id || branch.name == current_branch_id
            })
            .map(|branch| branch.name)
            .unwrap_or(current_branch_id))
    }

    pub fn list_known_branches(&self, root: &Path) -> Result<Vec<String>, CoreError> {
        let current_branch_id = self.current_branch(root)?;
        let mut names: Vec<String> = self
            .client
            .list_branches()?
            .into_iter()
            .map(|branch| branch.name)
            .collect();
        if !names.iter().any(|name| name == &current_branch_id) {
            names.push(current_branch_id);
        }
        Ok(names)
    }

    pub fn list_branch_map(
        &self,
        root: &Path,
    ) -> Result<indexmap::IndexMap<String, String>, CoreError> {
        let current_branch_id = self.current_branch(root)?;
        let mut branches = indexmap::IndexMap::new();
        for branch in self.client.list_branches()? {
            branches.insert(branch.name, branch.branch_id);
        }
        if !branches.values().any(|id| id == &current_branch_id)
            && !branches.contains_key(&current_branch_id)
        {
            branches.insert(current_branch_id.clone(), current_branch_id);
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
        let switched_to = if cfg.branch_id == branch_id {
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
        for (path, resource) in resources {
            let content = resource
                .payload
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if path.ends_with(".yaml") || path.ends_with(".yml") {
                if let Err(e) = serde_yaml::from_str::<serde_yaml::Value>(content) {
                    let _ = e;
                    return Err(DomainError::InvalidData(resource_read_error(root, &path)).into());
                }
            } else if path.ends_with(".json")
                && let Err(e) = serde_json::from_str::<serde_json::Value>(content)
            {
                errors.push(format!("{path}: invalid json: {e}"));
            }
        }
        errors.sort();
        Ok(errors)
    }

    pub fn format_local_resources(
        &self,
        root: &Path,
        files: &[String],
        check: bool,
    ) -> Result<Vec<String>, CoreError> {
        let resources = self.collect_local_resources(root)?;
        let matcher = if files.is_empty() {
            None
        } else {
            Some(build_file_matcher(files)?)
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
                let parsed: serde_yaml::Value = serde_yaml::from_str(content)
                    .map_err(|e| DomainError::InvalidData(format!("{path}: invalid yaml: {e}")))?;
                serde_yaml::to_string(&parsed)
                    .map_err(|e| DomainError::InvalidData(format!("{path}: yaml error: {e}")))?
            } else if path.ends_with(".json") {
                let parsed: serde_json::Value = serde_json::from_str(content)
                    .map_err(|e| DomainError::InvalidData(format!("{path}: invalid json: {e}")))?;
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(&parsed).map_err(|e| DomainError::InvalidData(
                        format!("{path}: json error: {e}")
                    ))?
                )
            } else {
                continue;
            };
            if formatted != content {
                changed_files.push(path.clone());
                if !check {
                    fs::write(root.join(&path), formatted)?;
                }
            }
        }
        changed_files.sort();
        Ok(changed_files)
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
        let mut file_paths = BTreeSet::new();

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
                file_paths.insert(file_path.clone());
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
        for file_path in file_paths {
            let content = fs::read_to_string(project_root.join(&file_path)).unwrap_or_default();
            file_structure_info.insert(
                file_path.clone(),
                serde_json::json!({
                    "type": "unknown",
                    "resource_id": file_path,
                    "resource_name": file_path,
                    "hash": compute_hash(&content),
                }),
            );
        }

        let branch_id = self
            .load_project_config(&project_root)
            .map(|cfg| cfg.branch_id)
            .unwrap_or_else(|_| "main".to_string());
        let status = serde_json::json!({
            "resources": resources,
            "file_structure_info": file_structure_info,
            "branch_id": branch_id,
        });
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&status)?);
        let gen_dir = project_root.join("_gen");
        fs::create_dir_all(&gen_dir)?;
        fs::write(gen_dir.join(".agent_studio_config"), encoded)?;
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

fn flatten_discovered_paths(paths: &DiscoveredResourcePaths) -> Vec<String> {
    let mut out: Vec<String> = paths.values().flat_map(|v| v.iter().cloned()).collect();
    out.sort();
    out
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
    let resource_name = Path::new(path)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    format!(
        "Error reading resource {resource_name} at {abs_path}: Error loading YAML file: {abs_path}"
    )
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
    let mut modified = Vec::new();
    for rel_path in kept_file_paths {
        let Some(expected_hash) = snapshot_hashes.get(&rel_path) else {
            continue;
        };
        let current_path = root.join(&rel_path);
        let current_content = fs::read_to_string(&current_path).unwrap_or_default();
        let current_hash = compute_hash(&current_content);
        if &current_hash != expected_hash {
            modified.push(rel_path);
        }
    }
    modified.sort();
    Ok(modified)
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
    let mut modified = Vec::new();
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
            modified.push(rel_path);
        }
    }
    modified.sort();
    modified.dedup();
    Ok(modified)
}
