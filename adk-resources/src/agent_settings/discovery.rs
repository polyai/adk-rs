use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::is_file;
use crate::resource_utils::rel_under_root;
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/agent_settings.py
const ALLOWED_ADJECTIVES: &[&str] = &[
    "Polite",
    "Calm",
    "Kind",
    "Funny",
    "Other",
    "Energetic",
    "Thoughtful",
];

/// Validation parity: TODO(DEVP-319) audit Python SettingsPersonality.validate().
pub(crate) struct SettingsPersonality;
impl DiscoverResources for SettingsPersonality {
    const LOCAL_PATH: LocalResourcePath =
        LocalResourcePath::File(crate::specs::AGENT_PERSONALITY_FILE.file_path);

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let p = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if is_file(fs, &p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }

    fn append_local_resource_errors(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_personality_yaml(path, yaml, errors);
    }
}

pub(crate) fn allowed_personality_adjective(adjective: &str) -> bool {
    ALLOWED_ADJECTIVES.contains(&adjective)
}

fn validate_personality_yaml(path: &str, yaml: &Value, errors: &mut Vec<String>) {
    let Some(adjectives) = yaml.get("adjectives").and_then(Value::as_mapping) else {
        return;
    };
    let other_enabled = adjectives
        .iter()
        .any(|(key, value)| key.as_str() == Some("Other") && value.as_bool().unwrap_or(false));
    if other_enabled
        && adjectives.iter().any(|(key, value)| {
            key.as_str().is_some_and(|key| key != "Other") && value.as_bool().unwrap_or(false)
        })
    {
        errors.push(format!(
            "Validation error in {path}/adjectives/Other: Other adjective can only be set if no other adjectives are selected."
        ));
    }
    let invalid_enabled = adjectives.iter().find_map(|(key, value)| {
        let key = key.as_str()?;
        (value.as_bool().unwrap_or(false) && !allowed_personality_adjective(key)).then_some(key)
    });
    if let Some(adjective) = invalid_enabled {
        errors.push(format!(
            "Validation error in {path}/adjectives/{adjective}: Enabled adjectives must be from the allowed set: {}",
            ALLOWED_ADJECTIVES.join(", ")
        ));
    }
}

/// Validation parity: TODO(DEVP-319) audit Python SettingsRole.validate().
pub(crate) struct SettingsRole;
impl DiscoverResources for SettingsRole {
    const LOCAL_PATH: LocalResourcePath =
        LocalResourcePath::File(crate::specs::AGENT_ROLE_FILE.file_path);

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let p = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if is_file(fs, &p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }

    fn append_local_resource_errors(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_role_yaml(path, yaml, errors);
    }
}

fn validate_role_yaml(path: &str, yaml: &Value, errors: &mut Vec<String>) {
    let custom = yaml
        .get("custom")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let value = yaml
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !custom.is_empty() && !value.eq_ignore_ascii_case("other") {
        errors.push(format!(
            "Validation error in {path}/custom: Custom role can only be set if role is 'other'."
        ));
    }
}

/// Validation parity: TODO(DEVP-319) audit Python SettingsRules.validate().
pub(crate) struct SettingsRules;
impl DiscoverResources for SettingsRules {
    const LOCAL_PATH: LocalResourcePath =
        LocalResourcePath::File(crate::specs::AGENT_RULES_FILE.file_path);

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let p = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if is_file(fs, &p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("personality YAML");
        let mut errors = Vec::new();
        validate_personality_yaml("agent_settings/personality.yaml", &yaml, &mut errors);
        errors
    }

    fn role_validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("role YAML");
        let mut errors = Vec::new();
        validate_role_yaml("agent_settings/role.yaml", &yaml, &mut errors);
        errors
    }

    #[test]
    fn disabled_unknown_personality_adjectives_are_allowed() {
        let errors = validation_errors(
            "adjectives:\n  Polite: true\n  RetiredAdjective: false\ncustom: ''\n",
        );

        assert!(
            errors.is_empty(),
            "unexpected validation errors: {errors:?}"
        );
    }

    #[test]
    fn enabled_unknown_personality_adjectives_are_rejected() {
        let errors = validation_errors("adjectives:\n  RetiredAdjective: true\ncustom: ''\n");

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Enabled adjectives must be from the allowed set"));
    }

    #[test]
    fn validates_python_role_custom_requires_other_value() {
        let errors = role_validation_errors(
            r#"
value: agent
custom: bespoke role text
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("Custom role can only be set if role is 'other'"))
        );
    }
}
