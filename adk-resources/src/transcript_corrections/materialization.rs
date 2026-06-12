use crate::CommandGenError;
use crate::materialization::to_yaml_string;
use crate::specs::TRANSCRIPT_CORRECTIONS;
use crate::transcript_corrections::local::{
    RegularExpressionRule, TranscriptCorrectionItem, TranscriptCorrectionsFile,
};
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_transcript_correction_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let corrections = transcript_correction_items(projection)?;
    if corrections.is_empty() {
        return Ok(());
    }

    let content = to_yaml_string(&TranscriptCorrectionsFile::new(corrections))
        .map_err(|error| CommandGenError::InvalidData(error.to_string()))?;
    crate::materialization::insert_content_resource(
        map,
        TRANSCRIPT_CORRECTIONS.file.file_path,
        TRANSCRIPT_CORRECTIONS.file.resource_id,
        TRANSCRIPT_CORRECTIONS.file.name,
        content,
    )
}

fn transcript_correction_items(
    projection: &Value,
) -> Result<Vec<TranscriptCorrectionItem>, CommandGenError> {
    TRANSCRIPT_CORRECTIONS
        .owned_entries(projection)
        .iter()
        .filter_map(|(_, correction)| local_correction_from_projection(correction).transpose())
        .collect()
}

fn local_correction_from_projection(
    correction: &Value,
) -> Result<Option<TranscriptCorrectionItem>, CommandGenError> {
    let Some(name) = correction.get("name").and_then(Value::as_str) else {
        return Ok(None);
    };
    let regular_expressions = correction
        .get("regularExpressions")
        .or_else(|| correction.get("regular_expressions"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(local_regex_from_projection)
        .collect::<Result<Vec<_>, _>>()?;
    TranscriptCorrectionItem::from_projection(
        name.to_string(),
        json_str(correction, &["description"]),
        regular_expressions,
    )
    .map(Some)
    .map_err(invalid_transcript_correction_projection)
}

fn local_regex_from_projection(regex: &Value) -> Result<RegularExpressionRule, CommandGenError> {
    RegularExpressionRule::new(
        json_str(regex, &["regularExpression", "regular_expression"]),
        json_str(regex, &["replacement"]),
        json_str(regex, &["replacementType", "replacement_type"]),
    )
    .map_err(invalid_transcript_correction_projection)
}

fn json_str(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

fn invalid_transcript_correction_projection(error: String) -> CommandGenError {
    CommandGenError::InvalidData(format!("Invalid transcript correction projection: {error}"))
}
