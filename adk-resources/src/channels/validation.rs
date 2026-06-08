use crate::specs::{CHAT_CONFIGURATION_FILE, CHAT_SAFETY_FILTERS_FILE};
use adk_types::ResourceMap;
use serde_yaml_ng::{Value as YamlValue, from_str};
use std::collections::BTreeSet;

const CHAT_GREETING: &str = "ChatGreeting";
const CHAT_SAFETY_FILTERS: &str = "ChatSafetyFilters";
const CHAT_STYLE_PROMPT: &str = "ChatStylePrompt";
const WEBCHAT_CONFIG_TYPES: [&str; 3] = [CHAT_GREETING, CHAT_SAFETY_FILTERS, CHAT_STYLE_PROMPT];

pub fn validate_webchat_config_resources(resources: &ResourceMap) -> Vec<String> {
    let present = present_webchat_config_types(resources);
    if present.is_empty() || present.len() == WEBCHAT_CONFIG_TYPES.len() {
        return Vec::new();
    }

    let missing = WEBCHAT_CONFIG_TYPES
        .into_iter()
        .filter(|name| !present.contains(*name))
        .collect::<Vec<_>>();
    let path = if present.contains(CHAT_GREETING) || present.contains(CHAT_STYLE_PROMPT) {
        CHAT_CONFIGURATION_FILE.file_path
    } else {
        CHAT_SAFETY_FILTERS_FILE.file_path
    };
    vec![format!(
        "Validation error in {path}: Webchat config resources must all be present together. Missing: {}.",
        missing.join(", ")
    )]
}

fn present_webchat_config_types(resources: &ResourceMap) -> BTreeSet<&'static str> {
    let mut present = BTreeSet::new();

    if let Some(yaml) = resource_yaml(resources, CHAT_CONFIGURATION_FILE.file_path) {
        if has_mapping_field(&yaml, "greeting") {
            present.insert(CHAT_GREETING);
        }
        if has_mapping_field(&yaml, "style_prompt") {
            present.insert(CHAT_STYLE_PROMPT);
        }
    }

    if resource_yaml(resources, CHAT_SAFETY_FILTERS_FILE.file_path).is_some() {
        present.insert(CHAT_SAFETY_FILTERS);
    }

    present
}

fn resource_yaml(resources: &ResourceMap, path: &str) -> Option<YamlValue> {
    let content = resources.get(path)?.payload.get("content")?.as_str()?;
    from_str(content).ok()
}

fn has_mapping_field(yaml: &YamlValue, key: &str) -> bool {
    yaml.get(key)
        .and_then(YamlValue::as_mapping)
        .is_some_and(|mapping| !mapping.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::local_resource;

    #[test]
    fn webchat_config_validation_requires_all_siblings() {
        let mut resources = ResourceMap::new();
        resources.insert(
            CHAT_CONFIGURATION_FILE.file_path.to_string(),
            local_resource(
                CHAT_CONFIGURATION_FILE.file_path,
                CHAT_CONFIGURATION_FILE.name,
                "greeting:\n  welcome_message: Hello\n  language_code: en-US\n",
            ),
        );

        let errors = validate_webchat_config_resources(&resources);

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Webchat config resources must all be present together"));
        assert!(errors[0].contains("ChatSafetyFilters"));
        assert!(errors[0].contains("ChatStylePrompt"));
    }

    #[test]
    fn webchat_config_validation_accepts_complete_set() {
        let mut resources = ResourceMap::new();
        resources.insert(
            CHAT_CONFIGURATION_FILE.file_path.to_string(),
            local_resource(
                CHAT_CONFIGURATION_FILE.file_path,
                CHAT_CONFIGURATION_FILE.name,
                r#"
greeting:
  welcome_message: Hello
  language_code: en-US
style_prompt:
  prompt: Keep it brief.
"#,
            ),
        );
        resources.insert(
            CHAT_SAFETY_FILTERS_FILE.file_path.to_string(),
            local_resource(
                CHAT_SAFETY_FILTERS_FILE.file_path,
                CHAT_SAFETY_FILTERS_FILE.name,
                "enabled: true\ncategories: {}\n",
            ),
        );

        let errors = validate_webchat_config_resources(&resources);

        assert!(errors.is_empty());
    }
}
