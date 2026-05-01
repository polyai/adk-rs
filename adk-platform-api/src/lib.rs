use adk_domain::{DeploymentList, PushResult, ResourceMap};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("http error: {0}")]
    Http(String),
    #[error("not implemented")]
    NotImplemented,
}

pub trait PlatformClient: Send + Sync {
    fn pull_resources(&self) -> Result<ResourceMap, ApiError>;
    fn push_resources(&self, _resources: &ResourceMap) -> Result<PushResult, ApiError>;
    fn list_deployments(&self, _environment: &str) -> Result<DeploymentList, ApiError>;
    fn create_chat_session(&self, _payload: Value) -> Result<Value, ApiError>;
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryPlatformClient {
    resources: Arc<Mutex<ResourceMap>>,
}

impl InMemoryPlatformClient {
    pub fn with_resources(resources: ResourceMap) -> Self {
        Self {
            resources: Arc::new(Mutex::new(resources)),
        }
    }
}

impl PlatformClient for InMemoryPlatformClient {
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

    fn list_deployments(&self, _environment: &str) -> Result<DeploymentList, ApiError> {
        Ok(DeploymentList {
            versions: vec![],
            active_deployment_hashes: Default::default(),
        })
    }

    fn create_chat_session(&self, _payload: Value) -> Result<Value, ApiError> {
        Ok(serde_json::json!({
            "conversation_id": "local-conversation",
            "response": "Mock chat session created",
            "conversation_ended": false
        }))
    }
}
