use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping, validate_duplicate_names};
use crate::resource_utils::{clean_name, rel_under_root};
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

    fn validate_local_yaml(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_local_yaml(yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = crate::specs::TRANSLATIONS_FILE.file_path;
    let Some(translations) = yaml.get("translations").and_then(Value::as_sequence) else {
        return;
    };
    for (idx, translation) in translations.iter().enumerate() {
        let name = translation
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("");
        if name.is_empty() {
            errors.push(format!(
                "Validation error in {path}/translations/{idx}: Translation name cannot be empty."
            ));
        }
        let translation_count = translation
            .get("translations")
            .and_then(Value::as_mapping)
            .map(|items| items.len())
            .unwrap_or(0);
        if translation_count == 0 {
            errors.push(format!(
                "Validation error in {path}/translations/{}: Translations cannot be empty.",
                if name.is_empty() {
                    idx.to_string()
                } else {
                    clean_name(name, false)
                }
            ));
        }
    }
    validate_duplicate_names(path, "translations", "translation", translations, errors);
}
