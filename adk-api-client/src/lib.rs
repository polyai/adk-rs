use adk_protobuf::{Command, CommandBatch};
use adk_resources::{
    CommandGenError, command_to_json_summary, projection_to_resource_map,
    try_build_push_commands_for_changed_resources, try_build_push_commands_with_created_by,
};
use adk_types::{
    BranchDescriptor, BranchMergeResult, ConversationDetail, ConversationListResponse,
    DeploymentList, PushResult, ResourceMap,
};
use prost::Message;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionSnapshot {
    pub projection: Value,
    pub last_known_sequence: u64,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("http error: {0}")]
    Http(String),
    #[error(
        "{status_code} {error_kind}: {reason} for url: {url} (correlation ID: {correlation_id})"
    )]
    HttpStatus {
        status_code: u16,
        error_kind: String,
        reason: String,
        url: String,
        correlation_id: String,
    },
    #[error("missing required configuration: {0}")]
    MissingConfig(String),
}

mod auth;
mod in_memory;

pub use auth::{Auth0Client, Auth0DeviceCode, Auth0TokenPoll, JupiterClient, PersonalAccessToken};
pub use in_memory::InMemoryPlatformClient;

impl From<CommandGenError> for ApiError {
    fn from(error: CommandGenError) -> Self {
        Self::Http(error.to_string())
    }
}

/// Platform API boundary used by `adk-service`.
///
/// NOTE:
/// - `HttpPlatformClient` is the real networked implementation.
/// - `InMemoryPlatformClient` is a deterministic test double for local/unit tests.
pub trait PlatformClient: Send + Sync {
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

    fn pull_resources(&self) -> Result<ResourceMap, ApiError>;
    fn pull_projection_json_by_name(&self, name: &str) -> Result<Value, ApiError> {
        let resources = self.pull_resources_by_name(name)?;
        serde_json::to_value(resources).map_err(|e| ApiError::Http(e.to_string()))
    }
    fn pull_resources_by_name(&self, name: &str) -> Result<ResourceMap, ApiError> {
        let _ = name;
        self.pull_resources()
    }
    fn push_baseline_resources(&self, projection: Option<&Value>) -> Result<ResourceMap, ApiError> {
        if let Some(projection) = projection {
            projection_to_resource_map(projection).map_err(ApiError::from)
        } else {
            self.pull_resources()
        }
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
    ) -> Result<PushResult, ApiError> {
        let _ = projection;
        self.preview_push_resources(resources)
    }
    fn preview_push_changed_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
    ) -> Result<PushResult, ApiError> {
        self.preview_push_resources_with_options(resources, projection)
    }
    fn push_resources(&self, _resources: &ResourceMap) -> Result<PushResult, ApiError>;
    fn push_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
    ) -> Result<PushResult, ApiError> {
        let _ = projection;
        self.push_resources(resources)
    }
    fn push_changed_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
    ) -> Result<PushResult, ApiError> {
        self.push_resources_with_options(resources, projection)
    }
    fn command_user_override(&self) -> Option<String> {
        None
    }
    fn push_command_batch(&self, _command_batch_bytes: &[u8]) -> Result<PushResult, ApiError> {
        Err(ApiError::Http(
            "command-batch push is not implemented for this platform client".to_string(),
        ))
    }
    fn push_command_batch_to_branch(
        &self,
        branch_id: &str,
        _command_batch_bytes: &[u8],
    ) -> Result<PushResult, ApiError> {
        Err(ApiError::Http(format!(
            "command-batch push to branch '{branch_id}' is not implemented for this platform client"
        )))
    }
    fn record_successful_push(&self, _resources: &ResourceMap) -> Result<(), ApiError> {
        Err(ApiError::Http(
            "successful push recording is not implemented for this platform client".to_string(),
        ))
    }
    fn create_branch_from_main(
        &self,
        branch_name: &str,
    ) -> Result<(String, ProjectionSnapshot), ApiError> {
        let snapshot = self.pull_projection_snapshot()?;
        let branch_id = self.create_branch(branch_name)?;
        Ok((branch_id, snapshot))
    }
    fn push_main_resources_to_new_branch(
        &self,
        branch_name: &str,
        resources: &ResourceMap,
    ) -> Result<(String, PushResult), ApiError> {
        let branch_id = self.create_branch(branch_name)?;
        let push = self.push_resources_with_options(resources, None)?;
        Ok((branch_id, push))
    }
    fn list_deployments(&self, _environment: &str) -> Result<DeploymentList, ApiError>;
    fn promote_deployment(
        &self,
        deployment_id: &str,
        target_env: &str,
        message: &str,
    ) -> Result<Value, ApiError>;
    fn rollback_deployment(&self, deployment_id: &str, message: &str) -> Result<Value, ApiError>;
    fn create_chat_session(&self, _payload: Value) -> Result<Value, ApiError>;
    fn send_chat_message(&self, _payload: Value) -> Result<Value, ApiError>;
    fn end_chat_session(&self, _payload: Value) -> Result<Value, ApiError>;
    fn list_conversations(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<ConversationListResponse, ApiError>;
    fn get_conversation(&self, conversation_id: &str) -> Result<ConversationDetail, ApiError>;
    fn get_conversation_audio(
        &self,
        conversation_id: &str,
        direction: &str,
        redacted: bool,
    ) -> Result<Vec<u8>, ApiError>;
    fn list_branches(&self) -> Result<Vec<BranchDescriptor>, ApiError>;
    fn create_branch(&self, branch_name: &str) -> Result<String, ApiError>;
    fn delete_branch(&self, branch_id: &str) -> Result<(), ApiError>;
    fn merge_branch(
        &self,
        deployment_message: &str,
        conflict_resolutions: Option<Vec<Value>>,
    ) -> Result<BranchMergeResult, ApiError>;
}

