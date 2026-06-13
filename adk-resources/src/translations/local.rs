use crate::local_parse::{
    NonEmptyString, ResourceParseErrors, ResourceParseResult, deserialize_yaml, duplicate_names,
    non_empty_map,
};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub(crate) struct TranslationsFile {
    pub(crate) translations: Vec<TranslationItem>,
}

impl TranslationsFile {
    pub(crate) fn new(translations: Vec<TranslationItem>) -> Self {
        Self { translations }
    }

    fn try_from_raw(path: &str, raw: RawTranslationsFile) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        for duplicate in duplicate_names(raw.translations.iter().map(|item| item.name.as_str())) {
            errors.push(
                &format!("{path}/translations/{duplicate}"),
                format!("duplicate translation name '{duplicate}'."),
            );
        }
        if errors.is_empty() {
            Ok(Self {
                translations: raw.translations,
            })
        } else {
            Err(errors)
        }
    }
}

pub(crate) fn parse_translations_file(
    path: &str,
    yaml: &Value,
) -> ResourceParseResult<TranslationsFile> {
    let raw = deserialize_yaml::<RawTranslationsFile>(path, yaml)?;
    TranslationsFile::try_from_raw(path, raw)
}

#[derive(Debug, Deserialize)]
struct RawTranslationsFile {
    #[serde(default)]
    translations: Vec<TranslationItem>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct TranslationItem {
    name: NonEmptyString,
    #[serde(deserialize_with = "translation_values")]
    translations: BTreeMap<String, String>,
}

impl TranslationItem {
    pub(crate) fn from_projection(
        name: String,
        translations: BTreeMap<String, String>,
    ) -> Result<Self, String> {
        Ok(Self {
            name: NonEmptyString::new(name)?,
            translations,
        })
    }

    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn translations(&self) -> &BTreeMap<String, String> {
        &self.translations
    }
}

fn translation_values<'de, D>(deserializer: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    non_empty_map(deserializer, "Translations cannot be empty.")
}
