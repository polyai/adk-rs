use crate::CommandGenError;
use crate::materialization::insert_yaml_resource;
use crate::specs::KEYPHRASE_BOOSTING;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_keyphrase_boosting_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(value) = keyphrase_boosting_yaml(projection) {
        insert_yaml_resource(
            map,
            KEYPHRASE_BOOSTING.file.file_path,
            KEYPHRASE_BOOSTING.file.resource_id,
            KEYPHRASE_BOOSTING.file.name,
            value,
        )?;
    }

    Ok(())
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
