use crate::local_parse::{
    NonEmptyString, ResourceParseErrors, ResourceParseResult, deserialize_yaml, duplicate_names,
};
use crate::resource_utils::clean_name;
use serde::Deserialize;
use serde_yaml_ng::Value;
use std::collections::BTreeSet;

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct VariantDefinitionsFile {
    variants: Vec<VariantItem>,
}

impl VariantDefinitionsFile {
    fn try_from_file(path: &str, raw: VariantAttributesFile) -> ResourceParseResult<Self> {
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

pub(crate) fn parse_variant_definitions_file(
    path: &str,
    yaml: &Value,
) -> ResourceParseResult<VariantDefinitionsFile> {
    let raw = parse_variant_attributes_file(path, yaml)?;
    VariantDefinitionsFile::try_from_file(path, raw)
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct VariantAttributeDefinitionsFile {
    attributes: Vec<VariantAttributeItem>,
}

impl VariantAttributeDefinitionsFile {
    fn try_from_file(path: &str, raw: VariantAttributesFile) -> ResourceParseResult<Self> {
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
            if attribute.values.is_empty() {
                errors.push(&error_path, "Mappings are required");
            }
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

pub(crate) fn parse_variant_attribute_definitions_file(
    path: &str,
    yaml: &Value,
) -> ResourceParseResult<VariantAttributeDefinitionsFile> {
    let raw = parse_variant_attributes_file(path, yaml)?;
    VariantAttributeDefinitionsFile::try_from_file(path, raw)
}

#[derive(Debug, Deserialize)]
pub(crate) struct VariantAttributesFile {
    #[serde(default)]
    pub(crate) variants: Vec<VariantItem>,
    #[serde(default)]
    pub(crate) attributes: Vec<VariantAttributeItem>,
}

pub(crate) fn parse_variant_attributes_file(
    path: &str,
    yaml: &Value,
) -> ResourceParseResult<VariantAttributesFile> {
    deserialize_yaml(path, yaml)
}

#[derive(Debug, Deserialize)]
pub(crate) struct VariantItem {
    name: NonEmptyString,
    #[serde(default)]
    is_default: bool,
}

impl VariantItem {
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn is_default(&self) -> bool {
        self.is_default
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct VariantAttributeItem {
    name: NonEmptyString,
    #[serde(default)]
    values: std::collections::BTreeMap<String, String>,
}

impl VariantAttributeItem {
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn values(&self) -> &std::collections::BTreeMap<String, String> {
        &self.values
    }
}

fn quoted_set(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ")
}
