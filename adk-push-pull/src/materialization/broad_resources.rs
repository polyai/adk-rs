use super::{insert_yaml_resource, projection_entities, projection_entities_at};
use crate::CommandGenError;
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::HashMap;

pub(super) fn insert_broad_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(value) = variant_attributes_yaml(projection) {
        insert_yaml_resource(
            map,
            "config/variant_attributes.yaml",
            "variant_attributes",
            "variant_attributes",
            value,
        )?;
    }

    if let Some(value) = api_integrations_yaml(projection) {
        insert_yaml_resource(
            map,
            "config/api_integrations.yaml",
            "api_integrations",
            "api_integrations",
            value,
        )?;
    }

    if let Some(value) = keyphrase_boosting_yaml(projection) {
        insert_yaml_resource(
            map,
            "voice/speech_recognition/keyphrase_boosting.yaml",
            "keyphrase_boosting",
            "keyphrase_boosting",
            value,
        )?;
    }

    if let Some(value) = transcript_corrections_yaml(projection) {
        insert_yaml_resource(
            map,
            "voice/speech_recognition/transcript_corrections.yaml",
            "transcript_corrections",
            "transcript_corrections",
            value,
        )?;
    }

    if let Some(value) = pronunciations_yaml(projection) {
        insert_yaml_resource(
            map,
            "voice/response_control/pronunciations.yaml",
            "pronunciations",
            "pronunciations",
            value,
        )?;
    }

    Ok(())
}

fn variant_attributes_yaml(projection: &Value) -> Option<Value> {
    let variants = projection_entities(projection, &["variantManagement", "variants"]);
    let attributes = projection_entities(projection, &["variantManagement", "attributes"]);
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
    for (variant_id, values) in
        projection_entities(projection, &["variantManagement", "variantAttributeValues"])
    {
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

fn api_integrations_yaml(projection: &Value) -> Option<Value> {
    let integrations = projection_entities(projection, &["apiIntegrations", "apiIntegrations"]);
    if integrations.is_empty() {
        return None;
    }
    let integrations = integrations
        .iter()
        .filter_map(|(_, integration)| {
            let name = integration.get("name")?.as_str()?;
            Some(serde_json::json!({
                "name": name,
                "description": integration.get("description").and_then(Value::as_str).unwrap_or(""),
                "environments": api_integration_environments_yaml(integration),
                "operations": api_integration_operations_yaml(integration),
            }))
        })
        .collect::<Vec<_>>();
    Some(serde_json::json!({ "api_integrations": integrations }))
}

fn api_integration_environments_yaml(integration: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for (yaml_key, source_keys) in [
        ("sandbox", &["sandbox"][..]),
        (
            "pre-release",
            &["pre-release", "preRelease", "pre_release"][..],
        ),
        ("live", &["live"][..]),
    ] {
        if let Some(env) = source_keys.iter().find_map(|key| {
            integration
                .get("environments")
                .and_then(|envs| envs.get(*key))
        }) {
            out.insert(
                yaml_key.to_string(),
                serde_json::json!({
                    "base_url": env.get("baseUrl").or_else(|| env.get("base_url")).and_then(Value::as_str).unwrap_or(""),
                    "auth_type": env.get("authType").or_else(|| env.get("auth_type")).and_then(Value::as_str).unwrap_or(""),
                }),
            );
        }
    }
    Value::Object(out)
}

fn api_integration_operations_yaml(integration: &Value) -> Value {
    let operations = integration
        .get("operations")
        .map(projection_entities_at)
        .unwrap_or_default();
    let operations = if operations.is_empty() {
        integration
            .get("operations")
            .and_then(Value::as_object)
            .map(|object| {
                let mut items = object
                    .iter()
                    .map(|(id, value)| (id.clone(), value.clone()))
                    .collect::<Vec<_>>();
                items.sort_by(|(left, _), (right, _)| left.cmp(right));
                items
            })
            .unwrap_or_default()
    } else {
        operations
    };
    Value::Array(
        operations
            .into_iter()
            .filter_map(|(_, operation)| {
                let name = operation.get("name")?.as_str()?;
                Some(serde_json::json!({
                    "name": name,
                    "method": operation.get("method").and_then(Value::as_str).unwrap_or(""),
                    "resource": operation.get("resource").and_then(Value::as_str).unwrap_or(""),
                }))
            })
            .collect(),
    )
}

fn keyphrase_boosting_yaml(projection: &Value) -> Option<Value> {
    let keyphrases = projection_entities(projection, &["keyphraseBoosting", "keyphraseBoosting"]);
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
    let corrections = projection_entities(
        projection,
        &["transcriptCorrections", "transcriptCorrections"],
    );
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
    let mut pronunciations = projection_entities(projection, &["pronunciations", "pronunciations"]);
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
