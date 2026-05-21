use adk_io::{FileSystem, StdFileSystem};
use serde_yaml::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(crate) fn read_yaml_mapping(path: &Path) -> Option<serde_yaml::Mapping> {
    let raw = StdFileSystem.read_to_string(path).ok()?;
    let v: Value = serde_yaml::from_str(&raw).ok()?;
    match v {
        Value::Mapping(m) => Some(m),
        _ => None,
    }
}

pub(crate) fn sorted_read_dir(dir: &Path) -> Option<Vec<PathBuf>> {
    StdFileSystem.read_dir(dir).ok()
}

pub(crate) fn is_file(path: impl AsRef<Path>) -> bool {
    StdFileSystem.is_file(path.as_ref())
}

pub(crate) fn is_dir(path: impl AsRef<Path>) -> bool {
    StdFileSystem.is_dir(path.as_ref())
}

pub(crate) fn validate_named_sequence(
    path: &str,
    yaml: &serde_yaml::Value,
    key: &str,
    label: &str,
    errors: &mut Vec<String>,
) {
    let Some(items) = yaml.get(key).and_then(serde_yaml::Value::as_sequence) else {
        return;
    };
    for (idx, item) in items.iter().enumerate() {
        if item
            .get("name")
            .and_then(serde_yaml::Value::as_str)
            .is_none_or(str::is_empty)
        {
            errors.push(format!(
                "Validation error in {path}/{key}/{idx}: {label} name is required."
            ));
        }
    }
    validate_duplicate_names(path, key, label, items, errors);
}

pub(crate) fn validate_duplicate_names(
    path: &str,
    key: &str,
    label: &str,
    items: &[serde_yaml::Value],
    errors: &mut Vec<String>,
) {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for item in items {
        let Some(name) = item.get("name").and_then(serde_yaml::Value::as_str) else {
            continue;
        };
        if !seen.insert(name.to_string()) {
            duplicates.insert(name.to_string());
        }
    }
    for name in duplicates {
        errors.push(format!(
            "Validation error in {path}/{key}/{name}: duplicate {label} name '{name}'."
        ));
    }
}
