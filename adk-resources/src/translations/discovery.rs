use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{ParseLocalResource, ResourceParseResult};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use crate::translations::local::{TranslationsFile, parse_translations_file};
use serde_yaml_ng::Value;
use std::path::Path;

/// Validation parity: implemented against Python Translation.validate().
pub(crate) struct Translation;
impl DiscoverResources for Translation {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::TRANSLATIONS_FILE.file_path,
        yaml_path: &["translations"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &yaml_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("translations") else {
            return vec![];
        };
        let mut out = Vec::new();
        for item in list {
            let Value::Mapping(map) = item else { continue };
            let Some(name) = map.get("name").and_then(Value::as_str) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            out.push(rel_under_root(
                base_path,
                &yaml_path.join("translations").join(clean_name(name, false)),
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
    let path = crate::specs::TRANSLATIONS_FILE.file_path;
    <Translation as ParseLocalResource>::append_parse_errors(path, yaml, errors);
}

impl ParseLocalResource for Translation {
    type Parsed = TranslationsFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_translations_file(path, yaml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("translations YAML");
        let mut errors = Vec::new();
        append_parse_errors(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_translation_required_fields_and_duplicates() {
        let missing_name = validation_errors(
            r#"
translations:
  - translations:
      en-US: Hello
"#,
        );
        assert!(
            missing_name
                .iter()
                .any(|error| error.contains("missing field `name`"))
        );

        let empty_translations = validation_errors(
            r#"
translations:
  - name: greeting
    translations: {}
"#,
        );
        assert!(
            empty_translations
                .iter()
                .any(|error| error.contains("Translations cannot be empty"))
        );

        let duplicate_names = validation_errors(
            r#"
translations:
  - name: greeting
    translations:
      en-US: Hello
  - name: greeting
    translations:
      en-US: Hi
"#,
        );
        assert!(
            duplicate_names
                .iter()
                .any(|error| error.contains("duplicate translation name 'greeting'"))
        );
    }
}
