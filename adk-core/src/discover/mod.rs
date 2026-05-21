//! Typed local resource discovery aligned with `poly/project.py` `discover_local_resources`
//! and each resource class `discover_resources(base_path)`.

use crate::resources::DiscoveredResourcePaths;
use crate::resources::{
    ApiIntegration, AsrSettings, ChatGreeting, ChatSafetyFilters, ChatStylePrompt, Entity,
    ExperimentalConfig, FlowConfig, FlowStep, Function, FunctionStep, GeneralSafetyFilters,
    Handoff, KeyphraseBoosting, PhraseFilter, Pronunciation, SMSTemplate, SettingsPersonality,
    SettingsRole, SettingsRules, Topic, TranscriptCorrection, Variable, Variant, VariantAttribute,
    VoiceDisclaimerMessage, VoiceGreeting, VoiceSafetyFilters, VoiceStylePrompt,
};
use adk_io::{FileSystem, StdFileSystem};
use indexmap::IndexMap;
use std::path::Path;

/// Mirrors each Python resource class exposing `discover_resources(base_path)`.
pub(crate) trait DiscoverResources {
    /// Logical paths relative to `base_path`, `/`-separated, matching Python logical paths.
    fn discover_resources(base_path: &Path) -> Vec<String>;
}

/// Maps each resource type to its discovery function.
pub const DISCOVER_DISPATCH: &[(&str, fn(&Path) -> Vec<String>)] = &[
    (
        "ApiIntegration",
        ApiIntegration::discover_resources as fn(&Path) -> Vec<String>,
    ),
    ("Function", Function::discover_resources),
    ("Topic", Topic::discover_resources),
    (
        "SettingsPersonality",
        SettingsPersonality::discover_resources,
    ),
    ("SettingsRole", SettingsRole::discover_resources),
    ("SettingsRules", SettingsRules::discover_resources),
    ("FlowStep", FlowStep::discover_resources),
    ("FunctionStep", FunctionStep::discover_resources),
    ("FlowConfig", FlowConfig::discover_resources),
    ("Entity", Entity::discover_resources),
    ("ExperimentalConfig", ExperimentalConfig::discover_resources),
    (
        "GeneralSafetyFilters",
        GeneralSafetyFilters::discover_resources,
    ),
    ("SMSTemplate", SMSTemplate::discover_resources),
    ("Handoff", Handoff::discover_resources),
    ("Variant", Variant::discover_resources),
    ("VariantAttribute", VariantAttribute::discover_resources),
    ("Variable", Variable::discover_resources),
    ("VoiceGreeting", VoiceGreeting::discover_resources),
    ("VoiceSafetyFilters", VoiceSafetyFilters::discover_resources),
    ("VoiceStylePrompt", VoiceStylePrompt::discover_resources),
    (
        "VoiceDisclaimerMessage",
        VoiceDisclaimerMessage::discover_resources,
    ),
    ("ChatGreeting", ChatGreeting::discover_resources),
    ("ChatSafetyFilters", ChatSafetyFilters::discover_resources),
    ("ChatStylePrompt", ChatStylePrompt::discover_resources),
    ("KeyphraseBoosting", KeyphraseBoosting::discover_resources),
    (
        "TranscriptCorrection",
        TranscriptCorrection::discover_resources,
    ),
    ("AsrSettings", AsrSettings::discover_resources),
    ("PhraseFilter", PhraseFilter::discover_resources),
    ("Pronunciation", Pronunciation::discover_resources),
];

/// Same iteration order as `RESOURCE_NAME_TO_CLASS` in `poly/project.py`.
pub fn discover_local_resources(root: &Path) -> DiscoveredResourcePaths {
    let root = StdFileSystem
        .canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf());

    let mut map = IndexMap::new();
    for &(type_name, discover_fn) in DISCOVER_DISPATCH {
        map.insert(type_name.to_string(), discover_fn(&root));
    }
    map
}
