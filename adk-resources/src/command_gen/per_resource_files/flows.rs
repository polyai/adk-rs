//! Push command generation for flow config, advanced/default steps, and function steps.

mod models;
mod parsing;
mod summary;

pub(crate) use summary::payload_json_summary;

use self::models::{
    FlowStepType, LocalCondition, LocalFlow, LocalFlowStep, LocalTransitionFunction, RemoteFlow,
    RemoteTransitionFunction,
};
use self::parsing::{
    default_dtmf_config, default_step_position, function_step_latency_control, local_flows,
    ordered_flow_steps, ordered_function_steps, ordered_transition_functions, remote_flows_by_name,
};
use super::super::CommandGroups;
use super::functions::{
    function_errors_update_from_projection, function_parameters_update_from_projection,
    function_update_latency_control, infer_function_parameters, latency_control_from_projection,
    local_latency_control_from_code, python_function_symbol, variable_reference_ids_from_code,
};
use crate::ids::stable_resource_id;
use crate::{
    flow_import_path_maps_from_projection, prompt_reference_maps_from_projection, push_command,
};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::flows::{
    ConditionDetails, CreateAdvancedStep, CreateFunctionStep, CreateFunctionStepDefinition,
    CreateNoCodeCondition, CreateNoCodeStep, DeleteNoCodeCondition, DeleteStep, ExitFlowCondition,
    FlowCreateFlow, FlowCreateTransitionFunction, FlowDeleteFlow, FlowDeleteTransitionFunction,
    FlowUpdateFlow, FlowUpdateStep, FlowUpdateStepAsrConfig, FlowUpdateStepDtmfConfig,
    FlowUpdateTransitionFunction, FlowUpdateTransitionFunctionLatencyControl, NoCodeStepReferences,
    StepAsrConfig, StepAsrConfigUpdate, StepDtmfConfig, StepDtmfConfigUpdate, StepPosition,
    StepReferences, TransitionFunctionCreateTransitionFunction, TransitionFunctionReferences,
    TransitionFunctionUpdateTransitionFunction, UpdateAdvancedStep, UpdateAsrKeywords,
    UpdateFunctionStep, UpdateFunctionStepDefinition, UpdateNoCodeCondition, UpdateNoCodeStep,
    create_no_code_condition, create_step, update_no_code_condition, update_step,
};
use adk_protobuf::{Command, Metadata};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(crate) fn flow_resource_command_groups(
    resources: &ResourceMap,
    projection: &Value,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let mut groups = CommandGroups::default();
    let remote_flows = remote_flows_by_name(projection);
    let flow_import_path_maps = flow_import_path_maps_from_projection(projection);
    let prompt_reference_maps = prompt_reference_maps_from_projection(projection);
    let local_flows = local_flows(resources, &prompt_reference_maps, &flow_import_path_maps);
    let local_flow_names = local_flows
        .iter()
        .map(|flow| flow.name.clone())
        .collect::<HashSet<_>>();
    for remote in remote_flows.values() {
        if !local_flow_names.contains(&remote.name) {
            push_command(
                &mut groups.deletes,
                metadata,
                "delete_flow",
                CommandPayload::DeleteFlow(FlowDeleteFlow {
                    flow_id: remote.id.clone(),
                }),
            );
        }
    }
    for flow in local_flows {
        if let Some(remote) = remote_flows.get(&flow.name) {
            update_flow_commands(&mut groups, &flow, remote, metadata);
        } else {
            create_flow_commands(&mut groups.creates, &flow, metadata);
        }
    }

    groups
}

