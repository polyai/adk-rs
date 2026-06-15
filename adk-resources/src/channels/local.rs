use crate::local_parse::{
    ResourceParseErrors, ResourceParseResult, default_if_null, deserialize_yaml,
};
use adk_protobuf::agent::{DisclaimerMessageUpdateDisclaimerMessage, GreetingUpdateGreeting};
use adk_protobuf::channels::StylePromptUpdateStylePrompt;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ChannelConfiguration {
    #[serde(default, serialize_with = "serialize_section_or_empty")]
    greeting: Option<ChannelGreeting>,
    #[serde(default, serialize_with = "serialize_section_or_empty")]
    style_prompt: Option<ChannelStylePrompt>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_disclaimers"
    )]
    disclaimer_messages: Option<DisclaimerMessages>,
}

impl ChannelConfiguration {
    pub(crate) fn from_projection(
        greeting: Option<&JsonValue>,
        style_prompt: Option<&JsonValue>,
        disclaimer: Option<&JsonValue>,
    ) -> Self {
        Self {
            greeting: greeting.map(ChannelGreeting::from_projection),
            style_prompt: style_prompt.map(ChannelStylePrompt::from_projection),
            disclaimer_messages: disclaimer
                .map(VoiceDisclaimerMessage::from_projection)
                .map(|disclaimer| DisclaimerMessages(vec![disclaimer])),
        }
    }

    pub(crate) fn greeting(&self) -> Option<&ChannelGreeting> {
        self.greeting.as_ref()
    }

    pub(crate) fn style_prompt(&self) -> Option<&ChannelStylePrompt> {
        self.style_prompt.as_ref()
    }

    pub(crate) fn disclaimer(&self) -> Option<&VoiceDisclaimerMessage> {
        self.disclaimer_messages
            .as_ref()
            .and_then(|disclaimers| disclaimers.first())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ChannelGreeting {
    #[serde(default, deserialize_with = "default_if_null")]
    welcome_message: String,
    #[serde(
        default = "default_language_code",
        deserialize_with = "default_if_null"
    )]
    language_code: String,
}

impl ChannelGreeting {
    fn from_projection(greeting: &JsonValue) -> Self {
        Self {
            welcome_message: greeting
                .get("welcomeMessage")
                .or_else(|| greeting.get("welcome_message"))
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .to_string(),
            language_code: greeting
                .get("languageCode")
                .or_else(|| greeting.get("language_code"))
                .and_then(JsonValue::as_str)
                .unwrap_or("en-GB")
                .to_string(),
        }
    }

