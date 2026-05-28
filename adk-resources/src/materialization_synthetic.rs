use crate::CommandGenError;
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_synthetic_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    crate::handoffs::insert_handoff_resources(map, projection)?;
    crate::sms_templates::insert_sms_template_resources(map, projection)?;
    crate::phrase_filters::insert_phrase_filter_resources(map, projection)?;
    crate::experimental_config::insert_experimental_config_resource(map, projection)?;
    Ok(())
}
