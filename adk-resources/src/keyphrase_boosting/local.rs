use crate::local_parse::{NonEmptyString, ResourceParseResult, deserialize_yaml};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct KeyphraseBoostingFile {
    #[serde(default)]
    pub(crate) keyphrases: Vec<KeyphraseItem>,
}

impl KeyphraseBoostingFile {
    pub(crate) fn new(keyphrases: Vec<KeyphraseItem>) -> Self {
        Self { keyphrases }
    }
}

pub(crate) fn parse_keyphrase_boosting_file(
    path: &str,
    yaml: &Value,
) -> ResourceParseResult<KeyphraseBoostingFile> {
    deserialize_yaml(path, yaml)
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct KeyphraseItem {
    keyphrase: NonEmptyString,
    #[serde(default)]
    level: KeyphraseLevel,
}

impl KeyphraseItem {
    pub(crate) fn new(keyphrase: String, level: String) -> Result<Self, String> {
        Ok(Self {
            keyphrase: NonEmptyString::new(keyphrase)?,
            level: KeyphraseLevel::from_projection_level(&level),
        })
    }

    pub(crate) fn keyphrase(&self) -> &str {
        self.keyphrase.as_str()
    }

    pub(crate) fn level(&self) -> &'static str {
        self.level.as_str()
    }
}

#[derive(Debug, Default, Serialize)]
enum KeyphraseLevel {
    #[default]
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "boosted")]
    Boosted,
    #[serde(rename = "maximum")]
    Maximum,
}

impl<'de> Deserialize<'de> for KeyphraseLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?.to_lowercase();
        match value.as_str() {
            "default" => Ok(Self::Default),
            "boosted" => Ok(Self::Boosted),
            "maximum" => Ok(Self::Maximum),
            _ => Err(serde::de::Error::custom(format!(
                "Invalid level '{value}'. Must be one of: default, boosted, maximum"
            ))),
        }
    }
}

impl KeyphraseLevel {
    fn from_projection_level(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "boosted" => Self::Boosted,
            "maximum" => Self::Maximum,
            _ => Self::Default,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Boosted => "boosted",
            Self::Maximum => "maximum",
        }
    }
}
