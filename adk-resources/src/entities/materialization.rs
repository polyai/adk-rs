use crate::entities::local::EntityItem;
use crate::materialization::{insert_content_resource, to_yaml_string};
use crate::specs::ENTITIES_FILE;
use crate::{CommandGenError, extract_entities_vec};
use adk_types::ResourceMap;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct EntitiesYaml {
    entities: Vec<EntityYaml>,
}

#[derive(Serialize)]
struct EntityYaml {
    name: String,
    description: String,
    entity_type: String,
    config: Value,
}

pub(crate) fn insert_entity_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let mut entity_yaml_list = Vec::new();
    for (id, entity) in entity_entries_vec(projection) {
        let Some(entity) = EntityItem::from_projection(&id, &entity)
            .map_err(|error| CommandGenError::InvalidData(format!("{error:?}")))?
        else {
            continue;
        };
        entity_yaml_list.push(EntityYaml {
            name: entity.name().to_string(),
            description: entity.description().to_string(),
            entity_type: entity.entity_type().to_string(),
            config: entity.config_json(),
        });
    }
    if !entity_yaml_list.is_empty() {
        let content = to_yaml_string(&EntitiesYaml {
            entities: entity_yaml_list,
        })
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(
            map,
            ENTITIES_FILE.file_path,
            ENTITIES_FILE.resource_id,
            ENTITIES_FILE.name,
            content,
        )?;
    }

    Ok(())
}

fn entity_entries_vec(projection: &Value) -> Vec<(String, Value)> {
    extract_entities_vec(projection, &["entities", "entities", "entities"])
}