fn create_flow_commands(out: &mut Vec<Command>, flow: &LocalFlow, metadata: &Option<Metadata>) {
    let flow_id = stable_resource_id("FLOW_CONFIG", &flow.name, &flow.config_path);
    let mut step_ids = HashMap::new();
    let mut advanced_steps = Vec::new();
    let mut no_code_steps = Vec::new();

    let ordered_steps = ordered_flow_steps(flow);
    let function_steps = ordered_function_steps(flow);
    let transition_functions = ordered_transition_functions(flow);

    for (index, step) in ordered_steps.iter().enumerate() {
        let step_id = stable_resource_id("FLOW_STEPS", &step.name, &step.path);
        step_ids.insert(step.name.clone(), step_id.clone());
        let position = Some(
            step.position
                .unwrap_or_else(|| default_step_position(index)),
        );
        match step.step_type {
            FlowStepType::Advanced => advanced_steps.push(CreateAdvancedStep {
                id: step_id,
                name: step.name.clone(),
                prompt: step.prompt.clone(),
                position,
                references: Some(StepReferences::default()),
                asr_biasing: Some(step.asr_biasing.clone().unwrap_or_default()),
                dtmf_config: Some(step.dtmf_config.clone().unwrap_or_else(default_dtmf_config)),
            }),
            FlowStepType::Default => no_code_steps.push(CreateNoCodeStep {
                flow_id: flow_id.clone(),
                step_id,
                name: step.name.clone(),
                prompt: step.prompt.clone(),
                position,
                references: Some(no_code_step_references(step)),
            }),
        }
    }

    let start_step_id = step_ids
        .get(&flow.start_step)
        .cloned()
        .or_else(|| advanced_steps.first().map(|step| step.id.clone()))
        .or_else(|| no_code_steps.first().map(|step| step.step_id.clone()))
        .unwrap_or_default();

    push_command(
        out,
        metadata,
        "create_flow",
        CommandPayload::CreateFlow(FlowCreateFlow {
            id: flow_id.clone(),
            name: flow.name.clone(),
            description: flow.description.clone(),
            start_step_id,
            steps: advanced_steps,
            transition_functions: transition_functions
                .iter()
                .map(|function| transition_function_create_payload(function, None, &Value::Null))
                .collect(),
            no_code_steps,
        }),
    );

    let next_index = ordered_steps.len();
    for (offset, step) in function_steps.iter().enumerate() {
        let step_id = stable_resource_id("FUNCTION_STEPS", &step.name, &step.path);
        let function_id = stable_resource_id("FUNCTION", &step.name, &step.path);
        push_command(
            out,
            metadata,
            "create_step",
            CommandPayload::CreateStep(adk_protobuf::flows::CreateStep {
                flow_id: flow_id.clone(),
                payload: Some(create_step::Payload::FunctionStep(CreateFunctionStep {
                    id: step_id,
                    name: step.name.clone(),
                    position: Some(
                        step.position
                            .unwrap_or_else(|| default_step_position(next_index + offset)),
                    ),
                    function: Some(CreateFunctionStepDefinition {
                        id: function_id,
                        name: step.name.clone(),
                        code: step.code.clone(),
                        errors: Vec::new(),
                        latency_control: Some(function_step_latency_control(step, None)),
                    }),
                })),
            }),
        );
    }

    for step in ordered_steps
        .iter()
        .filter(|step| step.step_type == FlowStepType::Default)
    {
        let Some(step_id) = step_ids.get(&step.name) else {
            continue;
        };
        let step_x = step
            .position
            .unwrap_or_else(|| {
                let index = ordered_steps
                    .iter()
                    .position(|candidate| candidate.name == step.name)
                    .unwrap_or_default();
                default_step_position(index)
            })
            .x;
        for condition in &step.conditions {
            if condition.condition_type != "exit_flow_condition" {
                continue;
            }
            let condition_id = stable_resource_id("CONDITION", &condition.name, &step.path);
            push_command(
                out,
                metadata,
                "create_no_code_condition",
                CommandPayload::CreateNoCodeCondition(CreateNoCodeCondition {
                    flow_id: flow_id.clone(),
                    step_id: step_id.clone(),
                    condition_id,
                    config: Some(create_no_code_condition::Config::ExitFlowCondition(
                        ExitFlowCondition {
                            details: Some(ConditionDetails {
                                label: condition.name.clone(),
                                description: Some(condition.description.clone()),
                                required_entities: condition.required_entities.clone(),
                                position: Some(condition.position.unwrap_or(StepPosition {
                                    x: step_x,
                                    y: 250.0,
                                })),
                                ingress_position: condition.ingress.clone(),
                            }),
                            exit_flow_position: Some(condition.exit_flow_position.unwrap_or(
                                StepPosition {
                                    x: step_x,
                                    y: 500.0,
                                },
                            )),
                        },
                    )),
                }),
            );
        }
    }
}

