use adk_domain::{
    BranchDescriptor, BranchMergeResult, DeploymentList, PushResult, Resource, ResourceMap,
};
use adk_protobuf::agent::{RulesReferences, RulesUpdateRules};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::entities::{self, EntityCreate, EntityDelete, EntityUpdate};
use adk_protobuf::functions::{
    ErrorsUpdate, FunctionCreateFunction, FunctionDeleteFunction, FunctionError,
    FunctionParameterUpdate, FunctionUpdateFunction, ParametersUpdate,
};
use adk_protobuf::knowledge_base::{
    ExampleQueries, KnowledgeBaseCreateTopic, KnowledgeBaseDeleteTopic, KnowledgeBaseUpdateTopic,
    TopicReferences,
};
use adk_protobuf::{Command, CommandBatch, Metadata};
use prost::Message;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
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
mod push_broad;
mod push_extended;

pub use in_memory::InMemoryPlatformClient;

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
        let api_key = env::var("POLY_ADK_KEY").map_err(|_| {
            ApiError::MissingConfig(
                "POLY_ADK_KEY is not set; export POLY_ADK_KEY=<api-key>".to_string(),
            )
        })?;
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
                Some(ProjectSummary {
                    id: project.get("id")?.as_str()?.to_string(),
                    name: project.get("name")?.as_str()?.to_string(),
                })
            })
            .collect())
    }

    fn request_region_json(region: &str, endpoint: &str) -> Result<Value, ApiError> {
        let api_key = env::var("POLY_ADK_KEY").map_err(|_| {
            ApiError::MissingConfig(
                "POLY_ADK_KEY is not set; export POLY_ADK_KEY=<api-key>".to_string(),
            )
        })?;
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
    fn pull_projection_json(&self) -> Result<Value, ApiError> {
        let response = self.fetch_projection_response()?;
        Ok(Self::extract_projection(response))
    }

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
        let content = function_raw_content(&function);
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

    if let Some(personality) = projection.pointer("/agentSettings/personality") {
        insert_yaml_resource(
            &mut map,
            "agent_settings/personality.yaml",
            "personality",
            "personality",
            personality.clone(),
        )?;
    }

    if let Some(role) = projection.pointer("/agentSettings/role") {
        insert_yaml_resource(
            &mut map,
            "agent_settings/role.yaml",
            "role",
            "role",
            role.clone(),
        )?;
    }

    if let Some(safety_filters) = projection.get("contentFilterSettings") {
        insert_yaml_resource(
            &mut map,
            "agent_settings/safety_filters.yaml",
            "safety_filters",
            "safety_filters",
            safety_filters.clone(),
        )?;
    }

    if let Some(voice_safety_filters) = projection.pointer("/channels/voice/config/safetyFilters") {
        insert_yaml_resource(
            &mut map,
            "voice/safety_filters.yaml",
            "voice_safety_filters",
            "voice_safety_filters",
            voice_safety_filters.clone(),
        )?;
    }

    if let Some(asr_settings) = projection
        .pointer("/channels/voice/asrSettings")
        .or_else(|| projection.get("asrSettings"))
    {
        insert_yaml_resource(
            &mut map,
            "voice/speech_recognition/asr_settings.yaml",
            "asr_settings",
            "asr_settings",
            asr_settings.clone(),
        )?;
    }

    let voice_greeting = projection
        .pointer("/channels/voice/config/greeting")
        .cloned();
    let voice_style_prompt = projection
        .pointer("/channels/voice/config/stylePrompt")
        .cloned();
    let voice_disclaimer = projection.pointer("/channels/voice/disclaimer").cloned();
    if voice_greeting.is_some() || voice_style_prompt.is_some() || voice_disclaimer.is_some() {
        insert_yaml_resource(
            &mut map,
            "voice/configuration.yaml",
            "voice_configuration",
            "voice_configuration",
            serde_json::json!({
                "greeting": voice_greeting.unwrap_or_else(|| serde_json::json!({})),
                "style_prompt": voice_style_prompt.unwrap_or_else(|| serde_json::json!({})),
                "disclaimer_messages": voice_disclaimer
                    .map(|disclaimer| serde_json::json!([disclaimer]))
                    .unwrap_or_else(|| serde_json::json!([])),
            }),
        )?;
    }

    if let Some(behaviour) = projection
        .pointer("/agentSettings/rules/behaviour")
        .and_then(Value::as_str)
    {
        map.insert(
            "agent_settings/rules.txt".to_string(),
            Resource {
                resource_id: "rules".to_string(),
                name: "rules".to_string(),
                file_path: "agent_settings/rules.txt".to_string(),
                payload: serde_json::json!({ "content": behaviour }),
            },
        );
    }
    Ok(map)
}

