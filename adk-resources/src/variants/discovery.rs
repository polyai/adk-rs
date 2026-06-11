use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping, validate_duplicate_names};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
use std::collections::BTreeSet;
use std::path::Path;

// poly/resources/variant_attributes.py
/// Validation parity: implemented against Python Variant.validate() and Variant.validate_collection().
pub(crate) struct Variant;
impl DiscoverResources for Variant {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::VARIANT_ATTRIBUTES_FILE.file_path,
        yaml_path: &["variants"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("variants") else {
            return vec![];
        };
        let mut out = Vec::new();
        for item in list {
            let Value::Mapping(map) = item else { continue };
            let Some(name) = map.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let safe = clean_name(name, false);
            out.push(rel_under_root(
                base_path,
                &path.join("variants").join(&safe),
            ));
        }
        out
    }

    fn validate_local_yaml(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_local_yaml(yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = Variant::LOCAL_PATH.primary_path().expect("local file path");
    let Some(variants) = yaml.get("variants").and_then(Value::as_sequence) else {
        return;
    };
    validate_duplicate_names(path, "variants", "variant", variants, errors);
    let default_names = variants
        .iter()
        .filter(|variant| {
            variant
                .get("is_default")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|variant| variant.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    if default_names.len() != 1 {
        let names = default_names
            .iter()
            .map(|name| format!("'{name}'"))
            .collect::<Vec<_>>()
            .join(", ");
        errors.push(format!(
            "Validation error: Multiple or zero default variants detected: [{names}]. One variant must be set as default."
        ));
    }
}

/// Validation parity: implemented against Python VariantAttribute.validate() for local variant names.
pub(crate) struct VariantAttribute;
impl DiscoverResources for VariantAttribute {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::VARIANT_ATTRIBUTES_FILE.file_path,
        yaml_path: &["attributes"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("attributes") else {
            return vec![];
        };
        let mut out = Vec::new();
        for item in list {
            let Value::Mapping(map) = item else { continue };
            let Some(name) = map.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let safe = clean_name(name, false);
            out.push(rel_under_root(
                base_path,
                &path.join("attributes").join(&safe),
            ));
        }
        out
    }

    fn validate_local_yaml(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_attribute_local_yaml(yaml, errors);
    }
}

pub(crate) fn validate_attribute_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = VariantAttribute::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    let known_variants = yaml
        .get("variants")
        .and_then(Value::as_sequence)
        .map(|variants| {
            variants
                .iter()
                .filter_map(|variant| variant.get("name").and_then(Value::as_str))
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let Some(attributes) = yaml.get("attributes").and_then(Value::as_sequence) else {
        return;
    };
    for (attribute_idx, attribute) in attributes.iter().enumerate() {
        let name = attribute.get("name").and_then(Value::as_str).unwrap_or("");
        let error_path = if name.is_empty() {
            format!("{path}/attributes/{attribute_idx}")
        } else {
            format!("{path}/attributes/{}", clean_name(name, false))
        };
        if name.is_empty() {
            errors.push(format!(
                "Validation error in {error_path}: Name is required"
            ));
        }
        let Some(values) = attribute.get("values").and_then(Value::as_mapping) else {
            errors.push(format!(
                "Validation error in {error_path}: Mappings are required"
            ));
            continue;
        };
        if values.is_empty() {
            errors.push(format!(
                "Validation error in {error_path}: Mappings are required"
            ));
            continue;
        }
        let attribute_variants = values
            .keys()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        let additional_variants = attribute_variants
            .difference(&known_variants)
            .cloned()
            .collect::<Vec<_>>();
        if !additional_variants.is_empty() {
            errors.push(format!(
                "Validation error in {error_path}: Additional variants found for attribute: {{{}}}",
                quoted_set(&additional_variants)
            ));
        }
        let missing_variants = known_variants
            .difference(&attribute_variants)
            .cloned()
            .collect::<Vec<_>>();
        if !missing_variants.is_empty() {
            errors.push(format!(
                "Validation error in {error_path}: Missing variants for variant attribute: [{}]",
                missing_variants
                    .iter()
                    .map(|name| format!("'{name}'"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
}

fn quoted_set(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("variant attributes YAML");
        let mut errors = Vec::new();
        validate_local_yaml(&yaml, &mut errors);
        validate_attribute_local_yaml(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_variant_default_and_attribute_mapping_rules() {
        let errors = validation_errors(
            r#"
variants:
  - name: Control
  - name: Treatment
attributes:
  - name: Channel
    values:
      Control: primary
      Ghost: secondary
  - values:
      Control: only
  - name: Empty
    values: {}
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("Multiple or zero default variants detected"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Additional variants found for attribute"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Missing variants for variant attribute"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Name is required"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Mappings are required"))
        );
    }
}
