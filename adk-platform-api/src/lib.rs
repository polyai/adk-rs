use adk_domain::{
    BranchDescriptor, BranchMergeResult, DeploymentList, PushResult, Resource, ResourceMap,
};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::entities::{self, EntityCreate, EntityDelete, EntityUpdate};
use adk_protobuf::functions::{
    ErrorsUpdate, FunctionCreateFunction, FunctionDeleteFunction, FunctionError,
    FunctionParameterUpdate, FunctionUpdateFunction, ParametersUpdate,
};
use adk_protobuf::knowledge_base::{
    ExampleQueries, KnowledgeBaseCreateTopic, KnowledgeBaseDeleteTopic, KnowledgeBaseUpdateTopic,
};
use adk_protobuf::{Command, CommandBatch, Metadata};
use prost::Message;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("http error: {0}")]
    Http(String),
    #[error("missing required configuration: {0}")]
    MissingConfig(String),
}

/// Platform API boundary used by `adk-core`.
///
/// NOTE:
/// - `HttpPlatformClient` is the real networked implementation.
/// - `InMemoryPlatformClient` is a deterministic test double for local/unit tests.
mod push_extended;

pub trait PlatformClient: Send + Sync {
    fn pull_resources(&self) -> Result<ResourceMap, ApiError>;
    fn pull_resources_by_name(&self, name: &str) -> Result<ResourceMap, ApiError> {
        let _ = name;
        self.pull_resources()
    }
    fn preview_push_resources(&self, resources: &ResourceMap) -> Result<PushResult, ApiError> {
        let _ = resources;
        Ok(PushResult {
            success: true,
            message: "Dry run completed. No changes were pushed.".to_string(),
            commands: vec![],
        })
    }
    fn preview_push_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
        actor: Option<&str>,
    ) -> Result<PushResult, ApiError> {
        let _ = (projection, actor);
        self.preview_push_resources(resources)
    }
    fn push_resources(&self, _resources: &ResourceMap) -> Result<PushResult, ApiError>;
    fn push_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
        actor: Option<&str>,
    ) -> Result<PushResult, ApiError> {
        let _ = (projection, actor);
        self.push_resources(resources)
    }
    fn list_deployments(&self, _environment: &str) -> Result<DeploymentList, ApiError>;
    fn create_chat_session(&self, _payload: Value) -> Result<Value, ApiError>;
    fn send_chat_message(&self, _payload: Value) -> Result<Value, ApiError>;
    fn end_chat_session(&self, _payload: Value) -> Result<Value, ApiError>;
    fn list_branches(&self) -> Result<Vec<BranchDescriptor>, ApiError>;
    fn create_branch(&self, branch_name: &str) -> Result<String, ApiError>;
    fn delete_branch(&self, branch_id: &str) -> Result<(), ApiError>;
    fn merge_branch(
        &self,
        deployment_message: &str,
        conflict_resolutions: Option<Vec<Value>>,
    ) -> Result<BranchMergeResult, ApiError>;
}

/// Test-only in-memory client used for deterministic non-network workflows.
#[derive(Debug, Clone)]
pub struct InMemoryPlatformClient {
    resources: Arc<Mutex<ResourceMap>>,
    branches: Arc<Mutex<indexmap::IndexMap<String, String>>>,
    named_resources: Arc<Mutex<indexmap::IndexMap<String, ResourceMap>>>,
    deployments: Arc<Mutex<DeploymentList>>,
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
        }
    }
}

fn default_branches() -> indexmap::IndexMap<String, String> {
    let mut branches = indexmap::IndexMap::new();
    branches.insert("main".to_string(), "main".to_string());
    branches
}

