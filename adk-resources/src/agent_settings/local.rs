use crate::local_parse::{
    ResourceParseErrors, ResourceParseResult, default_if_null, deserialize_yaml,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;
use std::collections::HashMap;

const ALLOWED_ADJECTIVES: &[&str] = &[
    "Polite",
    "Calm",
    "Kind",
    "Funny",
    "Other",
    "Energetic",
    "Thoughtful",
];

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct PersonalitySettings {
    #[serde(default)]
    adjectives: IndexMap<String, bool>,
    #[serde(default, deserialize_with = "default_if_null")]
    custom: String,
}

impl PersonalitySettings {
    pub(crate) fn from_projection(personality: &JsonValue) -> Self {
        Self {
            adjectives: personality_adjectives_from_projection(personality),
            custom: personality
                .get("custom")
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .to_string(),
        }
    }

    pub(crate) fn custom(&self) -> &str {
        &self.custom
    }

    pub(crate) fn allowed_adjective_values(&self) -> HashMap<String, bool> {
        self.adjectives
            .iter()
            .filter_map(|(adjective, enabled)| {
                allowed_personality_adjective(adjective).then_some((adjective.clone(), *enabled))
            })
            .collect()
    }

    fn validate(self, path: &str) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        let other_enabled = self.adjectives.get("Other").copied().unwrap_or(false);
        if other_enabled
            && self
                .adjectives
                .iter()
                .any(|(adjective, enabled)| adjective.to_lowercase() != "other" && *enabled)
        {
            errors.push(
                &format!("{path}/adjectives/Other"),
                "Other adjective can only be set if no other adjectives are selected.",
            );
        }

        if let Some(adjective) = self.adjectives.iter().find_map(|(adjective, enabled)| {
            (*enabled && !allowed_personality_adjective(adjective)).then_some(adjective)
        }) {
            errors.push(
                &format!("{path}/adjectives/{adjective}"),
                format!(
                    "Enabled adjectives must be from the allowed set: {}",
                    ALLOWED_ADJECTIVES.join(", ")
                ),
            );
        }

        if errors.is_empty() {
            Ok(self)
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RoleSettings {
    #[serde(default, deserialize_with = "default_if_null")]
    value: String,
    #[serde(default, deserialize_with = "default_if_null")]
    additional_info: String,
    #[serde(default, deserialize_with = "default_if_null")]
    custom: String,
}

impl RoleSettings {
    pub(crate) fn from_projection(role: &JsonValue) -> Self {
        Self {
            value: role
                .get("value")
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .to_string(),
            additional_info: role
                .get("additionalInfo")
                .or_else(|| role.get("additional_info"))
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .to_string(),
            custom: role
                .get("custom")
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .to_string(),
        }
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }

    pub(crate) fn additional_info(&self) -> &str {
        &self.additional_info
    }

    pub(crate) fn custom(&self) -> &str {
        &self.custom
    }

    fn validate(self, path: &str) -> ResourceParseResult<Self> {
        if !self.custom.is_empty() && !self.value.eq_ignore_ascii_case("other") {
            return Err(ResourceParseErrors::single(
                &format!("{path}/custom"),
                "Custom role can only be set if role is 'other'.",
            ));
        }
        Ok(self)
    }
}

pub(crate) fn parse_personality_settings(
    path: &str,
    yaml: &YamlValue,
) -> ResourceParseResult<PersonalitySettings> {
    deserialize_yaml::<PersonalitySettings>(path, yaml)?.validate(path)
}

pub(crate) fn parse_personality_settings_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<PersonalitySettings> {
    let yaml = serde_yaml_ng::from_str::<YamlValue>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    parse_personality_settings(path, &yaml)
}

pub(crate) fn parse_role_settings(
    path: &str,
    yaml: &YamlValue,
) -> ResourceParseResult<RoleSettings> {
    deserialize_yaml::<RoleSettings>(path, yaml)?.validate(path)
}

pub(crate) fn parse_role_settings_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<RoleSettings> {
    let yaml = serde_yaml_ng::from_str::<YamlValue>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    parse_role_settings(path, &yaml)
}

fn allowed_personality_adjective(adjective: &str) -> bool {
    ALLOWED_ADJECTIVES.contains(&adjective)
}

fn personality_adjectives_from_projection(personality: &JsonValue) -> IndexMap<String, bool> {
    personality
        .pointer("/adjectives/values")
        .or_else(|| personality.get("adjectives"))
        .and_then(JsonValue::as_object)
        .into_iter()
        .flat_map(|adjectives| {
            adjectives
                .iter()
                .filter_map(|(adjective, enabled)| {
                    enabled
                        .as_bool()
                        .map(|enabled| (adjective.clone(), enabled))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
