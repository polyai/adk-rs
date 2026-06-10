use crate::{ApiError, PlatformClient, ProjectionSnapshot, command_user_override_from_env};
use adk_protobuf::CommandBatch;
use adk_resources::{
    command_to_json_summary, try_build_push_commands_for_changed_resources,
    try_build_push_commands_with_created_by,
};
use adk_types::{
    BranchDescriptor, BranchMergeResult, ConversationDetail, ConversationListResponse,
    ConversationSummary, DeploymentList, PushResult, ResourceMap,
};
use prost::Message;
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// Deterministic non-network client used by local tests and explicit local/projection flows.
#[derive(Debug, Clone)]
pub struct InMemoryPlatformClient {
    resources: Arc<Mutex<ResourceMap>>,
    branches: Arc<Mutex<indexmap::IndexMap<String, String>>>,
    named_resources: Arc<Mutex<indexmap::IndexMap<String, ResourceMap>>>,
    deployments: Arc<Mutex<DeploymentList>>,
    command_user_override: Option<String>,
}

impl Default for InMemoryPlatformClient {
    fn default() -> Self {
        Self {
            resources: Arc::new(Mutex::new(ResourceMap::new())),
            branches: Arc::new(Mutex::new(default_branches())),
            named_resources: Arc::new(Mutex::new(indexmap::IndexMap::new())),
            deployments: Arc::new(Mutex::new(DeploymentList {
                versions: vec![],
                active_deployment_hashes: Default::default(),
            })),
            command_user_override: command_user_override_from_env(),
        }
    }
}

impl InMemoryPlatformClient {
    pub fn with_resources(resources: ResourceMap) -> Self {
        let mut named_resources = indexmap::IndexMap::new();
        named_resources.insert("main".to_string(), resources.clone());
        Self {
            resources: Arc::new(Mutex::new(resources)),
            branches: Arc::new(Mutex::new(default_branches())),
            named_resources: Arc::new(Mutex::new(named_resources)),
            deployments: Arc::new(Mutex::new(DeploymentList {
                versions: vec![],
                active_deployment_hashes: Default::default(),
            })),
            command_user_override: command_user_override_from_env(),
        }
    }

    pub fn with_named_resources(
        resources: ResourceMap,
        named_resources: indexmap::IndexMap<String, ResourceMap>,
        deployments: DeploymentList,
    ) -> Self {
        Self {
            resources: Arc::new(Mutex::new(resources)),
            branches: Arc::new(Mutex::new(default_branches())),
            named_resources: Arc::new(Mutex::new(named_resources)),
            deployments: Arc::new(Mutex::new(deployments)),
            command_user_override: command_user_override_from_env(),
        }
    }

    fn preview_projection(&self, projection: Option<&Value>) -> Result<Value, ApiError> {
        if let Some(projection) = projection {
            Ok(projection.clone())
        } else {
            self.pull_projection_json()
        }
    }
}

fn default_branches() -> indexmap::IndexMap<String, String> {
    let mut branches = indexmap::IndexMap::new();
    branches.insert("main".to_string(), "main".to_string());
    branches
}

impl PlatformClient for InMemoryPlatformClient {
    fn pull_projection_json(&self) -> Result<Value, ApiError> {
        let resources = self.pull_resources()?;
        serde_json::to_value(resources).map_err(|e| ApiError::Http(e.to_string()))
    }

    fn pull_projection_snapshot(&self) -> Result<ProjectionSnapshot, ApiError> {
        Ok(ProjectionSnapshot {
            projection: self.pull_projection_json()?,
            last_known_sequence: 0,
        })
    }

    fn preview_push_resources(&self, resources: &ResourceMap) -> Result<PushResult, ApiError> {
        self.preview_push_resources_with_options(resources, None)
    }