#[derive(Debug, Clone)]
pub struct HttpPlatformClient {
    client: reqwest::blocking::Client,
    base_url: String,
    api_key: String,
    account_id: String,
    project_id: String,
    branch_id: String,
    command_user_override: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct AccountApiResponse {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    active: bool,
}

#[derive(Debug, Deserialize)]
struct ProjectsResponse {
    #[serde(default)]
    projects: Vec<ProjectApiResponse>,
}

#[derive(Debug, Deserialize)]
struct ProjectApiResponse {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateProjectRequest<'a> {
    name: &'a str,
    response_settings: CreateProjectResponseSettings<'a>,
    voice_settings: CreateProjectVoiceSettings<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_id: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct CreateProjectResponseSettings<'a> {
    greeting: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateProjectVoiceSettings<'a> {
    voice_id: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatedProjectResponse {
    #[serde(default)]
    agent_id: String,
    #[serde(default)]
    agent_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSummary {
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub agent_name: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub branch_count: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateProjectRequest<'a> {
    new_agent_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_agent_id: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BranchSequenceResponse {
    #[serde(default, deserialize_with = "deserialize_optional_u64")]
    last_known_sequence: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepareBranchChatRequest {
    expected_branch_last_known_sequence: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrepareBranchChatResponse {
    #[serde(default)]
    artifact_version: Option<String>,
    #[serde(default)]
    lambda_deployment_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BranchesResponse {
    #[serde(default)]
    branches: Vec<BranchResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BranchResponse {
    #[serde(default)]
    branch_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateBranchRequest<'a> {
    expected_main_last_known_sequence: u64,
    branch_name: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBranchResponse {
    branch_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteBranchRequest {
    expected_branch_last_known_sequence: u64,
}

#[derive(Debug, Deserialize)]
struct DeploymentsResponse {
    #[serde(default)]
    deployments: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct DeploymentVersionLookup {
    #[serde(default, alias = "deployment_id", alias = "deploymentId")]
    id: Option<String>,
    #[serde(default, alias = "version_hash", alias = "versionHash", alias = "hash")]
    hash: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ActiveDeploymentValue {
    Hash(String),
    Object(ActiveDeploymentObject),
}

#[derive(Debug, Deserialize)]
struct ActiveDeploymentObject {
    #[serde(default, alias = "deployment_id", alias = "deploymentId")]
    id: Option<String>,
    #[serde(
        default,
        alias = "version",
        alias = "version_hash",
        alias = "versionHash",
        alias = "hash"
    )]
    hash: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PromoteDeploymentRequest<'a> {
    target_environment: &'a str,
    deployment_message: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RollbackDeploymentRequest<'a> {
    deployment_message: &'a str,
}

#[derive(Debug, Deserialize, Default)]
struct ChatSessionInput {
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    variant: Option<String>,
    #[serde(default)]
    input_lang: Option<String>,
    #[serde(default)]
    output_lang: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateChatSessionRequest<'a> {
    channel: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    variant_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    asr_lang_code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tts_lang_code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_env: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    artifact_version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lambda_deployment_version: Option<&'a str>,
}

#[derive(Debug, Deserialize, Default)]
struct ChatMessageInput {
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    input_lang: Option<String>,
    #[serde(default)]
    output_lang: Option<String>,
}

#[derive(Debug, Serialize)]
struct SendChatMessageRequest<'a> {
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_env: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    asr_lang_code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tts_lang_code: Option<&'a str>,
}

#[derive(Debug, Deserialize, Default)]
struct EndChatSessionInput {
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    environment: Option<String>,
}

#[derive(Debug, Serialize)]
struct EndChatSessionRequest<'a> {
    client_env: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeBranchRequest<'a> {
    expected_branch_last_known_sequence: u64,
    deployment_message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    conflict_resolutions: Option<Vec<Value>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MergeBranchResponse {
    #[serde(default)]
    has_conflicts: bool,
    #[serde(default)]
    conflicts: Vec<Value>,
    #[serde(default)]
    errors: Vec<Value>,
    #[serde(default)]
    sequence: Option<String>,
}

impl HttpPlatformClient {
    pub fn new(
        region: &str,
        account_id: &str,
        project_id: &str,
        branch_id: Option<&str>,
    ) -> Result<Self, ApiError> {
        let api_key = api_key_for_region(region)?;
        Self::new_with_api_key(region, account_id, project_id, branch_id, api_key)
    }

    pub fn new_with_api_key(
        region: &str,
        account_id: &str,
        project_id: &str,
        branch_id: Option<&str>,
        api_key: impl Into<String>,
    ) -> Result<Self, ApiError> {
        let base_url = base_url_for_region(region)?;
        Ok(Self {
            client: new_http_client()?,
            base_url,
            api_key: api_key.into(),
            account_id: account_id.to_string(),
            project_id: project_id.to_string(),
            branch_id: branch_id.unwrap_or("main").to_string(),
            command_user_override: command_user_override_from_env(),
        })
    }

    pub fn accessible_regions(regions: &[&str]) -> Vec<String> {
        regions
            .iter()
            .filter_map(|region| {
                Self::list_accounts(region)
                    .ok()
                    .filter(|accounts| !accounts.is_empty())
                    .map(|_| (*region).to_string())
            })
            .collect()
    }

    pub fn list_accounts(region: &str) -> Result<Vec<AccountSummary>, ApiError> {
        let api_key = api_key_for_region(region)?;
        Self::list_accounts_with_api_key(region, &api_key)
    }

    pub fn list_accounts_with_api_key(
        region: &str,
        api_key: &str,
    ) -> Result<Vec<AccountSummary>, ApiError> {
        let value = Self::request_region_json_with_api_key(region, "/accounts", api_key)?;
        let accounts: Vec<AccountApiResponse> = parse_json(value, "accounts response")?;
        Ok(accounts
            .into_iter()
            .filter(|account| account.active)
            .filter_map(|account| {
                Some(AccountSummary {
                    id: account.id?,
                    name: account.name?,
                })
            })
            .collect())
    }

    pub fn list_projects(region: &str, account_id: &str) -> Result<Vec<ProjectSummary>, ApiError> {
        let api_key = api_key_for_region(region)?;
        Self::list_projects_with_api_key(region, account_id, &api_key)
    }

    pub fn list_projects_with_api_key(
        region: &str,
        account_id: &str,
        api_key: &str,
    ) -> Result<Vec<ProjectSummary>, ApiError> {
        let endpoint = format!("/accounts/{account_id}/projects");
        let value = Self::request_region_json_with_api_key(region, &endpoint, api_key)?;
        let response: ProjectsResponse = parse_json(value, "projects response")?;
        Ok(response
            .projects
            .into_iter()
            .filter_map(|project| {
                let id = project.id?;
                let name = project.name?;
                if id.is_empty() || name.is_empty() {
                    return None;
                }
                Some(ProjectSummary { id, name })
            })
            .collect())
    }

    pub fn create_project(
        region: &str,
        account_id: &str,
        project_name: &str,
        project_id: Option<&str>,
        greeting: &str,
        voice_id: Option<&str>,
    ) -> Result<ProjectSummary, ApiError> {
        let api_key = api_key_for_region(region)?;
        Self::create_project_with_api_key(
            region,
            account_id,
            project_name,
            project_id,
            greeting,
            voice_id,
            &api_key,
        )
    }

    pub fn create_project_with_api_key(
        region: &str,
        account_id: &str,
        project_name: &str,
        project_id: Option<&str>,
        greeting: &str,
        voice_id: Option<&str>,
        api_key: &str,
    ) -> Result<ProjectSummary, ApiError> {
        let endpoint = format!("/v1/accounts/{account_id}/agents");
        let body = CreateProjectRequest {
            name: project_name,
            response_settings: CreateProjectResponseSettings { greeting },
            voice_settings: CreateProjectVoiceSettings {
                voice_id: voice_id.unwrap_or_else(|| default_voice_id(region)),
            },
            agent_id: project_id.filter(|value| !value.is_empty()),
        };

        let value = Self::request_region_platform_json_with_api_key(
            region,
            reqwest::Method::POST,
            &endpoint,
            serialize_json(&body)?,
            api_key,
        )?;
        let response: CreatedProjectResponse = parse_json(value, "create-project response")?;
        Ok(ProjectSummary {
            id: response.agent_id,
            name: response
                .agent_name
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| project_name.to_string()),
        })
    }

    pub fn list_agents_with_api_key(
        region: &str,
        account_id: &str,
        api_key: &str,
    ) -> Result<Vec<AgentSummary>, ApiError> {
        let endpoint = format!("/v1/accounts/{account_id}/agents");
        let base_url = base_url_for_region(region)?;
        let url = format!("{}{}", platform_root_url(&base_url), endpoint);
        let correlation_id = new_correlation_id();
        let response = new_http_client()?
            .get(&url)
            .header("X-API-KEY", api_key)
            .header("X-PolyAI-Correlation-Id", &correlation_id)
            .header("Content-Type", "application/json")
            .header("X-Poly-Source", "adk")
            .send()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url, &correlation_id));
        }
        let value: Value = response.json().map_err(|e| ApiError::Http(e.to_string()))?;
        let agents: Vec<AgentSummary> = if value.is_array() {
            parse_json(value, "agents response")?
        } else {
            let arr = value
                .get("agents")
                .cloned()
                .unwrap_or(Value::Array(vec![]));
            parse_json(arr, "agents response")?
        };
        Ok(agents
            .into_iter()
            .filter(|a| !a.agent_id.is_empty())
            .collect())
    }

    pub fn delete_project_with_api_key(
        region: &str,
        agent_id: &str,
        api_key: &str,
    ) -> Result<(), ApiError> {
        let endpoint = format!("/v1/agents/{agent_id}");
        let base_url = base_url_for_region(region)?;
        let url = format!("{}{}", platform_root_url(&base_url), endpoint);
        let correlation_id = new_correlation_id();
        let response = new_http_client()?
            .delete(&url)
            .header("X-API-KEY", api_key)
            .header("X-PolyAI-Correlation-Id", &correlation_id)
            .header("Content-Type", "application/json")
            .header("X-Poly-Source", "adk")
            .send()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url, &correlation_id));
        }
        Ok(())
    }

    pub fn duplicate_project_with_api_key(
        region: &str,
        agent_id: &str,
        new_name: &str,
        new_id: Option<&str>,
        api_key: &str,
    ) -> Result<ProjectSummary, ApiError> {
        let endpoint = format!("/v1/agents/{agent_id}/duplicate");
        let body = DuplicateProjectRequest {
            new_agent_name: new_name,
            new_agent_id: new_id.filter(|v| !v.is_empty()),
        };
        let value = Self::request_region_platform_json_with_api_key(
            region,
            reqwest::Method::POST,
            &endpoint,
            serialize_json(&body)?,
            api_key,
        )?;
        let response: CreatedProjectResponse = parse_json(value, "duplicate-project response")?;
        Ok(ProjectSummary {
            id: response.agent_id,
            name: response
                .agent_name
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| new_name.to_string()),
        })
    }

