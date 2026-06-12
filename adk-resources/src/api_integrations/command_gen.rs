use crate::ids::{stable_resource_id, stable_resource_uuid};
use crate::local_parse::ParseLocalResource;
use crate::push_command_inputs::{json_str, resource_yaml};
use crate::specs::API_INTEGRATIONS;
use crate::{api_integrations::local::ApiIntegrationItem as LocalApiIntegrationItem, push_command};
use adk_protobuf::Metadata;
use adk_protobuf::api_integrations::{
    ApiIntegrationConfig as ProtoApiIntegrationConfig, ApiIntegrationConfigUpdate,
    ApiIntegrationCreate, ApiIntegrationDelete, ApiIntegrationOperationCreate,
    ApiIntegrationOperationDelete, ApiIntegrationOperationUpdate, ApiIntegrationUpdate,
    Environments,
};
use adk_protobuf::command::Payload as CommandPayload;
use adk_types::ResourceMap;
use serde_json::{self, Value as JsonValue, json};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct ApiIntegrationLifecycleCommands {
    pub(crate) integration_deletes: Vec<adk_protobuf::Command>,
    pub(crate) operation_deletes: Vec<adk_protobuf::Command>,
    pub(crate) integration_creates: Vec<adk_protobuf::Command>,
    pub(crate) operation_creates: Vec<adk_protobuf::Command>,
    pub(crate) integration_updates: Vec<adk_protobuf::Command>,
    pub(crate) operation_updates: Vec<adk_protobuf::Command>,
    pub(crate) config_updates: Vec<adk_protobuf::Command>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ApiIntegrationItem {
    id: String,
    name: String,
    description: String,
    environments: HashMap<String, ApiEnvironmentItem>,
    operations: Vec<ApiOperationItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ApiEnvironmentItem {
    base_url: String,
    auth_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ApiOperationItem {
    id: String,
    name: String,
    method: String,
    resource: String,
}

pub(crate) fn api_integration_lifecycle_commands(
    resources: &ResourceMap,
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) -> ApiIntegrationLifecycleCommands {
    let Some(yaml) = resource_yaml(resources, API_INTEGRATIONS.file.file_path) else {
        return ApiIntegrationLifecycleCommands::default();
    };

    let local_integrations = local_api_integration_items(&yaml);
    let remote_integrations = remote_api_integration_items(projection);
    let local_names = local_integrations
        .iter()
        .map(|item| item.name.clone())
        .collect::<HashSet<_>>();
    let remote_by_name = remote_integrations
        .iter()
        .map(|item| (item.name.clone(), item.clone()))
        .collect::<HashMap<_, _>>();
    let mut integration_ids_by_name = remote_by_name
        .iter()
        .map(|(name, item)| (name.clone(), item.id.clone()))
        .collect::<HashMap<_, _>>();

    let mut commands = ApiIntegrationLifecycleCommands::default();
    for remote in &remote_integrations {
        if !local_names.contains(&remote.name) {
            push_command(
                &mut commands.integration_deletes,
                metadata,
                "delete_api_integration",
                CommandPayload::DeleteApiIntegration(ApiIntegrationDelete {
                    id: remote.id.clone(),
                }),
            );
        }
    }

    for local in &local_integrations {
        if remote_by_name.contains_key(&local.name) {
            continue;
        }
        let id = stable_resource_id(
            API_INTEGRATIONS.id_prefix,
            &local.name,
            API_INTEGRATIONS.file.file_path,
        );
        integration_ids_by_name.insert(local.name.clone(), id.clone());
        push_command(
            &mut commands.integration_creates,
            metadata,
            "create_api_integration",
            CommandPayload::CreateApiIntegration(ApiIntegrationCreate {
                id,
                name: local.name.clone(),
                description: Some(local.description.clone()),
                environments: Some(environments_from_items(&local.environments)),
            }),
        );
    }

    for local in &local_integrations {
        let Some(remote) = remote_by_name.get(&local.name) else {
            continue;
        };
        if local.description != remote.description {
            push_command(
                &mut commands.integration_updates,
                metadata,
                "update_api_integration",
                CommandPayload::UpdateApiIntegration(ApiIntegrationUpdate {
                    id: remote.id.clone(),
                    name: Some(local.name.clone()),
                    description: Some(local.description.clone()),
                }),
            );
        }

        let local_ops_by_name = local
            .operations
            .iter()
            .map(|operation| (operation.name.clone(), operation.clone()))
            .collect::<HashMap<_, _>>();
        let remote_ops_by_name = remote
            .operations
            .iter()
            .map(|operation| (operation.name.clone(), operation.clone()))
            .collect::<HashMap<_, _>>();
        for remote_operation in &remote.operations {
            if !local_ops_by_name.contains_key(&remote_operation.name) {
                push_command(
                    &mut commands.operation_deletes,
                    metadata,
                    "delete_api_integration_operation",
                    CommandPayload::DeleteApiIntegrationOperation(ApiIntegrationOperationDelete {
                        id: remote_operation.id.clone(),
                        integration_id: remote.id.clone(),
                    }),
                );
            }
        }
        for local_operation in &local.operations {
            match remote_ops_by_name.get(&local_operation.name) {
                Some(remote_operation) => {
                    if local_operation.method != remote_operation.method
                        || local_operation.resource != remote_operation.resource
                    {
                        push_command(
                            &mut commands.operation_updates,
                            metadata,
                            "update_api_integration_operation",
                            CommandPayload::UpdateApiIntegrationOperation(
                                ApiIntegrationOperationUpdate {
                                    id: remote_operation.id.clone(),
                                    name: Some(local_operation.name.clone()),
                                    method: Some(local_operation.method.clone()),
                                    resource: Some(local_operation.resource.clone()),
                                    integration_id: Some(remote.id.clone()),
                                },
                            ),
                        );
                    }
                }
                None => push_create_api_operation(
                    &mut commands.operation_creates,
                    metadata,
                    &local.name,
                    &remote.id,
                    local_operation,
                ),
            }
        }

        for env_name in ["sandbox", "pre_release", "live"] {
            let local_env = local.environments.get(env_name);
            let remote_env = remote.environments.get(env_name);
            if let Some(local_env) = local_env
                && Some(local_env) != remote_env
            {
                push_command(
                    &mut commands.config_updates,
                    metadata,
                    "update_api_integration_config",
                    CommandPayload::UpdateApiIntegrationConfig(ApiIntegrationConfigUpdate {
                        id: remote.id.clone(),
                        environment: env_name.to_string(),
                        base_url: Some(local_env.base_url.clone()),
                        auth_type: Some(local_env.auth_type.clone()),
                    }),
                );
            }
        }
    }

    for local in &local_integrations {
        if remote_by_name.contains_key(&local.name) {
            continue;
        }
        let integration_id = integration_ids_by_name
            .get(&local.name)
            .cloned()
            .unwrap_or_default();
        for local_operation in &local.operations {
            push_create_api_operation(
                &mut commands.operation_creates,
                metadata,
                &local.name,
                &integration_id,
                local_operation,
            );
        }
    }

    commands
}

fn local_api_integration_items(yaml: &serde_yaml_ng::Value) -> Vec<ApiIntegrationItem> {
    let Ok(file) = crate::api_integrations::ApiIntegration::parse_local_yaml(
        API_INTEGRATIONS.file.file_path,
        yaml,
    ) else {
        return Vec::new();
    };
    file.api_integrations
        .iter()
        .map(local_api_integration_item)
        .collect()
}

fn local_api_integration_item(item: &LocalApiIntegrationItem) -> ApiIntegrationItem {
    ApiIntegrationItem {
        id: String::new(),
        name: item.name().to_string(),
        description: item.description().to_string(),
        environments: item
            .environments()
            .entries()
            .into_iter()
            .map(|(name, config)| {
                (
                    name.to_string(),
                    ApiEnvironmentItem {
                        base_url: config.base_url().to_string(),
                        auth_type: config.auth_type().to_string(),
                    },
                )
            })
            .collect(),
        operations: item
            .operations()
            .iter()
            .map(|operation| ApiOperationItem {
                id: operation.id().to_string(),
                name: operation.name().to_string(),
                method: operation.method(),
                resource: operation.resource().to_string(),
            })
            .collect(),
    }
}

fn remote_api_integration_items(projection: &JsonValue) -> Vec<ApiIntegrationItem> {
    API_INTEGRATIONS
        .entries(projection)
        .into_iter()
        .filter_map(|(id, value)| {
            let name = json_str(value, &["name"]);
            if name.is_empty() {
                return None;
            }
            Some(ApiIntegrationItem {
                id,
                name,
                description: json_str(value, &["description"]),
                environments: api_environment_items_from_projection(value),
                operations: api_operations_from_projection(value),
            })
        })
        .collect()
}

fn api_environment_items_from_projection(value: &JsonValue) -> HashMap<String, ApiEnvironmentItem> {
    let Some(envs) = value.get("environments").and_then(JsonValue::as_object) else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for (source_key, normalized_key) in [
        ("sandbox", "sandbox"),
        ("pre-release", "pre_release"),
        ("preRelease", "pre_release"),
        ("pre_release", "pre_release"),
        ("live", "live"),
    ] {
        if let Some(env) = envs.get(source_key) {
            out.insert(
                normalized_key.to_string(),
                ApiEnvironmentItem {
                    base_url: json_str(env, &["baseUrl", "base_url"]),
                    auth_type: json_str(env, &["authType", "auth_type"]),
                },
            );
        }
    }
    out
}

fn api_operations_from_projection(integration: &JsonValue) -> Vec<ApiOperationItem> {
    let Some(operations) = integration.get("operations") else {
        return Vec::new();
    };
    let mut items =
        if let Some(entities) = operations.get("entities").and_then(JsonValue::as_object) {
            let ids = operations.get("ids").and_then(JsonValue::as_array);
            let mut ordered = Vec::new();
            let mut seen = HashSet::new();
            if let Some(ids) = ids {
                for id in ids.iter().filter_map(JsonValue::as_str) {
                    if let Some(operation) = entities.get(id) {
                        ordered.push((id.to_string(), operation));
                        seen.insert(id.to_string());
                    }
                }
            }
            let mut remaining = entities
                .iter()
                .filter(|(id, _)| !seen.contains(*id))
                .collect::<Vec<_>>();
            remaining.sort_by_key(|(left, _)| *left);
            ordered.extend(
                remaining
                    .into_iter()
                    .map(|(id, operation)| (id.clone(), operation)),
            );
            ordered
        } else if let Some(object) = operations.as_object() {
            let mut pairs = object
                .iter()
                .map(|(id, operation)| (id.clone(), operation))
                .collect::<Vec<_>>();
            pairs.sort_by(|(left, _), (right, _)| left.cmp(right));
            pairs
        } else {
            Vec::new()
        };

    items
        .drain(..)
        .filter_map(|(id, value)| {
            let name = json_str(value, &["name"]);
            if name.is_empty() {
                return None;
            }
            Some(ApiOperationItem {
                id,
                name,
                method: json_str(value, &["method"]),
                resource: json_str(value, &["resource"]),
            })
        })
        .collect()
}

fn environments_from_items(items: &HashMap<String, ApiEnvironmentItem>) -> Environments {
    Environments {
        sandbox: items.get("sandbox").map(api_config_from_item),
        pre_release: items.get("pre_release").map(api_config_from_item),
        live: items.get("live").map(api_config_from_item),
    }
}

fn api_config_from_item(item: &ApiEnvironmentItem) -> ProtoApiIntegrationConfig {
    ProtoApiIntegrationConfig {
        base_url: item.base_url.clone(),
        auth_type: item.auth_type.clone(),
    }
}

fn push_create_api_operation(
    commands: &mut Vec<adk_protobuf::Command>,
    metadata: &Option<Metadata>,
    integration_name: &str,
    integration_id: &str,
    operation: &ApiOperationItem,
) {
    let operation_path = format!("{}:{integration_name}", API_INTEGRATIONS.file.file_path);
    let id = stable_resource_uuid(&operation.name, &operation_path);
    push_command(
        commands,
        metadata,
        "create_api_integration_operation",
        CommandPayload::CreateApiIntegrationOperation(ApiIntegrationOperationCreate {
            name: operation.name.clone(),
            method: operation.method.clone(),
            resource: operation.resource.clone(),
            integration_id: integration_id.to_string(),
            id,
        }),
    );
}

pub(crate) fn environments_json(environments: Option<&Environments>) -> JsonValue {
    let Some(environments) = environments else {
        return json!({});
    };
    let mut value = serde_json::Map::new();
    if let Some(sandbox) = &environments.sandbox {
        value.insert("sandbox".to_string(), api_integration_config_json(sandbox));
    }
    if let Some(pre_release) = &environments.pre_release {
        value.insert(
            "pre_release".to_string(),
            api_integration_config_json(pre_release),
        );
    }
    if let Some(live) = &environments.live {
        value.insert("live".to_string(), api_integration_config_json(live));
    }
    JsonValue::Object(value)
}

fn api_integration_config_json(config: &ProtoApiIntegrationConfig) -> JsonValue {
    json!({
        "base_url": config.base_url,
        "auth_type": config.auth_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_types::Resource;

    #[test]
    fn api_integration_commands_parse_local_yaml_through_typed_model() {
        let mut resources = ResourceMap::new();
        resources.insert(
            API_INTEGRATIONS.file.file_path.to_string(),
            Resource {
                resource_id: API_INTEGRATIONS.file.resource_id.to_string(),
                name: API_INTEGRATIONS.file.name.to_string(),
                file_path: API_INTEGRATIONS.file.file_path.to_string(),
                payload: json!({
                    "content": r#"
api_integrations:
  - name: orders_api
    description: Order lookup API.
    environments:
      sandbox:
        base_url: https://sandbox.example.test
        auth_type: apiKey
      pre-release:
        base_url: ""
        auth_type: none
      live:
        base_url: ""
        auth_type: none
    operations:
      - name: get_order
        method: get
        resource: /orders/{id}
"#,
                }),
            },
        );
        let projection = json!({
            "apiIntegrations": {
                "apiIntegrations": {
                    "ids": ["api-1"],
                    "entities": {
                        "api-1": {
                            "id": "api-1",
                            "name": "orders_api",
                            "description": "Order lookup API.",
                            "environments": {
                                "sandbox": {
                                    "baseUrl": "https://sandbox.example.test",
                                    "authType": "apiKey"
                                },
                                "preRelease": {
                                    "baseUrl": "",
                                    "authType": "none"
                                },
                                "live": {
                                    "baseUrl": "",
                                    "authType": "none"
                                }
                            },
                            "operations": {
                                "ids": ["op-1"],
                                "entities": {
                                    "op-1": {
                                        "id": "op-1",
                                        "name": "get_order",
                                        "method": "GET",
                                        "resource": "/orders/{id}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        let commands = api_integration_lifecycle_commands(&resources, &projection, &None);

        assert!(commands.integration_deletes.is_empty());
        assert!(commands.operation_deletes.is_empty());
        assert!(commands.integration_creates.is_empty());
        assert!(commands.operation_creates.is_empty());
        assert!(commands.integration_updates.is_empty());
        assert!(commands.operation_updates.is_empty());
        assert!(commands.config_updates.is_empty());
    }
}
