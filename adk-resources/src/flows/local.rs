use crate::functions::{infer_function_description, try_function_code_from_local_content};
use crate::local_parse::{
    ResourceParseErrors, ResourceParseResult, default_if_null, deserialize_yaml,
};
use crate::{
    CommandGenError, FlowImportPathMaps, PromptReferenceMaps, replace_flow_import_names_with_ids,
    replace_resource_names_with_ids,
};
use adk_protobuf::flows::{StepAsrConfig, StepDtmfConfig, StepPosition};
use adk_types::{Resource, ResourceMap};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub(super) struct LocalFlow {
    pub(super) folder: String,
    pub(super) config_path: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) start_step: String,
    pub(super) steps: Vec<LocalFlowStep>,
    pub(super) function_steps: Vec<LocalFunctionStep>,
    pub(super) transition_functions: Vec<LocalTransitionFunction>,
}

#[derive(Debug, Clone)]
pub(super) struct LocalFlowStep {
    pub(super) path: String,
    pub(super) name: String,
    pub(super) step_type: FlowStepType,
    pub(super) prompt: String,
    pub(super) asr_biasing: Option<StepAsrConfig>,
    pub(super) dtmf_config: Option<StepDtmfConfig>,
    pub(super) conditions: Vec<LocalCondition>,
    pub(super) extracted_entities: Vec<String>,
    pub(super) position: Option<StepPosition>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum FlowStepType {
    #[default]
    Advanced,
    Default,
}

#[derive(Debug, Clone)]
pub(super) struct LocalFunctionStep {
    pub(super) path: String,
    pub(super) name: String,
    pub(super) content: String,
    pub(super) code: String,
    pub(super) position: Option<StepPosition>,
}

#[derive(Debug, Clone)]
pub(super) struct LocalTransitionFunction {
    pub(super) path: String,
    pub(super) name: String,
    pub(super) content: String,
    pub(super) code: String,
    pub(super) description: String,
}

#[derive(Debug, Clone)]
pub(super) struct LocalCondition {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) condition_type: String,
    pub(super) child_step: String,
    pub(super) required_entities: Vec<String>,
    pub(super) ingress: String,
    pub(super) position: Option<StepPosition>,
    pub(super) exit_flow_position: Option<StepPosition>,
}

pub(super) fn local_flows(
    resources: &ResourceMap,
    prompt_reference_maps: &PromptReferenceMaps,
    flow_import_path_maps: &FlowImportPathMaps,
) -> Result<Vec<LocalFlow>, CommandGenError> {
    let mut flows: HashMap<String, LocalFlow> = HashMap::new();

    for resource in resources.values() {
        let path = resource.file_path.as_str();
        if path.starts_with("flows/") && path.ends_with("/flow_config.yaml") {
            let Some(folder) = flow_folder_from_path(path) else {
                continue;
            };
            let Ok(config) = parse_flow_config_content(path, resource_content(resource)) else {
                continue;
            };
            let entry = flows.entry(folder.clone()).or_insert_with(|| LocalFlow {
                folder,
                config_path: path.to_string(),
                ..LocalFlow::default()
            });
            entry.config_path = path.to_string();
            entry.name = non_empty(config.name, &entry.folder);
            entry.description = config.description;
            entry.start_step = config.start_step;
        } else if path.starts_with("flows/") && path.contains("/steps/") && path.ends_with(".yaml")
        {
            let Some(folder) = flow_folder_from_path(path) else {
                continue;
            };
            let Ok(step) = parse_flow_step_content(path, resource_content(resource)) else {
                continue;
            };
            let entry = flows.entry(folder.clone()).or_insert_with(|| LocalFlow {
                folder: folder.clone(),
                ..LocalFlow::default()
            });
            entry
                .steps
                .push(step.into_local(path, &resource.name, &folder, prompt_reference_maps));
        } else if path.starts_with("flows/")
            && path.contains("/function_steps/")
            && path.ends_with(".py")
        {
            let Some(folder) = flow_folder_from_path(path) else {
                continue;
            };
            let entry = flows.entry(folder.clone()).or_insert_with(|| LocalFlow {
                folder,
                ..LocalFlow::default()
            });
            let content = resource_content(resource);
            entry.function_steps.push(LocalFunctionStep {
                path: path.to_string(),
                name: function_name_from_path(path),
                content: content.to_string(),
                code: replace_flow_import_names_with_ids(
                    &try_function_code_from_local_content(path, content)?,
                    flow_import_path_maps,
                ),
                position: None,
            });
        } else if path.starts_with("flows/")
            && path.contains("/functions/")
            && path.ends_with(".py")
        {
            let Some(folder) = flow_folder_from_path(path) else {
                continue;
            };
            let content = resource_content(resource);
            let code = replace_flow_import_names_with_ids(
                &try_function_code_from_local_content(path, content)?,
                flow_import_path_maps,
            );
            let entry = flows.entry(folder.clone()).or_insert_with(|| LocalFlow {
                folder,
                ..LocalFlow::default()
            });
            entry.transition_functions.push(LocalTransitionFunction {
                path: path.to_string(),
                name: function_name_from_path(path),
                content: content.to_string(),
                description: infer_function_description(content),
                code,
            });
        }
    }

    let mut flows = flows
        .into_values()
        .filter(|flow| !flow.name.is_empty())
        .collect::<Vec<_>>();
    flows.sort_by(|left, right| left.config_path.cmp(&right.config_path));
    Ok(flows)
}