fn transition_function_create_payload(
    function: &LocalTransitionFunction,
    id_override: Option<String>,
    projection: &Value,
) -> TransitionFunctionCreateTransitionFunction {
    let function_symbol = python_function_symbol(&function.content, &function.name);
    let parameters = infer_function_parameters(&function.code, &function_symbol);
    TransitionFunctionCreateTransitionFunction {
        id: id_override.unwrap_or_else(|| {
            stable_resource_id("FLOW_TRANSITION_FUNCTIONS", &function.name, &function.path)
        }),
        name: function.name.clone(),
        description: function.description.clone(),
        parameters,
        code: function.code.clone(),
        errors: Vec::new(),
        latency_control: None,
        references: Some(TransitionFunctionReferences {
            flow_steps: HashMap::new(),
            variables: variable_reference_ids_from_code(&function.code, projection),
        }),
        archived: Some(false),
    }
}

fn transition_function_changed(
    local: &LocalTransitionFunction,
    remote: &RemoteTransitionFunction,
) -> bool {
    local.code != remote.code
        || (!local.description.is_empty() && local.description != remote.description)
        || local.name != remote.name
}

fn update_flow_commands(
    groups: &mut CommandGroups,
    flow: &LocalFlow,
    remote: &RemoteFlow,
    metadata: &Option<Metadata>,
) {
    let ordered_steps = ordered_flow_steps(flow);
    let local_step_names = ordered_steps
        .iter()
        .map(|step| step.name.clone())
        .collect::<HashSet<_>>();

    for remote_step in remote.steps_by_name.values() {
        if !local_step_names.contains(&remote_step.name) {
            match remote_step.step_type {
                FlowStepType::Advanced => push_command(
                    &mut groups.deletes,
                    metadata,
                    "delete_flow_step",
                    CommandPayload::DeleteFlowStep(adk_protobuf::flows::FlowDeleteStep {
                        flow_id: remote.id.clone(),
                        step_id: remote_step.id.clone(),
                    }),
                ),
                FlowStepType::Default => push_command(
                    &mut groups.deletes,
                    metadata,
                    "delete_no_code_step",
                    CommandPayload::DeleteNoCodeStep(adk_protobuf::flows::DeleteNoCodeStep {
                        flow_id: remote.id.clone(),
                        step_id: remote_step.id.clone(),
                    }),
                ),
            }
        }
    }

    for step in &ordered_steps {
        let Some(remote_step) = remote.steps_by_name.get(&step.name) else {
            continue;
        };
        match step.step_type {
            FlowStepType::Advanced => {
                if step.prompt != remote_step.prompt || step.name != remote_step.name {
                    push_command(
                        &mut groups.updates,
                        metadata,
                        "update_flow_step",
                        CommandPayload::UpdateFlowStep(FlowUpdateStep {
                            flow_id: remote.id.clone(),
                            step: Some(UpdateAdvancedStep {
                                id: remote_step.id.clone(),
                                name: Some(step.name.clone()),
                                prompt: Some(step.prompt.clone()),
                                references: Some(StepReferences::default()),
                            }),
                        }),
                    );
                }
            }
            FlowStepType::Default => {
                let local_condition_names = step
                    .conditions
                    .iter()
                    .map(|condition| condition.name.clone())
                    .collect::<HashSet<_>>();
                let remote_condition_names = remote_step
                    .conditions_by_name
                    .keys()
                    .cloned()
                    .collect::<HashSet<_>>();
                if step.prompt != remote_step.prompt
                    || step.name != remote_step.name
                    || step.extracted_entities != remote_step.extracted_entities
                    || local_condition_names != remote_condition_names
                {
                    push_command(
                        &mut groups.updates,
                        metadata,
                        "update_no_code_step",
                        CommandPayload::UpdateNoCodeStep(UpdateNoCodeStep {
                            flow_id: remote.id.clone(),
                            step_id: remote_step.id.clone(),
                            name: Some(step.name.clone()),
                            prompt: Some(step.prompt.clone()),
                            position: None,
                            references: Some(no_code_step_references(step)),
                        }),
                    );
                }
            }
        }
    }

    let start_step_id = local_start_step_id(flow, remote);
    if flow.name != remote.name
        || flow.description != remote.description
        || start_step_id != remote.start_step_id
    {
        push_command(
            &mut groups.updates,
            metadata,
            "update_flow",
            CommandPayload::UpdateFlow(FlowUpdateFlow {
                flow_id: remote.id.clone(),
                name: Some(flow.name.clone()),
                description: Some(flow.description.clone()),
                start_step_id: Some(start_step_id),
                old_flow_name: None,
            }),
        );
    }

    for step in &ordered_steps {
        let Some(remote_step) = remote.steps_by_name.get(&step.name) else {
            continue;
        };
        if step.step_type == FlowStepType::Advanced {
            let local_asr = step.asr_biasing.clone().unwrap_or_default();
            if local_asr != remote_step.asr_biasing {
                push_command(
                    &mut groups.updates,
                    metadata,
                    "update_flow_step_asr_config",
                    CommandPayload::UpdateFlowStepAsrConfig(FlowUpdateStepAsrConfig {
                        flow_id: remote.id.clone(),
                        step_id: remote_step.id.clone(),
                        asr_biasing: Some(step_asr_config_update(&local_asr)),
                    }),
                );
            }
            let local_dtmf = step.dtmf_config.clone().unwrap_or_else(default_dtmf_config);
            if local_dtmf != remote_step.dtmf_config {
                push_command(
                    &mut groups.updates,
                    metadata,
                    "update_flow_step_dtmf_config",
                    CommandPayload::UpdateFlowStepDtmfConfig(FlowUpdateStepDtmfConfig {
                        flow_id: remote.id.clone(),
                        step_id: remote_step.id.clone(),
                        dtmf_config: Some(step_dtmf_config_update(&local_dtmf)),
                    }),
                );
            }
        }

        if step.step_type == FlowStepType::Default {
            let local_condition_names = step
                .conditions
                .iter()
                .map(|condition| condition.name.clone())
                .collect::<HashSet<_>>();
            for remote_condition in remote_step.conditions_by_name.values() {
                if !local_condition_names.contains(&remote_condition.condition.name) {
                    push_command(
                        &mut groups.deletes,
                        metadata,
                        "delete_no_code_condition",
                        CommandPayload::DeleteNoCodeCondition(DeleteNoCodeCondition {
                            flow_id: remote.id.clone(),
                            step_id: remote_step.id.clone(),
                            condition_id: remote_condition.id.clone(),
                        }),
                    );
                }
            }
            for condition in &step.conditions {
                let Some(remote_condition) = remote_step.conditions_by_name.get(&condition.name)
                else {
                    continue;
                };
                let merged =
                    condition_with_remote_positions(condition, &remote_condition.condition);
                if merged.description != remote_condition.condition.description
                    || merged.required_entities != remote_condition.condition.required_entities
                    || merged.ingress != remote_condition.condition.ingress
                    || merged.position != remote_condition.condition.position
                    || merged.exit_flow_position != remote_condition.condition.exit_flow_position
                {
                    push_command(
                        &mut groups.updates,
                        metadata,
                        "update_no_code_condition",
                        CommandPayload::UpdateNoCodeCondition(UpdateNoCodeCondition {
                            flow_id: remote.id.clone(),
                            step_id: remote_step.id.clone(),
                            condition_id: remote_condition.id.clone(),
                            config: Some(update_no_code_condition::Config::ExitFlowCondition(
                                exit_flow_condition_from_local(&merged),
                            )),
                        }),
                    );
                }
            }
        }
    }

    let local_function_steps = flow
        .function_steps
        .iter()
        .map(|step| (step.name.clone(), step))
        .collect::<HashMap<_, _>>();

    for remote_step in remote.function_steps_by_name.values() {
        if !local_function_steps.contains_key(&remote_step.name) {
            push_command(
                &mut groups.deletes,
                metadata,
                "delete_step",
                CommandPayload::DeleteStep(DeleteStep {
                    flow_id: remote.id.clone(),
                    step_id: remote_step.id.clone(),
                }),
            );
        }
    }

    let next_position = next_function_step_position(remote);
    for step in ordered_function_steps(flow) {
        if let Some(remote_step) = remote.function_steps_by_name.get(&step.name) {
            if step.code != remote_step.code {
                push_command(
                    &mut groups.updates,
                    metadata,
                    "update_step",
                    CommandPayload::UpdateStep(adk_protobuf::flows::UpdateStep {
                        flow_id: remote.id.clone(),
                        step_id: remote_step.id.clone(),
                        payload: Some(update_step::Payload::FunctionStep(UpdateFunctionStep {
                            name: Some(step.name.clone()),
                            position: remote_step.position,
                            function: Some(UpdateFunctionStepDefinition {
                                description: None,
                                code: Some(step.code.clone()),
                                errors: None,
                                archived: None,
                                latency_control: Some(function_step_latency_control(
                                    step,
                                    Some(&remote_step.function),
                                )),
                            }),
                        })),
                    }),
                );
            }
            continue;
        }

        let step_id = stable_resource_id("FUNCTION_STEPS", &step.name, &step.path);
        let function_id = stable_resource_id("FUNCTION", &step.name, &step.path);
        push_command(
            &mut groups.creates,
            metadata,
            "create_step",
            CommandPayload::CreateStep(adk_protobuf::flows::CreateStep {
                flow_id: remote.id.clone(),
                payload: Some(create_step::Payload::FunctionStep(CreateFunctionStep {
                    id: step_id,
                    name: step.name.clone(),
                    position: Some(step.position.unwrap_or(next_position)),
                    function: Some(CreateFunctionStepDefinition {
                        id: function_id,
                        name: step.name.clone(),
                        code: step.code.clone(),
                        errors: Vec::new(),
                        latency_control: Some(function_step_latency_control(step, None)),
                    }),
                })),
            }),
        );
    }

    let local_transition_functions = flow
        .transition_functions
        .iter()
        .map(|function| (function.name.clone(), function))
        .collect::<HashMap<_, _>>();

    for remote_function in remote.transition_functions_by_name.values() {
        if !local_transition_functions.contains_key(&remote_function.name) {
            push_command(
                &mut groups.deletes,
                metadata,
                "delete_flow_transition_function",
                CommandPayload::DeleteFlowTransitionFunction(FlowDeleteTransitionFunction {
                    flow_id: remote.id.clone(),
                    function_id: remote_function.id.clone(),
                }),
            );
        }
    }

    for function in ordered_transition_functions(flow) {
        if let Some(remote_function) = remote.transition_functions_by_name.get(&function.name) {
            if transition_function_changed(function, remote_function) {
                let parameters = function_parameters_update_from_projection(&remote_function.raw)
                    .or_else(|| {
                        let function_symbol =
                            python_function_symbol(&function.content, &function.name);
                        let params = infer_function_parameters(&function.code, &function_symbol);
                        (!params.is_empty()).then_some(adk_protobuf::functions::ParametersUpdate {
                            parameters: params,
                        })
                    });
                push_command(
                    &mut groups.updates,
                    metadata,
                    "update_flow_transition_function",
                    CommandPayload::UpdateFlowTransitionFunction(FlowUpdateTransitionFunction {
                        flow_id: remote.id.clone(),
                        transition_function: Some(TransitionFunctionUpdateTransitionFunction {
                            id: remote_function.id.clone(),
                            name: Some(function.name.clone()),
                            description: Some(function.description.clone()),
                            parameters,
                            code: Some(function.code.clone()),
                            errors: function_errors_update_from_projection(&remote_function.raw),
                            references: None,
                        }),
                    }),
                );
            }
            let local_latency =
                local_latency_control_from_code(&function.content, Some(&remote_function.raw));
            let remote_latency = latency_control_from_projection(&remote_function.raw);
            if local_latency != remote_latency {
                push_command(
                    &mut groups.post_updates,
                    metadata,
                    "update_flow_transition_function_latency_control",
                    CommandPayload::UpdateFlowTransitionFunctionLatencyControl(
                        FlowUpdateTransitionFunctionLatencyControl {
                            flow_id: remote.id.clone(),
                            latency_control: Some(function_update_latency_control(
                                &remote_function.id,
                                &local_latency,
                            )),
                        },
                    ),
                );
            }
            continue;
        }

        let function_id =
            stable_resource_id("FLOW_TRANSITION_FUNCTIONS", &function.name, &function.path);
        push_command(
            &mut groups.creates,
            metadata,
            "create_flow_transition_function",
            CommandPayload::CreateFlowTransitionFunction(FlowCreateTransitionFunction {
                flow_id: remote.id.clone(),
                transition_function: Some(transition_function_create_payload(
                    function,
                    Some(function_id.clone()),
                    &serde_json::Value::Null,
                )),
            }),
        );
        let local_latency = local_latency_control_from_code(&function.content, None);
        if local_latency.enabled {
            push_command(
                &mut groups.post_updates,
                metadata,
                "update_flow_transition_function_latency_control",
                CommandPayload::UpdateFlowTransitionFunctionLatencyControl(
                    FlowUpdateTransitionFunctionLatencyControl {
                        flow_id: remote.id.clone(),
                        latency_control: Some(function_update_latency_control(
                            &function_id,
                            &local_latency,
                        )),
                    },
                ),
            );
        }
    }
}

