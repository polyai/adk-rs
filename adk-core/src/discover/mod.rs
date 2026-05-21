//! Typed local resource discovery aligned with `poly/project.py` `discover_local_resources`
//! and each resource class `discover_resources(base_path)`.

pub(crate) mod resource_utils;

pub use resource_utils::{clean_name, extract_variable_names_from_code};

use crate::resources::DiscoveredResourcePaths;
use crate::resources::{
    ApiIntegration, AsrSettings, ChatGreeting, ChatSafetyFilters, ChatStylePrompt, Entity,
    ExperimentalConfig, FlowConfig, FlowStep, Function, FunctionStep, GeneralSafetyFilters,
    Handoff, KeyphraseBoosting, PhraseFilter, Pronunciation, SMSTemplate, SettingsPersonality,
    SettingsRole, SettingsRules, Topic, TranscriptCorrection, Variable, Variant, VariantAttribute,
    VoiceDisclaimerMessage, VoiceGreeting, VoiceSafetyFilters, VoiceStylePrompt,
};
use adk_io::{FileSystem, StdFileSystem};
use adk_types::{ORDERED_TYPE_NAMES, RESOURCE_TYPE_REGISTRY, ResourceTypeDescriptor};
use indexmap::IndexMap;
use std::path::Path;

/// Mirrors each Python resource class exposing `discover_resources(base_path)`.
pub trait DiscoverResources {
    /// Python class name (e.g. `Topic`, `Entity`).
    const TYPE_NAME: &'static str;
    /// Logical paths relative to `base_path`, `/`-separated, matching Python logical paths.
    fn discover_resources(base_path: &Path) -> Vec<String>;
}

pub use adk_types::ResourceTypeDescriptor as ResourceTypeMetadata;

pub type DiscoverFn = fn(&Path) -> Vec<String>;

/// Maps each resource type to its discovery function.
pub const DISCOVER_DISPATCH: &[(&str, DiscoverFn)] = &[
    (
        "ApiIntegration",
        ApiIntegration::discover_resources as DiscoverFn,
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

pub fn resource_type_metadata() -> &'static [ResourceTypeDescriptor] {
    RESOURCE_TYPE_REGISTRY
}

/// Ordered Python class names in `RESOURCE_NAME_TO_CLASS` order.
pub fn ordered_type_names() -> &'static [&'static str] {
    &ORDERED_TYPE_NAMES
}

/// Python status JSON key (RESOURCE_NAME_TO_CLASS key) -> class name.
pub fn resource_name_to_type_name(resource_name: &str) -> Option<&'static str> {
    adk_types::descriptor_by_status_name(resource_name).map(|d| d.type_name)
}

pub fn type_name_to_resource_name(type_name: &str) -> Option<&'static str> {
    adk_types::descriptor_by_type_name(type_name).map(|d| d.status_resource_name)
}

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
