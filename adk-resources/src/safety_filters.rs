use crate::local_parse::{ResourceParseErrors, ResourceParseResult, deserialize_yaml};
use adk_protobuf::content_filter_settings::{
    AzureContentFilter, AzureContentFilterCategory,
    ContentFilterSettingsUpdateContentFilterSettings,
};
use indexmap::IndexMap;
use serde::de::Error as DeError;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;

const CATEGORY_KEYS: &[&str] = &["violence", "hate", "sexual", "self_harm"];
const VALID_CATEGORY_KEYS_SORTED: &[&str] = &["hate", "self_harm", "sexual", "violence"];
const LEVELS: &[&str] = &["lenient", "medium", "strict"];

#[derive(Clone, Copy, Debug)]
pub(crate) enum SafetyFilterMode {
    General,
    Channel,
}

impl SafetyFilterMode {
    fn requires_enabled(self) -> bool {
        matches!(self, Self::Channel)
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SafetyFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    categories: IndexMap<String, SafetyFilterCategory>,
}

impl SafetyFilters {
    pub(crate) fn from_projection(settings: &JsonValue, include_enabled: bool) -> Self {
        let azure_config = settings
            .get("azureConfig")
            .or_else(|| settings.get("azure_config"))
            .unwrap_or(&JsonValue::Null);
        let categories = CATEGORY_KEYS
            .iter()
            .map(|yaml_key| {
                let backend_keys = match *yaml_key {
                    "self_harm" => ["selfHarm", "self_harm"],
                    key => [key, key],
                };
                let category = backend_keys
                    .iter()
                    .find_map(|key| azure_config.get(*key))
                    .map(SafetyFilterCategory::from_projection)
                    .unwrap_or_default();
                ((*yaml_key).to_string(), category)
            })
            .collect();
        Self {
            enabled: include_enabled.then(|| {
                !settings
                    .get("disabled")
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(false)
            }),
            categories,
        }
    }

    fn try_from_raw(
        path: &str,
        raw: RawSafetyFilters,
        mode: SafetyFilterMode,
    ) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        if mode.requires_enabled() {
            match raw.enabled.as_ref() {
                Some(RawBool::Bool(_)) => {}
                Some(RawBool::Invalid(value)) => errors.push(
                    &format!("{path}/enabled"),
                    format!(
                        "Invalid value {value} for 'enabled'. Must be true or false (unquoted)."
                    ),
                ),
                None => errors.push(
                    &format!("{path}/enabled"),
                    "Missing required field 'enabled'.",
                ),
            }
        }

        let Some(raw_categories) = raw.categories else {
            errors.push(
                &format!("{path}/categories"),
                "Safety filter config is missing 'categories'.",
            );
            return Err(errors);
        };
        let RawCategories::Categories(raw_categories) = raw_categories else {
            errors.push(
                &format!("{path}/categories"),
                "Safety filter config is missing 'categories'.",
            );
            return Err(errors);
        };

