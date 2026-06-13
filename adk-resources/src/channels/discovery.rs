use crate::channels::local::{
    ChannelConfiguration, parse_channel_configuration, validate_channel_greeting,
    validate_voice_disclaimers,
};
use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{ParseLocalResource, ResourceParseResult};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::rel_under_root;
use crate::safety_filters::{SafetyFilterMode, SafetyFilters, parse_safety_filters};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/channel_settings.py
/// Validation parity: TODO(DEVP-319) audit Python VoiceGreeting.validate().
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

    fn append_local_resource_errors(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        if let Err(parse_errors) = validate_channel_greeting(path, yaml) {
            errors.extend(parse_errors.into_validation_errors());
        }
    }
}

impl ParseLocalResource for VoiceGreeting {
    type Parsed = ChannelConfiguration;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_channel_configuration(path, yaml)
    }
}

/// Validation parity: implemented against Python VoiceSafetyFilters.validate().
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

    fn append_local_resource_errors(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        <Self as ParseLocalResource>::append_parse_errors(path, yaml, errors);
    }
}

impl ParseLocalResource for VoiceSafetyFilters {
    type Parsed = SafetyFilters;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_safety_filters(path, yaml, SafetyFilterMode::Channel)
    }
}

/// Validation parity: TODO(DEVP-319) audit Python VoiceStylePrompt.validate().
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

impl ParseLocalResource for VoiceStylePrompt {
    type Parsed = ChannelConfiguration;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_channel_configuration(path, yaml)
    }
}

/// Validation parity: TODO(DEVP-319) audit Python VoiceDisclaimerMessage.validate().
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

    fn append_local_resource_errors(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        if let Err(parse_errors) = validate_voice_disclaimers(path, yaml) {
            errors.extend(parse_errors.into_validation_errors());
        }
    }
}

impl ParseLocalResource for VoiceDisclaimerMessage {
    type Parsed = ChannelConfiguration;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_channel_configuration(path, yaml)
    }
}

/// Validation parity: TODO(DEVP-319) audit Python ChatGreeting.validate().
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

    fn append_local_resource_errors(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        if let Err(parse_errors) = validate_channel_greeting(path, yaml) {
            errors.extend(parse_errors.into_validation_errors());
        }
    }
}

impl ParseLocalResource for ChatGreeting {
    type Parsed = ChannelConfiguration;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_channel_configuration(path, yaml)
    }
}

/// Validation parity: implemented against Python ChatSafetyFilters.validate().
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

    fn append_local_resource_errors(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        <Self as ParseLocalResource>::append_parse_errors(path, yaml, errors);
    }
}

impl ParseLocalResource for ChatSafetyFilters {
    type Parsed = SafetyFilters;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_safety_filters(path, yaml, SafetyFilterMode::Channel)
    }
}

/// Validation parity: TODO(DEVP-319) audit Python ChatStylePrompt.validate().
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

impl ParseLocalResource for ChatStylePrompt {
    type Parsed = ChannelConfiguration;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_channel_configuration(path, yaml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(
        yaml: &str,
        validator: fn(&str, &Value) -> crate::local_parse::ResourceParseResult<()>,
    ) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("channel YAML");
        validator("voice/configuration.yaml", &yaml)
            .err()
            .map(crate::local_parse::ResourceParseErrors::into_validation_errors)
            .unwrap_or_default()
    }

    #[test]
    fn validates_python_channel_greeting_required_fields() {
        let errors = validation_errors(
            r#"
greeting:
  welcome_message: ""
  language_code: ""
"#,
            validate_channel_greeting,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("Welcome message cannot be empty"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Language code cannot be empty"))
        );
    }

    #[test]
    fn validates_python_voice_disclaimer_language_required() {
        let errors = validation_errors(
            r#"
disclaimer_messages:
  message: Hello
  language_code: ""
"#,
            validate_voice_disclaimers,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("Language code cannot be empty"))
        );
    }
}
