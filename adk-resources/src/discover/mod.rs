//! Typed local resource discovery aligned with `poly/project.py` `discover_local_resources`
//! and each resource class `discover_resources(base_path)`.

use crate::local_resources::DiscoveredResourcePaths;
use crate::local_resources::{
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

pub struct DiscoverDispatchEntry {
    pub type_name: &'static str,
    pub discover: fn(&Path) -> Vec<String>,
}

/// Maps each resource type to its discovery function.
pub const DISCOVER_DISPATCH: &[DiscoverDispatchEntry] = &[
    DiscoverDispatchEntry {
        type_name: "ApiIntegration",
        discover: ApiIntegration::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "Function",
        discover: Function::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "Topic",
        discover: Topic::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "SettingsPersonality",
        discover: SettingsPersonality::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "SettingsRole",
        discover: SettingsRole::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "SettingsRules",
        discover: SettingsRules::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "FlowStep",
        discover: FlowStep::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "FunctionStep",
        discover: FunctionStep::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "FlowConfig",
        discover: FlowConfig::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "Entity",
        discover: Entity::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "ExperimentalConfig",
        discover: ExperimentalConfig::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "GeneralSafetyFilters",
        discover: GeneralSafetyFilters::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "SMSTemplate",
        discover: SMSTemplate::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "Handoff",
        discover: Handoff::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "Variant",
        discover: Variant::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "VariantAttribute",
        discover: VariantAttribute::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "Variable",
        discover: Variable::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "VoiceGreeting",
        discover: VoiceGreeting::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "VoiceSafetyFilters",
        discover: VoiceSafetyFilters::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "VoiceStylePrompt",
        discover: VoiceStylePrompt::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "VoiceDisclaimerMessage",
        discover: VoiceDisclaimerMessage::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "ChatGreeting",
        discover: ChatGreeting::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "ChatSafetyFilters",
        discover: ChatSafetyFilters::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "ChatStylePrompt",
        discover: ChatStylePrompt::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "KeyphraseBoosting",
        discover: KeyphraseBoosting::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "TranscriptCorrection",
        discover: TranscriptCorrection::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "AsrSettings",
        discover: AsrSettings::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "PhraseFilter",
        discover: PhraseFilter::discover_resources,
    },
    DiscoverDispatchEntry {
        type_name: "Pronunciation",
        discover: Pronunciation::discover_resources,
    },
];

/// Same iteration order as `RESOURCE_NAME_TO_CLASS` in `poly/project.py`.
pub fn discover_local_resources(root: &Path) -> DiscoveredResourcePaths {
    let root = StdFileSystem
        .canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf());

    let mut map = IndexMap::new();
    for entry in DISCOVER_DISPATCH {
        map.insert(entry.type_name.to_string(), (entry.discover)(&root));
    }
    map
}
