use crate::CoreError;
use adk_resources::FunctionValidationFailure;
use adk_types::{DomainError, ResourceMap};
use std::collections::{BTreeSet, HashMap};
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
            let yaml = match serde_yaml::from_str::<serde_yaml::Value>(content) {
                Ok(yaml) => yaml,
                Err(e) => {
                    let _ = e;
                    return Err(DomainError::InvalidData(resource_read_error(root, path)).into());
                }
            };
            adk_resources::validate_semantic_resource(path, &yaml, &mut errors);
        } else if path.ends_with(".json")
            && let Err(e) = serde_json::from_str::<serde_json::Value>(content)
        {
            errors.push(format!("{path}: invalid json: {e}"));
        }
    }
    adk_resources::validate_python_function_resources(resources)
        .map_err(|error| function_validation_error(root, error))?;
    validate_flow_resources(root, resources, &mut errors)?;
    Ok(errors)
}

fn validate_flow_resources(
    root: &Path,
    resources: &ResourceMap,
    errors: &mut Vec<String>,
) -> Result<(), CoreError> {
    let flow_steps = flow_validation_step_names(resources);
    let entity_ids = flow_validation_entity_ids(resources);

    let mut step_paths = resources
        .keys()
        .filter(|path| {
            path.starts_with("flows/") && path.contains("/steps/") && path.ends_with(".yaml")
        })
        .cloned()
        .collect::<Vec<_>>();
    step_paths.sort();
    for path in step_paths {
        let Some(yaml) = resource_yaml_content(resources, &path) else {
            continue;
        };
        validate_flow_step_resource(&path, &yaml, &flow_steps, &entity_ids, errors);
    }

    let mut function_step_paths = resources
        .keys()
        .filter(|path| {
            path.starts_with("flows/") && path.contains("/function_steps/") && path.ends_with(".py")
        })
        .cloned()
        .collect::<Vec<_>>();
    function_step_paths.sort();
    for path in function_step_paths {
        let Some(content) = resource_content(resources, &path) else {
            continue;
        };
        validate_flow_function_step_resource(root, &path, content, errors)?;
    }

    let mut transition_function_paths = resources
        .keys()
        .filter(|path| {
            path.starts_with("flows/") && path.contains("/functions/") && path.ends_with(".py")
        })
        .cloned()
        .collect::<Vec<_>>();
    transition_function_paths.sort();
    for path in transition_function_paths {
        let Some(content) = resource_content(resources, &path) else {
            continue;
        };
        validate_flow_transition_function_resource(root, &path, content, errors)?;
    }

    let mut config_paths = resources
        .keys()
        .filter(|path| path.starts_with("flows/") && path.ends_with("/flow_config.yaml"))
        .cloned()
        .collect::<Vec<_>>();
    config_paths.sort();
    for path in config_paths {
        let Some(yaml) = resource_yaml_content(resources, &path) else {
            continue;
        };
        validate_flow_config_resource(&path, &yaml, &flow_steps, errors);
    }
    Ok(())
}

#[derive(Debug, Default)]
struct FlowValidationNames {
    by_flow: HashMap<String, BTreeSet<String>>,
}

impl FlowValidationNames {
    fn contains(&self, flow_name: &str, step_name: &str) -> bool {
        self.by_flow
            .get(flow_name)
            .is_some_and(|steps| steps.contains(step_name))
    }
}

fn flow_validation_step_names(resources: &ResourceMap) -> FlowValidationNames {
    let mut names = FlowValidationNames::default();
    for path in resources.keys() {
        let Some(flow_name) = flow_name_from_resource_path(path) else {
            continue;
        };
        if path.starts_with("flows/") && path.contains("/steps/") && path.ends_with(".yaml") {
            let flow_names = names.by_flow.entry(flow_name.to_string()).or_default();
            if let Some(stem) = path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".yaml"))
            {
                flow_names.insert(stem.to_string());
            }
            if let Some(yaml) = resource_yaml_content(resources, path)
                && let Some(name) = yaml.get("name").and_then(serde_yaml::Value::as_str)
            {
                flow_names.insert(name.to_string());
            }
        } else if path.starts_with("flows/")
            && path.contains("/function_steps/")
            && path.ends_with(".py")
            && let Some(stem) = path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".py"))
        {
            names
                .by_flow
                .entry(flow_name.to_string())
                .or_default()
                .insert(stem.to_string());
        }
    }
    names
}

fn flow_validation_entity_ids(resources: &ResourceMap) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let Some(yaml) = resource_yaml_content(resources, "config/entities.yaml") else {
        return ids;
    };
    let Some(items) = yaml
        .get("entities")
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return ids;
    };
    for item in items {
        let Some(name) = item.get("name").and_then(serde_yaml::Value::as_str) else {
            continue;
        };
        ids.insert(format!("ENTITY-{name}"));
        ids.insert(name.to_string());
    }
    ids
}

fn validate_flow_config_resource(
    path: &str,
    yaml: &serde_yaml::Value,
    flow_steps: &FlowValidationNames,
    errors: &mut Vec<String>,
) {
    let flow_name = flow_name_from_resource_path(path).unwrap_or_default();
    let start_step = yaml
        .get("start_step")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if start_step.is_empty() {
        errors.push(format!(
            "Validation error in {path}: Start step cannot be empty."
        ));
        return;
    }
    if !flow_steps.contains(flow_name, start_step) {
        errors.push(format!(
            "Validation error in {path}: Start step '{start_step}' not found."
        ));
        return;
    }
    let description = yaml
        .get("description")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if description.is_empty() {
        errors.push(format!(
            "Validation error in {path}: Description cannot be empty."
        ));
    } else if description != description.trim() {
        errors.push(format!(
            "Validation error in {path}: Description cannot contain leading or trailing whitespace."
        ));
    }
}

