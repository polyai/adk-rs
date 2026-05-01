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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceTypeMetadata {
    pub type_name: &'static str,
    pub status_resource_name: &'static str,
}

const RESOURCE_TYPE_METADATA: &[ResourceTypeMetadata] = &[
    ResourceTypeMetadata {
        type_name: "ApiIntegration",
        status_resource_name: "api_integration",
    },
    ResourceTypeMetadata {
        type_name: "Function",
        status_resource_name: "functions",
    },
    ResourceTypeMetadata {
        type_name: "Topic",
        status_resource_name: "topics",
    },
    ResourceTypeMetadata {
        type_name: "SettingsPersonality",
        status_resource_name: "personality",
    },
    ResourceTypeMetadata {
        type_name: "SettingsRole",
        status_resource_name: "role",
    },
    ResourceTypeMetadata {
        type_name: "SettingsRules",
        status_resource_name: "rules",
    },
    ResourceTypeMetadata {
        type_name: "FlowStep",
        status_resource_name: "flow_steps",
    },
    ResourceTypeMetadata {
        type_name: "FunctionStep",
        status_resource_name: "function_steps",
    },
    ResourceTypeMetadata {
        type_name: "FlowConfig",
        status_resource_name: "flow_config",
    },
    ResourceTypeMetadata {
        type_name: "Entity",
        status_resource_name: "entities",
    },
    ResourceTypeMetadata {
        type_name: "ExperimentalConfig",
        status_resource_name: "experimental_config",
    },
    ResourceTypeMetadata {
        type_name: "GeneralSafetyFilters",
        status_resource_name: "safety_filters",
    },
    ResourceTypeMetadata {
        type_name: "SMSTemplate",
        status_resource_name: "sms_templates",
    },
    ResourceTypeMetadata {
        type_name: "Handoff",
        status_resource_name: "handoffs",
    },
    ResourceTypeMetadata {
        type_name: "Variant",
        status_resource_name: "variants",
    },
    ResourceTypeMetadata {
        type_name: "VariantAttribute",
        status_resource_name: "variant_attributes",
    },
    ResourceTypeMetadata {
        type_name: "Variable",
        status_resource_name: "variables",
    },
    ResourceTypeMetadata {
        type_name: "VoiceGreeting",
        status_resource_name: "voice_greeting",
    },
    ResourceTypeMetadata {
        type_name: "VoiceSafetyFilters",
        status_resource_name: "voice_safety_filters",
    },
    ResourceTypeMetadata {
        type_name: "VoiceStylePrompt",
        status_resource_name: "voice_style_prompt",
    },
    ResourceTypeMetadata {
        type_name: "VoiceDisclaimerMessage",
        status_resource_name: "voice_disclaimer",
    },
    ResourceTypeMetadata {
        type_name: "ChatGreeting",
        status_resource_name: "chat_greeting",
    },
    ResourceTypeMetadata {
        type_name: "ChatSafetyFilters",
        status_resource_name: "chat_safety_filters",
    },
    ResourceTypeMetadata {
        type_name: "ChatStylePrompt",
        status_resource_name: "chat_style_prompt",
    },
    ResourceTypeMetadata {
        type_name: "KeyphraseBoosting",
        status_resource_name: "keyphrase_boosting",
    },
    ResourceTypeMetadata {
        type_name: "TranscriptCorrection",
        status_resource_name: "transcript_corrections",
    },
    ResourceTypeMetadata {
        type_name: "AsrSettings",
        status_resource_name: "asr_settings",
    },
    ResourceTypeMetadata {
        type_name: "PhraseFilter",
        status_resource_name: "phrase_filtering",
    },
    ResourceTypeMetadata {
        type_name: "Pronunciation",
        status_resource_name: "pronunciations",
    },
];

pub fn resource_type_metadata() -> &'static [ResourceTypeMetadata] {
    RESOURCE_TYPE_METADATA
}

