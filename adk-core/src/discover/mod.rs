//! Typed local resource discovery aligned with `poly/project.py` `discover_local_resources`
//! and each resource class `discover_resources(base_path)`.

mod resource_utils;
mod resources_impl;

pub use resource_utils::{clean_name, extract_variable_names_from_code};

use crate::discover::resources_impl::{
    ApiIntegration, AsrSettings, ChatGreeting, ChatSafetyFilters, ChatStylePrompt, Entity,
    ExperimentalConfig, FlowConfig, FlowStep, Function, FunctionStep, GeneralSafetyFilters,
    Handoff, KeyphraseBoosting, PhraseFilter, Pronunciation, SMSTemplate, SettingsPersonality,
    SettingsRole, SettingsRules, Topic, TranscriptCorrection, Variable, Variant, VariantAttribute,
    VoiceDisclaimerMessage, VoiceGreeting, VoiceSafetyFilters, VoiceStylePrompt,
};
use adk_io::{FileSystem, StdFileSystem};
use adk_types::{RESOURCE_TYPE_REGISTRY, ResourceTypeDescriptor};
use indexmap::{IndexMap, IndexSet};
use std::collections::BTreeSet;
use std::path::Path;

/// Mirrors each Python resource class exposing `discover_resources(base_path)`.
pub trait DiscoverResources {
    /// Python class name (e.g. `Topic`, `Entity`).
    const TYPE_NAME: &'static str;
    /// Logical paths relative to `base_path`, `/`-separated, matching Python logical paths.
    fn discover_resources(base_path: &Path) -> Vec<String>;
}

pub type DiscoveredResourcePaths = IndexMap<String, Vec<String>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredResourceChanges {
    pub new_resources: DiscoveredResourcePaths,
    pub kept_resources: DiscoveredResourcePaths,
    pub deleted_resources: DiscoveredResourcePaths,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedResourceLifecycle {
    pub type_name: String,
    pub file_path: String,
    pub resource_id: String,
    pub resource_prefix: Option<String>,
    pub is_existing: bool,
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
pub fn ordered_type_names() -> Vec<&'static str> {
    RESOURCE_TYPE_REGISTRY.iter().map(|d| d.type_name).collect()
}

/// Python status JSON key (RESOURCE_NAME_TO_CLASS key) -> class name.
pub fn resource_name_to_type_name(resource_name: &str) -> Option<&'static str> {
    adk_types::descriptor_by_status_name(resource_name).map(|d| d.type_name)
}

pub fn type_name_to_resource_name(type_name: &str) -> Option<&'static str> {
    adk_types::descriptor_by_type_name(type_name).map(|d| d.status_resource_name)
}

pub fn empty_discovered_resource_paths() -> DiscoveredResourcePaths {
    let mut out = DiscoveredResourcePaths::new();
    for d in RESOURCE_TYPE_REGISTRY {
        out.insert(d.type_name.to_string(), Vec::new());
    }
    out
}

pub fn type_name_to_resource_prefix(type_name: &str) -> Option<&'static str> {
    adk_types::descriptor_by_type_name(type_name).and_then(|d| d.id_prefix)
}

pub fn build_typed_resource_lifecycle(
    discovered: &DiscoveredResourcePaths,
    existing_resource_ids: &indexmap::IndexMap<String, String>,
) -> Vec<TypedResourceLifecycle> {
    let mut out = Vec::new();
    for (type_name, paths) in discovered {
        let resource_prefix = type_name_to_resource_prefix(type_name).map(str::to_string);
        for path in paths {
            if let Some(existing_id) = existing_resource_ids.get(path) {
                out.push(TypedResourceLifecycle {
                    type_name: type_name.clone(),
                    file_path: path.clone(),
                    resource_id: existing_id.clone(),
                    resource_prefix: resource_prefix.clone(),
                    is_existing: true,
                });
            } else {
                let digest = adk_io::compute_hash(&format!("{type_name}:{path}"));
                let short = &digest[..8];
                let generated_id = resource_prefix
                    .as_ref()
                    .map(|p| format!("{p}-{short}"))
                    .unwrap_or_else(|| format!("{}-{short}", type_name.to_uppercase()));
                out.push(TypedResourceLifecycle {
                    type_name: type_name.clone(),
                    file_path: path.clone(),
                    resource_id: generated_id,
                    resource_prefix: resource_prefix.clone(),
                    is_existing: false,
                });
            }
        }
    }
    out.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    out
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

/// Mirrors Python `AgentStudioProject.find_new_kept_deleted` at a typed path level.
/// Compares logical path lists per resource type and returns new/kept/deleted paths.
pub fn find_new_kept_deleted(
    discovered_resources: &DiscoveredResourcePaths,
    existing_resources: &DiscoveredResourcePaths,
) -> DiscoveredResourceChanges {
    let mut resource_types: IndexSet<String> = IndexSet::new();
    resource_types.extend(discovered_resources.keys().cloned());
    resource_types.extend(existing_resources.keys().cloned());

    let mut new_resources: DiscoveredResourcePaths = IndexMap::new();
    let mut kept_resources: DiscoveredResourcePaths = IndexMap::new();
    let mut deleted_resources: DiscoveredResourcePaths = IndexMap::new();

    for resource_type in resource_types {
        let discovered = discovered_resources
            .get(&resource_type)
            .cloned()
            .unwrap_or_default();
        let existing = existing_resources
            .get(&resource_type)
            .cloned()
            .unwrap_or_default();

        let discovered_set: BTreeSet<String> = discovered.iter().cloned().collect();
        let existing_set: BTreeSet<String> = existing.iter().cloned().collect();

        let new_paths: Vec<String> = discovered
            .iter()
            .filter(|path| !existing_set.contains(*path))
            .cloned()
            .collect();
        let kept_paths: Vec<String> = discovered
            .iter()
            .filter(|path| existing_set.contains(*path))
            .cloned()
            .collect();
        let deleted_paths: Vec<String> = existing
            .iter()
            .filter(|path| !discovered_set.contains(*path))
            .cloned()
            .collect();

        new_resources.insert(resource_type.clone(), new_paths);
        kept_resources.insert(resource_type.clone(), kept_paths);
        deleted_resources.insert(resource_type, deleted_paths);
    }

    DiscoveredResourceChanges {
        new_resources,
        kept_resources,
        deleted_resources,
    }
}