fn validate_flow_step_resource(
    path: &str,
    yaml: &serde_yaml::Value,
    flow_steps: &FlowValidationNames,
    entity_ids: &BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    let flow_name = flow_name_from_resource_path(path).unwrap_or_default();
    let name = yaml
        .get("name")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if name.is_empty() {
        errors.push(format!("Validation error in {path}: Name cannot be empty."));
        return;
    }
    if !valid_flow_step_name(name) {
        errors.push(format!(
            "Validation error in {path}: Name must contain only letters (including accented), numbers, and _ & , / . -"
        ));
        return;
    }
    let prompt = yaml
        .get("prompt")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if prompt.trim().is_empty() {
        errors.push(format!(
            "Validation error in {path}: Prompt cannot be empty."
        ));
        return;
    }
    let step_type = yaml
        .get("step_type")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if !matches!(
        step_type,
        "advanced_step" | "default_step" | "function_step"
    ) {
        errors.push(format!(
            "Validation error in {path}: Invalid step type: {step_type}. Valid types: ['advanced_step', 'default_step', 'function_step']"
        ));
        return;
    }
    let function_references = prompt_function_references(prompt);
    if step_type == "default_step" && !function_references.is_empty() {
        errors.push(format!(
            "Validation error in {path}: Default steps cannot reference functions. Found function references: [{}]",
            python_string_list(&function_references)
        ));
        return;
    }
    if step_type == "default_step"
        && let Some(conditions) = yaml
            .get("conditions")
            .and_then(serde_yaml::Value::as_sequence)
    {
        for condition in conditions {
            validate_flow_condition(path, condition, flow_name, flow_steps, entity_ids, errors);
        }
    }
}

fn validate_flow_condition(
    path: &str,
    condition: &serde_yaml::Value,
    flow_name: &str,
    flow_steps: &FlowValidationNames,
    entity_ids: &BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    let name = condition
        .get("name")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if name.is_empty() {
        errors.push(format!(
            "Validation error in {path}: Condition '{name}': Condition name cannot be empty."
        ));
        return;
    }
    let condition_type = condition
        .get("condition_type")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("exit_flow_condition");
    if condition_type != "exit_flow_condition" {
        let child_step = condition
            .get("child_step")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or_default();
        if !flow_steps.contains(flow_name, child_step) {
            errors.push(format!(
                "Validation error in {path}: Condition '{name}': Step '{child_step}' not found"
            ));
            return;
        }
    }
    let missing_entities = condition
        .get("required_entities")
        .and_then(serde_yaml::Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(serde_yaml::Value::as_str)
        .filter(|entity| !entity_ids.contains(*entity))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if !missing_entities.is_empty() {
        errors.push(format!(
            "Validation error in {path}: Condition '{name}': Required entities not found: {{{}}}",
            python_string_set(&missing_entities)
        ));
        return;
    }
    let description = condition
        .get("description")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default();
    if !description.is_empty() && description != description.trim() {
        errors.push(format!(
            "Validation error in {path}: Condition '{name}': Description cannot contain leading or trailing whitespace."
        ));
    }
}

fn validate_flow_function_step_resource(
    root: &Path,
    path: &str,
    content: &str,
    errors: &mut Vec<String>,
) -> Result<(), CoreError> {
    validate_flow_scoped_function_resource(root, path, content, errors, false)
}

fn validate_flow_transition_function_resource(
    root: &Path,
    path: &str,
    content: &str,
    errors: &mut Vec<String>,
) -> Result<(), CoreError> {
    validate_flow_scoped_function_resource(root, path, content, errors, true)
}

fn validate_flow_scoped_function_resource(
    root: &Path,
    path: &str,
    content: &str,
    errors: &mut Vec<String>,
    allow_user_parameters: bool,
) -> Result<(), CoreError> {
    let function_errors =
        adk_resources::validate_flow_scoped_function_resource(path, content, allow_user_parameters)
            .map_err(|error| function_validation_error(root, error))?;
    errors.extend(function_errors);
    Ok(())
}

fn resource_content<'a>(resources: &'a ResourceMap, path: &str) -> Option<&'a str> {
    resources.get(path)?.payload.get("content")?.as_str()
}

fn resource_yaml_content(resources: &ResourceMap, path: &str) -> Option<serde_yaml::Value> {
    serde_yaml::from_str(resource_content(resources, path)?).ok()
}

fn flow_name_from_resource_path(path: &str) -> Option<&str> {
    let mut parts = path.split('/');
    (parts.next()? == "flows").then_some(())?;
    parts.next()
}

fn valid_flow_step_name(name: &str) -> bool {
    name.chars()
        .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | ' ' | '&' | ',' | '/' | '.' | '-'))
}

fn prompt_function_references(prompt: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut rest = prompt;
    while let Some(index) = rest.find("{{f") {
        rest = &rest[index + 3..];
        let Some(prefix_end) = rest.find(':') else {
            continue;
        };
        let prefix = &rest[..prefix_end];
        if prefix != "n" && prefix != "t" {
            continue;
        }
        let tail = &rest[prefix_end + 1..];
        let Some(end) = tail.find("}}") else {
            break;
        };
        let name = tail[..end].trim();
        if !name.is_empty() {
            refs.push(name.to_string());
        }
        rest = &tail[end + 2..];
    }
    refs
}

fn python_string_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn python_string_set(values: &[String]) -> String {
    let mut values = values.to_vec();
    values.sort();
    python_string_list(&values)
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