    fn request_region_json_with_api_key(
        region: &str,
        endpoint: &str,
        api_key: &str,
    ) -> Result<Value, ApiError> {
        let base_url = base_url_for_region(region)?;
        let url = format!("{base_url}{endpoint}");
        let correlation_id = new_correlation_id();
        let response = new_http_client()?
            .get(&url)
            .header("X-API-KEY", api_key)
            .header("X-PolyAI-Correlation-Id", &correlation_id)
            .header("Content-Type", "application/json")
            .header("X-Poly-Source", "adk")
            .send()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url, &correlation_id));
        }
        response.json().map_err(|e| ApiError::Http(e.to_string()))
    }

    fn request_region_platform_json_with_api_key(
        region: &str,
        method: reqwest::Method,
        endpoint: &str,
        body: Value,
        api_key: &str,
    ) -> Result<Value, ApiError> {
        let base_url = base_url_for_region(region)?;
        let url = format!("{}{}", platform_root_url(&base_url), endpoint);
        let correlation_id = new_correlation_id();
        let response = new_http_client()?
            .request(method, &url)
            .header("X-API-KEY", api_key)
            .header("X-PolyAI-Correlation-Id", &correlation_id)
            .header("Content-Type", "application/json")
            .header("X-Poly-Source", "adk")
            .json(&body)
            .send()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url, &correlation_id));
        }
        response.json().map_err(|e| ApiError::Http(e.to_string()))
    }

    fn request_json(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        query: Option<&[(&str, &str)]>,
        body: Option<Value>,
    ) -> Result<Value, ApiError> {
        let url = format!("{}{}", self.base_url, endpoint);
        let correlation_id = new_correlation_id();
        let mut request = self
            .client
            .request(method, &url)
            .header("X-API-KEY", &self.api_key)
            .header("X-PolyAI-Correlation-Id", &correlation_id)
            .header("X-Poly-Source", "adk")
            .header("Content-Type", "application/json");
        request = self.with_command_user_override_header(request);
        if let Some(q) = query {
            request = request.query(q);
        }
        if let Some(json) = body {
            request = request.json(&json);
        }
        let response = request.send().map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url, &correlation_id));
        }
        response.json().map_err(|e| ApiError::Http(e.to_string()))
    }

    fn request_platform_json(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        payload: Option<Value>,
    ) -> Result<Value, ApiError> {
        self.request_platform_json_with_query(method, endpoint, None, payload)
    }

    fn request_platform_json_with_query(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        query: Option<&[(&str, String)]>,
        payload: Option<Value>,
    ) -> Result<Value, ApiError> {
        let url = format!("{}{}", platform_root_url(&self.base_url), endpoint);
        let correlation_id = new_correlation_id();
        let mut request = self
            .client
            .request(method, &url)
            .header("X-API-KEY", &self.api_key)
            .header("X-PolyAI-Correlation-Id", &correlation_id)
            .header("Content-Type", "application/json")
            .header("X-Poly-Source", "adk");
        request = self.with_command_user_override_header(request);
        if let Some(query) = query {
            request = request.query(query);
        }
        if let Some(payload) = payload {
            request = request.json(&payload);
        }
        let response = request.send().map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url, &correlation_id));
        }
        response.json().map_err(|e| ApiError::Http(e.to_string()))
    }

    fn request_platform_bytes(
        &self,
        endpoint: &str,
        query: Option<&[(&str, String)]>,
    ) -> Result<Vec<u8>, ApiError> {
        let url = format!("{}{}", platform_root_url(&self.base_url), endpoint);
        let correlation_id = new_correlation_id();
        let mut request = new_http_client()?
            .get(&url)
            .header("X-API-KEY", &self.api_key)
            .header("X-Poly-Source", "adk")
            .header("X-PolyAI-Correlation-Id", &correlation_id);
        request = self.with_command_user_override_header(request);
        if let Some(query) = query {
            request = request.query(query);
        }
        let response = request.send().map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url, &correlation_id));
        }
        response
            .bytes()
            .map(|bytes| bytes.to_vec())
            .map_err(|e| ApiError::Http(e.to_string()))
    }

    fn request_binary_json(&self, endpoint: &str, payload: &[u8]) -> Result<Value, ApiError> {
        let url = format!("{}{}", self.base_url, endpoint);
        let correlation_id = new_correlation_id();
        let request = self
            .client
            .post(&url)
            .header("X-API-KEY", &self.api_key)
            .header("X-PolyAI-Correlation-Id", &correlation_id)
            .header("Content-Type", "application/octet-stream")
            .header("X-Poly-Source", "adk");
        let response = self
            .with_command_user_override_header(request)
            .body(payload.to_vec())
            .send()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url, &correlation_id));
        }
        response.json().map_err(|e| ApiError::Http(e.to_string()))
    }

    fn with_command_user_override_header(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        if let Some(email) = &self.command_user_override {
            request.header("X-PolyAI-Email", email)
        } else {
            request
        }
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
        let response: BranchSequenceResponse = parse_json(payload, "branch-sequence response")?;
        Ok(response.last_known_sequence.unwrap_or(0))
    }

    fn prepare_branch_chat(&self) -> Result<PrepareBranchChatResponse, ApiError> {
        let sequence = self.fetch_branch_sequence(&self.branch_id)?;
        let endpoint = format!("{}/{}/chat", self.branches_endpoint(), self.branch_id);
        let request = PrepareBranchChatRequest {
            expected_branch_last_known_sequence: sequence,
        };
        let response = self.request_json(
            reqwest::Method::POST,
            &endpoint,
            None,
            Some(serialize_json(&request)?),
        )?;
        parse_json(response, "branch-chat response")
    }

    fn extract_projection(response: Value) -> Value {
        response.get("projection").cloned().unwrap_or(response)
    }

    fn projection_snapshot_from_response(response: Value) -> ProjectionSnapshot {
        let last_known_sequence = parse_last_known_sequence(&response);
        ProjectionSnapshot {
            projection: Self::extract_projection(response),
            last_known_sequence,
        }
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
        let response: DeploymentsResponse = parse_json(deployments, "deployments response")?;
        Ok(response.deployments)
    }

    fn deployment_id_from_active_env(&self, env_name: &str) -> Result<Option<String>, ApiError> {
        let active_endpoint = format!(
            "/accounts/{}/projects/{}/deployments/active",
            self.account_id, self.project_id
        );
        let active = self.request_json(reqwest::Method::GET, &active_endpoint, None, None)?;
        let active: indexmap::IndexMap<String, Option<ActiveDeploymentValue>> =
            parse_json(active, "active-deployments response")?;
        let Some(payload) = active.get(env_name).and_then(Option::as_ref) else {
            return Ok(None);
        };
        let (id, hash) = match payload {
            ActiveDeploymentValue::Hash(hash) => (None, Some(hash.as_str())),
            ActiveDeploymentValue::Object(payload) => {
                (payload.id.as_deref(), payload.hash.as_deref())
            }
        };
        if let Some(id) = id {
            return Ok(Some(id.to_string()));
        }
        if let Some(hash) = hash {
            return self.deployment_id_from_version_prefix(hash);
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
                let deployment: DeploymentVersionLookup =
                    parse_json(deployment, "deployment lookup response")?;
                let hash = deployment
                    .hash
                    .unwrap_or_default()
                    .chars()
                    .take(9)
                    .collect::<String>()
                    .to_lowercase();
                if hash == prefix
                    && let Some(id) = deployment.id
                {
                    return Ok(Some(id));
                }
            }
        }
        Ok(None)
    }
}

