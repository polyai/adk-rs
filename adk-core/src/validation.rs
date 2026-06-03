use crate::CoreError;
use adk_resources::FunctionValidationFailure;
use adk_types::{DomainError, ResourceMap};
use serde_json::Value as JsonValue;
use serde_yaml_ng::{Value as YamlValue, from_str};
use std::path::Path;

pub(crate) fn validate_local_resources(
    root: &Path,
    resources: &ResourceMap,
) -> Result<Vec<String>, CoreError> {
    // Keep validation phases in Python-compatible order: parse and resource-local
    // YAML/JSON checks first, Python function checks next, cross-resource flow
    // reference checks last.
    let mut errors = Vec::new();
    for (path, resource) in resources {
        let content = resource
            .payload
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if path.ends_with(".yaml") || path.ends_with(".yml") {
            let yaml = match from_str::<YamlValue>(content) {
                Ok(yaml) => yaml,
                Err(e) => {
                    let _ = e;
                    return Err(DomainError::InvalidData(resource_read_error(root, path)).into());
                }
            };
            adk_resources::validate_semantic_resource(path, &yaml, &mut errors);
        } else if path.ends_with(".json")
            && let Err(e) = serde_json::from_str::<JsonValue>(content)
        {
            errors.push(format!("{path}: invalid json: {e}"));
        }
    }
    adk_resources::validate_python_function_resources(resources)
        .map_err(|error| function_validation_error(root, error))?;
    errors.extend(adk_resources::validate_language_translation_resources(
        resources,
    ));
    errors.extend(
        adk_resources::validate_flow_resources(resources)
            .map_err(|error| function_validation_error(root, error))?,
    );
    Ok(errors)
}

fn resource_read_error(root: &Path, path: &str) -> String {
    let abs_path = root.join(path).to_string_lossy().to_string();
    resource_read_error_with_detail(root, path, &format!("Error loading YAML file: {abs_path}"))
}

fn function_validation_error(root: &Path, error: FunctionValidationFailure) -> CoreError {
    DomainError::InvalidData(resource_read_error_with_detail(
        root,
        error.path(),
        error.detail(),
    ))
    .into()
}

fn resource_read_error_with_detail(root: &Path, path: &str, detail: &str) -> String {
    let abs_path = root.join(path).to_string_lossy().to_string();
    let resource_name = Path::new(path)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    format!("Error reading resource {resource_name} at {abs_path}: {detail}")
}