    fn preview_push_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
    ) -> Result<PushResult, ApiError> {
        let projection = self.preview_projection(projection)?;
        let commands = try_build_push_commands_with_created_by(
            resources,
            &projection,
            self.command_user_override.as_deref(),
        )?;
        let summaries = commands.iter().map(command_to_json_summary).collect();
        Ok(PushResult {
            success: true,
            message: "Dry run completed. No changes were pushed.".to_string(),
            commands: summaries,
        })
    }

    fn preview_push_changed_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
    ) -> Result<PushResult, ApiError> {
        let projection = self.preview_projection(projection)?;
        let commands = try_build_push_commands_for_changed_resources(
            resources,
            &projection,
            self.command_user_override.as_deref(),
        )?;
        let summaries = commands.iter().map(command_to_json_summary).collect();
        Ok(PushResult {
            success: true,
            message: "Dry run completed. No changes were pushed.".to_string(),
            commands: summaries,
        })
    }

    fn pull_resources_by_name(&self, name: &str) -> Result<ResourceMap, ApiError> {
        let named = self
            .named_resources
            .lock()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        if let Some(resources) = named.get(name) {
            return Ok(resources.clone());
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
        self.pull_resources()
    }

    fn pull_resources(&self) -> Result<ResourceMap, ApiError> {
        Ok(self
            .resources
            .lock()
            .map_err(|e| ApiError::Http(e.to_string()))?
            .clone())
    }

    fn push_resources(&self, resources: &ResourceMap) -> Result<PushResult, ApiError> {
        *self
            .resources
            .lock()
            .map_err(|e| ApiError::Http(e.to_string()))? = resources.clone();
        Ok(PushResult {
            success: true,
            message: "Push successful".to_string(),
            commands: vec![],
        })
    }

    fn command_user_override(&self) -> Option<String> {
        self.command_user_override.clone()
    }

    fn push_command_batch(&self, command_batch_bytes: &[u8]) -> Result<PushResult, ApiError> {
        let batch = CommandBatch::decode(command_batch_bytes)
            .map_err(|e| ApiError::Http(format!("invalid command batch bytes: {e}")))?;
        let summaries = batch.commands.iter().map(command_to_json_summary).collect();
        Ok(PushResult {
            success: true,
            message: "Push successful".to_string(),
            commands: summaries,
        })
    }

    fn push_command_batch_to_branch(
        &self,
        _branch_id: &str,
        command_batch_bytes: &[u8],
    ) -> Result<PushResult, ApiError> {
        self.push_command_batch(command_batch_bytes)
    }

    fn record_successful_push(&self, resources: &ResourceMap) -> Result<(), ApiError> {
        *self
            .resources
            .lock()
            .map_err(|e| ApiError::Http(e.to_string()))? = resources.clone();
        Ok(())
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

    fn list_conversations(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<ConversationListResponse, ApiError> {
        Ok(ConversationListResponse {
            conversations: vec![],
            count: 0,
            limit,
            offset,
            extra: serde_json::Map::new(),
        })
    }

    fn get_conversation(&self, conversation_id: &str) -> Result<ConversationDetail, ApiError> {
        Ok(ConversationDetail {
            summary: ConversationSummary {
                conversation_id: conversation_id.to_string(),
                account_id: "local-account".to_string(),
                project_id: "local-project".to_string(),
                created_at: None,
                started_at: None,
                finished_at: None,
                channel: None,
                from_number: None,
                to_number: None,
                language: None,
                variant_id: None,
                client_env: None,
                total_duration: None,
                polyai_duration: None,
                duration: None,
                in_progress: None,
                handoff: None,
                handoff_destination: None,
                handoff_reason: None,
                direction: None,
                tags: vec![],
                poly_score: None,
                short_summary: None,
                deployment_id: None,
                note: None,
                audio_url: None,
                extra: serde_json::Map::new(),
            },
            turns: vec![],
            metrics: None,
            function_events: None,
        })
    }

    fn get_conversation_audio(
        &self,
        _conversation_id: &str,
        _direction: &str,
        _redacted: bool,
    ) -> Result<Vec<u8>, ApiError> {
        Ok(Vec::new())
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