impl PlatformClient for HttpPlatformClient {
    fn pull_projection_json(&self) -> Result<Value, ApiError> {
        let response = self.fetch_projection_response()?;
        Ok(Self::extract_projection(response))
    }

    fn pull_projection_snapshot(&self) -> Result<ProjectionSnapshot, ApiError> {
        let response = self.fetch_projection_response()?;
        Ok(Self::projection_snapshot_from_response(response))
    }

    fn pull_projection_json_by_name(&self, name: &str) -> Result<Value, ApiError> {
        let env_names = ["sandbox", "pre-release", "live"];
        if env_names.contains(&name) {
            if let Some(deployment_id) = self.deployment_id_from_active_env(name)? {
                let response = self.fetch_projection_response_for_deployment(&deployment_id)?;
                return Ok(Self::extract_projection(response));
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
            return Ok(Self::extract_projection(response));
        }

        if let Some(deployment_id) = self.deployment_id_from_version_prefix(name)? {
            let response = self.fetch_projection_response_for_deployment(&deployment_id)?;
            return Ok(Self::extract_projection(response));
        }

        Err(ApiError::Http(format!(
            "Name '{name}' not found in environments, branches, or deployments"
        )))
    }

    fn pull_resources_by_name(&self, name: &str) -> Result<ResourceMap, ApiError> {
        let env_names = ["sandbox", "pre-release", "live"];
        if env_names.contains(&name) {
            if let Some(deployment_id) = self.deployment_id_from_active_env(name)? {
                let response = self.fetch_projection_response_for_deployment(&deployment_id)?;
                return Ok(projection_to_resource_map(&Self::extract_projection(
                    response,
                ))?);
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
            return Ok(projection_to_resource_map(&Self::extract_projection(
                response,
            ))?);
        }

        if let Some(deployment_id) = self.deployment_id_from_version_prefix(name)? {
            let response = self.fetch_projection_response_for_deployment(&deployment_id)?;
            return Ok(projection_to_resource_map(&Self::extract_projection(
                response,
            ))?);
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
        Ok(projection_to_resource_map(&projection)?)
    }

    fn push_resources(&self, resources: &ResourceMap) -> Result<PushResult, ApiError> {
        self.push_resources_with_options(resources, None)
    }

    fn push_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
    ) -> Result<PushResult, ApiError> {
        let (commands, last_known_sequence) =
            self.build_push_commands_with_options(resources, projection)?;
        if commands.is_empty() {
            return Ok(PushResult {
                success: false,
                message: "No changes detected".to_string(),
                commands: vec![],
            });
        }

        self.push_commands_to_branch(&self.branch_id, last_known_sequence, commands)
    }

    fn push_changed_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
    ) -> Result<PushResult, ApiError> {
        let (commands, last_known_sequence) =
            self.build_changed_push_commands_with_options(resources, projection)?;
        if commands.is_empty() {
            return Ok(PushResult {
                success: false,
                message: "No changes detected".to_string(),
                commands: vec![],
            });
        }

        self.push_commands_to_branch(&self.branch_id, last_known_sequence, commands)
    }

    fn command_user_override(&self) -> Option<String> {
        self.command_user_override.clone()
    }

    fn push_command_batch(&self, command_batch_bytes: &[u8]) -> Result<PushResult, ApiError> {
        self.push_command_batch_to_branch(&self.branch_id, command_batch_bytes)
    }

    fn push_command_batch_to_branch(
        &self,
        branch_id: &str,
        command_batch_bytes: &[u8],
    ) -> Result<PushResult, ApiError> {
        self.push_command_batch_bytes_to_branch(branch_id, command_batch_bytes)
    }

    fn record_successful_push(&self, _resources: &ResourceMap) -> Result<(), ApiError> {
        Ok(())
    }

    fn create_branch_from_main(
        &self,
        branch_name: &str,
    ) -> Result<(String, ProjectionSnapshot), ApiError> {
        let main_projection_response = self.fetch_projection_response_for_branch("main")?;
        let expected_main_last_known_sequence =
            parse_last_known_sequence(&main_projection_response);
        let snapshot = Self::projection_snapshot_from_response(main_projection_response);
        let response = self.request_json(
            reqwest::Method::POST,
            &self.branches_endpoint(),
            None,
            Some(serialize_json(&CreateBranchRequest {
                expected_main_last_known_sequence,
                branch_name,
            })?),
        )?;
        let response: CreateBranchResponse = parse_json(response, "create-branch response")?;
        let branch_id = response.branch_id;
        Ok((branch_id, snapshot))
    }

    fn push_main_resources_to_new_branch(
        &self,
        branch_name: &str,
        resources: &ResourceMap,
    ) -> Result<(String, PushResult), ApiError> {
        let main_projection_response = self.fetch_projection_response_for_branch("main")?;
        let expected_main_last_known_sequence =
            parse_last_known_sequence(&main_projection_response);
        let response = self.request_json(
            reqwest::Method::POST,
            &self.branches_endpoint(),
            None,
            Some(serialize_json(&CreateBranchRequest {
                expected_main_last_known_sequence,
                branch_name,
            })?),
        )?;
        let response: CreateBranchResponse = parse_json(response, "create-branch response")?;
        let branch_id = response.branch_id;
        let projection = main_projection_response
            .get("projection")
            .cloned()
            .unwrap_or_else(|| main_projection_response.clone());
        let commands = try_build_push_commands_with_created_by(
            resources,
            &projection,
            self.command_user_override.as_deref(),
        )?;
        let push = if commands.is_empty() {
            PushResult {
                success: false,
                message: "No changes detected".to_string(),
                commands: vec![],
            }
        } else {
            self.push_commands_to_branch(&branch_id, expected_main_last_known_sequence, commands)?
        };
        Ok((branch_id, push))
    }

    fn preview_push_resources(&self, resources: &ResourceMap) -> Result<PushResult, ApiError> {
        self.preview_push_resources_with_options(resources, None)
    }

    fn preview_push_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
    ) -> Result<PushResult, ApiError> {
        let (commands, _, _) =
            self.build_push_commands_and_projection_with_options(resources, projection)?;
        let summaries: Vec<_> = commands.iter().map(command_to_json_summary).collect();
        if summaries.is_empty() {
            return Ok(PushResult {
                success: false,
                message: "No changes detected".to_string(),
                commands: vec![],
            });
        }
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
        let (commands, _, _) =
            self.build_changed_push_commands_and_projection_with_options(resources, projection)?;
        let summaries: Vec<_> = commands.iter().map(command_to_json_summary).collect();
        if summaries.is_empty() {
            return Ok(PushResult {
                success: false,
                message: "No changes detected".to_string(),
                commands: vec![],
            });
        }
        Ok(PushResult {
            success: true,
            message: "Dry run completed. No changes were pushed.".to_string(),
            commands: summaries,
        })
    }

