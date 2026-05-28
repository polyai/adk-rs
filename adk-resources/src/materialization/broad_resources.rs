use super::insert_yaml_resource;
use crate::CommandGenError;
use crate::specs::FileResourceSpec;
use crate::specs::{
    KEYPHRASE_BOOSTING, PRONUNCIATIONS, TRANSCRIPT_CORRECTIONS, VARIANT_ATTRIBUTE_VALUES,
    VARIANT_ATTRIBUTES, VARIANTS,
};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::HashMap;

pub(super) fn insert_broad_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(value) = variant_attributes_yaml(projection) {
        insert_spec_yaml_resource(map, VARIANTS.file, value)?;
    }

    crate::api_integrations::insert_api_integration_resources(map, projection)?;

    if let Some(value) = keyphrase_boosting_yaml(projection) {
        insert_spec_yaml_resource(map, KEYPHRASE_BOOSTING.file, value)?;
    }

    if let Some(value) = transcript_corrections_yaml(projection) {
        insert_spec_yaml_resource(map, TRANSCRIPT_CORRECTIONS.file, value)?;
    }

    if let Some(value) = pronunciations_yaml(projection) {
        insert_spec_yaml_resource(map, PRONUNCIATIONS.file, value)?;
    }

    Ok(())
}

fn insert_spec_yaml_resource(
    map: &mut ResourceMap,
    spec: FileResourceSpec,
    value: Value,
) -> Result<(), CommandGenError> {
    insert_yaml_resource(map, spec.file_path, spec.resource_id, spec.name, value)
}

fn variant_attributes_yaml(projection: &Value) -> Option<Value> {
    let variants = VARIANTS.owned_entries(projection);
    let attributes = VARIANT_ATTRIBUTES.owned_entries(projection);
    if variants.is_empty() && attributes.is_empty() {
        return None;
    }

    let variant_names_by_id = variants
        .iter()
        .filter_map(|(id, variant)| {
            Some((
                id.clone(),
                variant
                    .get("name")?
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ))
        })
        .collect::<HashMap<_, _>>();
    let variant_yaml = variants
        .iter()
        .filter_map(|(_, variant)| {
            let name = variant.get("name")?.as_str()?;
            let mut variant_yaml = serde_json::Map::new();
            variant_yaml.insert("name".to_string(), Value::String(name.to_string()));
            if variant
                .get("isDefault")
                .or_else(|| variant.get("is_default"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                variant_yaml.insert("is_default".to_string(), Value::Bool(true));
            }
            Some(Value::Object(variant_yaml))
        })
        .collect::<Vec<_>>();
    let values_by_attribute =
        variant_attribute_values_by_attribute(projection, &variant_names_by_id);
    let attribute_yaml = attributes
        .iter()
        .filter(|(_, attribute)| {
            !attribute
                .get("archived")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|(id, attribute)| {
            let name = attribute.get("name")?.as_str()?;
            Some(serde_json::json!({
                "name": name,
                "values": values_by_attribute.get(id).cloned().unwrap_or_default(),
            }))
        })
        .collect::<Vec<_>>();
    Some(serde_json::json!({
        "variants": variant_yaml,
        "attributes": attribute_yaml,
    }))
}

fn variant_attribute_values_by_attribute(
    projection: &Value,
    variant_names_by_id: &HashMap<String, String>,
) -> HashMap<String, HashMap<String, String>> {
    let mut out: HashMap<String, HashMap<String, String>> = HashMap::new();
    for (variant_id, values) in VARIANT_ATTRIBUTE_VALUES.owned_entries(projection) {
        let Some(variant_name) = variant_names_by_id.get(&variant_id) else {
            continue;
        };
        let Some(values) = values.get("values").and_then(Value::as_object) else {
            continue;
        };
        for (attribute_id, value) in values {
            out.entry(attribute_id.clone()).or_default().insert(
                variant_name.clone(),
                value.as_str().unwrap_or("").to_string(),
            );
        }
    }
    out
}

fn keyphrase_boosting_yaml(projection: &Value) -> Option<Value> {
    let keyphrases = KEYPHRASE_BOOSTING.owned_entries(projection);
    if keyphrases.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "keyphrases": keyphrases
            .iter()
            .filter_map(|(_, item)| {
                Some(serde_json::json!({
                    "keyphrase": item.get("keyphrase")?.as_str()?,
                    "level": item.get("level").and_then(Value::as_str).unwrap_or(""),
                }))
            })
            .collect::<Vec<_>>()
    }))
}

fn transcript_corrections_yaml(projection: &Value) -> Option<Value> {
    let corrections = TRANSCRIPT_CORRECTIONS.owned_entries(projection);
    if corrections.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "corrections": corrections
            .iter()
            .filter_map(|(_, correction)| {
                let name = correction.get("name")?.as_str()?;
                Some(serde_json::json!({
                    "name": name,
                    "description": correction.get("description").and_then(Value::as_str).unwrap_or(""),
                    "regular_expressions": correction
                        .get("regularExpressions")
                        .or_else(|| correction.get("regular_expressions"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|regex| serde_json::json!({
                            "regular_expression": regex.get("regularExpression").or_else(|| regex.get("regular_expression")).and_then(Value::as_str).unwrap_or(""),
                            "replacement": regex.get("replacement").and_then(Value::as_str).unwrap_or(""),
                            "replacement_type": regex.get("replacementType").or_else(|| regex.get("replacement_type")).and_then(Value::as_str).unwrap_or(""),
                        }))
                        .collect::<Vec<_>>(),
                }))
            })
            .collect::<Vec<_>>()
    }))
}

fn pronunciations_yaml(projection: &Value) -> Option<Value> {
    let mut pronunciations = PRONUNCIATIONS.owned_entries(projection);
    if pronunciations.is_empty() {
        return None;
    }
    pronunciations
        .sort_by_key(|(_, item)| item.get("position").and_then(Value::as_i64).unwrap_or(0));
    Some(serde_json::json!({
        "pronunciations": pronunciations
            .iter()
            .filter_map(|(_, item)| {
                let regex = item.get("regex")?.as_str()?;
                let mut pronunciation = serde_json::Map::new();
                pronunciation.insert("regex".to_string(), Value::String(regex.to_string()));
                pronunciation.insert(
                    "replacement".to_string(),
                    Value::String(
                        item.get("replacement")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    ),
                );
                pronunciation.insert(
                    "case_sensitive".to_string(),
                    Value::Bool(
                        item.get("caseSensitive")
                            .or_else(|| item.get("case_sensitive"))
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    ),
                );
                insert_non_empty_string(
                    &mut pronunciation,
                    "language_code",
                    item.get("languageCode")
                        .or_else(|| item.get("language_code"))
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                );
                insert_non_empty_string(
                    &mut pronunciation,
                    "description",
                    item.get("description").and_then(Value::as_str).unwrap_or(""),
                );
                Some(Value::Object(pronunciation))
            })
            .collect::<Vec<_>>()
    }))
}

fn insert_non_empty_string(map: &mut serde_json::Map<String, Value>, key: &str, value: &str) {
    if !value.is_empty() {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}
