use crate::CommandGenError;
use adk_types::ResourceMap;
use serde_json::Value;

pub(super) fn insert_broad_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    crate::variants::insert_variant_resources(map, projection)?;

    crate::api_integrations::insert_api_integration_resources(map, projection)?;

    crate::keyphrase_boosting::insert_keyphrase_boosting_resources(map, projection)?;

    crate::transcript_corrections::insert_transcript_correction_resources(map, projection)?;

    crate::pronunciations::insert_pronunciation_resources(map, projection)?;

    Ok(())
}
