use crate::specs::{CHAT_CONFIGURATION_FILE, CHAT_SAFETY_FILTERS_FILE};
use adk_types::ResourceMap;
use serde_yaml_ng::{Value as YamlValue, from_str};
use std::collections::BTreeSet;

const CHAT_GREETING: &str = "ChatGreeting";
const CHAT_SAFETY_FILTERS: &str = "ChatSafetyFilters";
const CHAT_STYLE_PROMPT: &str = "ChatStylePrompt";
const WEBCHAT_CONFIG_TYPES: [&str; 3] = [CHAT_GREETING, CHAT_SAFETY_FILTERS, CHAT_STYLE_PROMPT];
const SAFETY_FILTER_CATEGORIES: [&str; 4] = ["hate", "self_harm", "sexual", "violence"];
const SAFETY_FILTER_LEVELS: [&str; 3] = ["lenient", "medium", "strict"];

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
        if has_discoverable_field(&yaml, "greeting") {
            present.insert(CHAT_GREETING);
        }
        if has_discoverable_field(&yaml, "style_prompt") {
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

fn has_discoverable_field(yaml: &YamlValue, key: &str) -> bool {
    yaml.get(key).is_some_and(|value| match value {
        YamlValue::Null => false,
        YamlValue::Mapping(mapping) => !mapping.is_empty(),
        _ => true,
    })
}

pub(crate) fn validate_safety_filters_yaml(
    path: &str,
    yaml: &YamlValue,
    require_top_level_enabled: bool,
    errors: &mut Vec<String>,
) {
    if require_top_level_enabled {
        match yaml.get("enabled") {
            Some(YamlValue::Bool(_)) => {}
            Some(value) => errors.push(format!(
                "Validation error in {path}/enabled: Invalid value {value:?} for 'enabled'. Must be true or false (unquoted)."
            )),
            None => errors.push(format!(
                "Validation error in {path}/enabled: Missing required field 'enabled'."
            )),
        }
    }

    let Some(categories) = yaml.get("categories") else {
        errors.push(format!(
            "Validation error in {path}/categories: Safety filter config is missing 'categories'."
        ));
        return;
    };
    let Some(categories) = categories.as_mapping() else {
        errors.push(format!(
            "Validation error in {path}/categories: Safety filter config is missing 'categories'."
        ));
        return;
    };

    let invalid_keys = categories
        .keys()
        .filter_map(YamlValue::as_str)
        .filter(|key| !SAFETY_FILTER_CATEGORIES.contains(key))
        .collect::<Vec<_>>();
    if !invalid_keys.is_empty() {
        errors.push(format!(
            "Validation error in {path}/categories: Unrecognised safety filter categories: {}. Accepted categories are: {}",
            invalid_keys
                .iter()
                .map(|key| format!("'{key}'"))
                .collect::<Vec<_>>()
                .join(", "),
            SAFETY_FILTER_CATEGORIES.join(", ")
        ));
    }

    for category_name in SAFETY_FILTER_CATEGORIES {
        let key = YamlValue::String(category_name.to_string());
        let Some(category) = categories.get(&key) else {
            errors.push(format!(
                "Validation error in {path}/categories/{category_name}: Missing required safety filter category '{category_name}'. All of {} must be provided.",
                SAFETY_FILTER_CATEGORIES.join(", ")
            ));
            continue;
        };
        let Some(category) = category.as_mapping() else {
            errors.push(format!(
                "Validation error in {path}/categories/{category_name}: Missing required safety filter category '{category_name}'. All of {} must be provided.",
                SAFETY_FILTER_CATEGORIES.join(", ")
            ));
            continue;
        };
        match category.get(YamlValue::String("enabled".to_string())) {
            Some(YamlValue::Bool(_)) => {}
            Some(value) => errors.push(format!(
                "Validation error in {path}/categories/{category_name}/enabled: Invalid value '{value:?}' for 'enabled' in category '{category_name}'. Must be true or false."
            )),
            None => errors.push(format!(
                "Validation error in {path}/categories/{category_name}/enabled: Missing required field 'enabled' for safety filter category '{category_name}'."
            )),
        }
        match category
            .get(YamlValue::String("level".to_string()))
            .and_then(YamlValue::as_str)
        {
            Some(level) if SAFETY_FILTER_LEVELS.contains(&level) => {}
            Some(level) => errors.push(format!(
                "Validation error in {path}/categories/{category_name}/level: Invalid level set '{level}' for category '{category_name}'. Must be one of: {}",
                SAFETY_FILTER_LEVELS.join(", ")
            )),
            None => errors.push(format!(
                "Validation error in {path}/categories/{category_name}/level: Missing required field 'level' for safety filter category '{category_name}'."
            )),
        }
    }
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
    fn webchat_config_validation_matches_discovery_for_scalar_in_file_resources() {
        let mut resources = ResourceMap::new();
        resources.insert(
            CHAT_CONFIGURATION_FILE.file_path.to_string(),
            local_resource(
                CHAT_CONFIGURATION_FILE.file_path,
                CHAT_CONFIGURATION_FILE.name,
                "greeting: hello\nstyle_prompt: compact\n",
            ),
        );

        let errors = validate_webchat_config_resources(&resources);

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Missing: ChatSafetyFilters."));
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

    #[test]
    fn safety_filters_validate_python_schema_rules() {
        let yaml = from_str::<YamlValue>(
            r#"
enabled: "yes"
categories:
  violence:
    enabled: nope
    level: loud
  hate:
    enabled: true
    level: medium
  sexual:
    enabled: false
  unknown:
    enabled: true
    level: strict
"#,
        )
        .expect("safety filters YAML");
        let mut errors = Vec::new();

        validate_safety_filters_yaml("voice/safety_filters.yaml", &yaml, true, &mut errors);

        for expected in [
            "Invalid value",
            "Unrecognised safety filter categories",
            "Missing required safety filter category 'self_harm'",
            "Invalid level set 'loud'",
            "Missing required field 'level' for safety filter category 'sexual'",
        ] {
            assert!(
                errors.iter().any(|error| error.contains(expected)),
                "missing {expected:?}: {errors:#?}"
            );
        }
    }
}
