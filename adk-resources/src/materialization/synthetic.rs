use super::insert_content_resource;
use crate::CommandGenError;
use adk_types::ResourceMap;
use serde_json::Value;

pub(super) fn insert_synthetic_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    crate::handoffs::insert_handoff_resources(map, projection)?;
    crate::sms_templates::insert_sms_template_resources(map, projection)?;
    crate::phrase_filters::insert_phrase_filter_resources(map, projection)?;
    insert_experimental_config_resource(map, projection)?;
    Ok(())
}

fn insert_experimental_config_resource(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(features) = experimental_features(projection) {
        let content = serde_json::to_string_pretty(&features)
            .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(
            map,
            "agent_settings/experimental_config.json",
            "experimental_config",
            "experimental_config",
            content,
        )?;
    }

    Ok(())
}

fn experimental_features(projection: &Value) -> Option<Value> {
    Some(
        projection
            .get("experimentalConfig")?
            .get("experimentalConfigs")?
            .get("entities")?
            .get("default")?
            .get("features")?
            .clone(),
    )
}