pub(super) fn default_dtmf_config() -> StepDtmfConfig {
    RawDtmfConfig::default().into()
}

pub(super) fn default_step_position(index: usize) -> StepPosition {
    StepPosition {
        x: (index as f32) * 600.0,
        y: 0.0,
    }
}

pub(super) fn ordered_flow_steps(flow: &LocalFlow) -> Vec<&LocalFlowStep> {
    let mut steps = flow.steps.iter().collect::<Vec<_>>();
    steps.sort_by(|left, right| {
        let left_start = left.name == flow.start_step;
        let right_start = right.name == flow.start_step;
        right_start
            .cmp(&left_start)
            .then_with(|| left.path.cmp(&right.path))
    });
    steps
}

pub(super) fn ordered_function_steps(flow: &LocalFlow) -> Vec<&LocalFunctionStep> {
    let mut steps = flow.function_steps.iter().collect::<Vec<_>>();
    steps.sort_by(|left, right| left.path.cmp(&right.path));
    steps
}

pub(super) fn ordered_transition_functions(flow: &LocalFlow) -> Vec<&LocalTransitionFunction> {
    let mut functions = flow.transition_functions.iter().collect::<Vec<_>>();
    functions.sort_by(|left, right| left.path.cmp(&right.path));
    functions
}

#[derive(Debug, Deserialize)]
pub(crate) struct FlowConfigFile {
    #[serde(default, deserialize_with = "default_if_null")]
    name: String,
    #[serde(default, deserialize_with = "default_if_null")]
    description: String,
    #[serde(default, deserialize_with = "default_if_null")]
    start_step: String,
}

impl FlowConfigFile {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn start_step(&self) -> &str {
        &self.start_step
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }
}

pub(crate) fn parse_flow_config_file(
    path: &str,
    yaml: &YamlValue,
) -> ResourceParseResult<FlowConfigFile> {
    deserialize_yaml(path, yaml)
}

pub(crate) fn parse_flow_config_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<FlowConfigFile> {
    let yaml = serde_yaml_ng::from_str::<YamlValue>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    parse_flow_config_file(path, &yaml)
}

#[derive(Debug, Deserialize)]
pub(crate) struct FlowStepFile {
    #[serde(default, deserialize_with = "default_if_null")]
    step_type: String,
    #[serde(default, deserialize_with = "default_if_null")]
    name: String,
    #[serde(default, deserialize_with = "default_if_null")]
    asr_biasing: RawAsrBiasing,
    #[serde(default, deserialize_with = "default_if_null")]
    dtmf_config: RawDtmfConfig,
    #[serde(default, deserialize_with = "default_if_null")]
    conditions: Vec<RawCondition>,
    #[serde(default, deserialize_with = "default_if_null")]
    extracted_entities: Vec<String>,
    #[serde(default, deserialize_with = "default_if_null")]
    prompt: String,
    #[serde(default, deserialize_with = "default_if_null")]
    position: Option<RawStepPosition>,
}

impl FlowStepFile {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn prompt(&self) -> &str {
        &self.prompt
    }

