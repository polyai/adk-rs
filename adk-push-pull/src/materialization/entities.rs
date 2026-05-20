use super::insert_content_resource;
use crate::yaml_resources::{EntitiesYaml, EntityYaml, to_yaml_string};
use crate::{CommandGenError, extract_entities_vec, snake_case_json_keys, to_snake_case};
use adk_types::ResourceMap;
use serde_json::Value;

pub(super) fn insert_entity_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let mut entity_yaml_list = Vec::new();
    for (id, entity) in entity_entries_vec(projection) {
        let name = entity
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        entity_yaml_list.push(EntityYaml {
            name,
            description: entity
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            entity_type: to_snake_case(entity.get("type").and_then(Value::as_str).unwrap_or("")),
            config: projection_entity_config(&entity),
        });
    }
    if !entity_yaml_list.is_empty() {
        let content = to_yaml_string(&EntitiesYaml {
            entities: entity_yaml_list,
        })
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(map, "config/entities.yaml", "entities", "entities", content)?;
    }

    Ok(())
}

fn projection_entity_config(entity: &Value) -> Value {
    if let Some(cfg) = entity.pointer("/config/value") {
        let mut cfg = cfg.clone();
        snake_case_json_keys(&mut cfg);
        return cfg;
    }
    if let Some(cfg) = entity.get("config") {
        let mut cfg = cfg.clone();
        snake_case_json_keys(&mut cfg);
        return cfg;
    }
    let entity_type = to_snake_case(entity.get("type").and_then(Value::as_str).unwrap_or(""));
    let mut cfg = match entity_type.as_str() {
        "numeric" => entity
            .get("numberConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "alphanumeric" => entity
            .get("alphanumericConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "enum" => entity
            .get("multipleOptionsConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "date" => entity
            .get("dateConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "phone_number" => entity
            .get("phoneNumberConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "time" => entity
            .get("timeConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        _ => serde_json::json!({}),
    };
    snake_case_json_keys(&mut cfg);
    cfg
}

fn entity_entries_vec(projection: &Value) -> Vec<(String, Value)> {
    extract_entities_vec(projection, &["entities", "entities", "entities"])
}
