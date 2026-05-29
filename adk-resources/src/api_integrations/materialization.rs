use crate::CommandGenError;
use crate::materialization::insert_yaml_resource;
use crate::projection::projection_entity_values_at;
use crate::specs::API_INTEGRATIONS;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_api_integration_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(value) = api_integrations_yaml(projection) {
        insert_yaml_resource(
            map,
            API_INTEGRATIONS.file.file_path,
            API_INTEGRATIONS.file.resource_id,
            API_INTEGRATIONS.file.name,
            value,
        )?;
    }

    Ok(())
}

fn api_integrations_yaml(projection: &Value) -> Option<Value> {
    let integrations = API_INTEGRATIONS.owned_entries(projection);
    if integrations.is_empty() {
        return None;
    }
    let integrations = integrations
        .iter()
        .filter_map(|(_, integration)| {
            let name = integration.get("name")?.as_str()?;
            Some(serde_json::json!({
                "name": name,
                "description": integration.get("description").and_then(Value::as_str).unwrap_or(""),
                "environments": api_integration_environments_yaml(integration),
                "operations": api_integration_operations_yaml(integration),
            }))
        })
        .collect::<Vec<_>>();
    Some(serde_json::json!({ "api_integrations": integrations }))
}

fn api_integration_environments_yaml(integration: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for (yaml_key, source_keys) in [
        ("sandbox", &["sandbox"][..]),
        (
            "pre-release",
            &["pre-release", "preRelease", "pre_release"][..],
        ),
        ("live", &["live"][..]),
    ] {
        if let Some(env) = source_keys.iter().find_map(|key| {
            integration
                .get("environments")
                .and_then(|envs| envs.get(*key))
        }) {
            out.insert(
                yaml_key.to_string(),
                serde_json::json!({
                    "base_url": env.get("baseUrl").or_else(|| env.get("base_url")).and_then(Value::as_str).unwrap_or(""),
                    "auth_type": env.get("authType").or_else(|| env.get("auth_type")).and_then(Value::as_str).unwrap_or(""),
                }),
            );
        }
    }
    Value::Object(out)
}

fn api_integration_operations_yaml(integration: &Value) -> Value {
    let operations = integration
        .get("operations")
        .map(projection_entity_values_at)
        .unwrap_or_default();
    let operations = if operations.is_empty() {
        integration
            .get("operations")
            .and_then(Value::as_object)
            .map(|object| {
                let mut items = object
                    .iter()
                    .map(|(id, value)| (id.clone(), value.clone()))
                    .collect::<Vec<_>>();
                items.sort_by(|(left, _), (right, _)| left.cmp(right));
                items
            })
            .unwrap_or_default()
    } else {
        operations
    };
    Value::Array(
        operations
            .into_iter()
            .filter_map(|(_, operation)| {
                let name = operation.get("name")?.as_str()?;
                Some(serde_json::json!({
                    "name": name,
                    "method": operation.get("method").and_then(Value::as_str).unwrap_or(""),
                    "resource": operation.get("resource").and_then(Value::as_str).unwrap_or(""),
                }))
            })
            .collect(),
    )
}
