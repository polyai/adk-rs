use adk_protobuf::{Command, CommandBatch};
use adk_resources::{
    CommandGenError, build_phase1_commands_for_changed_resources, build_phase1_commands_with_actor,
    command_to_json_summary, projection_to_resource_map,
};
use adk_types::{BranchDescriptor, BranchMergeResult, DeploymentList, PushResult, ResourceMap};
use prost::Message;
use serde_json::Value;
use std::env;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("http error: {0}")]
    Http(String),
    #[error("{status_code} Client Error: {reason} for url: {url}")]
    HttpStatus {
        status_code: u16,
        reason: String,
        url: String,
    },
    #[error("missing required configuration: {0}")]
    MissingConfig(String),
}

mod in_memory;

pub use in_memory::InMemoryPlatformClient;

impl From<CommandGenError> for ApiError {
    fn from(error: CommandGenError) -> Self {
        Self::Http(error.to_string())
    }
}

/// Platform API boundary used by `adk-core`.
///
/// NOTE:
/// - `HttpPlatformClient` is the real networked implementation.
/// - `InMemoryPlatformClient` is a deterministic test double for local/unit tests.
pub trait PlatformClient: Send + Sync {
    fn pull_projection_json(&self) -> Result<Value, ApiError> {
        let resources = self.pull_resources()?;
        serde_json::to_value(resources).map_err(|e| ApiError::Http(e.to_string()))
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
        actor: Option<&str>,
    ) -> Result<PushResult, ApiError> {
        let _ = (projection, actor);
        self.preview_push_resources(resources)
    }
    fn preview_push_changed_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
        actor: Option<&str>,
    ) -> Result<PushResult, ApiError> {
        self.preview_push_resources_with_options(resources, projection, actor)
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
    fn push_changed_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
        actor: Option<&str>,
    ) -> Result<PushResult, ApiError> {
        self.push_resources_with_options(resources, projection, actor)
    }
    fn push_main_resources_to_new_branch(
        &self,
        branch_name: &str,
        resources: &ResourceMap,
        actor: Option<&str>,
    ) -> Result<(String, PushResult), ApiError> {
        let branch_id = self.create_branch(branch_name)?;
        let push = self.push_resources_with_options(resources, None, actor)?;
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

impl HttpPlatformClient {
    pub fn new(
        region: &str,
        account_id: &str,
        project_id: &str,
        branch_id: Option<&str>,
    ) -> Result<Self, ApiError> {
        let api_key = api_key_for_region(region)?;
        let base_url = base_url_for_region(region)?;
        Ok(Self {
            client: reqwest::blocking::Client::new(),
            base_url,
            api_key,
            account_id: account_id.to_string(),
            project_id: project_id.to_string(),
            branch_id: branch_id.unwrap_or("main").to_string(),
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
        let value = Self::request_region_json(region, "/accounts")?;
        let accounts = value
            .as_array()
            .ok_or_else(|| ApiError::Http("Expected a list of accounts".to_string()))?;
        Ok(accounts
            .iter()
            .filter(|account| {
                account
                    .get("active")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
            .filter_map(|account| {
                Some(AccountSummary {
                    id: account.get("id")?.as_str()?.to_string(),
                    name: account.get("name")?.as_str()?.to_string(),
                })
            })
            .collect())
    }

    pub fn list_projects(region: &str, account_id: &str) -> Result<Vec<ProjectSummary>, ApiError> {
        let endpoint = format!("/accounts/{account_id}/projects");
        let value = Self::request_region_json(region, &endpoint)?;
        let projects = value
            .get("projects")
            .and_then(Value::as_array)
            .ok_or_else(|| ApiError::Http("Expected a list of projects".to_string()))?;
        Ok(projects
            .iter()
            .filter_map(|project| {
                let id = project.get("id")?.as_str()?;
                let name = project.get("name")?.as_str()?;
                if id.is_empty() || name.is_empty() {
                    return None;
                }
                Some(ProjectSummary {
                    id: id.to_string(),
                    name: name.to_string(),
                })
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
        let endpoint = format!("/v1/accounts/{account_id}/agents");
        let mut body = serde_json::json!({
            "name": project_name,
            "responseSettings": {
                "greeting": greeting,
            },
            "voiceSettings": {
                "voiceId": voice_id.unwrap_or_else(|| default_voice_id(region)),
            },
        });
        if let Some(project_id) = project_id.filter(|value| !value.is_empty()) {
            body["agentId"] = Value::String(project_id.to_string());
        }

        let value =
            Self::request_region_platform_json(region, reqwest::Method::POST, &endpoint, body)?;
        Ok(ProjectSummary {
            id: value
                .get("agentId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            name: value
                .get("agentName")
                .and_then(Value::as_str)
                .unwrap_or(project_name)
                .to_string(),
        })
    }

    fn request_region_json(region: &str, endpoint: &str) -> Result<Value, ApiError> {
        let api_key = api_key_for_region(region)?;
        let base_url = base_url_for_region(region)?;
        let url = format!("{base_url}{endpoint}");
        let response = reqwest::blocking::Client::new()
            .get(&url)
            .header("X-API-KEY", api_key)
            .header("X-PolyAI-Correlation-Id", format!("adk-{}", Uuid::new_v4()))
            .header("Content-Type", "application/json")
            .send()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url));
        }
        response.json().map_err(|e| ApiError::Http(e.to_string()))
    }

    fn request_region_platform_json(
        region: &str,
        method: reqwest::Method,
        endpoint: &str,
        body: Value,
    ) -> Result<Value, ApiError> {
        let api_key = api_key_for_region(region)?;
        let base_url = base_url_for_region(region)?;
        let url = format!("{}{}", platform_root_url(&base_url), endpoint);
        let response = reqwest::blocking::Client::new()
            .request(method, &url)
            .header("X-API-KEY", api_key)
            .header("X-PolyAI-Correlation-Id", format!("adk-{}", Uuid::new_v4()))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url));
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
            return Err(http_status_error(status, &url));
        }
        response.json().map_err(|e| ApiError::Http(e.to_string()))
    }

    fn request_platform_json(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        payload: Option<Value>,
    ) -> Result<Value, ApiError> {
        let url = format!("{}{}", platform_root_url(&self.base_url), endpoint);
        let mut request = self
            .client
            .request(method, &url)
            .header("X-API-KEY", &self.api_key)
            .header("X-PolyAI-Correlation-Id", format!("adk-{}", Uuid::new_v4()))
            .header("Content-Type", "application/json");
        if let Some(payload) = payload {
            request = request.json(&payload);
        }
        let response = request.send().map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url));
        }
        response.json().map_err(|e| ApiError::Http(e.to_string()))
    }

    fn request_binary_json(&self, endpoint: &str, payload: &[u8]) -> Result<Value, ApiError> {
        let url = format!("{}{}", self.base_url, endpoint);
        let correlation_id = format!("adk-{}", Uuid::new_v4());
        let response = self
            .client
            .post(&url)
            .header("X-API-KEY", &self.api_key)
            .header("X-PolyAI-Correlation-Id", correlation_id)
            .header("Content-Type", "application/octet-stream")
            .body(payload.to_vec())
            .send()
            .map_err(|e| ApiError::Http(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url));
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
        Ok(parse_last_known_sequence(&payload))
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
                if hash == prefix
                    && let Some(id) = deployment
                        .get("id")
                        .or_else(|| deployment.get("deployment_id"))
                        .or_else(|| deployment.get("deploymentId"))
                        .and_then(Value::as_str)
                {
                    return Ok(Some(id.to_string()));
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

        self.push_commands_to_branch(&self.branch_id, last_known_sequence, commands)
    }

    fn push_changed_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
        actor: Option<&str>,
    ) -> Result<PushResult, ApiError> {
        let (commands, last_known_sequence) =
            self.build_changed_push_commands_with_options(resources, projection, actor)?;
        if commands.is_empty() {
            return Ok(PushResult {
                success: false,
                message: "No changes detected".to_string(),
                commands: vec![],
            });
        }

        self.push_commands_to_branch(&self.branch_id, last_known_sequence, commands)
    }

    fn push_main_resources_to_new_branch(
        &self,
        branch_name: &str,
        resources: &ResourceMap,
        actor: Option<&str>,
    ) -> Result<(String, PushResult), ApiError> {
        let main_projection_response = self.fetch_projection_response_for_branch("main")?;
        let expected_main_last_known_sequence =
            parse_last_known_sequence(&main_projection_response);
        let response = self.request_json(
            reqwest::Method::POST,
            &self.branches_endpoint(),
            None,
            Some(serde_json::json!({
                "expectedMainLastKnownSequence": expected_main_last_known_sequence,
                "branchName": branch_name,
            })),
        )?;
        let branch_id = response
            .get("branchId")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .ok_or_else(|| {
                ApiError::Http("missing branchId in create-branch response".to_string())
            })?;
        let projection = main_projection_response
            .get("projection")
            .cloned()
            .unwrap_or_else(|| main_projection_response.clone());
        let commands = build_phase1_commands_with_actor(resources, &projection, actor);
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
        self.preview_push_resources_with_options(resources, None, None)
    }

    fn preview_push_resources_with_options(
        &self,
        resources: &ResourceMap,
        projection: Option<&Value>,
        actor: Option<&str>,
    ) -> Result<PushResult, ApiError> {
        let (commands, _, _) =
            self.build_push_commands_and_projection_with_options(resources, projection, actor)?;
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
        actor: Option<&str>,
    ) -> Result<PushResult, ApiError> {
        let (commands, _, _) = self.build_changed_push_commands_and_projection_with_options(
            resources, projection, actor,
        )?;
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
                let hash = payload
                    .get("version")
                    .or_else(|| payload.get("version_hash"))
                    .or_else(|| payload.get("versionHash"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                active_hashes.insert(env_name.clone(), hash.to_string());
            }
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
            Some(serde_json::json!({
                "targetEnvironment": target_env,
                "deploymentMessage": message,
            })),
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
            Some(serde_json::json!({
                "deploymentMessage": message,
            })),
        )
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
        let main_projection = self.fetch_projection_response_for_branch("main")?;
        let expected_main_last_known_sequence = parse_last_known_sequence(&main_projection);
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
    fn push_commands_to_branch(
        &self,
        branch_id: &str,
        last_known_sequence: u64,
        commands: Vec<Command>,
    ) -> Result<PushResult, ApiError> {
        let endpoint = format!(
            "/accounts/{}/projects/{}/branches/{branch_id}/command-batch",
            self.account_id, self.project_id
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
            .unwrap_or("Resources pushed successfully.")
            .to_string();
        Ok(PushResult {
            success: true,
            message: response_message,
            commands: response_commands,
        })
    }

    fn build_push_commands_with_options(
        &self,
        resources: &ResourceMap,
        projection_override: Option<&Value>,
        actor: Option<&str>,
    ) -> Result<(Vec<Command>, u64), ApiError> {
        let (commands, last_known_sequence, _) = self
            .build_push_commands_and_projection_with_options(
                resources,
                projection_override,
                actor,
            )?;
        Ok((commands, last_known_sequence))
    }

    fn build_changed_push_commands_with_options(
        &self,
        resources: &ResourceMap,
        projection_override: Option<&Value>,
        actor: Option<&str>,
    ) -> Result<(Vec<Command>, u64), ApiError> {
        let (commands, last_known_sequence, _) = self
            .build_changed_push_commands_and_projection_with_options(
                resources,
                projection_override,
                actor,
            )?;
        Ok((commands, last_known_sequence))
    }

    fn build_push_commands_and_projection_with_options(
        &self,
        resources: &ResourceMap,
        projection_override: Option<&Value>,
        actor: Option<&str>,
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
        let commands = build_phase1_commands_with_actor(resources, &projection, actor);
        Ok((commands, last_known_sequence, projection))
    }

    fn build_changed_push_commands_and_projection_with_options(
        &self,
        resources: &ResourceMap,
        projection_override: Option<&Value>,
        actor: Option<&str>,
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
        let commands = build_phase1_commands_for_changed_resources(resources, &projection, actor);
        Ok((commands, last_known_sequence, projection))
    }
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
        "dev" | "staging" => "VOICE-e2b01d55",
        _ => "VOICE-afe2b8e8",
    }
}

fn http_status_error(status: reqwest::StatusCode, url: &str) -> ApiError {
    ApiError::HttpStatus {
        status_code: status.as_u16(),
        reason: status
            .canonical_reason()
            .unwrap_or_else(|| status.as_str())
            .to_string(),
        url: url.to_string(),
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