    pub(crate) fn step_type_value(&self) -> &str {
        &self.step_type
    }

    pub(crate) fn step_type(&self) -> FlowStepType {
        flow_step_type_from_str(&self.step_type)
    }

    pub(crate) fn extracted_entities(&self) -> &[String] {
        &self.extracted_entities
    }

    pub(crate) fn condition_required_entities(&self) -> Vec<String> {
        self.conditions
            .iter()
            .flat_map(|condition| condition.required_entities.iter().cloned())
            .collect()
    }

    pub(super) fn into_conditions(self) -> Vec<LocalCondition> {
        self.conditions
            .into_iter()
            .map(RawCondition::into_local)
            .collect()
    }

    fn into_local(
        self,
        path: &str,
        resource_name: &str,
        flow_folder: &str,
        prompt_reference_maps: &PromptReferenceMaps,
    ) -> LocalFlowStep {
        let step_type = flow_step_type_from_str(&self.step_type);
        let conditions = self
            .conditions
            .into_iter()
            .map(RawCondition::into_local)
            .collect();
        LocalFlowStep {
            path: path.to_string(),
            name: non_empty(self.name, resource_name),
            step_type,
            prompt: replace_resource_names_with_ids(
                self.prompt.trim(),
                prompt_reference_maps,
                Some(flow_folder),
            ),
            asr_biasing: Some(self.asr_biasing.into()),
            dtmf_config: Some(self.dtmf_config.into()),
            conditions,
            extracted_entities: self.extracted_entities,
            position: self.position.map(Into::into),
        }
    }
}

pub(crate) fn parse_flow_step_file(
    path: &str,
    yaml: &YamlValue,
) -> ResourceParseResult<FlowStepFile> {
    deserialize_yaml(path, yaml)
}

pub(crate) fn parse_flow_step_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<FlowStepFile> {
    let yaml = serde_yaml_ng::from_str::<YamlValue>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    parse_flow_step_file(path, &yaml)
}

