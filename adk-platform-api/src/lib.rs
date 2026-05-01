use adk_domain::{DeploymentList, PushResult, Resource, ResourceMap};
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
}

/// Test-only in-memory client used for deterministic non-network workflows.
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
}

impl PlatformClient for HttpPlatformClient {
    fn pull_resources(&self) -> Result<ResourceMap, ApiError> {
        let response = self.fetch_projection_response()?;
        let projection = response
            .get("projection")
            .cloned()
            .unwrap_or_else(|| response.clone());
        let mut map = ResourceMap::new();

        for (id, topic) in topic_entries(&projection) {
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

        for (id, function) in function_entries(&projection) {
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
        for (id, entity) in entity_entries(&projection) {
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
            let content =
                serde_yaml::to_string(&serde_json::json!({ "entities": entity_yaml_list }))
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

        Ok(map)
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

    let mut out: Vec<Command> = entity_del
        .into_iter()
        .chain(function_del)
        .chain(topic_del)
        .chain(entity_create)
        .chain(function_create)
        .chain(topic_create)
        .chain(entity_update)
        .chain(function_update)
        .chain(topic_update)
        .collect();
    out.extend(push_extended::extended_resource_commands(
        resources, projection, &metadata,
    ));
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
}