impl PlatformClient for InMemoryPlatformClient {
    fn preview_push_resources(&self, resources: &ResourceMap) -> Result<PushResult, ApiError> {
        let _ = resources;
        Ok(PushResult {
            success: true,
            message: "Dry run completed. No changes were pushed.".to_string(),
            commands: vec![],
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

    fn list_deployments(&self, _environment: &str) -> Result<DeploymentList, ApiError> {
        Ok(self
            .deployments
            .lock()
            .map_err(|e| ApiError::Http(e.to_string()))?
            .clone())
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

#[derive(Debug, Clone)]
pub struct HttpPlatformClient {
    client: reqwest::blocking::Client,
    base_url: String,
    api_key: String,
    account_id: String,
    project_id: String,
    branch_id: String,
}

impl HttpPlatformClient {
    pub fn new(
        region: &str,
        account_id: &str,
        project_id: &str,
        branch_id: Option<&str>,
    ) -> Result<Self, ApiError> {
        let api_key = env::var("POLY_ADK_KEY").map_err(|_| {
            ApiError::MissingConfig(
                "POLY_ADK_KEY is not set; export POLY_ADK_KEY=<api-key>".to_string(),
            )
        })?;
        let base_url = base_url_for_region(region)?;
        Ok(Self {
            client: reqwest::blocking::Client::new(),
            base_url: base_url.to_string(),
            api_key,
            account_id: account_id.to_string(),
            project_id: project_id.to_string(),
            branch_id: branch_id.unwrap_or("main").to_string(),
        })
    }

    fn request_json(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        query: Option<&[(&str, &str)]>,
        body: Option<Value>,
    ) -> Result<Value, ApiError> {
        let url = format!("{}{}", self.base_url, endpoint);
        let mut request = self
            .client
            .request(method, &url)
            .header("X-API-KEY", &self.api_key)
            .header("Content-Type", "application/json");
        if let Some(q) = query {
            request = request.query(q);
        }
        if let Some(json) = body {
            request = request.json(&json);
        }
        let response = request.send().map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().unwrap_or_default();
            return Err(ApiError::Http(format!("status={status} body={text}")));
        }
        response.json().map_err(|e| ApiError::Http(e.to_string()))
    }

    fn request_binary_json(&self, endpoint: &str, payload: &[u8]) -> Result<Value, ApiError> {
        let url = format!("{}{}", self.base_url, endpoint);
        let correlation_id = format!("adk-{}", Uuid::new_v4());
        let response = self
            .client
            .post(url)
            .header("X-API-KEY", &self.api_key)
            .header("X-PolyAI-Correlation-Id", correlation_id)
            .header("Content-Type", "application/octet-stream")
            .body(payload.to_vec())
            .send()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().unwrap_or_default();
            return Err(ApiError::Http(format!("status={status} body={text}")));
        }
        response.json().map_err(|e| ApiError::Http(e.to_string()))
    }

    fn fetch_projection_response(&self) -> Result<Value, ApiError> {
        self.fetch_projection_response_for_branch(&self.branch_id)
    }

    fn fetch_projection_response_for_branch(&self, branch_id: &str) -> Result<Value, ApiError> {
        let endpoint = format!(
            "/accounts/{}/projects/{}/branches/{branch_id}/projection",
            self.account_id, self.project_id
        );
        self.request_json(reqwest::Method::GET, &endpoint, None, None)
    }

    fn fetch_projection_response_for_deployment(
        &self,
        deployment_id: &str,
    ) -> Result<Value, ApiError> {
        let endpoint = format!(
            "/accounts/{}/projects/{}/deployments/{deployment_id}/projection",
            self.account_id, self.project_id
        );
        self.request_json(reqwest::Method::GET, &endpoint, None, None)
    }

    fn branches_endpoint(&self) -> String {
        format!(
            "/accounts/{}/projects/{}/branches",
            self.account_id, self.project_id
        )
    }

    fn fetch_branch_sequence(&self, branch_id: &str) -> Result<u64, ApiError> {
        let endpoint = format!("{}/{branch_id}/sequence", self.branches_endpoint());
        let payload = self.request_json(reqwest::Method::GET, &endpoint, None, None)?;
        Ok(payload
            .get("lastKnownSequence")
            .and_then(|v| match v {
                Value::String(s) => s.parse::<u64>().ok(),
                Value::Number(n) => n.as_u64(),
                _ => None,
            })
            .unwrap_or(0))
    }

    fn prepare_branch_chat(&self) -> Result<Value, ApiError> {
        let sequence = self.fetch_branch_sequence(&self.branch_id)?;
        let endpoint = format!("{}/{}/chat", self.branches_endpoint(), self.branch_id);
        self.request_json(
            reqwest::Method::POST,
            &endpoint,
            None,
            Some(serde_json::json!({
                "expectedBranchLastKnownSequence": sequence,
            })),
        )
    }

    fn extract_projection(response: Value) -> Value {
        response.get("projection").cloned().unwrap_or(response)
    }

    fn fetch_deployments_raw(&self, client_env: Option<&str>) -> Result<Vec<Value>, ApiError> {
        let endpoint = format!(
            "/accounts/{}/projects/{}/deployments",
            self.account_id, self.project_id
        );
        let deployments = if let Some(env_name) = client_env {
            let query = [("client_env", env_name)];
            self.request_json(reqwest::Method::GET, &endpoint, Some(&query), None)?
        } else {
            self.request_json(reqwest::Method::GET, &endpoint, None, None)?
        };
        Ok(deployments
            .get("deployments")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    fn deployment_id_from_active_env(&self, env_name: &str) -> Result<Option<String>, ApiError> {
        let active_endpoint = format!(
            "/accounts/{}/projects/{}/deployments/active",
            self.account_id, self.project_id
        );
        let active = self.request_json(reqwest::Method::GET, &active_endpoint, None, None)?;
        let payload = active.get(env_name);
        if let Some(id) = payload
            .and_then(|v| {
                v.get("deployment_id")
                    .or_else(|| v.get("deploymentId"))
                    .or_else(|| v.get("id"))
            })
            .and_then(Value::as_str)
        {
            return Ok(Some(id.to_string()));
        }
        let hash = payload.and_then(|v| {
            if v.is_string() {
                v.as_str().map(ToString::to_string)
            } else {
                v.get("version_hash")
                    .or_else(|| v.get("versionHash"))
                    .or_else(|| v.get("hash"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
            }
        });
        if let Some(hash) = hash {
            return self.deployment_id_from_version_prefix(&hash);
        }
        Ok(None)
    }

    fn deployment_id_from_version_prefix(&self, version: &str) -> Result<Option<String>, ApiError> {
        let prefix = version.chars().take(9).collect::<String>().to_lowercase();
        if prefix.is_empty() {
            return Ok(None);
        }
        for env_name in [None, Some("sandbox"), Some("pre-release"), Some("live")] {
            for deployment in self.fetch_deployments_raw(env_name)? {
                let hash = deployment
                    .get("version_hash")
                    .or_else(|| deployment.get("versionHash"))
                    .or_else(|| deployment.get("hash"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .chars()
                    .take(9)
                    .collect::<String>()
                    .to_lowercase();
                if hash == prefix {
                    if let Some(id) = deployment
                        .get("id")
                        .or_else(|| deployment.get("deployment_id"))
                        .or_else(|| deployment.get("deploymentId"))
                        .and_then(Value::as_str)
                    {
                        return Ok(Some(id.to_string()));
                    }
                }
            }
        }
        Ok(None)
    }
}

impl PlatformClient for HttpPlatformClient {
    fn pull_resources_by_name(&self, name: &str) -> Result<ResourceMap, ApiError> {
        let env_names = ["sandbox", "pre-release", "live"];
        if env_names.contains(&name) {
            if let Some(deployment_id) = self.deployment_id_from_active_env(name)? {
                let response = self.fetch_projection_response_for_deployment(&deployment_id)?;
                return projection_to_resource_map(&Self::extract_projection(response));
            }
            return Err(ApiError::Http(format!(
                "No active deployment found for environment '{name}'"
            )));
        }

        let branches = self.list_branches()?;
        if let Some(branch) = branches
            .iter()
            .find(|b| b.name == name || b.branch_id == name)
        {
            let response = self.fetch_projection_response_for_branch(&branch.branch_id)?;
            return projection_to_resource_map(&Self::extract_projection(response));
        }

        if let Some(deployment_id) = self.deployment_id_from_version_prefix(name)? {
            let response = self.fetch_projection_response_for_deployment(&deployment_id)?;
            return projection_to_resource_map(&Self::extract_projection(response));
        }

        Err(ApiError::Http(format!(
            "Name '{name}' not found in environments, branches, or deployments"
        )))
    }

    fn pull_resources(&self) -> Result<ResourceMap, ApiError> {
        let response = self.fetch_projection_response()?;
        let projection = response
            .get("projection")
            .cloned()
            .unwrap_or_else(|| response.clone());
        projection_to_resource_map(&projection)
    }

    fn push_resources(&self, resources: &ResourceMap) -> Result<PushResult, ApiError> {
        self.push_resources_with_options(resources, None, None)
    }

    fn push_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
        actor: Option<&str>,
    ) -> Result<PushResult, ApiError> {
        let (commands, last_known_sequence) =
            self.build_push_commands_with_options(resources, projection, actor)?;
        if commands.is_empty() {
            return Ok(PushResult {
                success: false,
                message: "No changes detected".to_string(),
                commands: vec![],
            });
        }

        let endpoint = format!(
            "/accounts/{}/projects/{}/branches/{}/command-batch",
            self.account_id, self.project_id, self.branch_id
        );
        let batch = CommandBatch {
            last_known_sequence,
            commands,
        };
        let bytes = batch.encode_to_vec();
        let response = self.request_binary_json(&endpoint, &bytes)?;
        let response_commands = extract_response_commands(&response);
        let response_message = response
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Push accepted by platform endpoint (protobuf command-batch)")
            .to_string();
        Ok(PushResult {
            success: true,
            message: response_message,
            commands: response_commands,
        })
    }

    fn preview_push_resources(&self, resources: &ResourceMap) -> Result<PushResult, ApiError> {
        self.preview_push_resources_with_options(resources, None, None)
    }

    fn preview_push_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
        actor: Option<&str>,
    ) -> Result<PushResult, ApiError> {
        let (commands, _) = self.build_push_commands_with_options(resources, projection, actor)?;
        if commands.is_empty() {
            return Ok(PushResult {
                success: false,
                message: "No changes detected".to_string(),
                commands: vec![],
            });
        }
        Ok(PushResult {
            success: true,
            message: "Dry run completed. No changes were pushed.".to_string(),
            commands: commands
                .iter()
                .map(command_to_json_summary)
                .collect::<Vec<_>>(),
        })
    }

    fn list_deployments(&self, environment: &str) -> Result<DeploymentList, ApiError> {
        let endpoint = format!(
            "/accounts/{}/projects/{}/deployments",
            self.account_id, self.project_id
        );
        let query = [("client_env", environment)];
        let deployments = self.request_json(reqwest::Method::GET, &endpoint, Some(&query), None)?;
        let versions = deployments
            .get("deployments")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let active_endpoint = format!(
            "/accounts/{}/projects/{}/deployments/active",
            self.account_id, self.project_id
        );
        let active = self.request_json(reqwest::Method::GET, &active_endpoint, None, None)?;
        let mut active_hashes: indexmap::IndexMap<String, String> = Default::default();
        if let Some(obj) = active.as_object() {
            for (env_name, payload) in obj {
                if let Some(hash) = payload.get("version_hash").and_then(Value::as_str) {
                    active_hashes.insert(env_name.clone(), hash.to_string());
                }
            }
        }

        Ok(DeploymentList {
            versions,
            active_deployment_hashes: active_hashes,
        })
    }

    fn create_chat_session(&self, payload: Value) -> Result<Value, ApiError> {
        let environment = payload
            .get("environment")
            .and_then(Value::as_str)
            .unwrap_or("sandbox");
        let channel = payload
            .get("channel")
            .and_then(Value::as_str)
            .unwrap_or("chat.polyai");
        let mut body = serde_json::json!({
            "channel": channel,
        });
        if let Some(variant) = payload.get("variant").and_then(Value::as_str) {
            body["variant_id"] = Value::String(variant.to_string());
        }
        if let Some(input_lang) = payload.get("input_lang").and_then(Value::as_str) {
            body["asr_lang_code"] = Value::String(input_lang.to_string());
        }
        if let Some(output_lang) = payload.get("output_lang").and_then(Value::as_str) {
            body["tts_lang_code"] = Value::String(output_lang.to_string());
        }

        let endpoint = if environment == "draft" {
            let chat_info = self.prepare_branch_chat()?;
            let artifact_version = chat_info
                .get("artifactVersion")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ApiError::Http(format!(
                        "missing artifactVersion in branch chat response: {chat_info}"
                    ))
                })?;
            let lambda_deployment_version = chat_info
                .get("lambdaDeploymentVersion")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ApiError::Http(format!(
                        "missing lambdaDeploymentVersion in branch chat response: {chat_info}"
                    ))
                })?;
            body["artifact_version"] = Value::String(artifact_version.to_string());
            body["lambda_deployment_version"] =
                Value::String(lambda_deployment_version.to_string());
            format!(
                "/accounts/{}/projects/{}/draft/chat",
                self.account_id, self.project_id
            )
        } else {
            body["client_env"] = Value::String(environment.to_string());
            format!(
                "/accounts/{}/projects/{}/chat",
                self.account_id, self.project_id
            )
        };
        self.request_json(reqwest::Method::POST, &endpoint, None, Some(body))
    }

    fn send_chat_message(&self, payload: Value) -> Result<Value, ApiError> {
        let conversation_id = payload
            .get("conversation_id")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError::MissingConfig("conversation_id".to_string()))?;
        let environment = payload
            .get("environment")
            .and_then(Value::as_str)
            .unwrap_or("sandbox");
        let message = payload
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut body = serde_json::json!({"message": message});
        if environment != "draft" {
            body["client_env"] = Value::String(environment.to_string());
        }
        if let Some(input_lang) = payload.get("input_lang").and_then(Value::as_str) {
            body["asr_lang_code"] = Value::String(input_lang.to_string());
        }
        if let Some(output_lang) = payload.get("output_lang").and_then(Value::as_str) {
            body["tts_lang_code"] = Value::String(output_lang.to_string());
        }
        let endpoint = if environment == "draft" {
            format!(
                "/accounts/{}/projects/{}/draft/chat/{conversation_id}",
                self.account_id, self.project_id
            )
        } else {
            format!(
                "/accounts/{}/projects/{}/chat/{conversation_id}",
                self.account_id, self.project_id
            )
        };
        self.request_json(reqwest::Method::POST, &endpoint, None, Some(body))
    }

    fn end_chat_session(&self, payload: Value) -> Result<Value, ApiError> {
        let conversation_id = payload
            .get("conversation_id")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError::MissingConfig("conversation_id".to_string()))?;
        let environment = payload
            .get("environment")
            .and_then(Value::as_str)
            .unwrap_or("sandbox");
        let endpoint = format!(
            "/accounts/{}/projects/{}/chat/{conversation_id}/end",
            self.account_id, self.project_id
        );
        self.request_json(
            reqwest::Method::POST,
            &endpoint,
            None,
            Some(serde_json::json!({"client_env": environment})),
        )
    }

    fn list_branches(&self) -> Result<Vec<BranchDescriptor>, ApiError> {
        let payload =
            self.request_json(reqwest::Method::GET, &self.branches_endpoint(), None, None)?;
        let branches = payload
            .get("branches")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(branches.len() + 1);
        out.push(BranchDescriptor {
            name: "main".to_string(),
            branch_id: "main".to_string(),
        });
        for branch in branches {
            let Some(branch_id) = branch.get("branchId").and_then(Value::as_str) else {
                continue;
            };
            let name = branch
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(branch_id)
                .to_string();
            if !out.iter().any(|existing| existing.branch_id == branch_id) {
                out.push(BranchDescriptor {
                    name,
                    branch_id: branch_id.to_string(),
                });
            }
        }
        Ok(out)
    }

    fn create_branch(&self, branch_name: &str) -> Result<String, ApiError> {
        let expected_main_last_known_sequence = self.fetch_branch_sequence("main")?;
        let response = self.request_json(
            reqwest::Method::POST,
            &self.branches_endpoint(),
            None,
            Some(serde_json::json!({
                "expectedMainLastKnownSequence": expected_main_last_known_sequence,
                "branchName": branch_name,
            })),
        )?;
        response
            .get("branchId")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .ok_or_else(|| ApiError::Http("missing branchId in create-branch response".to_string()))
    }

    fn delete_branch(&self, branch_id: &str) -> Result<(), ApiError> {
        let sequence = self.fetch_branch_sequence(branch_id)?;
        let endpoint = format!("{}/{branch_id}", self.branches_endpoint());
        let _ = self.request_json(
            reqwest::Method::DELETE,
            &endpoint,
            None,
            Some(serde_json::json!({
                "expectedBranchLastKnownSequence": sequence,
            })),
        )?;
        Ok(())
    }

    fn merge_branch(
        &self,
        deployment_message: &str,
        conflict_resolutions: Option<Vec<Value>>,
    ) -> Result<BranchMergeResult, ApiError> {
        let expected_branch_last_known_sequence = self.fetch_branch_sequence(&self.branch_id)?;
        let mut payload = serde_json::json!({
            "expectedBranchLastKnownSequence": expected_branch_last_known_sequence,
            "deploymentMessage": deployment_message,
        });
        if let Some(resolutions) = conflict_resolutions {
            payload["conflictResolutions"] = Value::Array(resolutions);
        }
        let endpoint = format!(
            "/accounts/{}/projects/{}/branches/{}/merge",
            self.account_id, self.project_id, self.branch_id
        );
        let url = format!("{}{}", self.base_url, endpoint);
        let response = self
            .client
            .post(url)
            .header("X-API-KEY", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .map_err(|e| ApiError::Http(e.to_string()))?;

        let status = response.status();
        let body: Value = response
            .json()
            .map_err(|e| ApiError::Http(format!("failed to parse merge response: {e}")))?;
        if status == reqwest::StatusCode::BAD_REQUEST {
            if body
                .get("hasConflicts")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || body.get("conflicts").is_some()
            {
                return Ok(BranchMergeResult {
                    success: false,
                    conflicts: body
                        .get("conflicts")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                    errors: body
                        .get("errors")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                    sequence: body
                        .get("sequence")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                });
            }
            return Err(ApiError::Http(format!("status={status} body={body}")));
        }
        if !status.is_success() {
            return Err(ApiError::Http(format!("status={status} body={body}")));
        }
        Ok(BranchMergeResult {
            success: true,
            conflicts: vec![],
            errors: vec![],
            sequence: body
                .get("sequence")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        })
    }
}

impl HttpPlatformClient {
    fn build_push_commands_with_options(
        &self,
        resources: &ResourceMap,
        projection_override: Option<&Value>,
        actor: Option<&str>,
    ) -> Result<(Vec<Command>, u64), ApiError> {
        let (projection, last_known_sequence) = if let Some(projection) = projection_override {
            (projection.clone(), 0)
        } else {
            let projection_response = self.fetch_projection_response()?;
            let projection = projection_response
                .get("projection")
                .cloned()
                .unwrap_or_else(|| projection_response.clone());
            let last_known_sequence = projection_response
                .get("lastKnownSequence")
                .and_then(|v| match v {
                    Value::String(s) => s.parse::<u64>().ok(),
                    Value::Number(n) => n.as_u64(),
                    _ => None,
                })
                .unwrap_or(0);
            (projection, last_known_sequence)
        };
        let commands = build_phase1_commands_with_actor(resources, &projection, actor);
        Ok((commands, last_known_sequence))
    }
}

pub fn projection_to_resource_map(projection: &Value) -> Result<ResourceMap, ApiError> {
    let mut map = ResourceMap::new();

    for (id, topic) in topic_entries(projection) {
        let name = topic
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        let file_name = clean_name(&name).to_lowercase();
        let file_path = format!("topics/{file_name}.yaml");
        let content = serde_yaml::to_string(&serde_json::json!({
            "name": name,
            "enabled": topic.get("isActive").and_then(Value::as_bool).unwrap_or(true),
            "actions": topic.get("actions").and_then(Value::as_str).unwrap_or(""),
            "content": topic.get("content").and_then(Value::as_str).unwrap_or(""),
            "example_queries": topic.get("exampleQueries").and_then(Value::as_array).map(|arr| {
                arr.iter()
                    .filter_map(|x| x.get("query").and_then(Value::as_str).map(ToString::to_string))
                    .collect::<Vec<String>>()
            }).unwrap_or_default(),
        }))
        .map_err(|e| ApiError::Http(e.to_string()))?;
        map.insert(
            file_path.clone(),
            Resource {
                resource_id: id.clone(),
                name: name.clone(),
                file_path,
                payload: serde_json::json!({"content": content}),
            },
        );
    }

    for (id, function) in function_entries(projection) {
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        let file_name = clean_name(&name).to_lowercase();
        let file_path = format!("functions/{file_name}.py");
        let content = function
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        map.insert(
            file_path.clone(),
            Resource {
                resource_id: id.clone(),
                name,
                file_path,
                payload: serde_json::json!({"content": content}),
            },
        );
    }

    let mut entity_yaml_list = Vec::new();
    for (id, entity) in entity_entries(projection) {
        let name = entity
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        entity_yaml_list.push(serde_json::json!({
            "name": name,
            "description": entity.get("description").and_then(Value::as_str).unwrap_or(""),
            "entity_type": to_snake_case(entity.get("type").and_then(Value::as_str).unwrap_or("")),
            "config": projection_entity_config(&entity),
        }));
    }
    if !entity_yaml_list.is_empty() {
        let content = serde_yaml::to_string(&serde_json::json!({ "entities": entity_yaml_list }))
            .map_err(|e| ApiError::Http(e.to_string()))?;
        map.insert(
            "config/entities.yaml".to_string(),
            Resource {
                resource_id: "entities".to_string(),
                name: "entities".to_string(),
                file_path: "config/entities.yaml".to_string(),
                payload: serde_json::json!({"content": content}),
            },
        );
    }

    // variables/<name> logical resources
    for (id, variable) in variable_entries(projection) {
        let name = variable
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        let file_path = format!("variables/{name}");
        map.insert(
            file_path.clone(),
            Resource {
                resource_id: id,
                name,
                file_path,
                payload: serde_json::json!({
                    "content": serde_json::to_string_pretty(&variable).unwrap_or_else(|_| "{}".to_string())
                }),
            },
        );
    }

    // config/handoffs.yaml multi-resource file
    let mut handoff_yaml_list = Vec::new();
    for (_id, handoff) in handoff_entries(projection) {
        if !handoff
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            continue;
        }
        handoff_yaml_list.push(serde_json::json!({
            "name": handoff.get("name").and_then(Value::as_str).unwrap_or(""),
            "description": handoff.get("description").and_then(Value::as_str).unwrap_or(""),
            "is_default": handoff.get("isDefault").and_then(Value::as_bool).unwrap_or(false),
            "sip_config": handoff_sip_config_yaml(&handoff),
            "sip_headers": handoff_sip_headers_yaml(&handoff)
        }));
    }
    if !handoff_yaml_list.is_empty() {
        let content = serde_yaml::to_string(&serde_json::json!({ "handoffs": handoff_yaml_list }))
            .map_err(|e| ApiError::Http(e.to_string()))?;
        map.insert(
            "config/handoffs.yaml".to_string(),
            Resource {
                resource_id: "handoffs".to_string(),
                name: "handoffs".to_string(),
                file_path: "config/handoffs.yaml".to_string(),
                payload: serde_json::json!({ "content": content }),
            },
        );
    }

    // config/sms_templates.yaml multi-resource file
    let mut sms_yaml_list = Vec::new();
    for (_id, sms) in sms_entries(projection) {
        if !sms.get("active").and_then(Value::as_bool).unwrap_or(true) {
            continue;
        }
        sms_yaml_list.push(serde_json::json!({
                "name": sms.get("name").and_then(Value::as_str).unwrap_or(""),
                "text": sms.get("text").and_then(Value::as_str).unwrap_or(""),
                "env_phone_numbers": {
                    "sandbox": sms.get("envPhoneNumbers").and_then(|v| v.get("sandbox")).and_then(Value::as_str).unwrap_or(""),
                    "pre_release": sms.get("envPhoneNumbers").and_then(|v| v.get("preRelease")).and_then(Value::as_str).unwrap_or(""),
                    "live": sms.get("envPhoneNumbers").and_then(|v| v.get("live")).and_then(Value::as_str).unwrap_or(""),
                }
            }));
    }
    if !sms_yaml_list.is_empty() {
        let content = serde_yaml::to_string(&serde_json::json!({ "sms_templates": sms_yaml_list }))
            .map_err(|e| ApiError::Http(e.to_string()))?;
        map.insert(
            "config/sms_templates.yaml".to_string(),
            Resource {
                resource_id: "sms_templates".to_string(),
                name: "sms_templates".to_string(),
                file_path: "config/sms_templates.yaml".to_string(),
                payload: serde_json::json!({ "content": content }),
            },
        );
    }

    // phrase filters
    let mut phrase_yaml_list = Vec::new();
    for (_id, pf) in phrase_filter_entries(projection) {
        phrase_yaml_list.push(serde_json::json!({
                "name": pf.get("title").and_then(Value::as_str).unwrap_or(""),
                "description": pf.get("description").and_then(Value::as_str).unwrap_or(""),
                "regular_expressions": pf.get("regularExpressions").and_then(Value::as_array).cloned().unwrap_or_default(),
                "say_phrase": pf.get("sayPhrase").and_then(Value::as_bool).unwrap_or(false),
                "language_code": pf.get("languageCode").and_then(Value::as_str).unwrap_or(""),
            }));
    }
    if !phrase_yaml_list.is_empty() {
        let content = serde_yaml::to_string(&serde_json::json!({
            "phrase_filtering": phrase_yaml_list
        }))
        .map_err(|e| ApiError::Http(e.to_string()))?;
        map.insert(
            "voice/response_control/phrase_filtering.yaml".to_string(),
            Resource {
                resource_id: "phrase_filtering".to_string(),
                name: "phrase_filtering".to_string(),
                file_path: "voice/response_control/phrase_filtering.yaml".to_string(),
                payload: serde_json::json!({ "content": content }),
            },
        );
    }

    if let Some(features) = experimental_features(projection) {
        let content =
            serde_json::to_string_pretty(&features).map_err(|e| ApiError::Http(e.to_string()))?;
        map.insert(
            "agent_settings/experimental_config.json".to_string(),
            Resource {
                resource_id: "experimental_config".to_string(),
                name: "experimental_config".to_string(),
                file_path: "agent_settings/experimental_config.json".to_string(),
                payload: serde_json::json!({ "content": content }),
            },
        );
    }
    Ok(map)
}

fn projection_entity_config(entity: &Value) -> Value {
    if let Some(cfg) = entity.get("config") {
        return cfg.clone();
    }
    let entity_type = to_snake_case(entity.get("type").and_then(Value::as_str).unwrap_or(""));
    match entity_type.as_str() {
        "numeric" => entity
            .get("numberConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "alphanumeric" => entity
            .get("alphanumericConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "enum" => entity
            .get("multipleOptionsConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "date" => entity
            .get("dateConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "phone_number" => entity
            .get("phoneNumberConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "time" => entity
            .get("timeConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        _ => serde_json::json!({}),
    }
}

fn handoff_sip_config_yaml(handoff: &Value) -> Value {
    let sip_config = handoff.get("sipConfig");
    let config = sip_config
        .and_then(|v| v.get("config"))
        .or(sip_config)
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(invite) = config.get("invite") {
        return serde_json::json!({
            "method": "invite",
            "phone_number": invite.get("phoneNumber").and_then(Value::as_str).unwrap_or(""),
            "outbound_endpoint": invite.get("outboundEndpoint").and_then(Value::as_str).unwrap_or(""),
            "outbound_encryption": invite.get("outboundEncryption").and_then(Value::as_str).unwrap_or(""),
        });
    }
    if let Some(refer) = config.get("refer") {
        return serde_json::json!({
            "method": "refer",
            "phone_number": refer.get("phoneNumber").and_then(Value::as_str).unwrap_or(""),
        });
    }
    serde_json::json!({ "method": "bye" })
}

fn handoff_sip_headers_yaml(handoff: &Value) -> Value {
    let headers = handoff
        .get("sipHeaders")
        .and_then(|v| v.get("headers").or(Some(v)))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let yaml_headers: Vec<Value> = headers
        .iter()
        .map(|h| {
            serde_json::json!({
                "key": h.get("key").and_then(Value::as_str).unwrap_or(""),
                "value": h.get("value").and_then(Value::as_str).unwrap_or(""),
            })
        })
        .collect();
    Value::Array(yaml_headers)
}

fn base_url_for_region(region: &str) -> Result<&'static str, ApiError> {
    match region {
        "dev" => Ok("https://api.dev.poly.ai/adk/v1"),
        "staging" => Ok("https://api.staging.poly.ai/adk/v1"),
        "euw-1" => Ok("https://api.eu.poly.ai/adk/v1"),
        "uk-1" => Ok("https://api.uk.poly.ai/adk/v1"),
        "us-1" => Ok("https://api.us.poly.ai/adk/v1"),
        "studio" => Ok("https://api.studio.poly.ai/adk/v1"),
        _ => Err(ApiError::MissingConfig(format!("unknown region: {region}"))),
    }
}

/// Builds protobuf commands for push (topics, functions, entities, variables, handoffs, SMS,
/// phrase filters / stop keywords, experimental config).
///
/// **Execution ordering:** Python `SyncClientHandler.queue_resources` applies a strict global
/// ordering (deletes, creates, updates) plus `PRIORITY_*` lists per resource family. This
/// implementation builds those phases across the supported phase-1 and extended resource
/// families, then applies explicit command-type priority lists within each phase.
#[cfg(test)]
fn build_phase1_commands(resources: &ResourceMap, projection: &Value) -> Vec<Command> {
    build_phase1_commands_with_actor(resources, projection, None)
}

fn build_phase1_commands_with_actor(
    resources: &ResourceMap,
    projection: &Value,
    actor: Option<&str>,
) -> Vec<Command> {
    let metadata = command_metadata_with_actor(actor);
    let mut entity_del = Vec::new();
    let mut function_del = Vec::new();
    let mut topic_del = Vec::new();
    let mut entity_create = Vec::new();
    let mut function_create = Vec::new();
    let mut topic_create = Vec::new();
    let mut entity_update = Vec::new();
    let mut function_update = Vec::new();
    let mut topic_update = Vec::new();

    let remote_topics = topic_entries(projection)
        .into_iter()
        .map(|(id, t)| {
            (
                t.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(id.as_str())
                    .to_string(),
                id,
            )
        })
        .collect::<HashMap<_, _>>();
    let remote_functions = function_entries(projection)
        .into_iter()
        .map(|(id, f)| {
            (
                f.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(id.as_str())
                    .to_string(),
                (id, f),
            )
        })
        .collect::<HashMap<_, _>>();
    let remote_entities = entity_entries(projection)
        .into_iter()
        .map(|(id, e)| {
            (
                e.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(id.as_str())
                    .to_string(),
                id,
            )
        })
        .collect::<HashMap<_, _>>();

    let mut local_topic_names = HashSet::new();
    let mut local_function_names = HashSet::new();
    let mut local_entity_names = HashSet::new();

    for resource in resources.values() {
        let path = resource.file_path.as_str();
        let content = resource
            .payload
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();

        if path.starts_with("topics/") && path.ends_with(".yaml") {
            if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content) {
                let name = yaml
                    .get("name")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or(&resource.name)
                    .to_string();
                local_topic_names.insert(name.clone());
                let id = remote_topics
                    .get(&name)
                    .cloned()
                    .or_else(|| {
                        (!resource.resource_id.trim().is_empty() && resource.resource_id != "local")
                            .then_some(resource.resource_id.clone())
                    })
                    .unwrap_or_else(|| format!("topic-{}", clean_name(&name).to_lowercase()));
                let actions = yaml
                    .get("actions")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let text = yaml
                    .get("content")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let enabled = yaml
                    .get("enabled")
                    .and_then(serde_yaml::Value::as_bool)
                    .unwrap_or(true);
                let example_queries = yaml
                    .get("example_queries")
                    .and_then(serde_yaml::Value::as_sequence)
                    .map(|seq| {
                        seq.iter()
                            .filter_map(serde_yaml::Value::as_str)
                            .map(ToString::to_string)
                            .collect::<Vec<String>>()
                    })
                    .unwrap_or_default();

                if remote_topics.contains_key(&name) {
                    push_command(
                        &mut topic_update,
                        &metadata,
                        "update_topic",
                        CommandPayload::UpdateTopic(KnowledgeBaseUpdateTopic {
                            id: id.clone(),
                            name: Some(name.clone()),
                            content: Some(text),
                            actions: Some(actions),
                            example_queries: Some(ExampleQueries {
                                queries: example_queries,
                            }),
                            references: None,
                            is_active: Some(enabled),
                        }),
                    );
                } else {
                    push_command(
                        &mut topic_create,
                        &metadata,
                        "create_topic",
                        CommandPayload::CreateTopic(KnowledgeBaseCreateTopic {
                            id: id.clone(),
                            name: name.clone(),
                            content: text,
                            actions,
                            example_queries: Some(ExampleQueries {
                                queries: example_queries,
                            }),
                            references: None,
                            is_active: Some(enabled),
                        }),
                    );
                }
            }
        } else if path.starts_with("functions/") && path.ends_with(".py") {
            let name = path
                .split('/')
                .next_back()
                .unwrap_or_default()
                .trim_end_matches(".py")
                .to_string();
            local_function_names.insert(name.clone());
            let remote_function = remote_functions.get(&name);
            let id = remote_functions
                .get(&name)
                .map(|(id, _)| id.clone())
                .or_else(|| {
                    (!resource.resource_id.trim().is_empty() && resource.resource_id != "local")
                        .then_some(resource.resource_id.clone())
                })
                .unwrap_or_else(|| format!("function-{}", clean_name(&name).to_lowercase()));
            let inferred_description = infer_function_description(content);
            let inferred_parameters = infer_function_parameters(content);
            if remote_functions.contains_key(&name) {
                let description = remote_function
                    .and_then(|(_, f)| f.get("description").and_then(Value::as_str))
                    .map(ToString::to_string)
                    .or_else(|| {
                        (!inferred_description.is_empty()).then_some(inferred_description.clone())
                    });
                let parameters = remote_function
                    .and_then(|(_, f)| function_parameters_update_from_projection(f))
                    .or_else(|| {
                        (!inferred_parameters.is_empty()).then_some(ParametersUpdate {
                            parameters: inferred_parameters.clone(),
                        })
                    });
                let errors =
                    remote_function.and_then(|(_, f)| function_errors_update_from_projection(f));
                push_command(
                    &mut function_update,
                    &metadata,
                    "update_function",
                    CommandPayload::UpdateFunction(FunctionUpdateFunction {
                        id: id.clone(),
                        name: Some(name.clone()),
                        description,
                        parameters,
                        code: Some(content.to_string()),
                        errors,
                        references: None,
                        archived: remote_function
                            .and_then(|(_, f)| f.get("archived").and_then(Value::as_bool))
                            .or(Some(false)),
                    }),
                );
            } else {
                push_command(
                    &mut function_create,
                    &metadata,
                    "create_function",
                    CommandPayload::CreateFunction(FunctionCreateFunction {
                        id: id.clone(),
                        name: name.clone(),
                        description: inferred_description,
                        parameters: inferred_parameters,
                        code: content.to_string(),
                        errors: vec![],
                        latency_control: None,
                        references: None,
                        archived: Some(false),
                    }),
                );
            }
        } else if path == "config/entities.yaml"
            && let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content)
            && let Some(items) = yaml
                .get("entities")
                .and_then(serde_yaml::Value::as_sequence)
        {
            for item in items {
                let name = item
                    .get("name")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                local_entity_names.insert(name.clone());
                let id = remote_entities
                    .get(&name)
                    .cloned()
                    .or_else(|| {
                        (!resource.resource_id.trim().is_empty() && resource.resource_id != "local")
                            .then_some(resource.resource_id.clone())
                    })
                    .unwrap_or_else(|| format!("entity-{}", clean_name(&name).to_lowercase()));
                let entity_type = item
                    .get("entity_type")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or("free_text");
                let description = item
                    .get("description")
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let config = item.get("config");
                if remote_entities.contains_key(&name) {
                    push_command(
                        &mut entity_update,
                        &metadata,
                        "entity_update",
                        CommandPayload::EntityUpdate(EntityUpdate {
                            id: id.clone(),
                            name: name.clone(),
                            r#type: to_camel_case(entity_type),
                            description: description.clone(),
                            references: None,
                            config: build_entity_update_config(entity_type, config),
                        }),
                    );
                } else {
                    push_command(
                        &mut entity_create,
                        &metadata,
                        "entity_create",
                        CommandPayload::EntityCreate(EntityCreate {
                            id: id.clone(),
                            name: name.clone(),
                            r#type: to_camel_case(entity_type),
                            description: description.clone(),
                            references: None,
                            config: build_entity_create_config(entity_type, config),
                        }),
                    );
                }
            }
        }
    }

    for (name, id) in remote_topics {
        if !local_topic_names.contains(&name) {
            push_command(
                &mut topic_del,
                &metadata,
                "delete_topic",
                CommandPayload::DeleteTopic(KnowledgeBaseDeleteTopic { id }),
            );
        }
    }
    for (name, (id, _)) in remote_functions {
        if !local_function_names.contains(&name) {
            push_command(
                &mut function_del,
                &metadata,
                "delete_function",
                CommandPayload::DeleteFunction(FunctionDeleteFunction { id }),
            );
        }
    }
    for (name, id) in remote_entities {
        if !local_entity_names.contains(&name) {
            push_command(
                &mut entity_del,
                &metadata,
                "entity_delete",
                CommandPayload::EntityDelete(EntityDelete { id }),
            );
        }
    }

    let ext_groups =
        push_extended::extended_resource_command_groups(resources, projection, &metadata);

    let mut deletes: Vec<Command> = entity_del
        .into_iter()
        .chain(function_del)
        .chain(topic_del)
        .chain(ext_groups.deletes)
        .collect();
    order_commands_with_priority(&mut deletes, &DELETE_COMMAND_PRIORITY);

    let mut creates: Vec<Command> = entity_create
        .into_iter()
        .chain(function_create)
        .chain(topic_create)
        .chain(ext_groups.creates)
        .collect();
    order_commands_with_priority(&mut creates, &CREATE_COMMAND_PRIORITY);

    let mut updates: Vec<Command> = entity_update
        .into_iter()
        .chain(function_update)
        .chain(topic_update)
        .chain(ext_groups.updates)
        .collect();
    order_commands_with_priority(&mut updates, &UPDATE_COMMAND_PRIORITY);

    let mut out: Vec<Command> = Vec::new();
    out.extend(deletes);
    out.extend(creates);
    out.extend(updates);
    out.extend(ext_groups.post_updates);
    out
}

const DELETE_COMMAND_PRIORITY: &[&str] = &[
    "variable_delete",
    "entity_delete",
    "delete_function",
    "delete_topic",
    "handoff_delete",
    "sms_delete_template",
    "stop_keywords_delete",
];

const CREATE_COMMAND_PRIORITY: &[&str] = &[
    "variable_create",
    "entity_create",
    "sms_create_template",
    "handoff_create",
    "create_function",
    "create_topic",
    "stop_keywords_create",
];

const UPDATE_COMMAND_PRIORITY: &[&str] = &[
    "variable_update",
    "entity_update",
    "update_function",
    "update_topic",
    "handoff_update",
    "sms_update_template",
    "stop_keywords_update",
    "experimental_config_update_config",
];

fn order_commands_with_priority(commands: &mut Vec<Command>, priority: &[&str]) {
    commands.sort_by_key(|command| {
        priority
            .iter()
            .position(|value| *value == command.r#type.as_str())
            .unwrap_or(priority.len())
    });
}

fn command_metadata_with_actor(actor: Option<&str>) -> Option<Metadata> {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let created_by = actor
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(actor_identity);
    Some(Metadata {
        created_at: Some(prost_types::Timestamp {
            seconds: dur.as_secs() as i64,
            nanos: dur.subsec_nanos() as i32,
        }),
        created_by,
    })
}

pub(crate) fn actor_identity() -> String {
    if let Ok(user) = env::var("POLY_ADK_USER")
        && !user.trim().is_empty()
    {
        return user;
    }
    if let Ok(user) = env::var("USER")
        && !user.trim().is_empty()
    {
        return user;
    }
    if let Ok(user) = env::var("USERNAME")
        && !user.trim().is_empty()
    {
        return user;
    }
    "unknown-user".to_string()
}

pub(crate) fn push_command(
    out: &mut Vec<Command>,
    metadata: &Option<Metadata>,
    type_str: &str,
    payload: CommandPayload,
) {
    out.push(Command {
        r#type: type_str.to_string(),
        metadata: metadata.clone(),
        command_id: Uuid::new_v4().to_string(),
        payload: Some(payload),
    });
}

fn topic_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["knowledgeBase", "topics", "entities"])
}

fn function_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["functions", "functions", "entities"])
}

fn entity_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["entities", "entities", "entities"])
}