        let invalid_keys = raw_categories
            .keys()
            .filter(|key| !CATEGORY_KEYS.contains(&key.as_str()))
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !invalid_keys.is_empty() {
            errors.push(
                &format!("{path}/categories"),
                format!(
                    "Unrecognised safety filter categories: {}. Accepted categories are: {}",
                    invalid_keys
                        .iter()
                        .map(|key| format!("'{key}'"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    VALID_CATEGORY_KEYS_SORTED.join(", ")
                ),
            );
        }

        let mut categories = IndexMap::new();
        for category_name in CATEGORY_KEYS {
            match raw_categories.get(*category_name) {
                Some(RawCategory::Category(category)) => {
                    validate_category(path, category_name, category, &mut errors);
                    categories.insert(
                        (*category_name).to_string(),
                        SafetyFilterCategory {
                            enabled: category.enabled.as_ref().and_then(RawBool::as_bool),
                            level: category.level.clone(),
                        },
                    );
                }
                Some(RawCategory::Invalid) | None => errors.push(
                    &format!("{path}/categories/{category_name}"),
                    format!(
                        "Missing required safety filter category '{category_name}'. All of {} must be provided.",
                        VALID_CATEGORY_KEYS_SORTED.join(", ")
                    ),
                ),
            }
        }

        if errors.is_empty() {
            Ok(Self {
                enabled: mode
                    .requires_enabled()
                    .then(|| raw.enabled.as_ref().and_then(RawBool::as_bool))
                    .flatten(),
                categories,
            })
        } else {
            Err(errors)
        }
    }

    pub(crate) fn to_update_proto(&self) -> ContentFilterSettingsUpdateContentFilterSettings {
        ContentFilterSettingsUpdateContentFilterSettings {
            r#type: Some("azure".to_string()),
            disabled: Some(!self.effective_enabled()),
            azure_config: Some(AzureContentFilter {
                violence: self.category_proto("violence"),
                hate: self.category_proto("hate"),
                sexual: self.category_proto("sexual"),
                self_harm: self.category_proto("self_harm"),
            }),
        }
    }

    fn effective_enabled(&self) -> bool {
        self.enabled.unwrap_or_else(|| {
            self.categories
                .values()
                .any(|category| category.enabled.unwrap_or(false))
        })
    }

    fn category_proto(&self, name: &str) -> Option<AzureContentFilterCategory> {
        let category = self.categories.get(name)?;
        Some(AzureContentFilterCategory {
            is_active: category.enabled.unwrap_or(false),
            precision: category.precision(),
        })
    }
}

#[derive(Debug, Clone, Default, Serialize)]
struct SafetyFilterCategory {
    enabled: Option<bool>,
    level: Option<String>,
}

impl SafetyFilterCategory {
    fn from_projection(category: &JsonValue) -> Self {
        Self {
            enabled: category
                .get("isActive")
                .or_else(|| category.get("is_active"))
                .and_then(JsonValue::as_bool),
            level: Some(precision_to_level(
                category
                    .get("precision")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default(),
            )),
        }
    }

    fn precision(&self) -> String {
        self.level
            .as_deref()
            .map(level_to_precision)
            .unwrap_or_default()
    }
}

#[derive(Debug, Deserialize)]
struct RawSafetyFilters {
    #[serde(default)]
    enabled: Option<RawBool>,
    #[serde(default)]
    categories: Option<RawCategories>,
}

#[derive(Debug)]
enum RawCategories {
    Categories(IndexMap<String, RawCategory>),
    Invalid,
}

impl<'de> Deserialize<'de> for RawCategories {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = YamlValue::deserialize(deserializer)?;
        match value {
            YamlValue::Mapping(_) => {
                deserialize_yaml::<IndexMap<String, RawCategory>>("categories", &value)
                    .map(Self::Categories)
                    .map_err(|error| D::Error::custom(format!("{error:?}")))
            }
            _ => Ok(Self::Invalid),
        }
    }
}

#[derive(Debug)]
enum RawCategory {
    Category(RawCategoryFields),
    Invalid,
}

impl<'de> Deserialize<'de> for RawCategory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = YamlValue::deserialize(deserializer)?;
        match value {
            YamlValue::Mapping(_) => deserialize_yaml::<RawCategoryFields>("category", &value)
                .map(Self::Category)
                .map_err(|error| D::Error::custom(format!("{error:?}"))),
            _ => Ok(Self::Invalid),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawCategoryFields {
    #[serde(default)]
    enabled: Option<RawBool>,
    #[serde(default)]
    level: Option<String>,
}

#[derive(Debug)]
enum RawBool {
    Bool(bool),
    Invalid(String),
}

impl RawBool {
    fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Invalid(_) => None,
        }
    }
}

impl<'de> Deserialize<'de> for RawBool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = YamlValue::deserialize(deserializer)?;
        match value {
            YamlValue::Bool(value) => Ok(Self::Bool(value)),
            value => Ok(Self::Invalid(format!("{value:?}"))),
        }
    }
}

