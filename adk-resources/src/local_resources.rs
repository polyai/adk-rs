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

use adk_io::FileSystem;
use serde_yaml::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(crate) use crate::agent_settings::{
    GeneralSafetyFilters, SettingsPersonality, SettingsRole, SettingsRules,
};
pub(crate) use crate::api_integrations::ApiIntegration;
pub(crate) use crate::asr_settings::AsrSettings;
pub(crate) use crate::channels::{
    ChatGreeting, ChatSafetyFilters, ChatStylePrompt, VoiceDisclaimerMessage, VoiceGreeting,
    VoiceSafetyFilters, VoiceStylePrompt,
};
pub(crate) use crate::entities::Entity;
pub(crate) use crate::experimental_config::ExperimentalConfig;
pub(crate) use crate::flows::{FlowConfig, FlowStep, FunctionStep};
pub(crate) use crate::functions::Function;
pub(crate) use crate::handoffs::Handoff;
pub(crate) use crate::keyphrase_boosting::KeyphraseBoosting;
pub(crate) use crate::phrase_filters::PhraseFilter;
pub(crate) use crate::pronunciations::Pronunciation;
pub use crate::resource_lifecycle::{
    DiscoveredResourceChanges, DiscoveredResourcePaths, TypedResourceLifecycle,
    build_typed_resource_lifecycle, empty_discovered_resource_paths, find_new_kept_deleted,
    type_name_to_resource_prefix,
};
pub(crate) use crate::sms_templates::SMSTemplate;
pub(crate) use crate::topics::Topic;
pub(crate) use crate::transcript_corrections::TranscriptCorrection;
pub(crate) use crate::variables::Variable;
pub(crate) use crate::variants::{Variant, VariantAttribute};

pub(crate) fn read_yaml_mapping<Fs: FileSystem>(
    fs: &Fs,
    path: &Path,
) -> Option<serde_yaml::Mapping> {
    let raw = fs.read_to_string(path).ok()?;
    let v: Value = serde_yaml::from_str(&raw).ok()?;
    match v {
        Value::Mapping(m) => Some(m),
        _ => None,
    }
}

pub(crate) fn sorted_read_dir<Fs: FileSystem>(fs: &Fs, dir: &Path) -> Option<Vec<PathBuf>> {
    fs.read_dir(dir).ok()
}

pub(crate) fn is_file<Fs: FileSystem>(fs: &Fs, path: impl AsRef<Path>) -> bool {
    fs.is_file(path.as_ref())
}

pub(crate) fn is_dir<Fs: FileSystem>(fs: &Fs, path: impl AsRef<Path>) -> bool {
    fs.is_dir(path.as_ref())
}

pub(crate) fn validate_named_sequence(
    path: &str,
    yaml: &serde_yaml::Value,
    key: &str,
    label: &str,
    errors: &mut Vec<String>,
) {
    let Some(items) = yaml.get(key).and_then(serde_yaml::Value::as_sequence) else {
        return;
    };
    for (idx, item) in items.iter().enumerate() {
        if item
            .get("name")
            .and_then(serde_yaml::Value::as_str)
            .is_none_or(str::is_empty)
        {
            errors.push(format!(
                "Validation error in {path}/{key}/{idx}: {label} name is required."
            ));
        }
    }
    validate_duplicate_names(path, key, label, items, errors);
}

pub(crate) fn validate_duplicate_names(
    path: &str,
    key: &str,
    label: &str,
    items: &[serde_yaml::Value],
    errors: &mut Vec<String>,
) {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for item in items {
        let Some(name) = item.get("name").and_then(serde_yaml::Value::as_str) else {
            continue;
        };
        if !seen.insert(name.to_string()) {
            duplicates.insert(name.to_string());
        }
    }
    for name in duplicates {
        errors.push(format!(
            "Validation error in {path}/{key}/{name}: duplicate {label} name '{name}'."
        ));
    }
}