fn local_start_step_id(flow: &LocalFlow, remote: &RemoteFlow) -> String {
    if flow.start_step == remote.start_step_id {
        return flow.start_step.clone();
    }
    remote
        .steps_by_name
        .get(&flow.start_step)
        .map(|step| step.id.clone())
        .unwrap_or_else(|| flow.start_step.clone())
}

fn next_function_step_position(remote: &RemoteFlow) -> StepPosition {
    let max_x = remote
        .steps_by_name
        .values()
        .filter_map(|step| step.position)
        .chain(
            remote
                .function_steps_by_name
                .values()
                .filter_map(|step| step.position),
        )
        .map(|position| position.x)
        .fold(500.0_f32, f32::max);
    StepPosition {
        x: max_x + 400.0,
        y: 0.0,
    }
}

fn condition_with_remote_positions(
    local: &LocalCondition,
    remote: &LocalCondition,
) -> LocalCondition {
    let mut merged = local.clone();
    if merged.position.is_none() {
        merged.position = remote.position;
    }
    if merged.exit_flow_position.is_none() {
        merged.exit_flow_position = remote.exit_flow_position;
    }
    if merged.ingress.is_empty() {
        merged.ingress = remote.ingress.clone();
    }
    merged
}

fn exit_flow_condition_from_local(condition: &LocalCondition) -> ExitFlowCondition {
    ExitFlowCondition {
        details: Some(ConditionDetails {
            label: condition.name.clone(),
            description: Some(condition.description.clone()),
            required_entities: condition.required_entities.clone(),
            position: condition.position,
            ingress_position: condition.ingress.clone(),
        }),
        exit_flow_position: condition.exit_flow_position,
    }
}

