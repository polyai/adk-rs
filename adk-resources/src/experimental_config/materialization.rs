use crate::CommandGenError;
use crate::materialization::insert_content_resource;
use crate::specs::EXPERIMENTAL_CONFIG_FILE;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_experimental_config_resource(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(features) = experimental_features(projection) {
        let content = serde_json::to_string_pretty(&features)
            .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(
            map,
            EXPERIMENTAL_CONFIG_FILE.file_path,
            EXPERIMENTAL_CONFIG_FILE.resource_id,
            EXPERIMENTAL_CONFIG_FILE.name,
            content,
        )?;
    }

    Ok(())
}

pub(crate) fn experimental_features(projection: &Value) -> Option<Value> {
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
