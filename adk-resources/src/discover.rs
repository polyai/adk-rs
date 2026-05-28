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
use adk_io::FileSystem;
use indexmap::IndexMap;
use std::path::Path;

/// Mirrors each Python resource class exposing `discover_resources(base_path)`.
pub(crate) trait DiscoverResources {
    /// Logical paths relative to `base_path`, `/`-separated, matching Python logical paths.
    fn discover_resources<Fs: FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String>;
}

pub struct DiscoverDispatchEntry {
    pub type_name: &'static str,
}

macro_rules! discover_resources {
    ($(($type_name:literal, $resource:ty)),+ $(,)?) => {
        /// Maps each resource type to its discovery order.
        pub const DISCOVER_DISPATCH: &[DiscoverDispatchEntry] = &[
            $(
                DiscoverDispatchEntry { type_name: $type_name },
            )+
        ];

        /// Same iteration order as `RESOURCE_NAME_TO_CLASS` in `poly/project.py`.
        pub fn discover_local_resources<Fs: FileSystem>(
            fs: &Fs,
            root: &Path,
        ) -> DiscoveredResourcePaths {
            let root = fs.canonicalize(root).unwrap_or_else(|_| root.to_path_buf());

            let mut map = IndexMap::new();
            $(
                map.insert(
                    $type_name.to_string(),
                    <$resource as DiscoverResources>::discover_resources(fs, &root),
                );
            )+
            map
        }
    };
}

discover_resources! {
    ("ApiIntegration", ApiIntegration),
    ("Function", Function),
    ("Topic", Topic),
    ("SettingsPersonality", SettingsPersonality),
    ("SettingsRole", SettingsRole),
    ("SettingsRules", SettingsRules),
    ("FlowStep", FlowStep),
    ("FunctionStep", FunctionStep),
    ("FlowConfig", FlowConfig),
    ("Entity", Entity),
    ("ExperimentalConfig", ExperimentalConfig),
    ("GeneralSafetyFilters", GeneralSafetyFilters),
    ("SMSTemplate", SMSTemplate),
    ("Handoff", Handoff),
    ("Variant", Variant),
    ("VariantAttribute", VariantAttribute),
    ("Variable", Variable),
    ("VoiceGreeting", VoiceGreeting),
    ("VoiceSafetyFilters", VoiceSafetyFilters),
    ("VoiceStylePrompt", VoiceStylePrompt),
    ("VoiceDisclaimerMessage", VoiceDisclaimerMessage),
    ("ChatGreeting", ChatGreeting),
    ("ChatSafetyFilters", ChatSafetyFilters),
    ("ChatStylePrompt", ChatStylePrompt),
    ("KeyphraseBoosting", KeyphraseBoosting),
    ("TranscriptCorrection", TranscriptCorrection),
    ("AsrSettings", AsrSettings),
    ("PhraseFilter", PhraseFilter),
    ("Pronunciation", Pronunciation),
}
