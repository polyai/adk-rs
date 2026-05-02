use adk_domain::{
    BranchMergeResult, DeploymentList, DiffMap, DomainError, ProjectConfig, PushResult, Resource,
    ResourceMap, StatusSummary,
};

pub mod discover;

use adk_io::{compute_hash, diff_resources, parse_multi_resource_path};
use adk_platform_api::{ApiError, PlatformClient};
use anyhow::Result;
use base64::Engine;
use chrono::Utc;
pub use discover::discover_local_resources;
pub use discover::{DiscoveredResourceChanges, DiscoveredResourcePaths, TypedResourceLifecycle};
use globset::{Glob, GlobSetBuilder};
use std::collections::HashSet;
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
                "last_seen": Utc::now(),
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
        let local = self.collect_local_resources(root)?;
        let remote = self.client.pull_resources()?;
        let mut summary = StatusSummary::default();

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
            let before_name = before.unwrap_or_else(|| "local".to_string());
            let after_name = after.unwrap_or_else(|| "local".to_string());
            let before_state = self.resolve_named_state(root, &before_name)?;
            let after_state = self.resolve_named_state(root, &after_name)?;
            let mut diffs = diff_resources(&before_state, &after_state);
            if !files.is_empty() {
                let matcher = build_file_matcher(files)?;
                diffs.retain(|k, _| matcher.is_match(k));
            }
            return Ok(diffs);
        }
        let local = self.collect_local_resources(root)?;
        let remote = self.client.pull_resources()?;
        let mut diffs = diff_resources(&remote, &local);

        if let Some(existing_typed) = self.load_status_snapshot_discovered_resources(root)? {
            let discovered_typed = self.discover_local_resources(root);
            let typed_changes = self.find_new_kept_deleted(&discovered_typed, &existing_typed);
            let changed_paths = flatten_discovered_paths(&typed_changes.new_resources)
                .into_iter()
                .chain(flatten_discovered_paths(&typed_changes.deleted_resources))
                .collect::<Vec<_>>();

            if changed_paths.is_empty() {
                diffs.clear();
            } else {
                let changed_file_paths: HashSet<String> = changed_paths
                    .iter()
                    .map(|p| parse_multi_resource_path(p).0)
                    .collect();
                diffs.retain(|k, _| changed_file_paths.contains(k));
            }
        }

        if !files.is_empty() {
            let matcher = build_file_matcher(files)?;
            diffs.retain(|k, _| matcher.is_match(k));
        }
        Ok(diffs)
    }

    pub fn push(
        &self,
        root: &Path,
        _force: bool,
        _skip_validation: bool,
        _dry_run: bool,
    ) -> Result<PushResult, CoreError> {
        let local = self.collect_local_resources(root)?;
        let result = self.client.push_resources(&local)?;
        Ok(result)
    }

    pub fn pull(&self, root: &Path) -> Result<Vec<String>, CoreError> {
        let remote = self.client.pull_resources()?;
        for (path, resource) in remote {
            let target = root.join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let content = resource
                .payload
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            fs::write(target, content)?;
        }
        Ok(vec![])
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

    pub fn current_branch(&self, root: &Path) -> Result<String, CoreError> {
        Ok(self.load_project_config(root)?.branch_id)
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

    pub fn create_branch(
        &self,
        root: &Path,
        branch_name: &str,
    ) -> Result<ProjectConfig, CoreError> {
        let branch_id = self.client.create_branch(branch_name)?;
        let mut cfg = self.load_project_config(root)?;
        cfg.branch_id = branch_id;
        self.write_project_config(root, &cfg)?;
        Ok(cfg)
    }

    pub fn set_branch(&self, root: &Path, branch_name: &str) -> Result<ProjectConfig, CoreError> {
        let mut cfg = self.load_project_config(root)?;
        cfg.branch_id = branch_name.to_string();
        self.write_project_config(root, &cfg)?;
        Ok(cfg)
    }

    pub fn delete_branch(&self, root: &Path, branch_name: &str) -> Result<bool, CoreError> {
        if branch_name == "main" {
            return Err(DomainError::InvalidData("cannot delete main branch".to_string()).into());
        }
        let cfg = self.load_project_config(root)?;
        let branches = self.client.list_branches()?;
        let branch_id = branches
            .iter()
            .find(|branch| branch.name == branch_name)
            .map(|branch| branch.branch_id.clone())
            .unwrap_or_else(|| branch_name.to_string());
        self.client.delete_branch(&branch_id)?;
        if cfg.branch_id == branch_id {
            let mut updated_cfg = cfg;
            updated_cfg.branch_id = "main".to_string();
            self.write_project_config(root, &updated_cfg)?;
        }
        Ok(true)
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
                    errors.push(format!("{path}: invalid yaml: {e}"));
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
            return self.collect_local_resources(root);
        }
        Ok(self.client.pull_resources()?)
    }

    fn write_project_config(&self, root: &Path, cfg: &ProjectConfig) -> Result<(), CoreError> {
        let project_root = find_project_root(root)
            .ok_or_else(|| DomainError::ConfigNotFound(root.to_string_lossy().to_string()))?;
        let serialized =
            serde_yaml::to_string(cfg).map_err(|e| DomainError::InvalidData(e.to_string()))?;
        fs::write(project_root.join(PROJECT_CONFIG_FILE), serialized)?;
        Ok(())
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
