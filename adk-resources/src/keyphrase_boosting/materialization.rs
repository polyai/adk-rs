use crate::CommandGenError;
use crate::keyphrase_boosting::local::{KeyphraseBoostingFile, KeyphraseItem};
use crate::materialization::to_yaml_string;
use crate::specs::KEYPHRASE_BOOSTING;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_keyphrase_boosting_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let keyphrases = keyphrase_boosting_items(projection)?;
    if keyphrases.is_empty() {
        return Ok(());
    }

    let content = to_yaml_string(&KeyphraseBoostingFile::new(keyphrases))
        .map_err(|error| CommandGenError::InvalidData(error.to_string()))?;
    crate::materialization::insert_content_resource(
        map,
        KEYPHRASE_BOOSTING.file.file_path,
        KEYPHRASE_BOOSTING.file.resource_id,
        KEYPHRASE_BOOSTING.file.name,
        content,
    )
}

fn keyphrase_boosting_items(projection: &Value) -> Result<Vec<KeyphraseItem>, CommandGenError> {
    KEYPHRASE_BOOSTING
        .owned_entries(projection)
        .iter()
        .filter_map(|(_, item)| local_keyphrase_item_from_projection(item).transpose())
        .collect()
}

fn local_keyphrase_item_from_projection(
    item: &Value,
) -> Result<Option<KeyphraseItem>, CommandGenError> {
    let Some(keyphrase) = item.get("keyphrase").and_then(Value::as_str) else {
        return Ok(None);
    };
    KeyphraseItem::new(
        keyphrase.to_string(),
        item.get("level")
            .and_then(Value::as_str)
            .unwrap_or("default")
            .to_string(),
    )
    .map(Some)
    .map_err(invalid_keyphrase_projection)
}

fn invalid_keyphrase_projection(error: String) -> CommandGenError {
    CommandGenError::InvalidData(format!("Invalid keyphrase boosting projection: {error}"))
}
