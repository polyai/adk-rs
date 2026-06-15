use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{ParseLocalResource, ResourceParseResult};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use crate::variants::local::{
    VariantAttributeDefinitionsFile, VariantDefinitionsFile,
    parse_variant_attribute_definitions_file, parse_variant_definitions_file,
};
use serde_yaml_ng::Value;
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
    let path = Variant::LOCAL_PATH.primary_path().expect("local file path");
    <Variant as ParseLocalResource>::append_parse_errors(path, yaml, errors);
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

    fn append_local_resource_errors(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        <Self as ParseLocalResource>::append_parse_errors(
            Self::LOCAL_PATH.primary_path().expect("local file path"),
            yaml,
            errors,
        );
    }
}

#[cfg(test)]
pub(crate) fn append_attribute_parse_errors(yaml: &Value, errors: &mut Vec<String>) {
    let path = VariantAttribute::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    <VariantAttribute as ParseLocalResource>::append_parse_errors(path, yaml, errors);
}

impl ParseLocalResource for Variant {
    type Parsed = VariantDefinitionsFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_variant_definitions_file(path, yaml)
    }
}

impl ParseLocalResource for VariantAttribute {
    type Parsed = VariantAttributeDefinitionsFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_variant_attribute_definitions_file(path, yaml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("variant attributes YAML");
        let mut errors = Vec::new();
        append_parse_errors(&yaml, &mut errors);
        append_attribute_parse_errors(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_variant_default_and_attribute_mapping_rules() {
        let missing_default = validation_errors(
            r#"
variants:
  - name: Control
  - name: Treatment
attributes:
  - name: Channel
    values:
      Control: primary
      Treatment: secondary
"#,
        );
        assert!(
            missing_default
                .iter()
                .any(|error| error.contains("Multiple or zero default variants detected"))
        );

        let attribute_coverage = validation_errors(
            r#"
variants:
  - name: Control
    is_default: true
  - name: Treatment
attributes:
  - name: Channel
    values:
      Control: primary
      Ghost: secondary
"#,
        );
        assert!(
            attribute_coverage
                .iter()
                .any(|error| error.contains("Additional variants found for attribute"))
        );
        assert!(
            attribute_coverage
                .iter()
                .any(|error| error.contains("Missing variants for variant attribute"))
        );

        let missing_name = validation_errors(
            r#"
variants:
  - name: Control
    is_default: true
attributes:
  - values:
      Control: primary
"#,
        );
        assert!(
            missing_name
                .iter()
                .any(|error| error.contains("missing field `name`"))
        );

        let empty_values = validation_errors(
            r#"
variants:
  - name: Control
    is_default: true
attributes:
  - name: Empty
    values: {}
"#,
        );
        assert!(
            empty_values
                .iter()
                .any(|error| error.contains("Mappings are required"))
        );
    }
}
