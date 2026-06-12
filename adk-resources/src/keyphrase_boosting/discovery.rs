use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{
    NonEmptyString, ParseLocalResource, ResourceParseErrors, deserialize_yaml,
};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde::Deserialize;
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/keyphrase_boosting.py
/// Validation parity: implemented against Python KeyphraseBoosting.validate().
pub(crate) struct KeyphraseBoosting;
impl DiscoverResources for KeyphraseBoosting {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::KEYPHRASE_BOOSTING_FILE.file_path,
        yaml_path: &["keyphrases"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("keyphrases") else {
            return vec![];
        };
        let mut out = Vec::new();
        for item in list {
            let Value::Mapping(map) = item else { continue };
            let Some(name) = map.get("keyphrase").and_then(|v| v.as_str()) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let safe = clean_name(name, false);
            out.push(rel_under_root(
                base_path,
                &yaml_path.join("keyphrases").join(&safe),
            ));
        }
        out
    }

    fn append_local_resource_errors(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        <Self as ParseLocalResource>::append_parse_errors(
            Self::LOCAL_PATH.primary_path().expect("local file path"),
            yaml,
            errors,
        );
    }
}

#[cfg(test)]
pub(crate) fn append_parse_errors(yaml: &Value, errors: &mut Vec<String>) {
    let path = KeyphraseBoosting::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    <KeyphraseBoosting as ParseLocalResource>::append_parse_errors(path, yaml, errors);
}

impl ParseLocalResource for KeyphraseBoosting {
    type Parsed = KeyphraseBoostingFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> Result<Self::Parsed, ResourceParseErrors> {
        deserialize_yaml(path, yaml)
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct KeyphraseBoostingFile {
    #[serde(default)]
    keyphrases: Vec<KeyphraseItem>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct KeyphraseItem {
    keyphrase: NonEmptyString,
    #[serde(default)]
    level: KeyphraseLevel,
}

#[derive(Debug, Default)]
enum KeyphraseLevel {
    #[default]
    Default,
    Boosted,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("keyphrase YAML");
        let mut errors = Vec::new();
        append_parse_errors(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_keyphrase_required_and_level_rules() {
        let missing_keyphrase = validation_errors(
            r#"
keyphrases:
  - keyphrase: ""
    level: boosted
"#,
        );

        assert!(
            missing_keyphrase
                .iter()
                .any(|error| error.contains("cannot be empty"))
        );

        let bad_level = validation_errors(
            r#"
keyphrases:
  - keyphrase: Open sesame
    level: loud
"#,
        );
        assert!(
            bad_level
                .iter()
                .any(|error| error.contains("Invalid level 'loud'"))
        );

        let uppercase_level = validation_errors(
            r#"
keyphrases:
  - keyphrase: Open sesame
    level: BOOSTED
"#,
        );
        assert!(
            uppercase_level.is_empty(),
            "uppercase level should follow Python lower-casing behavior: {uppercase_level:?}"
        );
    }
}