    pub(crate) fn to_update_proto(&self) -> GreetingUpdateGreeting {
        GreetingUpdateGreeting {
            welcome_message: Some(self.welcome_message.clone()),
            references: None,
            language_code: self.language_code.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ChannelStylePrompt {
    #[serde(default, deserialize_with = "default_if_null")]
    prompt: String,
}

impl ChannelStylePrompt {
    fn from_projection(style_prompt: &JsonValue) -> Self {
        Self {
            prompt: style_prompt
                .get("prompt")
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .to_string(),
        }
    }

    pub(crate) fn to_update_proto(&self) -> StylePromptUpdateStylePrompt {
        StylePromptUpdateStylePrompt {
            prompt: self.prompt.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct VoiceDisclaimerMessage {
    #[serde(default, deserialize_with = "default_if_null")]
    message: String,
    #[serde(default, alias = "is_enabled", deserialize_with = "default_if_null")]
    enabled: bool,
    #[serde(
        default = "default_language_code",
        deserialize_with = "default_if_null"
    )]
    language_code: String,
}

impl VoiceDisclaimerMessage {
    fn from_projection(disclaimer: &JsonValue) -> Self {
        Self {
            message: disclaimer
                .get("message")
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .to_string(),
            enabled: disclaimer
                .get("isEnabled")
                .or_else(|| disclaimer.get("enabled"))
                .and_then(JsonValue::as_bool)
                .unwrap_or(false),
            language_code: disclaimer
                .get("languageCode")
                .or_else(|| disclaimer.get("language_code"))
                .and_then(JsonValue::as_str)
                .unwrap_or("en-GB")
                .to_string(),
        }
    }

    pub(crate) fn to_update_proto(&self) -> DisclaimerMessageUpdateDisclaimerMessage {
        DisclaimerMessageUpdateDisclaimerMessage {
            message: Some(self.message.clone()),
            is_enabled: Some(self.enabled),
            ringing_tone: None,
            language_code: self.language_code.clone(),
            references: None,
        }
    }
}

#[derive(Debug, Clone)]
struct DisclaimerMessages(Vec<VoiceDisclaimerMessage>);

impl DisclaimerMessages {
    fn first(&self) -> Option<&VoiceDisclaimerMessage> {
        self.0.first()
    }
}

impl Serialize for DisclaimerMessages {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.first() {
            Some(disclaimer) => disclaimer.serialize(serializer),
            None => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for DisclaimerMessages {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = YamlValue::deserialize(deserializer)?;
        match value {
            YamlValue::Sequence(_) => {
                deserialize_yaml::<Vec<VoiceDisclaimerMessage>>("disclaimer_messages", &value)
                    .map(Self)
                    .map_err(|error| serde::de::Error::custom(format!("{error:?}")))
            }
            YamlValue::Mapping(_) => {
                deserialize_yaml::<VoiceDisclaimerMessage>("disclaimer_messages", &value)
                    .map(|disclaimer| Self(vec![disclaimer]))
                    .map_err(|error| serde::de::Error::custom(format!("{error:?}")))
            }
            YamlValue::Null => Ok(Self(Vec::new())),
            _ => Ok(Self(Vec::new())),
        }
    }
}

pub(crate) fn parse_channel_configuration(
    path: &str,
    yaml: &YamlValue,
) -> ResourceParseResult<ChannelConfiguration> {
    deserialize_yaml(path, yaml)
}

pub(crate) fn parse_channel_configuration_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<ChannelConfiguration> {
    let yaml = serde_yaml_ng::from_str::<YamlValue>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    parse_channel_configuration(path, &yaml)
}

pub(crate) fn validate_channel_greeting(path: &str, yaml: &YamlValue) -> ResourceParseResult<()> {
    let file = parse_channel_configuration(path, yaml)?;
    let Some(greeting) = file.greeting else {
        return Ok(());
    };
    let mut errors = ResourceParseErrors::new();
    if greeting.welcome_message.is_empty() {
        errors.push(
            &format!("{path}/greeting/welcome_message"),
            "Welcome message cannot be empty.",
        );
    }
    if greeting.language_code.is_empty() {
        errors.push(
            &format!("{path}/greeting/language_code"),
            "Language code cannot be empty.",
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub(crate) fn validate_voice_disclaimers(path: &str, yaml: &YamlValue) -> ResourceParseResult<()> {
    let file = parse_channel_configuration(path, yaml)?;
    let Some(disclaimers) = file.disclaimer_messages else {
        return Ok(());
    };
    let mut errors = ResourceParseErrors::new();
    for (idx, disclaimer) in disclaimers.0.iter().enumerate() {
        if disclaimer.language_code.is_empty() {
            errors.push(
                &format!("{path}/disclaimer_messages/{idx}/language_code"),
                "Language code cannot be empty.",
            );
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn deserialize_optional_disclaimers<'de, D>(
    deserializer: D,
) -> Result<Option<DisclaimerMessages>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<DisclaimerMessages>::deserialize(deserializer)
}

fn serialize_section_or_empty<S, T>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: Serialize,
{
    match value {
        Some(value) => value.serialize(serializer),
        None => {
            let map = serializer.serialize_map(Some(0))?;
            map.end()
        }
    }
}

fn default_language_code() -> String {
    "en-GB".to_string()
}