    fn list_deployments(&self, environment: &str) -> Result<DeploymentList, ApiError> {
        let endpoint = format!(
            "/accounts/{}/projects/{}/deployments",
            self.account_id, self.project_id
        );
        let query = [("client_env", environment)];
        let deployments = self.request_json(reqwest::Method::GET, &endpoint, Some(&query), None)?;
        let versions =
            parse_json::<DeploymentsResponse>(deployments, "deployments response")?.deployments;

        let active_endpoint = format!(
            "/accounts/{}/projects/{}/deployments/active",
            self.account_id, self.project_id
        );
        let active = self.request_json(reqwest::Method::GET, &active_endpoint, None, None)?;
        let active: indexmap::IndexMap<String, Option<ActiveDeploymentValue>> =
            parse_json(active, "active-deployments response")?;
        let mut active_hashes: indexmap::IndexMap<String, String> = Default::default();
        for (env_name, payload) in active {
            let hash = match payload {
                Some(ActiveDeploymentValue::Hash(hash)) => hash,
                Some(ActiveDeploymentValue::Object(payload)) => payload.hash.unwrap_or_default(),
                None => String::new(),
            };
            active_hashes.insert(env_name, hash);
        }

        Ok(DeploymentList {
            versions,
            active_deployment_hashes: active_hashes,
        })
    }