fn insert_yaml_resource(
    map: &mut ResourceMap,
    file_path: &str,
    resource_id: &str,
    name: &str,
    value: Value,
) -> Result<(), ApiError> {
    let content = serde_yaml::to_string(&value).map_err(|e| ApiError::Http(e.to_string()))?;
    map.insert(
        file_path.to_string(),
        Resource {
            resource_id: resource_id.to_string(),
            name: name.to_string(),
            file_path: file_path.to_string(),
            payload: serde_json::json!({ "content": content }),
        },
    );
    Ok(())
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
        _ => return Err(ApiError::MissingConfig(format!("unknown region: {region}"))),
    };
    Ok(base_url.to_string())
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
            names.push("POLY_ADK_BASE_URL_EUW_1");
            names.push("POLY_ADK_BASE_URL_EU");
        }
        "uk-1" => names.push("POLY_ADK_BASE_URL_UK_1"),
        "us-1" => {
            names.push("POLY_ADK_BASE_URL_US_1");
            names.push("POLY_ADK_BASE_URL_US");
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
    let mut settings_update = Vec::new();

    let remote_topics = topic_entries(projection)
        .into_iter()
        .map(|(id, t)| {
            (
                t.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(id.as_str())
                    .to_string(),
                (id, t),
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
                let remote_topic = remote_topics.get(&name);
                let id = remote_topic
                    .map(|(id, _)| id.clone())
                    .or_else(|| {
                        (!is_synthetic_local_resource_id(&resource.resource_id))
                            .then_some(resource.resource_id.clone())
                    })
                    .or_else(|| generated_replay_resource_id("topic", &name, path))
                    .unwrap_or_else(|| random_resource_id("TOPICS"));
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

                if let Some((_, remote_topic)) = remote_topic {
                    if topic_yaml_matches_projection(
                        &name,
                        enabled,
                        &actions,
                        &text,
                        &example_queries,
                        remote_topic,
                    ) {
                        continue;
                    }
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
            let function_code = function_code_from_local_content(content);
            let inferred_description = infer_function_description(content);
            let inferred_parameters = infer_function_parameters(&function_code);
            if remote_functions.contains_key(&name) {
                let remote_code =
                    remote_function.and_then(|(_, f)| f.get("code").and_then(Value::as_str));
                let remote_description =
                    remote_function.and_then(|(_, f)| f.get("description").and_then(Value::as_str));
                let description_changed = !inferred_description.is_empty()
                    && remote_description != Some(inferred_description.as_str());
                if remote_code == Some(function_code.as_str()) && !description_changed {
                    continue;
                }
                let description = if description_changed {
                    Some(inferred_description.clone())
                } else {
                    remote_function
                        .and_then(|(_, f)| f.get("description").and_then(Value::as_str))
                        .map(ToString::to_string)
                        .or_else(|| {
                            (!inferred_description.is_empty())
                                .then_some(inferred_description.clone())
                        })
                };
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
                        code: Some(function_code.clone()),
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
                        code: function_code,
                        errors: vec![],
                        latency_control: None,
                        references: None,
                        archived: Some(false),
                    }),
                );
            }
        } else if path == "agent_settings/rules.txt" {
            let remote_behaviour = projection
                .pointer("/agentSettings/rules/behaviour")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if content != remote_behaviour {
                push_command(
                    &mut settings_update,
                    &metadata,
                    "update_rules",
                    CommandPayload::UpdateRules(RulesUpdateRules {
                        behaviour: Some(content.to_string()),
                        references: rules_references_from_behaviour(content)
                            .or_else(|| rules_references_from_projection(projection)),
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

    for (name, (id, _)) in remote_topics {
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
    let broad_groups = push_broad::broad_resource_command_groups(resources, projection, &metadata);

    let mut deletes: Vec<Command> = entity_del
        .into_iter()
        .chain(function_del)
        .chain(topic_del)
        .chain(ext_groups.deletes)
        .chain(broad_groups.deletes)
        .collect();
    order_commands_with_priority(&mut deletes, &DELETE_COMMAND_PRIORITY);

    let mut creates: Vec<Command> = entity_create
        .into_iter()
        .chain(function_create)
        .chain(topic_create)
        .chain(ext_groups.creates)
        .chain(broad_groups.creates)
        .collect();
    order_commands_with_priority(&mut creates, &CREATE_COMMAND_PRIORITY);

    let mut updates: Vec<Command> = entity_update
        .into_iter()
        .chain(function_update)
        .chain(topic_update)
        .chain(settings_update)
        .chain(ext_groups.updates)
        .chain(broad_groups.updates)
        .collect();
    order_commands_with_priority(&mut updates, &UPDATE_COMMAND_PRIORITY);

    let mut out: Vec<Command> = Vec::new();
    out.extend(deletes);
    out.extend(creates);
    out.extend(updates);
    out.extend(ext_groups.post_updates);
    out.extend(broad_groups.post_updates);
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
    "update_rules",
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

fn function_raw_content(function: &Value) -> String {
    let code = function
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut content = String::new();
    if let Some(description) = function.get("description").and_then(Value::as_str)
        && !description.is_empty()
    {
        content.push_str("@func_description(");
        content.push_str(&python_string_literal(description));
        content.push_str(")\n");
    }
    content.push_str(code);
    content
}

fn python_string_literal(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::new();
    out.push(quote);
    for ch in value.chars() {
        if ch == '\\' || ch == quote {
            out.push('\\');
        }
        match ch {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push(quote);
    out
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

fn rules_references_from_projection(projection: &Value) -> Option<RulesReferences> {
    let references = projection.pointer("/agentSettings/rules/references")?;
    let refs = RulesReferences {
        sms: json_bool_map(references.get("sms")),
        handoff: json_bool_map(references.get("handoff")),
        attributes: json_bool_map(references.get("attributes")),
        global_functions: json_bool_map(
            references
                .get("globalFunctions")
                .or_else(|| references.get("global_functions")),
        ),
        variables: json_bool_map(references.get("variables")),
        translations: json_bool_map(references.get("translations")),
    };
    if refs.sms.is_empty()
        && refs.handoff.is_empty()
        && refs.attributes.is_empty()
        && refs.global_functions.is_empty()
        && refs.variables.is_empty()
        && refs.translations.is_empty()
    {
        None
    } else {
        Some(refs)
    }
}

fn rules_references_from_behaviour(behaviour: &str) -> Option<RulesReferences> {
    let refs = RulesReferences {
        sms: extract_template_references(behaviour, "sms"),
        handoff: extract_template_references(behaviour, "ho"),
        attributes: extract_template_references(behaviour, "attr"),
        global_functions: extract_template_references(behaviour, "fn"),
        variables: extract_template_references(behaviour, "var"),
        translations: HashMap::new(),
    };
    if refs.sms.is_empty()
        && refs.handoff.is_empty()
        && refs.attributes.is_empty()
        && refs.global_functions.is_empty()
        && refs.variables.is_empty()
        && refs.translations.is_empty()
    {
        None
    } else {
        Some(refs)
    }
}

fn extract_template_references(behaviour: &str, prefix: &str) -> HashMap<String, bool> {
    let marker = format!("{{{{{prefix}:");
    let mut out = HashMap::new();
    let mut start = 0;
    while let Some(index) = behaviour[start..].find(&marker) {
        let value_start = start + index + marker.len();
        let tail = &behaviour[value_start..];
        let Some(end) = tail.find("}}") else {
            break;
        };
        let value = tail[..end].trim();
        if !value.is_empty() {
            out.insert(value.to_string(), true);
        }
        start = value_start + end + 2;
    }
    out
}

fn json_bool_map(value: Option<&Value>) -> HashMap<String, bool> {
    value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.as_bool().unwrap_or(true)))
                .collect()
        })
        .unwrap_or_default()
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

fn is_synthetic_local_resource_id(resource_id: &str) -> bool {
    let trimmed = resource_id.trim();
    trimmed.is_empty()
        || trimmed == "local"
        || trimmed.contains('/')
        || trimmed.ends_with(".yaml")
        || trimmed.ends_with(".yml")
        || trimmed.ends_with(".py")
}

pub(crate) fn random_resource_id(prefix: &str) -> String {
    let hex = Uuid::new_v4().simple().to_string();
    format!("{prefix}-{}", &hex[..8])
}

pub(crate) fn generated_replay_resource_id(kind: &str, name: &str, path: &str) -> Option<String> {
    let env_name = format!("POLY_ADK_GENERATED_{}_IDS", kind.to_ascii_uppercase());
    let mappings = env::var(env_name).ok()?;
    for raw in mappings.lines() {
        let Some((key, id)) = raw.split_once('=') else {
            continue;
        };
        if key == name || key == path {
            return Some(id.to_string());
        }
    }
    None
}

fn topic_yaml_matches_projection(
    name: &str,
    enabled: bool,
    actions: &str,
    content: &str,
    example_queries: &[String],
    topic: &Value,
) -> bool {
    let remote_name = topic.get("name").and_then(Value::as_str).unwrap_or(name);
    let remote_enabled = topic
        .get("isActive")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let remote_actions = topic.get("actions").and_then(Value::as_str).unwrap_or("");
    let remote_content = topic.get("content").and_then(Value::as_str).unwrap_or("");
    let remote_queries = topic
        .get("exampleQueries")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    item.get("query")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    remote_name == name
        && remote_enabled == enabled
        && remote_actions == actions
        && remote_content == content
        && remote_queries == example_queries
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

fn function_code_from_local_content(content: &str) -> String {
    let mut out = String::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        if line == "from _gen import *  # <AUTO GENERATED>"
            || line == "from imports import *  # <AUTO GENERATED>"
            || trimmed.starts_with("@func_description(")
            || trimmed.starts_with("@func_parameter(")
            || trimmed.starts_with("@func_latency_control(")
        {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !content.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out.trim_start_matches('\n').to_string()
}

fn infer_function_description(code: &str) -> String {
    if let Some(description) = function_description_decorator(code) {
        return description;
    }
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

fn function_description_decorator(code: &str) -> Option<String> {
    for line in code.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("@func_description(") else {
            continue;
        };
        let arg = rest.strip_suffix(')').unwrap_or(rest).trim();
        return Some(parse_python_string_literal(arg));
    }
    None
}

fn parse_python_string_literal(value: &str) -> String {
    let mut chars = value.chars();
    let Some(quote @ ('\'' | '"')) = chars.next() else {
        return value.to_string();
    };
    let mut out = String::new();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            match ch {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                other => out.push(other),
            }
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            break;
        } else {
            out.push(ch);
        }
    }
    out
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

pub(crate) fn yaml_str(config: &serde_yaml::Value, key: &str) -> String {
    yaml_string(Some(config), key)
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
    let mut value = serde_json::json!({
        "type": command.r#type,
        "command_id": command.command_id,
    });
    if let Some(metadata) = &command.metadata {
        value["metadata"] = metadata_to_json(metadata);
    }
    if let Some(payload) = &command.payload {
        match payload {
            CommandPayload::DeleteFunction(delete) => {
                value["delete_function"] = serde_json::json!({ "id": delete.id });
            }
            CommandPayload::CreateTopic(topic) => {
                value["create_topic"] = create_topic_to_json(topic);
            }
            CommandPayload::UpdateRules(update) => {
                value["update_rules"] = rules_update_to_json(update);
            }
            _ => {}
        }
        if let Some((key, payload_value)) = push_broad::payload_json_summary(payload) {
            value[key] = payload_value;
        }
    }
    value
}

fn metadata_to_json(metadata: &Metadata) -> Value {
    let created_at = metadata
        .created_at
        .as_ref()
        .map(|timestamp| format!("{}.{:09}Z", timestamp.seconds, timestamp.nanos))
        .unwrap_or_default();
    serde_json::json!({
        "created_at": created_at,
        "created_by": metadata.created_by,
    })
}

fn rules_update_to_json(update: &RulesUpdateRules) -> Value {
    let mut value = serde_json::Map::new();
    if let Some(behaviour) = &update.behaviour {
        value.insert("behaviour".to_string(), Value::String(behaviour.clone()));
    }
    if let Some(references) = &update.references {
        let references_json = rules_references_to_json(references);
        if references_json
            .as_object()
            .map(|object| !object.is_empty())
            .unwrap_or(false)
        {
            value.insert("references".to_string(), references_json);
        }
    }
    Value::Object(value)
}

fn create_topic_to_json(topic: &KnowledgeBaseCreateTopic) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(topic.id.clone()));
    value.insert("name".to_string(), Value::String(topic.name.clone()));
    value.insert("content".to_string(), Value::String(topic.content.clone()));
    value.insert("actions".to_string(), Value::String(topic.actions.clone()));
    if let Some(example_queries) = &topic.example_queries {
        value.insert(
            "example_queries".to_string(),
            serde_json::json!({ "queries": example_queries.queries.clone() }),
        );
    }
    value.insert(
        "references".to_string(),
        topic
            .references
            .as_ref()
            .map(topic_references_to_json)
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
    );
    if let Some(is_active) = topic.is_active {
        value.insert("is_active".to_string(), Value::Bool(is_active));
    }
    Value::Object(value)
}

fn topic_references_to_json(references: &TopicReferences) -> Value {
    let mut value = serde_json::Map::new();
    insert_bool_map(&mut value, "sms", &references.sms);
    insert_bool_map(&mut value, "handoff", &references.handoff);
    insert_bool_map(&mut value, "attributes", &references.attributes);
    insert_bool_map(&mut value, "globalFunctions", &references.global_functions);
    insert_bool_map(&mut value, "variables", &references.variables);
    insert_bool_map(&mut value, "translations", &references.translations);
    Value::Object(value)
}

fn rules_references_to_json(references: &RulesReferences) -> Value {
    let mut value = serde_json::Map::new();
    insert_bool_map(&mut value, "sms", &references.sms);
    insert_bool_map(&mut value, "handoff", &references.handoff);
    insert_bool_map(&mut value, "attributes", &references.attributes);
    insert_bool_map(&mut value, "globalFunctions", &references.global_functions);
    insert_bool_map(&mut value, "variables", &references.variables);
    insert_bool_map(&mut value, "translations", &references.translations);
    Value::Object(value)
}

fn insert_bool_map(
    target: &mut serde_json::Map<String, Value>,
    key: &str,
    source: &HashMap<String, bool>,
) {
    if source.is_empty() {
        return;
    }
    target.insert(key.to_string(), serde_json::json!(source));
}

#[cfg(test)]
mod tests;