fn variable_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["variables", "variables", "entities"])
}

fn handoff_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["handoff", "handoffs", "entities"])
}

fn sms_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["sms", "templates", "entities"])
}

fn phrase_filter_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["stopKeywords", "filters", "entities"])
}

fn experimental_features(projection: &Value) -> Option<Value> {
    Some(
        projection
            .get("experimentalConfig")?
            .get("experimentalConfigs")?
            .get("entities")?
            .get("default")?
            .get("features")?
            .clone(),
    )
}

pub(crate) fn extract_entities_map(root: &Value, path: &[&str]) -> HashMap<String, Value> {
    let mut cur = root;
    for key in path {
        cur = match cur.get(*key) {
            Some(v) => v,
            None => return HashMap::new(),
        };
    }
    cur.as_object()
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default()
}

fn to_camel_case(s: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for ch in s.chars() {
        if ch == '_' || ch == '-' {
            upper = true;
            continue;
        }
        if upper {
            out.push(ch.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

pub(crate) fn clean_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
}

fn build_entity_create_config(
    entity_type: &str,
    config: Option<&serde_yaml::Value>,
) -> Option<entities::entity_create::Config> {
    match entity_type {
        "numeric" => Some(entities::entity_create::Config::Numeric(
            entities::NumberConfig {
                has_decimal: yaml_bool(config, "has_decimal", false),
                has_range: yaml_bool(config, "has_range", false),
                min: yaml_f32_opt(config, "min"),
                max: yaml_f32_opt(config, "max"),
            },
        )),
        "alphanumeric" => Some(entities::entity_create::Config::Alphanumeric(
            entities::AlphanumericConfig {
                enabled: yaml_bool(config, "enabled", true),
                validation_type: yaml_string(config, "validation_type"),
                regular_expression: yaml_string(config, "regular_expression"),
            },
        )),
        "enum" => Some(entities::entity_create::Config::Enum(
            entities::MultipleOptionsConfig {
                options: yaml_string_list(config, "options"),
            },
        )),
        "date" => Some(entities::entity_create::Config::Date(
            entities::DateConfig {
                relative_date: yaml_bool(config, "relative_date", false),
            },
        )),
        "phone_number" => Some(entities::entity_create::Config::PhoneNumber(
            entities::PhoneNumberConfig {
                enabled: yaml_bool(config, "enabled", true),
                country_codes: yaml_string_list(config, "country_codes"),
            },
        )),
        "time" => Some(entities::entity_create::Config::Time(
            entities::TimeConfig {
                enabled: yaml_bool(config, "enabled", true),
                start_time: yaml_string(config, "start_time"),
                end_time: yaml_string(config, "end_time"),
            },
        )),
        "address" => Some(entities::entity_create::Config::Address(
            entities::AddressConfig {},
        )),
        "free_text" => Some(entities::entity_create::Config::FreeText(
            entities::FreeTextConfig {},
        )),
        "name_config" => Some(entities::entity_create::Config::NameConfig(
            entities::NameConfig {},
        )),
        _ => None,
    }
}

fn infer_function_description(code: &str) -> String {
    let mut in_docstring = false;
    let mut delimiter = "";
    for raw in code.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if !in_docstring && (line.starts_with("\"\"\"") || line.starts_with("'''")) {
            delimiter = if line.starts_with("\"\"\"") {
                "\"\"\""
            } else {
                "'''"
            };
            let stripped = line.trim_start_matches(delimiter).trim();
            if let Some((first, _)) = stripped.split_once(delimiter) {
                return first.trim().to_string();
            }
            if !stripped.is_empty() {
                return stripped.to_string();
            }
            in_docstring = true;
            continue;
        }
        if in_docstring {
            if line.contains(delimiter) {
                return line
                    .split(delimiter)
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
            }
            return line.to_string();
        }
    }
    String::new()
}

fn infer_function_parameters(code: &str) -> Vec<FunctionParameterUpdate> {
    let signature = code
        .lines()
        .find_map(|line| line.trim().strip_prefix("def ").map(ToString::to_string));
    let Some(signature) = signature else {
        return vec![];
    };
    let Some(open) = signature.find('(') else {
        return vec![];
    };
    let Some(close) = signature[open + 1..].find(')') else {
        return vec![];
    };
    let params = &signature[open + 1..open + 1 + close];
    params
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != "self" && *p != "conv")
        .map(|p| p.split('=').next().unwrap_or_default().trim())
        .map(|p| p.split(':').next().unwrap_or_default().trim())
        .filter(|p| !p.is_empty())
        .map(|name| FunctionParameterUpdate {
            id: clean_name(name).to_lowercase(),
            name: name.to_string(),
            description: String::new(),
            r#type: "string".to_string(),
        })
        .collect()
}

fn function_parameters_update_from_projection(function: &Value) -> Option<ParametersUpdate> {
    let parameters = function.get("parameters")?.as_array()?;
    let updates: Vec<FunctionParameterUpdate> = parameters
        .iter()
        .map(|p| FunctionParameterUpdate {
            id: p
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            name: p
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            description: p
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            r#type: p
                .get("type")
                .or_else(|| p.get("parameterType"))
                .and_then(Value::as_str)
                .unwrap_or("string")
                .to_string(),
        })
        .collect();
    (!updates.is_empty()).then_some(ParametersUpdate {
        parameters: updates,
    })
}

fn function_errors_update_from_projection(function: &Value) -> Option<ErrorsUpdate> {
    let errors = function.get("errors")?.as_array()?;
    let updates: Vec<FunctionError> = errors
        .iter()
        .map(|e| FunctionError {
            lineno: e.get("lineno").and_then(Value::as_i64).unwrap_or_default() as i32,
            message: e
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            text: e
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        })
        .collect();
    (!updates.is_empty()).then_some(ErrorsUpdate { errors: updates })
}

fn build_entity_update_config(
    entity_type: &str,
    config: Option<&serde_yaml::Value>,
) -> Option<entities::entity_update::Config> {
    match entity_type {
        "numeric" => Some(entities::entity_update::Config::Numeric(
            entities::NumberConfig {
                has_decimal: yaml_bool(config, "has_decimal", false),
                has_range: yaml_bool(config, "has_range", false),
                min: yaml_f32_opt(config, "min"),
                max: yaml_f32_opt(config, "max"),
            },
        )),
        "alphanumeric" => Some(entities::entity_update::Config::Alphanumeric(
            entities::AlphanumericConfig {
                enabled: yaml_bool(config, "enabled", true),
                validation_type: yaml_string(config, "validation_type"),
                regular_expression: yaml_string(config, "regular_expression"),
            },
        )),
        "enum" => Some(entities::entity_update::Config::Enum(
            entities::MultipleOptionsConfig {
                options: yaml_string_list(config, "options"),
            },
        )),
        "date" => Some(entities::entity_update::Config::Date(
            entities::DateConfig {
                relative_date: yaml_bool(config, "relative_date", false),
            },
        )),
        "phone_number" => Some(entities::entity_update::Config::PhoneNumber(
            entities::PhoneNumberConfig {
                enabled: yaml_bool(config, "enabled", true),
                country_codes: yaml_string_list(config, "country_codes"),
            },
        )),
        "time" => Some(entities::entity_update::Config::Time(
            entities::TimeConfig {
                enabled: yaml_bool(config, "enabled", true),
                start_time: yaml_string(config, "start_time"),
                end_time: yaml_string(config, "end_time"),
            },
        )),
        "address" => Some(entities::entity_update::Config::Address(
            entities::AddressConfig {},
        )),
        "free_text" => Some(entities::entity_update::Config::FreeText(
            entities::FreeTextConfig {},
        )),
        "name_config" => Some(entities::entity_update::Config::NameConfig(
            entities::NameConfig {},
        )),
        _ => None,
    }
}

fn yaml_get<'a>(config: Option<&'a serde_yaml::Value>, key: &str) -> Option<&'a serde_yaml::Value> {
    config.and_then(|c| c.get(key))
}

