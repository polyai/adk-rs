//! Resource-specific local discovery and validation implementations.
//!
//! Resource type metadata, including Python class names, status resource keys,
//! ID prefixes, and registry order, lives in `adk-types`. This module owns
//! resource/domain behavior for local filesystem discovery, typed resource
//! lifecycle bookkeeping, and validation that can be decided from one resource
//! file or one resource-owned collection file.
//!
//! Cross-resource validation, such as flow step references, entity references,
//! and function call-site checks, remains in `adk-core`. Projection
//! materialization and push command generation live alongside these definitions
//! in this crate.

mod asr_settings;
mod channels;
pub(crate) mod common;
mod lifecycle;

pub(crate) use crate::agent_settings::{
    GeneralSafetyFilters, SettingsPersonality, SettingsRole, SettingsRules,
};
pub(crate) use crate::api_integrations::ApiIntegration;
pub(crate) use crate::entities::Entity;
pub(crate) use crate::experimental_config::ExperimentalConfig;
pub(crate) use crate::flows::{FlowConfig, FlowStep, FunctionStep};
pub(crate) use crate::functions::Function;
pub(crate) use crate::handoffs::Handoff;
pub(crate) use crate::keyphrase_boosting::KeyphraseBoosting;
pub(crate) use crate::phrase_filters::PhraseFilter;
pub(crate) use crate::pronunciations::Pronunciation;
pub(crate) use crate::sms_templates::SMSTemplate;
pub(crate) use crate::topics::Topic;
pub(crate) use crate::transcript_corrections::TranscriptCorrection;
pub(crate) use crate::variables::Variable;
pub(crate) use crate::variants::{Variant, VariantAttribute};
pub(crate) use asr_settings::AsrSettings;
pub(crate) use channels::{
    ChatGreeting, ChatSafetyFilters, ChatStylePrompt, VoiceDisclaimerMessage, VoiceGreeting,
    VoiceSafetyFilters, VoiceStylePrompt,
};
pub use lifecycle::{DiscoveredResourceChanges, DiscoveredResourcePaths, TypedResourceLifecycle};
pub use lifecycle::{
    build_typed_resource_lifecycle, empty_discovered_resource_paths, find_new_kept_deleted,
    type_name_to_resource_prefix,
};

pub fn validate_semantic_resource(path: &str, yaml: &serde_yaml::Value, errors: &mut Vec<String>) {
    match path {
        "config/api_integrations.yaml" => {
            crate::api_integrations::validate_local_yaml(yaml, errors)
        }
        "config/entities.yaml" => crate::entities::validate_local_yaml(yaml, errors),
        "config/handoffs.yaml" => crate::handoffs::validate_local_yaml(yaml, errors),
        "config/sms_templates.yaml" => crate::sms_templates::validate_local_yaml(yaml, errors),
        "config/variant_attributes.yaml" => crate::variants::validate_local_yaml(yaml, errors),
        "voice/speech_recognition/transcript_corrections.yaml" => {
            crate::transcript_corrections::validate_local_yaml(yaml, errors);
        }
        _ if path.starts_with("topics/") => crate::topics::validate_local_yaml(path, yaml, errors),
        _ => {}
    }
}
