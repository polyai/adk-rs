use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{ParseLocalResource, ResourceParseResult};
use crate::local_resources::is_file;
use crate::resource_utils::rel_under_root;
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/agent_settings.py

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
        <Self as ParseLocalResource>::append_parse_errors(path, yaml, errors);
    }
}

impl ParseLocalResource for SettingsPersonality {
    type Parsed = crate::agent_settings::local::PersonalitySettings;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        crate::agent_settings::local::parse_personality_settings(path, yaml)
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
        <Self as ParseLocalResource>::append_parse_errors(path, yaml, errors);
    }
}

impl ParseLocalResource for SettingsRole {
    type Parsed = crate::agent_settings::local::RoleSettings;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        crate::agent_settings::local::parse_role_settings(path, yaml)
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
        <SettingsPersonality as ParseLocalResource>::append_parse_errors(
            "agent_settings/personality.yaml",
            &yaml,
            &mut errors,
        );
        errors
    }

    fn role_validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("role YAML");
        let mut errors = Vec::new();
        <SettingsRole as ParseLocalResource>::append_parse_errors(
            "agent_settings/role.yaml",
            &yaml,
            &mut errors,
        );
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