    fn promote_deployment(
        &self,
        deployment_id: &str,
        target_env: &str,
        message: &str,
    ) -> Result<Value, ApiError> {
        let endpoint = format!(
            "/v1/agents/{}/deployments/{deployment_id}/promote",
            self.project_id
        );
        self.request_platform_json(
            reqwest::Method::POST,
            &endpoint,
            Some(serialize_json(&PromoteDeploymentRequest {
                target_environment: target_env,
                deployment_message: message,
            })?),
        )
    }

    fn rollback_deployment(&self, deployment_id: &str, message: &str) -> Result<Value, ApiError> {
        let endpoint = format!(
            "/v1/agents/{}/deployments/{deployment_id}/rollback",
            self.project_id
        );
        self.request_platform_json(
            reqwest::Method::POST,
            &endpoint,
            Some(serialize_json(&RollbackDeploymentRequest {
                deployment_message: message,
            })?),
        )
    }

    fn create_chat_session(&self, payload: Value) -> Result<Value, ApiError> {
        let input: ChatSessionInput = parse_json(payload, "chat-session input")?;
        let environment = input.environment.as_deref().unwrap_or("sandbox");
        let channel = input.channel.as_deref().unwrap_or("chat.polyai");

        let (endpoint, artifact_version, lambda_deployment_version, client_env) =
            if environment == "draft" {
                let chat_info = self.prepare_branch_chat()?;
                let artifact_version = chat_info.artifact_version.ok_or_else(|| {
                    ApiError::Http("missing artifactVersion in branch chat response".to_string())
                })?;
                let lambda_deployment_version =
                    chat_info.lambda_deployment_version.ok_or_else(|| {
                        ApiError::Http(
                            "missing lambdaDeploymentVersion in branch chat response".to_string(),
                        )
                    })?;
                (
                    format!(
                        "/accounts/{}/projects/{}/draft/chat",
                        self.account_id, self.project_id
                    ),
                    Some(artifact_version),
                    Some(lambda_deployment_version),
                    None,
                )
            } else {
                (
                    format!(
                        "/accounts/{}/projects/{}/chat",
                        self.account_id, self.project_id
                    ),
                    None,
                    None,
                    Some(environment),
                )
            };
        let body = CreateChatSessionRequest {
            channel,
            variant_id: input.variant.as_deref(),
            asr_lang_code: input.input_lang.as_deref(),
            tts_lang_code: input.output_lang.as_deref(),
            client_env,
            artifact_version: artifact_version.as_deref(),
            lambda_deployment_version: lambda_deployment_version.as_deref(),
        };
        self.request_json(
            reqwest::Method::POST,
            &endpoint,
            None,
            Some(serialize_json(&body)?),
        )
    }

    fn send_chat_message(&self, payload: Value) -> Result<Value, ApiError> {
        let input: ChatMessageInput = parse_json(payload, "chat-message input")?;
        let conversation_id = input
            .conversation_id
            .as_deref()
            .ok_or_else(|| ApiError::MissingConfig("conversation_id".to_string()))?;
        let environment = input.environment.as_deref().unwrap_or("sandbox");
        let message = input.message.as_deref().unwrap_or_default();
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
        let body = SendChatMessageRequest {
            message,
            client_env: (environment != "draft").then_some(environment),
            asr_lang_code: input.input_lang.as_deref(),
            tts_lang_code: input.output_lang.as_deref(),
        };
        self.request_json(
            reqwest::Method::POST,
            &endpoint,
            None,
            Some(serialize_json(&body)?),
        )
    }

    fn end_chat_session(&self, payload: Value) -> Result<Value, ApiError> {
        let input: EndChatSessionInput = parse_json(payload, "end-chat-session input")?;
        let conversation_id = input
            .conversation_id
            .as_deref()
            .ok_or_else(|| ApiError::MissingConfig("conversation_id".to_string()))?;
        let environment = input.environment.as_deref().unwrap_or("sandbox");
        let endpoint = format!(
            "/accounts/{}/projects/{}/chat/{conversation_id}/end",
            self.account_id, self.project_id
        );
        self.request_json(
            reqwest::Method::POST,
            &endpoint,
            None,
            Some(serialize_json(&EndChatSessionRequest {
                client_env: environment,
            })?),
        )
    }

