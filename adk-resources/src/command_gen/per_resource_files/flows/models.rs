use adk_protobuf::flows::{StepAsrConfig, StepDtmfConfig, StepPosition};
use serde_json::Value;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FlowStepType {
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
    pub(super) required_entities: Vec<String>,
    pub(super) ingress: String,
    pub(super) position: Option<StepPosition>,
    pub(super) exit_flow_position: Option<StepPosition>,
}

#[derive(Debug, Default)]
pub(super) struct RemoteFlow {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) start_step_id: String,
    pub(super) steps_by_name: HashMap<String, RemoteFlowStep>,
    pub(super) function_steps_by_name: HashMap<String, RemoteFunctionStep>,
    pub(super) transition_functions_by_name: HashMap<String, RemoteTransitionFunction>,
}

#[derive(Debug, Clone)]
pub(super) struct RemoteFlowStep {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) step_type: FlowStepType,
    pub(super) prompt: String,
    pub(super) asr_biasing: StepAsrConfig,
    pub(super) dtmf_config: StepDtmfConfig,
    pub(super) conditions_by_name: HashMap<String, RemoteCondition>,
    pub(super) extracted_entities: Vec<String>,
    pub(super) position: Option<StepPosition>,
}

#[derive(Debug, Clone)]
pub(super) struct RemoteFunctionStep {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) code: String,
    pub(super) function: Value,
    pub(super) position: Option<StepPosition>,
}

#[derive(Debug, Clone)]
pub(super) struct RemoteTransitionFunction {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) code: String,
    pub(super) raw: Value,
}

#[derive(Debug, Clone)]
pub(super) struct RemoteCondition {
    pub(super) id: String,
    pub(super) condition: LocalCondition,
}
