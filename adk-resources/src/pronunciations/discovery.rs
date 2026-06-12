use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{
    NonEmptyString, ParseLocalResource, ResourceParseErrors, deserialize_yaml,
};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde::Deserialize;
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/pronunciation.py
/// Validation parity: implemented against Python Pronunciation.validate().
pub(crate) struct Pronunciation;
impl DiscoverResources for Pronunciation {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::PRONUNCIATIONS_FILE.file_path,
        yaml_path: &["pronunciations"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(items)) = m.get("pronunciations") else {
            return vec![];
        };
        let mut out = Vec::new();
        for (i, _item) in items.iter().enumerate() {
            let safe = clean_name(&i.to_string(), false);
            out.push(rel_under_root(
                base_path,
                &yaml_path.join("pronunciations").join(&safe),
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
    let path = Pronunciation::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    <Pronunciation as ParseLocalResource>::append_parse_errors(path, yaml, errors);
}

impl ParseLocalResource for Pronunciation {
    type Parsed = PronunciationsFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> Result<Self::Parsed, ResourceParseErrors> {
        deserialize_yaml(path, yaml)
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct PronunciationsFile {
    #[serde(default)]
    pub(crate) pronunciations: Vec<PronunciationItem>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PronunciationItem {
    regex: NonEmptyString,
    #[serde(default)]
    replacement: String,
    #[serde(default)]
    case_sensitive: bool,
    #[serde(default)]
    language_code: String,
    #[serde(default, deserialize_with = "deserialize_trimmed_string")]
    description: String,
    #[serde(default)]
    name: String,
}

impl PronunciationItem {
    pub(crate) fn regex(&self) -> &str {
        self.regex.as_str()
    }

    pub(crate) fn replacement(&self) -> &str {
        &self.replacement
    }

    pub(crate) fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub(crate) fn language_code(&self) -> &str {
        &self.language_code
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

fn deserialize_trimmed_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?
        .unwrap_or_default()
        .trim()
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    #[test]
    fn validates_python_pronunciation_regex_required_rule() {
        let yaml = from_str::<Value>(
            r#"
pronunciations:
  - regex: ""
    replacement: poly
"#,
        )
        .expect("pronunciation YAML");
        let mut errors = Vec::new();

        append_parse_errors(&yaml, &mut errors);

        assert!(errors.iter().any(|error| error.contains("cannot be empty")));
    }
}
