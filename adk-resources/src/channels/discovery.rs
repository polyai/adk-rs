use crate::discover::DiscoverResources;
use crate::local_resource_common::{is_file, read_yaml_mapping};
use crate::resource_utils::rel_under_root;
use serde_yaml::Value;
use std::path::Path;

// poly/resources/channel_settings.py
pub(crate) struct VoiceGreeting;
impl DiscoverResources for VoiceGreeting {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let file_path = base_path.join("voice/configuration.yaml");
        if !is_file(&file_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&file_path) else {
            return vec![];
        };
        let greeting = m.get("greeting");
        if greeting.is_none()
            || greeting.is_some_and(|g| matches!(g, Value::Null))
            || greeting.is_some_and(|g| matches!(g, Value::Mapping(mp) if mp.is_empty()))
        {
            return vec![];
        }
        vec![rel_under_root(base_path, &file_path.join("greeting"))]
    }
}

pub(crate) struct VoiceSafetyFilters;
impl DiscoverResources for VoiceSafetyFilters {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("voice/safety_filters.yaml");
        if is_file(&p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

pub(crate) struct VoiceStylePrompt;
impl DiscoverResources for VoiceStylePrompt {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let file_path = base_path.join("voice/configuration.yaml");
        if !is_file(&file_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&file_path) else {
            return vec![];
        };
        let style = m.get("style_prompt");
        if style.is_none()
            || style.is_some_and(|g| matches!(g, Value::Null))
            || style.is_some_and(|g| matches!(g, Value::Mapping(mp) if mp.is_empty()))
        {
            return vec![];
        }
        vec![rel_under_root(base_path, &file_path.join("style_prompt"))]
    }
}

pub(crate) struct VoiceDisclaimerMessage;
impl DiscoverResources for VoiceDisclaimerMessage {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let file_path = base_path.join("voice/configuration.yaml");
        if !is_file(&file_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&file_path) else {
            return vec![];
        };
        let disclaimers = m.get("disclaimer_messages");
        let has_list = disclaimers
            .and_then(|v| v.as_sequence())
            .is_some_and(|s| !s.is_empty());
        let has_dict = disclaimers
            .and_then(|v| v.as_mapping())
            .is_some_and(|d| !d.is_empty());
        if !has_list && !has_dict {
            return vec![];
        }
        vec![rel_under_root(
            base_path,
            &file_path.join("disclaimer_messages"),
        )]
    }
}

pub(crate) struct ChatGreeting;
impl DiscoverResources for ChatGreeting {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let file_path = base_path.join("chat/configuration.yaml");
        if !is_file(&file_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&file_path) else {
            return vec![];
        };
        let greeting = m.get("greeting");
        if greeting.is_none()
            || greeting.is_some_and(|g| matches!(g, Value::Null))
            || greeting.is_some_and(|g| matches!(g, Value::Mapping(mp) if mp.is_empty()))
        {
            return vec![];
        }
        vec![rel_under_root(base_path, &file_path.join("greeting"))]
    }
}

pub(crate) struct ChatSafetyFilters;
impl DiscoverResources for ChatSafetyFilters {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("chat/safety_filters.yaml");
        if is_file(&p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

pub(crate) struct ChatStylePrompt;
impl DiscoverResources for ChatStylePrompt {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let file_path = base_path.join("chat/configuration.yaml");
        if !is_file(&file_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&file_path) else {
            return vec![];
        };
        let style = m.get("style_prompt");
        if style.is_none()
            || style.is_some_and(|g| matches!(g, Value::Null))
            || style.is_some_and(|g| matches!(g, Value::Mapping(mp) if mp.is_empty()))
        {
            return vec![];
        }
        vec![rel_under_root(base_path, &file_path.join("style_prompt"))]
    }
}