    fn list_conversations(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<ConversationListResponse, ApiError> {
        let endpoint = format!("/v1/agents/{}/conversations", self.project_id);
        let limit = limit.to_string();
        let offset = offset.to_string();
        let query = [("limit", limit), ("offset", offset)];
        let value = self.request_platform_json_with_query(
            reqwest::Method::GET,
            &endpoint,
            Some(&query),
            None,
        )?;
        serde_json::from_value(value).map_err(|e| ApiError::Http(e.to_string()))
    }

    fn get_conversation(&self, conversation_id: &str) -> Result<ConversationDetail, ApiError> {
        let endpoint = format!(
            "/v1/agents/{}/conversations/{conversation_id}",
            self.project_id
        );
        let value = self.request_platform_json(reqwest::Method::GET, &endpoint, None)?;
        serde_json::from_value(value).map_err(|e| ApiError::Http(e.to_string()))
    }

    fn get_conversation_audio(
        &self,
        conversation_id: &str,
        direction: &str,
        redacted: bool,
    ) -> Result<Vec<u8>, ApiError> {
        let endpoint = format!(
            "/v1/agents/{}/conversations/{conversation_id}/audio",
            self.project_id
        );
        let redacted = redacted.to_string();
        let query = [("direction", direction.to_string()), ("redacted", redacted)];
        self.request_platform_bytes(&endpoint, Some(&query))
    }

    fn list_branches(&self) -> Result<Vec<BranchDescriptor>, ApiError> {
        let payload =
            self.request_json(reqwest::Method::GET, &self.branches_endpoint(), None, None)?;
        let response: BranchesResponse = parse_json(payload, "branches response")?;
        let mut out = Vec::with_capacity(response.branches.len() + 1);
        out.push(BranchDescriptor {
            name: "main".to_string(),
            branch_id: "main".to_string(),
        });
        for branch in response.branches {
            let Some(branch_id) = branch.branch_id else {
                continue;
            };
            let name = branch.name.unwrap_or_else(|| branch_id.clone());
            if !out
                .iter()
                .any(|existing| existing.branch_id == branch_id.as_str())
            {
                out.push(BranchDescriptor { name, branch_id });
            }
        }
        Ok(out)
    }

    fn create_branch(&self, branch_name: &str) -> Result<String, ApiError> {
        let main_projection = self.fetch_projection_response_for_branch("main")?;
        let expected_main_last_known_sequence = parse_last_known_sequence(&main_projection);
        let response = self.request_json(
            reqwest::Method::POST,
            &self.branches_endpoint(),
            None,
            Some(serialize_json(&CreateBranchRequest {
                expected_main_last_known_sequence,
                branch_name,
            })?),
        )?;
        let response: CreateBranchResponse = parse_json(response, "create-branch response")?;
        Ok(response.branch_id)
    }

    fn delete_branch(&self, branch_id: &str) -> Result<(), ApiError> {
        let sequence = self.fetch_branch_sequence(branch_id)?;
        let endpoint = format!("{}/{branch_id}", self.branches_endpoint());
        let _ = self.request_json(
            reqwest::Method::DELETE,
            &endpoint,
            None,
            Some(serialize_json(&DeleteBranchRequest {
                expected_branch_last_known_sequence: sequence,
            })?),
        )?;
        Ok(())
    }

    fn merge_branch(
        &self,
        deployment_message: &str,
        conflict_resolutions: Option<Vec<Value>>,
    ) -> Result<BranchMergeResult, ApiError> {
        let expected_branch_last_known_sequence = self.fetch_branch_sequence(&self.branch_id)?;
        let payload = MergeBranchRequest {
            expected_branch_last_known_sequence,
            deployment_message,
            conflict_resolutions,
        };
        let endpoint = format!(
            "/accounts/{}/projects/{}/branches/{}/merge",
            self.account_id, self.project_id, self.branch_id
        );
        let url = format!("{}{}", self.base_url, endpoint);
        let correlation_id = new_correlation_id();
        let response = self
            .client
            .post(&url)
            .header("X-API-KEY", &self.api_key)
            .header("X-PolyAI-Correlation-Id", &correlation_id)
            .header("Content-Type", "application/json")
            .header("X-Poly-Source", "adk")
            .json(&payload)
            .send()
            .map_err(|e| ApiError::Http(e.to_string()))?;

        let status = response.status();
        let body: Value = response
            .json()
            .map_err(|e| ApiError::Http(format!("failed to parse merge response: {e}")))?;
        if status == reqwest::StatusCode::BAD_REQUEST {
            let merge_response: MergeBranchResponse =
                parse_json(body.clone(), "merge-branch conflict response")?;
            if merge_response.has_conflicts || !merge_response.conflicts.is_empty() {
                return Ok(BranchMergeResult {
                    success: false,
                    conflicts: merge_response.conflicts,
                    errors: merge_response.errors,
                    sequence: merge_response.sequence,
                });
            }
            return Err(ApiError::Http(format!(
                "status={status} body={body} (correlation ID: {correlation_id})"
            )));
        }
        if !status.is_success() {
            return Err(ApiError::Http(format!(
                "status={status} body={body} (correlation ID: {correlation_id})"
            )));
        }
        let merge_response: MergeBranchResponse = parse_json(body, "merge-branch response")?;
        Ok(BranchMergeResult {
            success: true,
            conflicts: vec![],
            errors: vec![],
            sequence: merge_response.sequence,
        })
    }
}

impl HttpPlatformClient {
    fn push_command_batch_bytes_to_branch(
        &self,
        branch_id: &str,
        command_batch_bytes: &[u8],
    ) -> Result<PushResult, ApiError> {
        let endpoint = format!(
            "/accounts/{}/projects/{}/branches/{branch_id}/command-batch",
            self.account_id, self.project_id
        );
        let response = self.request_binary_json(&endpoint, command_batch_bytes)?;
        let response_commands = extract_response_commands(&response);
        let response_message = response
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Resources pushed successfully.")
            .to_string();
        Ok(PushResult {
            success: true,
            message: response_message,
            commands: response_commands,
        })
    }

    fn push_commands_to_branch(
        &self,
        branch_id: &str,
        last_known_sequence: u64,
        commands: Vec<Command>,
    ) -> Result<PushResult, ApiError> {
        let batch = CommandBatch {
            last_known_sequence,
            commands,
        };
        let bytes = batch.encode_to_vec();
        self.push_command_batch_bytes_to_branch(branch_id, &bytes)
    }

    fn build_push_commands_with_options(
        &self,
        resources: &ResourceMap,
        projection_override: Option<&Value>,
    ) -> Result<(Vec<Command>, u64), ApiError> {
        let (commands, last_known_sequence, _) =
            self.build_push_commands_and_projection_with_options(resources, projection_override)?;
        Ok((commands, last_known_sequence))
    }

    fn build_changed_push_commands_with_options(
        &self,
        resources: &ResourceMap,
        projection_override: Option<&Value>,
    ) -> Result<(Vec<Command>, u64), ApiError> {
        let (commands, last_known_sequence, _) = self
            .build_changed_push_commands_and_projection_with_options(
                resources,
                projection_override,
            )?;
        Ok((commands, last_known_sequence))
    }

