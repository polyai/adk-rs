use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type ResourceMap = IndexMap<String, Resource>;
pub type DiffMap = IndexMap<String, String>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectConfig {
    pub region: String,
    pub account_id: String,
    pub project_id: String,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default = "default_branch")]
    pub branch_id: String,
}

fn default_branch() -> String {
    "main".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectStatus {
    #[serde(default)]
    pub resources: IndexMap<String, IndexMap<String, serde_json::Value>>,
    pub last_updated: Option<DateTime<Utc>>,
    #[serde(default = "default_branch")]
    pub branch_id: String,
    #[serde(default)]
    pub project_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resource {
    pub resource_id: String,
    pub name: String,
    pub file_path: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StatusSummary {
    pub files_with_conflicts: Vec<String>,
    pub modified_files: Vec<String>,
    pub new_files: Vec<String>,
    pub deleted_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PushResult {
    pub success: bool,
    pub message: String,
    #[serde(default)]
    pub commands: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeploymentList {
    #[serde(default)]
    pub versions: Vec<serde_json::Value>,
    #[serde(default)]
    pub active_deployment_hashes: IndexMap<String, String>,
}

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("project configuration not found at {0}")]
    ConfigNotFound(String),
    #[error("invalid project data: {0}")]
    InvalidData(String),
}
