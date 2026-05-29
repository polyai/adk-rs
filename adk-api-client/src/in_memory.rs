use crate::{ApiError, PlatformClient, ProjectionSnapshot};
use adk_protobuf::Command;
use adk_types::{BranchDescriptor, BranchMergeResult, DeploymentList, PushResult};
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// Deterministic non-network client used by local tests and explicit local/projection flows.
#[derive(Debug, Clone)]
pub struct InMemoryPlatformClient {
    projection: Arc<Mutex<Value>>,
    branches: Arc<Mutex<indexmap::IndexMap<String, String>>>,
    named_projections: Arc<Mutex<indexmap::IndexMap<String, Value>>>,
    deployments: Arc<Mutex<DeploymentList>>,
}

impl Default for InMemoryPlatformClient {
    fn default() -> Self {
        Self {
            projection: Arc::new(Mutex::new(Value::Object(Default::default()))),
            branches: Arc::new(Mutex::new(default_branches())),
            named_projections: Arc::new(Mutex::new(indexmap::IndexMap::new())),
            deployments: Arc::new(Mutex::new(DeploymentList {
                versions: vec![],
                active_deployment_hashes: Default::default(),
            })),
        }
    }
}

impl InMemoryPlatformClient {
    pub fn with_projection(projection: Value) -> Self {
        let mut named_projections = indexmap::IndexMap::new();
        named_projections.insert("main".to_string(), projection.clone());
        Self {
            projection: Arc::new(Mutex::new(projection)),
            branches: Arc::new(Mutex::new(default_branches())),
            named_projections: Arc::new(Mutex::new(named_projections)),
            deployments: Arc::new(Mutex::new(DeploymentList {
                versions: vec![],
                active_deployment_hashes: Default::default(),
            })),
        }
    }

    pub fn with_named_projections(
        projection: Value,
        named_projections: indexmap::IndexMap<String, Value>,
        deployments: DeploymentList,
    ) -> Self {
        Self {
            projection: Arc::new(Mutex::new(projection)),
            branches: Arc::new(Mutex::new(default_branches())),
            named_projections: Arc::new(Mutex::new(named_projections)),
            deployments: Arc::new(Mutex::new(deployments)),
        }
    }

    pub fn set_projection(&self, projection: Value) -> Result<(), ApiError> {
        *self
            .projection
            .lock()
            .map_err(|e| ApiError::Http(e.to_string()))? = projection;
        Ok(())
    }
}

fn default_branches() -> indexmap::IndexMap<String, String> {
    let mut branches = indexmap::IndexMap::new();
    branches.insert("main".to_string(), "main".to_string());
    branches
}

impl PlatformClient for InMemoryPlatformClient {
    fn pull_projection_json(&self) -> Result<Value, ApiError> {
        Ok(self
            .projection
            .lock()
            .map_err(|e| ApiError::Http(e.to_string()))?
            .clone())
    }

    fn pull_projection_json_by_name(&self, name: &str) -> Result<Value, ApiError> {
        let named = self
            .named_projections
            .lock()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        if let Some(projection) = named.get(name) {
            return Ok(projection.clone());
        }
        let prefix = name.chars().take(9).collect::<String>().to_lowercase();
        if !prefix.is_empty() {
            for (key, value) in named.iter() {
                if key.chars().take(9).collect::<String>().to_lowercase() == prefix {
                    return Ok(value.clone());
                }
            }
        }
        drop(named);
        self.pull_projection_json()
    }

    fn pull_projection_snapshot_for_branch(
        &self,
        branch_id: &str,
    ) -> Result<ProjectionSnapshot, ApiError> {
        Ok(ProjectionSnapshot {
            projection: self.pull_projection_json_by_name(branch_id)?,
            last_known_sequence: 0,
        })
    }

    fn push_commands(
        &self,
        last_known_sequence: u64,
        commands: Vec<Command>,
    ) -> Result<PushResult, ApiError> {
        self.push_commands_to_branch("main", last_known_sequence, commands)
    }

    fn push_commands_to_branch(
        &self,
        _branch_id: &str,
        _last_known_sequence: u64,
        _commands: Vec<Command>,
    ) -> Result<PushResult, ApiError> {
        Ok(PushResult {
            success: true,
            message: "Push successful".to_string(),
            commands: vec![],
        })
    }

    fn list_deployments(&self, _environment: &str) -> Result<DeploymentList, ApiError> {
        Ok(self
            .deployments
            .lock()
            .map_err(|e| ApiError::Http(e.to_string()))?
            .clone())
    }

    fn promote_deployment(
        &self,
        deployment_id: &str,
        target_env: &str,
        message: &str,
    ) -> Result<Value, ApiError> {
        Ok(serde_json::json!({
            "success": true,
            "deployment_id": deployment_id,
            "targetEnvironment": target_env,
            "deploymentMessage": message,
        }))
    }

    fn rollback_deployment(&self, deployment_id: &str, message: &str) -> Result<Value, ApiError> {
        Ok(serde_json::json!({
            "success": true,
            "deployment_id": deployment_id,
            "deploymentMessage": message,
        }))
    }

    fn create_chat_session(&self, _payload: Value) -> Result<Value, ApiError> {
        Ok(serde_json::json!({
            "conversation_id": "local-conversation",
            "response": "Mock chat session created",
            "conversation_ended": false
        }))
    }

    fn send_chat_message(&self, payload: Value) -> Result<Value, ApiError> {
        let text = payload
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default();
        Ok(serde_json::json!({
            "response": format!("Mock reply to: {text}"),
            "conversation_ended": false
        }))
    }

    fn end_chat_session(&self, _payload: Value) -> Result<Value, ApiError> {
        Ok(serde_json::json!({"success": true}))
    }

    fn list_branches(&self) -> Result<Vec<BranchDescriptor>, ApiError> {
        let branches = self
            .branches
            .lock()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        Ok(branches
            .iter()
            .map(|(name, branch_id)| BranchDescriptor {
                name: name.clone(),
                branch_id: branch_id.clone(),
            })
            .collect())
    }

    fn create_branch(&self, branch_name: &str) -> Result<String, ApiError> {
        let mut branches = self
            .branches
            .lock()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        if branches.contains_key(branch_name) {
            return Err(ApiError::Http(format!(
                "branch '{branch_name}' already exists"
            )));
        }
        let branch_id = branch_name.to_string();
        branches.insert(branch_name.to_string(), branch_id.clone());
        Ok(branch_id)
    }

    fn delete_branch(&self, branch_id: &str) -> Result<(), ApiError> {
        if branch_id == "main" {
            return Err(ApiError::Http("cannot delete main branch".to_string()));
        }
        let mut branches = self
            .branches
            .lock()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        let Some(key) = branches
            .iter()
            .find_map(|(name, id)| (id == branch_id).then_some(name.clone()))
        else {
            // Stateless test fallback: deletion is still considered successful even when the
            // branch inventory wasn't persisted across command invocations.
            return Ok(());
        };
        branches.shift_remove(&key);
        Ok(())
    }

    fn merge_branch(
        &self,
        _deployment_message: &str,
        _conflict_resolutions: Option<Vec<Value>>,
    ) -> Result<BranchMergeResult, ApiError> {
        Ok(BranchMergeResult {
            success: true,
            conflicts: vec![],
            errors: vec![],
            sequence: Some("0".to_string()),
        })
    }
}