/// Ordered Python class names in `RESOURCE_NAME_TO_CLASS` order.
pub fn ordered_type_names() -> &'static [&'static str] {
    const ORDERED: &[&str] = &[
        "ApiIntegration",
        "Function",
        "Topic",
        "SettingsPersonality",
        "SettingsRole",
        "SettingsRules",
        "FlowStep",
        "FunctionStep",
        "FlowConfig",
        "Entity",
        "ExperimentalConfig",
        "GeneralSafetyFilters",
        "SMSTemplate",
        "Handoff",
        "Variant",
        "VariantAttribute",
        "Variable",
        "VoiceGreeting",
        "VoiceSafetyFilters",
        "VoiceStylePrompt",
        "VoiceDisclaimerMessage",
        "ChatGreeting",
        "ChatSafetyFilters",
        "ChatStylePrompt",
        "KeyphraseBoosting",
        "TranscriptCorrection",
        "AsrSettings",
        "PhraseFilter",
        "Pronunciation",
    ];
    ORDERED
}

/// Python status JSON key (RESOURCE_NAME_TO_CLASS key) -> class name.
pub fn resource_name_to_type_name(resource_name: &str) -> Option<&'static str> {
    resource_type_metadata()
        .iter()
        .find(|m| m.status_resource_name == resource_name)
        .map(|m| m.type_name)
}

pub fn type_name_to_resource_name(type_name: &str) -> Option<&'static str> {
    resource_type_metadata()
        .iter()
        .find(|m| m.type_name == type_name)
        .map(|m| m.status_resource_name)
}

pub fn empty_discovered_resource_paths() -> DiscoveredResourcePaths {
    let mut out = DiscoveredResourcePaths::new();
    for t in ordered_type_names() {
        out.insert((*t).to_string(), Vec::new());
    }
    out
}

pub fn type_name_to_resource_prefix(type_name: &str) -> Option<&'static str> {
    match type_name {
        "Function" => Some("fn"),
        "Topic" => Some("topic"),
        "Entity" => Some("entity"),
        "FlowConfig" => Some("flow"),
        "FlowStep" => Some("step"),
        "FunctionStep" => Some("step"),
        "Variable" => Some("var"),
        "SMSTemplate" => Some("sms"),
        "Handoff" => Some("ho"),
        "KeyphraseBoosting" => Some("kp"),
        "PhraseFilter" => Some("sk"),
        _ => None,
    }
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
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

    let mut map = IndexMap::new();

    macro_rules! insert {
        ($t:ty) => {
            map.insert(
                <$t as DiscoverResources>::TYPE_NAME.to_string(),
                <$t as DiscoverResources>::discover_resources(&root),
            );
        };
    }

    insert!(ApiIntegration);
    insert!(Function);
    insert!(Topic);
    insert!(SettingsPersonality);
    insert!(SettingsRole);
    insert!(SettingsRules);
    insert!(FlowStep);
    insert!(FunctionStep);
    insert!(FlowConfig);
    insert!(Entity);
    insert!(ExperimentalConfig);
    insert!(GeneralSafetyFilters);
    insert!(SMSTemplate);
    insert!(Handoff);
    insert!(Variant);
    insert!(VariantAttribute);
    insert!(Variable);
    insert!(VoiceGreeting);
    insert!(VoiceSafetyFilters);
    insert!(VoiceStylePrompt);
    insert!(VoiceDisclaimerMessage);
    insert!(ChatGreeting);
    insert!(ChatSafetyFilters);
    insert!(ChatStylePrompt);
    insert!(KeyphraseBoosting);
    insert!(TranscriptCorrection);
    insert!(AsrSettings);
    insert!(PhraseFilter);
    insert!(Pronunciation);

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

        let discovered_set: BTreeSet<String> = discovered.into_iter().collect();
        let existing_set: BTreeSet<String> = existing.into_iter().collect();

        let new_paths: Vec<String> = discovered_set.difference(&existing_set).cloned().collect();
        let kept_paths: Vec<String> = discovered_set
            .intersection(&existing_set)
            .cloned()
            .collect();
        let deleted_paths: Vec<String> =
            existing_set.difference(&discovered_set).cloned().collect();

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