pub(crate) fn parse_safety_filters(
    path: &str,
    yaml: &YamlValue,
    mode: SafetyFilterMode,
) -> ResourceParseResult<SafetyFilters> {
    let raw = deserialize_yaml::<RawSafetyFilters>(path, yaml)?;
    SafetyFilters::try_from_raw(path, raw, mode)
}

fn validate_category(
    path: &str,
    category_name: &str,
    category: &RawCategoryFields,
    errors: &mut ResourceParseErrors,
) {
    match category.enabled.as_ref() {
        Some(RawBool::Bool(_)) => {}
        Some(RawBool::Invalid(value)) => errors.push(
            &format!("{path}/categories/{category_name}/enabled"),
            format!(
                "Invalid value '{value}' for 'enabled' in category '{category_name}'. Must be true or false."
            ),
        ),
        None => errors.push(
            &format!("{path}/categories/{category_name}/enabled"),
            format!("Missing required field 'enabled' for safety filter category '{category_name}'."),
        ),
    }
    match category.level.as_deref() {
        Some(level) if LEVELS.contains(&level) => {}
        Some(level) => errors.push(
            &format!("{path}/categories/{category_name}/level"),
            format!(
                "Invalid level set '{level}' for category '{category_name}'. Must be one of: {}",
                LEVELS.join(", ")
            ),
        ),
        None => errors.push(
            &format!("{path}/categories/{category_name}/level"),
            format!("Missing required field 'level' for safety filter category '{category_name}'."),
        ),
    }
}

fn precision_to_level(precision: &str) -> String {
    match precision {
        "LOOSE" => "lenient".to_string(),
        "MEDIUM" => "medium".to_string(),
        "STRICT" => "strict".to_string(),
        value => value.to_ascii_lowercase(),
    }
}

fn level_to_precision(level: &str) -> String {
    match level {
        "lenient" => "LOOSE".to_string(),
        "medium" => "MEDIUM".to_string(),
        "strict" => "STRICT".to_string(),
        value => value.to_ascii_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str, mode: SafetyFilterMode) -> Vec<String> {
        let yaml = from_str::<YamlValue>(yaml).expect("safety filters YAML");
        parse_safety_filters("voice/safety_filters.yaml", &yaml, mode)
            .err()
            .map(ResourceParseErrors::into_validation_errors)
            .unwrap_or_default()
    }

    #[test]
    fn validates_python_safety_filter_schema_rules() {
        let errors = validation_errors(
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
            SafetyFilterMode::Channel,
        );

        for expected in [
            "Invalid value",
            "Unrecognised safety filter categories",
            "Missing required safety filter category 'self_harm'",
            "Invalid level set 'loud'",
            "Missing required field 'level' for safety filter category 'sexual'",
        ] {
            assert!(
                errors.iter().any(|error| error.contains(expected)),
                "expected error containing {expected:?}, got {errors:?}"
            );
        }
    }

    #[test]
    fn general_safety_filters_derive_enabled_from_categories() {
        let yaml = from_str::<YamlValue>(
            r#"
enabled: false
categories:
  violence:
    enabled: false
    level: strict
  hate:
    enabled: false
    level: medium
  sexual:
    enabled: false
    level: lenient
  self_harm:
    enabled: true
    level: strict
"#,
        )
        .expect("safety filters YAML");

        let filters = parse_safety_filters(
            "agent_settings/safety_filters.yaml",
            &yaml,
            SafetyFilterMode::General,
        )
        .expect("valid filters");

        let proto = filters.to_update_proto();
        assert_eq!(proto.disabled, Some(false));
        assert_eq!(
            proto
                .azure_config
                .as_ref()
                .and_then(|config| config.sexual.as_ref())
                .map(|category| category.precision.as_str()),
            Some("LOOSE")
        );
    }
}