fn no_code_step_references(step: &LocalFlowStep) -> NoCodeStepReferences {
    NoCodeStepReferences {
        extracted_entities: step
            .extracted_entities
            .iter()
            .map(|entity| (entity.clone(), true))
            .collect(),
        ..NoCodeStepReferences::default()
    }
}

fn step_asr_config_update(config: &StepAsrConfig) -> StepAsrConfigUpdate {
    StepAsrConfigUpdate {
        alphanumeric: Some(config.alphanumeric),
        name_spelling: Some(config.name_spelling),
        numeric: Some(config.numeric),
        party_size: Some(config.party_size),
        precise_date: Some(config.precise_date),
        relative_date: Some(config.relative_date),
        single_number: Some(config.single_number),
        time: Some(config.time),
        yes_no: Some(config.yes_no),
        address: Some(config.address),
        custom_keywords: Some(UpdateAsrKeywords {
            custom_keywords: config.custom_keywords.clone(),
        }),
        is_enabled: Some(config.is_enabled),
    }
}

fn step_dtmf_config_update(config: &StepDtmfConfig) -> StepDtmfConfigUpdate {
    StepDtmfConfigUpdate {
        is_enabled: Some(config.is_enabled),
        inter_digit_timeout: Some(config.inter_digit_timeout),
        max_digits: Some(config.max_digits),
        end_key: Some(config.end_key.clone()),
        collect_while_agent_speaking: Some(config.collect_while_agent_speaking),
        is_pii: Some(config.is_pii),
    }
}
