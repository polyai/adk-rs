use crate::FunctionValidationFailure;
use crate::entities::{ENTITIES_FILE_PATH, parse_entities_content};
use crate::flows::local::{FlowConfigFile, FlowStepFile, LocalCondition};
use crate::flows::{FlowStepType, parse_flow_config_content, parse_flow_step_content};
use adk_types::ResourceMap;
use std::collections::{BTreeSet, HashMap};

pub fn validate_flow_resources(
    resources: &ResourceMap,
) -> Result<Vec<String>, FunctionValidationFailure> {
    let flow_steps = flow_validation_step_names(resources);
    let entity_ids = flow_validation_entity_ids(resources);
    let mut errors = Vec::new();

    let mut step_paths = resources
        .keys()
        .filter(|path| {
            path.starts_with("flows/") && path.contains("/steps/") && path.ends_with(".yaml")
        })
        .cloned()
        .collect::<Vec<_>>();
    step_paths.sort();
    for path in step_paths {
        let Some(content) = resource_content(resources, &path) else {
            continue;
        };
        match parse_flow_step_content(&path, content) {
            Ok(step) => {
                validate_flow_step_resource(&path, step, &flow_steps, &entity_ids, &mut errors)
            }
            Err(parse_errors) => errors.extend(parse_errors.into_validation_errors()),
        }
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
        errors.extend(crate::functions::validate_flow_scoped_function_resource(
            &path, content, false,
        )?);
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
        errors.extend(crate::functions::validate_flow_scoped_function_resource(
            &path, content, true,
        )?);
    }

    let mut config_paths = resources
        .keys()
        .filter(|path| path.starts_with("flows/") && path.ends_with("/flow_config.yaml"))
        .cloned()
        .collect::<Vec<_>>();
    config_paths.sort();
    for path in config_paths {
        let Some(content) = resource_content(resources, &path) else {
            continue;
        };
        match parse_flow_config_content(&path, content) {
            Ok(config) => validate_flow_config_resource(&path, &config, &flow_steps, &mut errors),
            Err(parse_errors) => errors.extend(parse_errors.into_validation_errors()),
        }
    }

    Ok(errors)
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
            if let Some(content) = resource_content(resources, path)
                && let Ok(step) = parse_flow_step_content(path, content)
                && !step.name().is_empty()
            {
                flow_names.insert(step.name().to_string());
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
    let Some(content) = resource_content(resources, ENTITIES_FILE_PATH) else {
        return ids;
    };
    let Ok(items) = parse_entities_content(ENTITIES_FILE_PATH, content) else {
        return ids;
    };
    for item in items {
        let name = item.name();
        ids.insert(format!("ENTITY-{name}"));
        ids.insert(name.to_string());
    }
    ids
}

fn validate_flow_config_resource(
    path: &str,
    config: &FlowConfigFile,
    flow_steps: &FlowValidationNames,
    errors: &mut Vec<String>,
) {
    let flow_name = flow_name_from_resource_path(path).unwrap_or_default();
    let start_step = config.start_step();
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
    let description = config.description();
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
    step: FlowStepFile,
    flow_steps: &FlowValidationNames,
    entity_ids: &BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    let flow_name = flow_name_from_resource_path(path).unwrap_or_default();
    let name = step.name();
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
    let prompt = step.prompt();
    if prompt.trim().is_empty() {
        errors.push(format!(
            "Validation error in {path}: Prompt cannot be empty."
        ));
        return;
    }
    let step_type = step.step_type_value();
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
    if step.step_type() == FlowStepType::Default && !function_references.is_empty() {
        errors.push(format!(
            "Validation error in {path}: Default steps cannot reference functions. Found function references: [{}]",
            python_string_list(&function_references)
        ));
        return;
    }
    if step.step_type() == FlowStepType::Default {
        for condition in step.into_conditions() {
            validate_flow_condition(&condition, path, flow_name, flow_steps, entity_ids, errors);
        }
    }
}

fn validate_flow_condition(
    condition: &LocalCondition,
    path: &str,
    flow_name: &str,
    flow_steps: &FlowValidationNames,
    entity_ids: &BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    let name = condition.name.as_str();
    if name.is_empty() {
        errors.push(format!(
            "Validation error in {path}: Condition '{name}': Condition name cannot be empty."
        ));
        return;
    }
    let condition_type = if condition.condition_type.is_empty() {
        "exit_flow_condition"
    } else {
        condition.condition_type.as_str()
    };
    if condition_type != "exit_flow_condition" {
        let child_step = condition.child_step.as_str();
        if !flow_steps.contains(flow_name, child_step) {
            errors.push(format!(
                "Validation error in {path}: Condition '{name}': Step '{child_step}' not found"
            ));
            return;
        }
    }
    let missing_entities = condition
        .required_entities
        .iter()
        .map(String::as_str)
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
    let description = condition.description.as_str();
    if !description.is_empty() && description != description.trim() {
        errors.push(format!(
            "Validation error in {path}: Condition '{name}': Description cannot contain leading or trailing whitespace."
        ));
    }
}

fn resource_content<'a>(resources: &'a ResourceMap, path: &str) -> Option<&'a str> {
    resources.get(path)?.payload.get("content")?.as_str()
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
