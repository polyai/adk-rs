use adk_domain::{
    BranchDescriptor, BranchMergeResult, DeploymentList, PushResult, Resource, ResourceMap,
};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::entities::{self, EntityCreate, EntityDelete, EntityUpdate};
use adk_protobuf::functions::{
    FunctionCreateFunction, FunctionDeleteFunction, FunctionUpdateFunction,
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
    #[error("not implemented")]
    NotImplemented,
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
    fn push_resources(&self, _resources: &ResourceMap) -> Result<PushResult, ApiError>;
    fn list_deployments(&self, _environment: &str) -> Result<DeploymentList, ApiError>;
    fn create_chat_session(&self, _payload: Value) -> Result<Value, ApiError>;
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
}

impl Default for InMemoryPlatformClient {
    fn default() -> Self {
        Self {
            resources: Arc::new(Mutex::new(ResourceMap::new())),
            branches: Arc::new(Mutex::new(default_branches())),
        }
    }
}

impl InMemoryPlatformClient {
    pub fn with_resources(resources: ResourceMap) -> Self {
        Self {
            resources: Arc::new(Mutex::new(resources)),
            branches: Arc::new(Mutex::new(default_branches())),
        }
    }
}

fn default_branches() -> indexmap::IndexMap<String, String> {
    let mut branches = indexmap::IndexMap::new();
    branches.insert("main".to_string(), "main".to_string());
    branches
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
        let endpoint = format!(
            "/accounts/{}/projects/{}/branches/{}/projection",
            self.account_id, self.project_id, self.branch_id
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
}

impl PlatformClient for HttpPlatformClient {
    fn pull_resources(&self) -> Result<ResourceMap, ApiError> {
        let response = self.fetch_projection_response()?;
        let projection = response
            .get("projection")
            .cloned()
            .unwrap_or_else(|| response.clone());
        projection_to_resource_map(&projection)
    }

    fn push_resources(&self, resources: &ResourceMap) -> Result<PushResult, ApiError> {
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

        let commands = build_phase1_commands(resources, &projection);
        if commands.is_empty() {
            return Ok(PushResult {
                success: true,
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
        let _ = self.request_binary_json(&endpoint, &bytes)?;
        Ok(PushResult {
            success: true,
            message: "Push accepted by platform endpoint (protobuf command-batch)".to_string(),
            commands: vec![],
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
        let endpoint = format!(
            "/accounts/{}/projects/{}/chat",
            self.account_id, self.project_id
        );
        self.request_json(reqwest::Method::POST, &endpoint, None, Some(payload))
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

fn projection_to_resource_map(projection: &Value) -> Result<ResourceMap, ApiError> {
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
            "config": {},
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
                payload: serde_json::json!({ "content": "" }),
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
            "sip_config": {
                "method": "bye"
            },
            "sip_headers": []
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
/// implementation groups commands in that broad shape for the original phase-1 types only; extra
/// resource families are appended afterward and **do not yet** mirror every Python priority edge
/// case; tighten ordering when parity tests require it.
fn build_phase1_commands(resources: &ResourceMap, projection: &Value) -> Vec<Command> {
    let metadata = command_metadata();
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
                id,
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
            let id = remote_functions
                .get(&name)
                .cloned()
                .unwrap_or_else(|| format!("function-{}", clean_name(&name).to_lowercase()));
            if remote_functions.contains_key(&name) {
                push_command(
                    &mut function_update,
                    &metadata,
                    "update_function",
                    CommandPayload::UpdateFunction(FunctionUpdateFunction {
                        id: id.clone(),
                        name: Some(name.clone()),
                        description: Some(String::new()),
                        parameters: None,
                        code: Some(content.to_string()),
                        errors: None,
                        references: None,
                        archived: Some(false),
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
                        description: String::new(),
                        parameters: vec![],
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
    for (name, id) in remote_functions {
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
    let out: Vec<Command> = entity_del
        .into_iter()
        .chain(function_del)
        .chain(topic_del)
        .chain(ext_groups.deletes)
        .chain(entity_create)
        .chain(function_create)
        .chain(topic_create)
        .chain(ext_groups.creates)
        .chain(entity_update)
        .chain(function_update)
        .chain(topic_update)
        .chain(ext_groups.updates)
        .chain(ext_groups.post_updates)
        .collect();
    out
}

fn command_metadata() -> Option<Metadata> {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Some(Metadata {
        created_at: Some(prost_types::Timestamp {
            seconds: dur.as_secs() as i64,
            nanos: dur.subsec_nanos() as i32,
        }),
        created_by: env::var("POLY_ADK_USER").unwrap_or_else(|_| "sdk-user".to_string()),
    })
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
            "variables/NewVar".to_string(),
            Resource {
                resource_id: "local".to_string(),
                name: "NewVar".to_string(),
                file_path: "variables/NewVar".to_string(),
                payload: serde_json::json!({"content": ""}),
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
    fn projection_to_resource_map_includes_extended_resource_files() {
        let projection = serde_json::json!({
            "variables": {"variables": {"entities": {"vrbl-1": {"name": "MyVar"}}}},
            "handoff": {"handoffs": {"entities": {"ho-1": {"name": "Sales", "description": "to sales", "active": true, "isDefault": true}}}},
            "sms": {"templates": {"entities": {"twilio_sms-1": {"name": "Welcome", "text": "hi", "active": true, "envPhoneNumbers": {"sandbox": "+1", "preRelease": "+2", "live": "+3"}}}}},
            "stopKeywords": {"filters": {"entities": {"sk-1": {"title": "HangUp", "description": "end", "regularExpressions": ["^bye$"], "sayPhrase": false, "languageCode": "en-US"}}}},
            "experimentalConfig": {"experimentalConfigs": {"entities": {"default": {"features": {"foo": true}}}}}
        });
        let map = projection_to_resource_map(&projection).expect("map");
        assert!(map.contains_key("variables/MyVar"));
        assert!(map.contains_key("config/handoffs.yaml"));
        assert!(map.contains_key("config/sms_templates.yaml"));
        assert!(map.contains_key("voice/response_control/phrase_filtering.yaml"));
        assert!(map.contains_key("agent_settings/experimental_config.json"));
    }
}
