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
use indexmap::IndexMap;
use std::path::Path;

/// Mirrors each Python resource class exposing `discover_resources(base_path)`.
pub trait DiscoverResources {
    /// Python class name (e.g. `Topic`, `Entity`).
    const TYPE_NAME: &'static str;
    /// Logical paths relative to `base_path`, `/`-separated, matching Python logical paths.
    fn discover_resources(base_path: &Path) -> Vec<String>;
}

/// Same iteration order as `RESOURCE_NAME_TO_CLASS` in `poly/project.py`.
pub fn discover_local_resources(root: &Path) -> IndexMap<String, Vec<String>> {
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
