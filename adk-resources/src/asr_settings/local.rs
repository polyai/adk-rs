use crate::local_parse::{
    ResourceParseErrors, ResourceParseResult, default_if_null, deserialize_yaml,
};
use adk_protobuf::asr_settings::{AsrSettingsUpdateAsrSettings, LatencyConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct AsrSettingsFile {
    #[serde(default, deserialize_with = "default_if_null")]
    barge_in: bool,
    #[serde(default, deserialize_with = "default_if_null")]
    interaction_style: InteractionStyle,
}

impl AsrSettingsFile {
    pub(crate) fn from_projection(settings: &JsonValue) -> Self {
        let latency_config = settings
            .get("latencyConfig")
            .or_else(|| settings.get("latency_config"));
        Self {
            barge_in: settings
                .get("bargeIn")
                .or_else(|| settings.get("barge_in"))
                .and_then(JsonValue::as_bool)
                .unwrap_or(false),
            interaction_style: latency_config
                .and_then(|config| {
                    config
                        .get("interactionStyle")
                        .or_else(|| config.get("interaction_style"))
                })
                .or_else(|| {
                    settings
                        .get("interactionStyle")
                        .or_else(|| settings.get("interaction_style"))
                })
                .and_then(JsonValue::as_str)
                .and_then(InteractionStyle::from_projection_str)
                .unwrap_or_default(),
        }
    }

    pub(crate) fn to_update_proto(&self) -> AsrSettingsUpdateAsrSettings {
        AsrSettingsUpdateAsrSettings {
            barge_in: Some(self.barge_in),
            latency_config: Some(LatencyConfig {
                interaction_style: self.interaction_style.proto_value().to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum InteractionStyle {
    #[default]
    Balanced,
    Precise,
    Swift,
    Sonic,
    Turbo,
}

impl InteractionStyle {
    fn from_projection_str(value: &str) -> Option<Self> {
        match value {
            "balanced" | "BALANCED" => Some(Self::Balanced),
            "precise" | "PRECISE" => Some(Self::Precise),
            "swift" | "SWIFT" => Some(Self::Swift),
            "sonic" | "SONIC" => Some(Self::Sonic),
            "turbo" | "TURBO" => Some(Self::Turbo),
            _ => None,
        }
    }

    fn proto_value(&self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::Precise => "precise",
            Self::Swift => "swift",
            Self::Sonic => "sonic",
            // Python ADK compatibility: `turbo` appears in the frontend/YAML
            // shape, but the backend update proto expects `sonic`.
            Self::Turbo => "sonic",
        }
    }
}

pub(crate) fn parse_asr_settings(
    path: &str,
    yaml: &YamlValue,
) -> ResourceParseResult<AsrSettingsFile> {
    deserialize_yaml(path, yaml)
}

pub(crate) fn parse_asr_settings_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<AsrSettingsFile> {
    let yaml = serde_yaml_ng::from_str::<YamlValue>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    parse_asr_settings(path, &yaml)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_turbo_interaction_style_pushes_python_sonic_value() {
        let yaml = serde_yaml_ng::from_str("barge_in: false\ninteraction_style: turbo\n")
            .expect("ASR settings YAML");
        let parsed = parse_asr_settings("voice/speech_recognition/asr_settings.yaml", &yaml)
            .expect("parsed ASR settings");

        assert_eq!(
            parsed
                .to_update_proto()
                .latency_config
                .as_ref()
                .map(|config| config.interaction_style.as_str()),
            Some("sonic")
        );
    }
}
