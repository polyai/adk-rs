//! Typed local resource discovery aligned with `poly/project.py` `discover_local_resources`
//! and each resource class `discover_resources(base_path)`.

use crate::local_resources::DiscoveredResourcePaths;
use crate::local_resources::{
    AdditionalLanguage, ApiIntegration, AsrSettings, ChatGreeting, ChatSafetyFilters,
    ChatStylePrompt, DefaultLanguage, Entity, ExperimentalConfig, FlowConfig, FlowStep, Function,
    FunctionStep, GeneralSafetyFilters, Handoff, KeyphraseBoosting, PhraseFilter, Pronunciation,
    SMSTemplate, SettingsPersonality, SettingsRole, SettingsRules, Topic, TranscriptCorrection,
    Translation, Variable, Variant, VariantAttribute, VoiceDisclaimerMessage, VoiceGreeting,
    VoiceSafetyFilters, VoiceStylePrompt,
};
use adk_io::FileSystem;
use indexmap::IndexMap;
use serde_yaml_ng::Value;
use std::path::Path;

/// Canonical local storage shape for a resource family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalResourcePath {
    File(&'static str),
    Directory(&'static str),
    GlobSet(&'static [&'static str]),
    InFile {
        path: &'static str,
        yaml_path: &'static [&'static str],
    },
    Inferred {
        logical_prefix: &'static str,
        source_patterns: &'static [&'static str],
    },
}

impl LocalResourcePath {
    pub(crate) fn primary_path(self) -> Option<&'static str> {
        match self {
            Self::File(path) | Self::Directory(path) | Self::InFile { path, .. } => Some(path),
            Self::GlobSet(_) | Self::Inferred { .. } => None,
        }
    }

    fn owns_yaml_path(self, path: &str) -> bool {
        match self {
            Self::File(file_path)
            | Self::InFile {
                path: file_path, ..
            } => path == file_path,
            Self::Directory(dir_path) => path
                .strip_prefix(dir_path)
                .is_some_and(|suffix| suffix.starts_with('/')),
            Self::GlobSet(_) | Self::Inferred { .. } => false,
        }
    }
}

/// Mirrors each Python resource class exposing `discover_resources(base_path)`.
pub(crate) trait DiscoverResources {
    const LOCAL_PATH: LocalResourcePath;

    /// Logical paths relative to `base_path`, `/`-separated, matching Python logical paths.
    fn discover_resources<Fs: FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String>;

    /// Append errors for resources owned by this discovery entry.
    ///
    /// For resource-scoped rules, prefer delegating to `ParseLocalResource` so
    /// YAML is parsed into typed values before invariants are checked. Keep
    /// broad reference checks in `adk-core`.
    fn append_local_resource_errors(_path: &str, _yaml: &Value, _errors: &mut Vec<String>) {}
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

        pub fn append_semantic_resource_errors(
            path: &str,
            yaml: &Value,
            errors: &mut Vec<String>,
        ) {
            $(
                if <$resource as DiscoverResources>::LOCAL_PATH.owns_yaml_path(path) {
                    <$resource as DiscoverResources>::append_local_resource_errors(path, yaml, errors);
                }
            )+
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
    ("Translation", Translation),
    ("DefaultLanguage", DefaultLanguage),
    ("AdditionalLanguage", AdditionalLanguage),
}
