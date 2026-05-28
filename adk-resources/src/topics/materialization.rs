use super::topic_entries;
use crate::materialization::insert_content_resource;
use crate::yaml_resources::{TopicYaml, to_yaml_string};
use crate::{CommandGenError, clean_name};
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_topic_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    for (id, topic) in topic_entries(projection) {
        let name = topic
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        let file_name = clean_name(&name, true);
        let file_path = format!("topics/{file_name}.yaml");
        let content = to_yaml_string(&TopicYaml {
            name: name.clone(),
            enabled: topic
                .get("isActive")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            actions: topic
                .get("actions")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            content: topic
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            example_queries: topic
                .get("exampleQueries")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| {
                            x.get("query")
                                .and_then(Value::as_str)
                                .map(ToString::to_string)
                        })
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default(),
        })
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(map, &file_path, &id, &name, content)?;
    }

    Ok(())
}
