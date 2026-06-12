use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{
    NonEmptyString, ParseLocalResource, ResourceParseErrors, ResourceParseResult, deserialize_yaml,
    duplicate_names, non_empty_map,
};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde::Deserialize;
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
pub(crate) fn validate_attribute_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = VariantAttribute::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    <VariantAttribute as ParseLocalResource>::append_parse_errors(path, yaml, errors);
}

impl ParseLocalResource for Variant {
    type Parsed = VariantDefinitionsFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        let raw = deserialize_yaml::<VariantAttributesFileUnchecked>(path, yaml)?;
        VariantDefinitionsFile::try_from_unchecked(path, raw)
    }
}

impl ParseLocalResource for VariantAttribute {
    type Parsed = VariantAttributeDefinitionsFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        let raw = deserialize_yaml::<VariantAttributesFileUnchecked>(path, yaml)?;
        VariantAttributeDefinitionsFile::try_from_unchecked(path, raw)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct VariantDefinitionsFile {
    variants: Vec<VariantItem>,
}

impl VariantDefinitionsFile {
    fn try_from_unchecked(
        path: &str,
        raw: VariantAttributesFileUnchecked,
    ) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        for duplicate in duplicate_names(raw.variants.iter().map(|variant| variant.name.as_str())) {
            errors.push(
                &format!("{path}/variants/{duplicate}"),
                format!("duplicate variant name '{duplicate}'."),
            );
        }
        let default_names = raw
            .variants
            .iter()
            .filter(|variant| variant.is_default)
            .map(|variant| variant.name.as_str().to_string())
            .collect::<Vec<_>>();
        if default_names.len() != 1 {
            errors.push_validation(
                format!(
                    "Multiple or zero default variants detected: [{}]. One variant must be set as default.",
                    quoted_set(&default_names)
                ),
            );
        }
        if errors.is_empty() {
            Ok(Self {
                variants: raw.variants,
            })
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct VariantAttributeDefinitionsFile {
    attributes: Vec<VariantAttributeItem>,
}

impl VariantAttributeDefinitionsFile {
    fn try_from_unchecked(
        path: &str,
        raw: VariantAttributesFileUnchecked,
    ) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        let known_variants = raw
            .variants
            .iter()
            .map(|variant| variant.name.as_str().to_string())
            .collect::<BTreeSet<_>>();
        for attribute in &raw.attributes {
            let error_path = format!(
                "{path}/attributes/{}",
                clean_name(attribute.name.as_str(), false)
            );
            let attribute_variants = attribute.values.keys().cloned().collect::<BTreeSet<_>>();
            let additional_variants = attribute_variants
                .difference(&known_variants)
                .cloned()
                .collect::<Vec<_>>();
            if !additional_variants.is_empty() {
                errors.push(
                    &error_path,
                    format!(
                        "Additional variants found for attribute: {{{}}}",
                        quoted_set(&additional_variants)
                    ),
                );
            }
            let missing_variants = known_variants
                .difference(&attribute_variants)
                .cloned()
                .collect::<Vec<_>>();
            if !missing_variants.is_empty() {
                errors.push(
                    &error_path,
                    format!(
                        "Missing variants for variant attribute: [{}]",
                        quoted_set(&missing_variants)
                    ),
                );
            }
        }
        if errors.is_empty() {
            Ok(Self {
                attributes: raw.attributes,
            })
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Deserialize)]
struct VariantAttributesFileUnchecked {
    #[serde(default)]
    variants: Vec<VariantItem>,
    #[serde(default)]
    attributes: Vec<VariantAttributeItem>,
}

#[derive(Debug, Deserialize)]
struct VariantItem {
    name: NonEmptyString,
    #[serde(default)]
    is_default: bool,
}

#[derive(Debug, Deserialize)]
struct VariantAttributeItem {
    name: NonEmptyString,
    #[serde(deserialize_with = "variant_attribute_values")]
    values: std::collections::BTreeMap<String, String>,
}

fn variant_attribute_values<'de, D>(
    deserializer: D,
) -> Result<std::collections::BTreeMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    non_empty_map(deserializer, "Mappings are required")
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
        append_parse_errors(&yaml, &mut errors);
        validate_attribute_local_yaml(&yaml, &mut errors);
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
