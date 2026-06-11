use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::rel_under_root;
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

    fn validate_local_yaml(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_channel_greeting_yaml(path, yaml, errors);
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

    fn validate_local_yaml(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        crate::channels::validate_safety_filters_yaml(path, yaml, true, errors);
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

    fn validate_local_yaml(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_voice_disclaimer_yaml(path, yaml, errors);
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

    fn validate_local_yaml(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_channel_greeting_yaml(path, yaml, errors);
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

    fn validate_local_yaml(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        crate::channels::validate_safety_filters_yaml(path, yaml, true, errors);
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

fn validate_channel_greeting_yaml(path: &str, yaml: &Value, errors: &mut Vec<String>) {
    let Some(greeting) = yaml.get("greeting") else {
        return;
    };
    if greeting
        .get("welcome_message")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        errors.push(format!(
            "Validation error in {path}/greeting/welcome_message: Welcome message cannot be empty."
        ));
    }
    if greeting
        .get("language_code")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        errors.push(format!(
            "Validation error in {path}/greeting/language_code: Language code cannot be empty."
        ));
    }
}

fn validate_voice_disclaimer_yaml(path: &str, yaml: &Value, errors: &mut Vec<String>) {
    let Some(disclaimers) = yaml.get("disclaimer_messages") else {
        return;
    };
    match disclaimers {
        Value::Sequence(items) => {
            for (idx, disclaimer) in items.iter().enumerate() {
                validate_disclaimer_language(path, disclaimer, &idx.to_string(), errors);
            }
        }
        Value::Mapping(_) => validate_disclaimer_language(path, disclaimers, "0", errors),
        _ => {}
    }
}

fn validate_disclaimer_language(
    path: &str,
    disclaimer: &Value,
    key: &str,
    errors: &mut Vec<String>,
) {
    if disclaimer
        .get("language_code")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        errors.push(format!(
            "Validation error in {path}/disclaimer_messages/{key}/language_code: Language code cannot be empty."
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str, validator: fn(&str, &Value, &mut Vec<String>)) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("channel YAML");
        let mut errors = Vec::new();
        validator("voice/configuration.yaml", &yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_channel_greeting_required_fields() {
        let errors = validation_errors(
            r#"
greeting:
  welcome_message: ""
  language_code: ""
"#,
            validate_channel_greeting_yaml,
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
            validate_voice_disclaimer_yaml,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("Language code cannot be empty"))
        );
    }
}
