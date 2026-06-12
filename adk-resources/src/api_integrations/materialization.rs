use crate::CommandGenError;
use crate::api_integrations::local::{
    ApiIntegrationConfig, ApiIntegrationEnvironments, ApiIntegrationItem, ApiIntegrationsFile,
    ApiOperation,
};
use crate::materialization::to_yaml_string;
use crate::projection::projection_entity_values_at;
use crate::specs::API_INTEGRATIONS;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_api_integration_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let integrations = api_integration_items(projection)?;
    if integrations.is_empty() {
        return Ok(());
    }

    let content = to_yaml_string(&ApiIntegrationsFile::new(integrations))
        .map_err(|error| CommandGenError::InvalidData(error.to_string()))?;
    crate::materialization::insert_content_resource(
        map,
        API_INTEGRATIONS.file.file_path,
        API_INTEGRATIONS.file.resource_id,
        API_INTEGRATIONS.file.name,
        content,
    )
}

fn api_integration_items(projection: &Value) -> Result<Vec<ApiIntegrationItem>, CommandGenError> {
    API_INTEGRATIONS
        .owned_entries(projection)
        .iter()
        .filter_map(|(_, integration)| {
            local_api_integration_from_projection(integration).transpose()
        })
        .collect()
}

fn local_api_integration_from_projection(
    integration: &Value,
) -> Result<Option<ApiIntegrationItem>, CommandGenError> {
    let Some(name) = integration.get("name").and_then(Value::as_str) else {
        return Ok(None);
    };
    ApiIntegrationItem::from_projection(
        name.to_string(),
        json_str(integration, &["description"]),
        api_integration_environments_from_projection(integration),
        api_integration_operations_from_projection(integration)?,
    )
    .map(Some)
    .map_err(invalid_api_integration_projection)
}

fn api_integration_environments_from_projection(integration: &Value) -> ApiIntegrationEnvironments {
    let envs = integration.get("environments");
    ApiIntegrationEnvironments::new(
        environment_config(envs, &["sandbox"]),
        environment_config(envs, &["pre-release", "preRelease", "pre_release"]),
        environment_config(envs, &["live"]),
    )
}

fn environment_config(envs: Option<&Value>, keys: &[&str]) -> Option<ApiIntegrationConfig> {
    keys.iter()
        .find_map(|key| envs.and_then(|envs| envs.get(*key)))
        .map(|env| {
            ApiIntegrationConfig::new(
                json_str(env, &["baseUrl", "base_url"]),
                json_str(env, &["authType", "auth_type"]),
            )
        })
}

fn api_integration_operations_from_projection(
    integration: &Value,
) -> Result<Vec<ApiOperation>, CommandGenError> {
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
    operations
        .into_iter()
        .filter_map(|(id, operation)| local_operation_from_projection(id, &operation).transpose())
        .collect()
}

fn local_operation_from_projection(
    id: String,
    operation: &Value,
) -> Result<Option<ApiOperation>, CommandGenError> {
    let Some(name) = operation.get("name").and_then(Value::as_str) else {
        return Ok(None);
    };
    ApiOperation::from_projection(
        id,
        name.to_string(),
        json_str(operation, &["method"]),
        json_str(operation, &["resource"]),
    )
    .map(Some)
    .map_err(invalid_api_integration_projection)
}

fn json_str(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

fn invalid_api_integration_projection(error: String) -> CommandGenError {
    CommandGenError::InvalidData(format!("Invalid API integration projection: {error}"))
}
