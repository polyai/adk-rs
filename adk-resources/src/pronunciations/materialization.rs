use crate::CommandGenError;
use crate::materialization::to_yaml_string;
use crate::pronunciations::local::{PronunciationItem, PronunciationsFile};
use crate::specs::PRONUNCIATIONS;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_pronunciation_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let pronunciations = pronunciation_items(projection)?;
    if pronunciations.is_empty() {
        return Ok(());
    }

    let content = to_yaml_string(&PronunciationsFile::new(pronunciations))
        .map_err(|error| CommandGenError::InvalidData(error.to_string()))?;
    crate::materialization::insert_content_resource(
        map,
        PRONUNCIATIONS.file.file_path,
        PRONUNCIATIONS.file.resource_id,
        PRONUNCIATIONS.file.name,
        content,
    )
}

fn pronunciation_items(projection: &Value) -> Result<Vec<PronunciationItem>, CommandGenError> {
    let mut pronunciations = PRONUNCIATIONS.owned_entries(projection);
    pronunciations
        .sort_by_key(|(_, item)| item.get("position").and_then(Value::as_i64).unwrap_or(0));
    pronunciations
        .iter()
        .filter_map(|(_, item)| local_pronunciation_item_from_projection(item).transpose())
        .collect()
}

fn local_pronunciation_item_from_projection(
    item: &Value,
) -> Result<Option<PronunciationItem>, CommandGenError> {
    let Some(regex) = item.get("regex").and_then(Value::as_str) else {
        return Ok(None);
    };
    PronunciationItem::new(
        regex.to_string(),
        json_str(item, &["replacement"]),
        json_bool(item, &["caseSensitive", "case_sensitive"]),
        json_str(item, &["languageCode", "language_code"]),
        json_str(item, &["description"]),
        json_str(item, &["name"]),
    )
    .map(Some)
    .map_err(invalid_pronunciation_projection)
}

fn json_str(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

fn json_bool(value: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_bool))
        .unwrap_or(false)
}

fn invalid_pronunciation_projection(error: String) -> CommandGenError {
    CommandGenError::InvalidData(format!("Invalid pronunciation projection: {error}"))
}