pub(super) fn flow_step_type_from_str(value: &str) -> FlowStepType {
    match value {
        "default_step" => FlowStepType::Default,
        _ => FlowStepType::Advanced,
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawAsrBiasing {
    #[serde(default, deserialize_with = "default_if_null")]
    alphanumeric: bool,
    #[serde(default, deserialize_with = "default_if_null")]
    name_spelling: bool,
    #[serde(default, deserialize_with = "default_if_null")]
    numeric: bool,
    #[serde(default, deserialize_with = "default_if_null")]
    party_size: bool,
    #[serde(default, deserialize_with = "default_if_null")]
    precise_date: bool,
    #[serde(default, deserialize_with = "default_if_null")]
    relative_date: bool,
    #[serde(default, deserialize_with = "default_if_null")]
    single_number: bool,
    #[serde(default, deserialize_with = "default_if_null")]
    time: bool,
    #[serde(default, deserialize_with = "default_if_null")]
    yes_no: bool,
    #[serde(default, deserialize_with = "default_if_null")]
    address: bool,
    #[serde(default, deserialize_with = "default_if_null")]
    custom_keywords: Vec<String>,
    #[serde(default, deserialize_with = "default_if_null")]
    is_enabled: bool,
}

impl From<RawAsrBiasing> for StepAsrConfig {
    fn from(config: RawAsrBiasing) -> Self {
        Self {
            alphanumeric: config.alphanumeric,
            name_spelling: config.name_spelling,
            numeric: config.numeric,
            party_size: config.party_size,
            precise_date: config.precise_date,
            relative_date: config.relative_date,
            single_number: config.single_number,
            time: config.time,
            yes_no: config.yes_no,
            address: config.address,
            custom_keywords: config.custom_keywords,
            is_enabled: config.is_enabled,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawDtmfConfig {
    #[serde(default, deserialize_with = "default_if_null")]
    is_enabled: bool,
    #[serde(default, deserialize_with = "default_if_null")]
    inter_digit_timeout: i32,
    #[serde(default, deserialize_with = "default_if_null")]
    max_digits: i32,
    #[serde(default, deserialize_with = "default_if_null")]
    end_key: String,
    #[serde(default, deserialize_with = "default_if_null")]
    collect_while_agent_speaking: bool,
    #[serde(default, deserialize_with = "default_if_null")]
    is_pii: bool,
}

impl From<RawDtmfConfig> for StepDtmfConfig {
    fn from(config: RawDtmfConfig) -> Self {
        Self {
            is_enabled: config.is_enabled,
            inter_digit_timeout: config.inter_digit_timeout,
            max_digits: config.max_digits,
            end_key: non_empty(config.end_key, "#"),
            collect_while_agent_speaking: config.collect_while_agent_speaking,
            is_pii: config.is_pii,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawCondition {
    #[serde(default, deserialize_with = "default_if_null")]
    name: String,
    #[serde(default, deserialize_with = "default_if_null")]
    description: String,
    #[serde(default, deserialize_with = "default_if_null")]
    condition_type: String,
    #[serde(default, deserialize_with = "default_if_null")]
    child_step: String,
    #[serde(default, deserialize_with = "default_if_null")]
    required_entities: Vec<String>,
    #[serde(
        default,
        alias = "ingress_position",
        deserialize_with = "default_if_null"
    )]
    ingress: String,
    #[serde(default, deserialize_with = "default_if_null")]
    position: Option<RawStepPosition>,
    #[serde(default, deserialize_with = "default_if_null")]
    exit_flow_position: Option<RawStepPosition>,
}

impl RawCondition {
    fn into_local(self) -> LocalCondition {
        LocalCondition {
            name: self.name,
            description: self.description,
            condition_type: self.condition_type,
            child_step: self.child_step,
            required_entities: self.required_entities,
            ingress: non_empty(self.ingress, "top"),
            position: self.position.map(Into::into),
            exit_flow_position: self.exit_flow_position.map(Into::into),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawStepPosition {
    #[serde(default, deserialize_with = "default_if_null")]
    x: f32,
    #[serde(default, deserialize_with = "default_if_null")]
    y: f32,
}

impl From<RawStepPosition> for StepPosition {
    fn from(position: RawStepPosition) -> Self {
        Self {
            x: position.x,
            y: position.y,
        }
    }
}

fn resource_content(resource: &Resource) -> &str {
    resource
        .payload
        .get("content")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
}

fn flow_folder_from_path(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    (parts.next()? == "flows").then_some(parts.next()?.to_string())
}

fn function_name_from_path(path: &str) -> String {
    path.rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".py"))
        .unwrap_or(path)
        .to_string()
}

fn non_empty(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flow_step_defaults_without_yaml_traversal() {
        let step = parse_flow_step_content(
            "flows/support/steps/collect.yaml",
            "name: Collect\nprompt: Collect details\n",
        )
        .expect("flow step");

        let local = step.into_local(
            "flows/support/steps/collect.yaml",
            "collect",
            "support",
            &PromptReferenceMaps::default(),
        );

        assert_eq!(local.name, "Collect");
        assert_eq!(local.step_type, FlowStepType::Advanced);
        assert_eq!(local.prompt, "Collect details");
        assert_eq!(local.dtmf_config.expect("dtmf").end_key, "#");
        assert_eq!(
            local.asr_biasing.expect("asr").custom_keywords,
            Vec::<String>::new()
        );
    }

    #[test]
    fn parses_default_step_conditions_with_python_field_aliases() {
        let step = parse_flow_step_content(
            "flows/support/steps/collect.yaml",
            r#"
step_type: default_step
name: Collect
prompt: Collect details
conditions:
  - name: done
    description: Finished
    condition_type: exit_flow_condition
    ingress_position: right
    required_entities:
      - ENTITY-name
    position:
      x: 10.5
      y: 20.5
    exit_flow_position:
      x: 30.5
      y: 40.5
"#,
        )
        .expect("flow step");

        let local = step.into_local(
            "flows/support/steps/collect.yaml",
            "collect",
            "support",
            &PromptReferenceMaps::default(),
        );

        assert_eq!(local.step_type, FlowStepType::Default);
        let condition = local.conditions.first().expect("condition");
        assert_eq!(condition.name, "done");
        assert_eq!(condition.ingress, "right");
        assert_eq!(condition.required_entities, vec!["ENTITY-name"]);
        assert_eq!(condition.position, Some(StepPosition { x: 10.5, y: 20.5 }));
        assert_eq!(
            condition.exit_flow_position,
            Some(StepPosition { x: 30.5, y: 40.5 })
        );
    }
}
