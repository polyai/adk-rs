//! Resource-specific local discovery implementations.
//!
//! The central resource metadata registry lives in `adk-types`. This module owns
//! per-resource filesystem discovery so the details for a resource can live near
//! its resource-domain code.

mod agent_settings;
mod api_integration;
mod asr_settings;
mod channels;
mod common;
mod entity;
mod experimental_config;
mod flow;
mod function;
mod handoff;
mod keyphrase_boosting;
mod phrase_filter;
mod pronunciation;
mod safety_filters;
mod sms_template;
mod topic;
mod transcript_correction;
mod variable;
mod variant;

pub(crate) use agent_settings::{SettingsPersonality, SettingsRole, SettingsRules};
pub(crate) use api_integration::ApiIntegration;
pub(crate) use asr_settings::AsrSettings;
pub(crate) use channels::{
    ChatGreeting, ChatSafetyFilters, ChatStylePrompt, VoiceDisclaimerMessage, VoiceGreeting,
    VoiceSafetyFilters, VoiceStylePrompt,
};
pub(crate) use entity::Entity;
pub(crate) use experimental_config::ExperimentalConfig;
pub(crate) use flow::{FlowConfig, FlowStep, FunctionStep};
pub(crate) use function::Function;
pub(crate) use handoff::Handoff;
pub(crate) use keyphrase_boosting::KeyphraseBoosting;
pub(crate) use phrase_filter::PhraseFilter;
pub(crate) use pronunciation::Pronunciation;
pub(crate) use safety_filters::GeneralSafetyFilters;
pub(crate) use sms_template::SMSTemplate;
pub(crate) use topic::Topic;
pub(crate) use transcript_correction::TranscriptCorrection;
pub(crate) use variable::Variable;
pub(crate) use variant::{Variant, VariantAttribute};

pub(crate) fn validate_semantic_resource(
    path: &str,
    yaml: &serde_yaml::Value,
    errors: &mut Vec<String>,
) {
    match path {
        "config/api_integrations.yaml" => api_integration::validate_local_yaml(yaml, errors),
        "config/entities.yaml" => entity::validate_local_yaml(yaml, errors),
        "config/handoffs.yaml" => handoff::validate_local_yaml(yaml, errors),
        "config/sms_templates.yaml" => sms_template::validate_local_yaml(yaml, errors),
        "config/variant_attributes.yaml" => variant::validate_local_yaml(yaml, errors),
        "voice/speech_recognition/transcript_corrections.yaml" => {
            transcript_correction::validate_local_yaml(yaml, errors);
        }
        _ if path.starts_with("topics/") => topic::validate_local_yaml(path, yaml, errors),
        _ => {}
    }
}
