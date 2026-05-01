use adk_domain::{
    DeploymentList, DiffMap, DomainError, ProjectConfig, PushResult, Resource, ResourceMap,
    StatusSummary,
};
use adk_io::diff_resources;
use adk_platform_api::{ApiError, PlatformClient};
use anyhow::Result;
use base64::Engine;
use chrono::Utc;
use globset::{Glob, GlobSetBuilder};
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

    pub fn status(&self, root: &Path) -> Result<StatusSummary, CoreError> {
        let local = self.collect_local_resources(root)?;
        let remote = self.client.pull_resources()?;
        let mut summary = StatusSummary::default();

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
            // Placeholder compatibility: remote named-version diffing is wired later.
            return Ok(DiffMap::new());
        }
        let local = self.collect_local_resources(root)?;
        let remote = self.client.pull_resources()?;
        let mut diffs = diff_resources(&remote, &local);
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
