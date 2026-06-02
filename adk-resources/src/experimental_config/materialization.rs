use crate::CommandGenError;
use crate::materialization::insert_content_resource;
use crate::specs::EXPERIMENTAL_CONFIG_FILE;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_experimental_config_resource(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some((id, features)) = experimental_config_entry(projection) {
        let content = serde_json::to_string_pretty(&features)
            .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(
            map,
            EXPERIMENTAL_CONFIG_FILE.file_path,
            &id,
            EXPERIMENTAL_CONFIG_FILE.name,
            content,
        )?;
    }

    Ok(())
}

pub(crate) fn experimental_config_entry(projection: &Value) -> Option<(String, Value)> {
    let entities = projection
        .get("experimentalConfig")?
        .get("experimentalConfigs")?
        .get("entities")?
        .as_object()?;
    let (id, config) = entities.iter().next()?;
    Some((id.clone(), config.get("features")?.clone()))
}

pub(crate) fn experimental_features(projection: &Value) -> Option<Value> {
    experimental_config_entry(projection).map(|(_, features)| features)
}
