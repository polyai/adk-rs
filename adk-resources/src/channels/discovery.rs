use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::rel_under_root;
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/channel_settings.py
pub(crate) struct VoiceGreeting;
impl DiscoverResources for VoiceGreeting {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::VOICE_CONFIGURATION_FILE.file_path,
        yaml_path: &["greeting"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let file_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &file_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &file_path) else {
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
    const LOCAL_PATH: LocalResourcePath =
        LocalResourcePath::File(crate::specs::VOICE_SAFETY_FILTERS_FILE.file_path);

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let p = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if is_file(fs, &p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

pub(crate) struct VoiceStylePrompt;
impl DiscoverResources for VoiceStylePrompt {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::VOICE_CONFIGURATION_FILE.file_path,
        yaml_path: &["style_prompt"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let file_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &file_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &file_path) else {
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
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::VOICE_CONFIGURATION_FILE.file_path,
        yaml_path: &["disclaimer_messages"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let file_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &file_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &file_path) else {
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
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::CHAT_CONFIGURATION_FILE.file_path,
        yaml_path: &["greeting"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let file_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &file_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &file_path) else {
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
    const LOCAL_PATH: LocalResourcePath =
        LocalResourcePath::File(crate::specs::CHAT_SAFETY_FILTERS_FILE.file_path);

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let p = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if is_file(fs, &p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

pub(crate) struct ChatStylePrompt;
impl DiscoverResources for ChatStylePrompt {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::CHAT_CONFIGURATION_FILE.file_path,
        yaml_path: &["style_prompt"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let file_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &file_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &file_path) else {
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
