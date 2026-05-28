//! Platform projection to local resource materialization.

pub(crate) use crate::materialization_reference_handling::{
    FlowImportPathMaps, PromptReferenceMaps, flow_import_path_maps_from_projection,
    prompt_reference_maps_from_projection, replace_flow_import_ids_with_names,
    replace_flow_import_names_with_ids, replace_resource_names_with_ids,
    rewrite_materialized_prompt_references,
};

use crate::CommandGenError;
use adk_types::{Resource, ResourceMap};
use serde::Serialize;
use serde_json::Value;

// Define mapping from projection to resources in file system.
pub fn projection_to_resource_map(projection: &Value) -> Result<ResourceMap, CommandGenError> {
    let mut map = ResourceMap::new();
    let prompt_reference_maps = prompt_reference_maps_from_projection(projection);
    let flow_import_path_maps = flow_import_path_maps_from_projection(projection);

    crate::topics::insert_topic_resources(&mut map, projection)?;
    crate::functions::insert_function_resources(&mut map, projection, &flow_import_path_maps)?;
    crate::flows::insert_flow_resources(&mut map, projection, &flow_import_path_maps)?;
    crate::entities::insert_entity_resources(&mut map, projection)?;
    crate::handoffs::insert_handoff_resources(&mut map, projection)?;
    crate::sms_templates::insert_sms_template_resources(&mut map, projection)?;
    crate::phrase_filters::insert_phrase_filter_resources(&mut map, projection)?;
    crate::experimental_config::insert_experimental_config_resource(&mut map, projection)?;
    crate::variants::insert_variant_resources(&mut map, projection)?;
    crate::api_integrations::insert_api_integration_resources(&mut map, projection)?;
    crate::keyphrase_boosting::insert_keyphrase_boosting_resources(&mut map, projection)?;
    crate::transcript_corrections::insert_transcript_correction_resources(&mut map, projection)?;
    crate::pronunciations::insert_pronunciation_resources(&mut map, projection)?;
    crate::agent_settings::insert_profile_and_safety_resources(&mut map, projection)?;
    crate::asr_settings::insert_asr_settings_resource(&mut map, projection)?;
    crate::channels::insert_channel_resources(&mut map, projection)?;
    crate::agent_settings::insert_rules_resource(&mut map, projection)?;

    rewrite_materialized_prompt_references(&mut map, &prompt_reference_maps);
    Ok(map)
}

pub(crate) fn insert_yaml_resource(
    map: &mut ResourceMap,
    file_path: &str,
    resource_id: &str,
    name: &str,
    value: impl Serialize,
) -> Result<(), CommandGenError> {
    let content =
        to_yaml_string(&value).map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
    insert_content_resource(map, file_path, resource_id, name, content)
}

pub(crate) fn to_yaml_string<T: Serialize>(value: &T) -> Result<String, serde_yaml::Error> {
    serde_yaml::to_string(value)
}

pub(crate) fn insert_content_resource(
    map: &mut ResourceMap,
    file_path: &str,
    resource_id: &str,
    name: &str,
    content: String,
) -> Result<(), CommandGenError> {
    insert_resource(
        map,
        Resource {
            resource_id: resource_id.to_string(),
            name: name.to_string(),
            file_path: file_path.to_string(),
            payload: serde_json::json!({ "content": content }),
        },
    )
}

fn insert_resource(map: &mut ResourceMap, resource: Resource) -> Result<(), CommandGenError> {
    if map.contains_key(&resource.file_path) {
        return Err(CommandGenError::InvalidData(format!(
            "Duplicate resource file path found: {} for resource {}\nPlease rename the resource to avoid conflicts.",
            resource.file_path, resource.name
        )));
    }
    map.insert(resource.file_path.clone(), resource);
    Ok(())
}