fn yaml_bool(config: Option<&serde_yaml::Value>, key: &str, default: bool) -> bool {
    yaml_get(config, key)
        .and_then(serde_yaml::Value::as_bool)
        .unwrap_or(default)
}

fn yaml_string(config: Option<&serde_yaml::Value>, key: &str) -> String {
    yaml_get(config, key)
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn yaml_string_list(config: Option<&serde_yaml::Value>, key: &str) -> Vec<String> {
    yaml_get(config, key)
        .and_then(serde_yaml::Value::as_sequence)
        .map(|seq| {
            seq.iter()
                .filter_map(serde_yaml::Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn yaml_f32_opt(config: Option<&serde_yaml::Value>, key: &str) -> Option<f32> {
    yaml_get(config, key).and_then(|v| match v {
        serde_yaml::Value::Number(n) => n.as_f64().map(|x| x as f32),
        _ => None,
    })
}

fn extract_response_commands(response: &Value) -> Vec<Value> {
    if let Some(commands) = response.get("commands").and_then(Value::as_array) {
        return commands.clone();
    }
    if let Some(commands) = response
        .get("commandBatch")
        .and_then(|v| v.get("commands"))
        .and_then(Value::as_array)
    {
        return commands.clone();
    }
    if let Some(commands) = response
        .get("result")
        .and_then(|v| v.get("commands"))
        .and_then(Value::as_array)
    {
        return commands.clone();
    }
    vec![]
}

fn command_to_json_summary(command: &Command) -> Value {
    serde_json::json!({
        "type": command.r#type,
        "command_id": command.command_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_create_topic_command_when_remote_missing() {
        let mut resources = ResourceMap::new();
        resources.insert(
            "topics/sample.yaml".to_string(),
            Resource {
                resource_id: "local".to_string(),
                name: "sample".to_string(),
                file_path: "topics/sample.yaml".to_string(),
                payload: serde_json::json!({
                    "content": "name: sample\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
                }),
            },
        );
        let projection = serde_json::json!({});
        let commands = build_phase1_commands(&resources, &projection);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].r#type, "create_topic");
        assert!(commands[0].metadata.is_some());
        assert!(matches!(
            commands[0].payload,
            Some(CommandPayload::CreateTopic(_))
        ));
    }

    #[test]
    fn create_topic_uses_local_resource_id_before_synthetic_fallback() {
        let mut resources = ResourceMap::new();
        resources.insert(
            "topics/sample.yaml".to_string(),
            Resource {
                resource_id: "TOPIC-custom-id".to_string(),
                name: "sample".to_string(),
                file_path: "topics/sample.yaml".to_string(),
                payload: serde_json::json!({
                    "content": "name: sample\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
                }),
            },
        );
        let projection = serde_json::json!({});
        let commands = build_phase1_commands(&resources, &projection);
        let create_cmd = commands
            .iter()
            .find(|c| c.r#type == "create_topic")
            .expect("create topic command");
        match &create_cmd.payload {
            Some(CommandPayload::CreateTopic(msg)) => assert_eq!(msg.id, "TOPIC-custom-id"),
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn push_commands_can_use_supplied_projection_and_actor() {
        let client = HttpPlatformClient {
            client: reqwest::blocking::Client::new(),
            base_url: "http://localhost".to_string(),
            api_key: "test-key".to_string(),
            account_id: "test-account".to_string(),
            project_id: "test-project".to_string(),
            branch_id: "main".to_string(),
        };
        let mut resources = ResourceMap::new();
        resources.insert(
            "topics/sample.yaml".to_string(),
            Resource {
                resource_id: "topic-1".to_string(),
                name: "sample".to_string(),
                file_path: "topics/sample.yaml".to_string(),
                payload: serde_json::json!({
                    "content": "name: sample\nenabled: true\nactions: \"\"\ncontent: \"local\"\nexample_queries: []\n"
                }),
            },
        );
        let projection = serde_json::json!({
            "knowledgeBase": {
                "topics": {
                    "entities": {
                        "topic-1": {
                            "name": "sample",
                            "isActive": true,
                            "actions": "",
                            "content": "remote",
                            "exampleQueries": []
                        }
                    }
                }
            }
        });

        let (commands, last_known_sequence) = client
            .build_push_commands_with_options(
                &resources,
                Some(&projection),
                Some("reviewer@example.com"),
            )
            .expect("build commands");

        assert_eq!(last_known_sequence, 0);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].r#type, "update_topic");
        assert_eq!(
            commands[0].metadata.as_ref().map(|m| m.created_by.as_str()),
            Some("reviewer@example.com")
        );
    }

    #[test]
    fn push_no_changes_uses_python_failure_contract() {
        let client = HttpPlatformClient {
            client: reqwest::blocking::Client::new(),
            base_url: "http://localhost".to_string(),
            api_key: "test-key".to_string(),
            account_id: "test-account".to_string(),
            project_id: "test-project".to_string(),
            branch_id: "main".to_string(),
        };
        let resources = ResourceMap::new();
        let projection = serde_json::json!({});

        let result = client
            .push_resources_with_options(&resources, Some(&projection), None)
            .expect("push result");

        assert!(!result.success);
        assert_eq!(result.message, "No changes detected");
        assert!(result.commands.is_empty());
    }

    #[test]
    fn builds_delete_topic_command_when_local_removed() {
        let resources = ResourceMap::new();
        let projection = serde_json::json!({
            "knowledgeBase": {
                "topics": {
                    "entities": {
                        "topic-1": {
                            "name": "sample",
                            "actions": "",
                            "content": "hello"
                        }
                    }
                }
            }
        });
        let commands = build_phase1_commands(&resources, &projection);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].r#type, "delete_topic");
        assert!(matches!(
            commands[0].payload,
            Some(CommandPayload::DeleteTopic(_))
        ));
    }

    #[test]
    fn update_function_uses_remote_metadata_when_available() {
        let mut resources = ResourceMap::new();
        resources.insert(
            "functions/test.py".to_string(),
            Resource {
                resource_id: "local".to_string(),
                name: "test".to_string(),
                file_path: "functions/test.py".to_string(),
                payload: serde_json::json!({
                    "content": "def test(conv):\n    return 'ok'\n"
                }),
            },
        );
        let projection = serde_json::json!({
            "functions": {
                "functions": {
                    "entities": {
                        "fn-1": {
                            "name": "test",
                            "description": "Remote description",
                            "parameters": [{"id": "p1", "name": "customer", "description": "Customer id", "type": "string"}],
                            "errors": [{"lineno": 2, "message": "bad", "text": "raise"}],
                            "archived": true
                        }
                    }
                }
            }
        });
        let commands = build_phase1_commands(&resources, &projection);
        let update = commands
            .iter()
            .find(|c| c.r#type == "update_function")
            .expect("update function command");
        match &update.payload {
            Some(CommandPayload::UpdateFunction(msg)) => {
                assert_eq!(msg.description.as_deref(), Some("Remote description"));
                assert!(
                    msg.parameters
                        .as_ref()
                        .is_some_and(|p| !p.parameters.is_empty())
                );
                assert!(msg.errors.as_ref().is_some_and(|e| !e.errors.is_empty()));
                assert_eq!(msg.archived, Some(true));
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn create_function_infers_description_and_parameters_from_code() {
        let mut resources = ResourceMap::new();
        resources.insert(
            "functions/new_func.py".to_string(),
            Resource {
                resource_id: "local".to_string(),
                name: "new_func".to_string(),
                file_path: "functions/new_func.py".to_string(),
                payload: serde_json::json!({
                    "content": "def new_func(name, age=0):\n    \"\"\"Create greeting.\"\"\"\n    return f'Hi {name}'\n"
                }),
            },
        );
        let commands = build_phase1_commands(&resources, &serde_json::json!({}));
        let create = commands
            .iter()
            .find(|c| c.r#type == "create_function")
            .expect("create function command");
        match &create.payload {
            Some(CommandPayload::CreateFunction(msg)) => {
                assert_eq!(msg.description, "Create greeting.");
                assert_eq!(msg.parameters.len(), 2);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn phase1_plus_extended_appends_variable_commands() {
        let mut resources = ResourceMap::new();
        resources.insert(
            "topics/sample.yaml".to_string(),
            Resource {
                resource_id: "local".to_string(),
                name: "sample".to_string(),
                file_path: "topics/sample.yaml".to_string(),
                payload: serde_json::json!({
                    "content": "name: sample\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
                }),
            },
        );
        resources.insert(
            "variables/MyVar".to_string(),
            Resource {
                resource_id: "local".to_string(),
                name: "MyVar".to_string(),
                file_path: "variables/MyVar".to_string(),
                payload: serde_json::json!({ "content": "" }),
            },
        );
        let projection = serde_json::json!({});
        let commands = build_phase1_commands(&resources, &projection);
        let types: Vec<&str> = commands.iter().map(|c| c.r#type.as_str()).collect();
        assert!(types.contains(&"create_topic"));
        assert!(types.contains(&"variable_create"));
    }

    #[test]
    fn phase1_and_extended_follow_global_delete_create_update_order() {
        let mut resources = ResourceMap::new();
        resources.insert(
            "topics/new.yaml".to_string(),
            Resource {
                resource_id: "local".to_string(),
                name: "new".to_string(),
                file_path: "topics/new.yaml".to_string(),
                payload: serde_json::json!({
                    "content": "name: new\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
                }),
            },
        );
        resources.insert(
            "topics/create_only.yaml".to_string(),
            Resource {
                resource_id: "local".to_string(),
                name: "create_only".to_string(),
                file_path: "topics/create_only.yaml".to_string(),
                payload: serde_json::json!({
                    "content": "name: create_only\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
                }),
            },
        );
        resources.insert(
            "variables/NewVar".to_string(),
            Resource {
                resource_id: "local".to_string(),
                name: "NewVar".to_string(),
                file_path: "variables/NewVar".to_string(),
                payload: serde_json::json!({"content": ""}),
            },
        );
        resources.insert(
            "variables/FreshVar".to_string(),
            Resource {
                resource_id: "local".to_string(),
                name: "FreshVar".to_string(),
                file_path: "variables/FreshVar".to_string(),
                payload: serde_json::json!({"content": "{\"name\":\"FreshVar\"}"}),
            },
        );
        let projection = serde_json::json!({
            "knowledgeBase": {"topics": {"entities": {"topic-old": {"name": "old"}}}},
            "variables": {"variables": {"entities": {"vrbl-old": {"name": "OldVar"}}}}
        });
        let commands = build_phase1_commands(&resources, &projection);
        let types: Vec<&str> = commands.iter().map(|c| c.r#type.as_str()).collect();
        let delete_topic_idx = types
            .iter()
            .position(|t| *t == "delete_topic")
            .expect("delete_topic");
        let variable_delete_idx = types
            .iter()
            .position(|t| *t == "variable_delete")
            .expect("variable_delete");
        let create_topic_idx = types
            .iter()
            .position(|t| *t == "create_topic")
            .expect("create_topic");
        let variable_create_idx = types
            .iter()
            .position(|t| *t == "variable_create")
            .expect("variable_create");
        assert!(delete_topic_idx < create_topic_idx);
        assert!(variable_delete_idx < variable_create_idx);
        assert!(delete_topic_idx < variable_create_idx);
    }

    #[test]
    fn queue_prioritizes_variable_commands_across_all_phases() {
        let mut resources = ResourceMap::new();
        resources.insert(
            "topics/new.yaml".to_string(),
            Resource {
                resource_id: "local".to_string(),
                name: "new".to_string(),
                file_path: "topics/new.yaml".to_string(),
                payload: serde_json::json!({
                    "content": "name: new\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n"
                }),
            },
        );
        resources.insert(
            "variables/NewVar".to_string(),
            Resource {
                resource_id: "local".to_string(),
                name: "NewVar".to_string(),
                file_path: "variables/NewVar".to_string(),
                payload: serde_json::json!({"content": "{\"name\":\"NewVar\"}"}),
            },
        );
        let projection = serde_json::json!({
            "knowledgeBase": {"topics": {"entities": {"topic-old": {"name": "old"}, "topic-new": {"name": "new"}}}},
            "variables": {"variables": {"entities": {"vrbl-old": {"name": "OldVar"}, "vrbl-keep": {"name": "NewVar"}}}}
        });
        let commands = build_phase1_commands(&resources, &projection);
        let types: Vec<&str> = commands.iter().map(|c| c.r#type.as_str()).collect();
        let variable_delete_idx = types
            .iter()
            .position(|t| *t == "variable_delete")
            .expect("variable_delete");
        let topic_delete_idx = types
            .iter()
            .position(|t| *t == "delete_topic")
            .expect("delete_topic");
        let variable_update_idx = types
            .iter()
            .position(|t| *t == "variable_update")
            .expect("variable_update");
        let topic_update_idx = types
            .iter()
            .position(|t| *t == "update_topic")
            .expect("update_topic");
        assert!(variable_delete_idx < topic_delete_idx);
        assert!(variable_update_idx < topic_update_idx);
    }

    #[test]
    fn projection_to_resource_map_includes_extended_resource_files() {
        let projection = serde_json::json!({
            "variables": {"variables": {"entities": {"vrbl-1": {"name": "MyVar"}}}},
            "entities": {"entities": {"entities": {"ent-1": {"name": "Age", "description": "age", "type": "numeric", "numberConfig": {"min": 1, "max": 120}}}}},
            "handoff": {"handoffs": {"entities": {"ho-1": {"name": "Sales", "description": "to sales", "active": true, "isDefault": true, "sipConfig": {"invite": {"phoneNumber": "+1555", "outboundEndpoint": "trunk", "outboundEncryption": "tls"}}, "sipHeaders": {"headers": [{"key": "X-Test", "value": "1"}]}}}}},
            "sms": {"templates": {"entities": {"twilio_sms-1": {"name": "Welcome", "text": "hi", "active": true, "envPhoneNumbers": {"sandbox": "+1", "preRelease": "+2", "live": "+3"}}}}},
            "stopKeywords": {"filters": {"entities": {"sk-1": {"title": "HangUp", "description": "end", "regularExpressions": ["^bye$"], "sayPhrase": false, "languageCode": "en-US"}}}},
            "experimentalConfig": {"experimentalConfigs": {"entities": {"default": {"features": {"foo": true}}}}}
        });
        let map = projection_to_resource_map(&projection).expect("map");
        assert!(map.contains_key("variables/MyVar"));
        assert!(map.contains_key("config/entities.yaml"));
        assert!(map.contains_key("config/handoffs.yaml"));
        assert!(map.contains_key("config/sms_templates.yaml"));
        assert!(map.contains_key("voice/response_control/phrase_filtering.yaml"));
        assert!(map.contains_key("agent_settings/experimental_config.json"));
        let variable_content = map
            .get("variables/MyVar")
            .and_then(|r| r.payload.get("content"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(variable_content.contains("\"name\": \"MyVar\""));
        let entities_content = map
            .get("config/entities.yaml")
            .and_then(|r| r.payload.get("content"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(entities_content.contains("min: 1"));
        assert!(entities_content.contains("max: 120"));
        let handoff_content = map
            .get("config/handoffs.yaml")
            .and_then(|r| r.payload.get("content"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(handoff_content.contains("method: invite"));
        assert!(handoff_content.contains("phone_number: '+1555'"));
        assert!(handoff_content.contains("key: X-Test"));
    }

    #[test]
    fn extract_response_commands_reads_common_response_shapes() {
        let direct = serde_json::json!({
            "commands": [{"type": "create_topic"}]
        });
        assert_eq!(extract_response_commands(&direct).len(), 1);

        let nested_batch = serde_json::json!({
            "commandBatch": {"commands": [{"type": "delete_topic"}]}
        });
        assert_eq!(extract_response_commands(&nested_batch).len(), 1);

        let nested_result = serde_json::json!({
            "result": {"commands": [{"type": "update_topic"}]}
        });
        assert_eq!(extract_response_commands(&nested_result).len(), 1);
    }
}
