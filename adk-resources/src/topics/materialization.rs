use super::topic_entries;
use crate::materialization::{insert_content_resource, to_yaml_string};
use crate::topics::local::LocalTopic;
use crate::{CommandGenError, clean_name};
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_topic_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    for (id, topic) in topic_entries(projection) {
        let topic =
            LocalTopic::from_projection(&id, &topic).map_err(CommandGenError::InvalidData)?;
        let file_name = clean_name(topic.name(), true);
        let file_path = format!("topics/{file_name}.yaml");
        let content =
            to_yaml_string(&topic).map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(map, &file_path, &id, topic.name(), content)?;
    }

    Ok(())
}
