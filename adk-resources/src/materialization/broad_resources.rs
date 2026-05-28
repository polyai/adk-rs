use super::insert_yaml_resource;
use crate::CommandGenError;
use crate::specs::FileResourceSpec;
use crate::specs::{KEYPHRASE_BOOSTING, PRONUNCIATIONS, TRANSCRIPT_CORRECTIONS};
use adk_types::ResourceMap;
use serde_json::Value;

pub(super) fn insert_broad_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    crate::variants::insert_variant_resources(map, projection)?;

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
