use crate::CommandGenError;
use crate::materialization::insert_yaml_resource;
use crate::specs::TRANSCRIPT_CORRECTIONS;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_transcript_correction_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(value) = transcript_corrections_yaml(projection) {
        insert_yaml_resource(
            map,
            TRANSCRIPT_CORRECTIONS.file.file_path,
            TRANSCRIPT_CORRECTIONS.file.resource_id,
            TRANSCRIPT_CORRECTIONS.file.name,
            value,
        )?;
    }

    Ok(())
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