    fn build_push_commands_and_projection_with_options(
        &self,
        resources: &ResourceMap,
        projection_override: Option<&Value>,
    ) -> Result<(Vec<Command>, u64, Value), ApiError> {
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
        let commands = try_build_push_commands_with_created_by(
            resources,
            &projection,
            self.command_user_override.as_deref(),
        )?;
        Ok((commands, last_known_sequence, projection))
    }

    fn build_changed_push_commands_and_projection_with_options(
        &self,
        resources: &ResourceMap,
        projection_override: Option<&Value>,
    ) -> Result<(Vec<Command>, u64, Value), ApiError> {
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
        let commands = try_build_push_commands_for_changed_resources(
            resources,
            &projection,
            self.command_user_override.as_deref(),
        )?;
        Ok((commands, last_known_sequence, projection))
    }
}

fn command_user_override_from_env() -> Option<String> {
    env::var("ADK_COMMAND_USER_OVERRIDE")
        .ok()
        .filter(|value| !value.is_empty())
}

fn serialize_json<T: Serialize>(payload: &T) -> Result<Value, ApiError> {
    serde_json::to_value(payload).map_err(|e| ApiError::Http(e.to_string()))
}

fn parse_json<T: for<'de> Deserialize<'de>>(value: Value, context: &str) -> Result<T, ApiError> {
    serde_json::from_value(value)
        .map_err(|e| ApiError::Http(format!("failed to parse {context}: {e}")))
}

fn deserialize_optional_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| match value {
        Value::String(value) => value.parse::<u64>().ok(),
        Value::Number(value) => value.as_u64(),
        _ => None,
    }))
}

fn new_http_client() -> Result<reqwest::blocking::Client, ApiError> {
    reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("poly-adk-rs")
        .build()
        .map_err(|e| ApiError::Http(e.to_string()))
}

fn api_key_for_region(region: &str) -> Result<String, ApiError> {
    for name in api_key_env_names(region) {
        if let Ok(value) = env::var(name)
            && !value.trim().is_empty()
        {
            return Ok(value);
        }
    }
    Err(ApiError::MissingConfig(
        "POLY_ADK_KEY environment variable is not set. Export your API key with: export POLY_ADK_KEY=<your-api-key>"
            .to_string(),
    ))
}

fn api_key_env_names(region: &str) -> Vec<&'static str> {
    let mut names = Vec::new();
    match region {
        "us-1" => names.push("POLY_ADK_KEY_US"),
        "euw-1" => names.push("POLY_ADK_KEY_EUW"),
        "uk-1" => names.push("POLY_ADK_KEY_UK"),
        "studio" => names.push("POLY_ADK_KEY_STUDIO"),
        "staging" => names.push("POLY_ADK_KEY_STAGING"),
        "dev" => names.push("POLY_ADK_KEY_DEV"),
        _ => {}
    }
    names.push("POLY_ADK_KEY");
    names
}

fn base_url_for_region(region: &str) -> Result<String, ApiError> {
    for name in base_url_env_names(region) {
        if let Ok(value) = env::var(name)
            && !value.trim().is_empty()
        {
            return Ok(value.trim_end_matches('/').to_string());
        }
    }

    let base_url = match region {
        "dev" => "https://api.dev.poly.ai/adk/v1",
        "staging" => "https://api.staging.poly.ai/adk/v1",
        "euw-1" => "https://api.eu.poly.ai/adk/v1",
        "uk-1" => "https://api.uk.poly.ai/adk/v1",
        "us-1" => "https://api.us.poly.ai/adk/v1",
        "studio" => "https://api.studio.poly.ai/adk/v1",
        _ => return Err(ApiError::MissingConfig(format!("Unknown region: {region}"))),
    };
    Ok(base_url.to_string())
}

fn platform_root_url(adk_base_url: &str) -> &str {
    adk_base_url.strip_suffix("/adk/v1").unwrap_or(adk_base_url)
}

fn default_voice_id(region: &str) -> &'static str {
    match region {
        "us-1" => "VOICE-6fad73f6",
        "euw-1" => "VOICE-8b814724",
        "uk-1" => "VOICE-37966683",
        "studio" => "VOICE-071db756",
        "dev" | "staging" => "VOICE-e2b01d55",
        _ => "VOICE-afe2b8e8",
    }
}

fn new_correlation_id() -> String {
    format!("adk-{}", Uuid::new_v4())
}

pub(crate) fn http_status_error(
    status: reqwest::StatusCode,
    url: &str,
    correlation_id: &str,
) -> ApiError {
    ApiError::HttpStatus {
        status_code: status.as_u16(),
        error_kind: if status.is_server_error() {
            "Server Error"
        } else if status.is_client_error() {
            "Client Error"
        } else {
            "HTTP Error"
        }
        .to_string(),
        reason: status
            .canonical_reason()
            .unwrap_or_else(|| status.as_str())
            .to_string(),
        url: url.to_string(),
        correlation_id: correlation_id.to_string(),
    }
}

fn base_url_env_names(region: &str) -> Vec<&'static str> {
    let mut names = Vec::new();
    match region {
        "dev" => names.push("POLY_ADK_BASE_URL_DEV"),
        "staging" => names.push("POLY_ADK_BASE_URL_STAGING"),
        "euw-1" => {
            names.push("POLY_ADK_BASE_URL_EUW");
            names.push("POLY_ADK_BASE_URL_EUW_1");
        }
        "uk-1" => {
            names.push("POLY_ADK_BASE_URL_UK");
            names.push("POLY_ADK_BASE_URL_UK_1");
        }
        "us-1" => {
            names.push("POLY_ADK_BASE_URL_US");
            names.push("POLY_ADK_BASE_URL_US_1");
        }
        "studio" => names.push("POLY_ADK_BASE_URL_STUDIO"),
        _ => {}
    }
    names.push("POLY_ADK_BASE_URL");
    names
}

fn parse_last_known_sequence(value: &Value) -> u64 {
    value
        .get("lastKnownSequence")
        .and_then(|v| match v {
            Value::String(s) => s.parse::<u64>().ok(),
            Value::Number(n) => n.as_u64(),
            _ => None,
        })
        .unwrap_or(0)
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

#[cfg(test)]
mod tests;
