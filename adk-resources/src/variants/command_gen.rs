use crate::ids::stable_resource_id;
use crate::push_command;
use crate::push_command_inputs::{json_bool, json_str};
use crate::specs::{VARIANT_ATTRIBUTE_VALUES, VARIANT_ATTRIBUTES, VARIANTS};
use crate::variants::local::{
    VariantAttributeItem as LocalVariantAttributeItem, VariantAttributesFile,
    VariantItem as LocalVariantItem, parse_variant_attributes_content,
};
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::variant::{
    AttributeReferences, AttributeValues, VariantCreateAttribute, VariantCreateVariant,
    VariantDeleteAttribute, VariantDeleteVariant, VariantSetDefaultVariant, VariantUpdateAttribute,
    VariantValues,
};
use adk_types::ResourceMap;
use serde_json::{self, Value as JsonValue, json};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct VariantLifecycleCommands {
    pub(crate) variant_deletes: Vec<adk_protobuf::Command>,
    pub(crate) attribute_deletes: Vec<adk_protobuf::Command>,
    pub(crate) variant_creates: Vec<adk_protobuf::Command>,
    pub(crate) attribute_creates: Vec<adk_protobuf::Command>,
    pub(crate) variant_updates: Vec<adk_protobuf::Command>,
    pub(crate) attribute_updates: Vec<adk_protobuf::Command>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VariantItem {
    id: String,
    name: String,
    is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VariantAttributeItem {
    id: String,
    name: String,
    values: HashMap<String, String>,
}

pub(crate) fn variant_lifecycle_commands(
    resources: &ResourceMap,
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) -> VariantLifecycleCommands {
    let Some(file) = local_variant_attributes_file(resources) else {
        return VariantLifecycleCommands::default();
    };

    let remote_variants = remote_variant_items(projection);
    let remote_attributes = remote_variant_attribute_items(projection);
    let mut local_variants = local_variant_items(&file);
    let mut local_attributes = local_variant_attribute_items(&file);

    let remote_variants_by_name = remote_variants
        .iter()
        .map(|item| (item.name.clone(), item.clone()))
        .collect::<HashMap<_, _>>();
    let remote_attributes_by_name = remote_attributes
        .iter()
        .map(|item| (item.name.clone(), item.clone()))
        .collect::<HashMap<_, _>>();
    let local_variant_names = local_variants
        .iter()
        .map(|item| item.name.clone())
        .collect::<HashSet<_>>();
    let local_attribute_names = local_attributes
        .iter()
        .map(|item| item.name.clone())
        .collect::<HashSet<_>>();

    let mut commands = VariantLifecycleCommands::default();
    for remote in &remote_variants {
        if !local_variant_names.contains(&remote.name) {
            push_command(
                &mut commands.variant_deletes,
                metadata,
                "variant_delete_variant",
                CommandPayload::VariantDeleteVariant(VariantDeleteVariant {
                    id: remote.id.clone(),
                }),
            );
        }
    }
    for remote in &remote_attributes {
        if !local_attribute_names.contains(&remote.name) {
            push_command(
                &mut commands.attribute_deletes,
                metadata,
                "variant_delete_attribute",
                CommandPayload::VariantDeleteAttribute(VariantDeleteAttribute {
                    id: remote.id.clone(),
                }),
            );
        }
    }

    let mut variant_ids_by_name = remote_variants_by_name
        .iter()
        .map(|(name, item)| (name.clone(), item.id.clone()))
        .collect::<HashMap<_, _>>();
    let remote_attribute_ids = remote_attributes
        .iter()
        .map(|item| (item.id.clone(), String::new()))
        .collect::<HashMap<_, _>>();

    for local in &mut local_variants {
        if remote_variants_by_name.contains_key(&local.name) {
            continue;
        }
        let id = stable_resource_id(VARIANTS.id_prefix, &local.name, VARIANTS.file.file_path);
        local.id = id.clone();
        variant_ids_by_name.insert(local.name.clone(), id.clone());
        push_command(
            &mut commands.variant_creates,
            metadata,
            "variant_create_variant",
            CommandPayload::VariantCreateVariant(VariantCreateVariant {
                id,
                name: local.name.clone(),
                attribute_values: Some(AttributeValues {
                    values: remote_attribute_ids.clone(),
                }),
            }),
        );
    }

    let remote_default = remote_variants.iter().find(|item| item.is_default);
    let local_default = local_variants.iter().find(|item| item.is_default);
    if let (Some(remote_default), Some(local_default)) = (remote_default, local_default)
        && remote_default.name != local_default.name
        && let Some(id) = variant_ids_by_name.get(&local_default.name)
    {
        push_command(
            &mut commands.variant_updates,
            metadata,
            "variant_set_default_variant",
            CommandPayload::VariantSetDefaultVariant(VariantSetDefaultVariant { id: id.clone() }),
        );
    }

    for local in &mut local_attributes {
        if remote_attributes_by_name.contains_key(&local.name) {
            continue;
        }
        let id = stable_resource_id(
            VARIANT_ATTRIBUTES.id_prefix,
            &local.name,
            VARIANT_ATTRIBUTES.file.file_path,
        );
        local.id = id.clone();
        push_command(
            &mut commands.attribute_creates,
            metadata,
            "variant_create_attribute",
            CommandPayload::VariantCreateAttribute(VariantCreateAttribute {
                id,
                name: local.name.clone(),
                references: Some(empty_attribute_references()),
                variant_values: Some(VariantValues {
                    values: variant_attribute_values_with_ids(&local.values, &variant_ids_by_name),
                }),
            }),
        );
    }

    for local in &local_attributes {
        let Some(remote) = remote_attributes_by_name.get(&local.name) else {
            continue;
        };
        let values = variant_attribute_values_with_ids(&local.values, &variant_ids_by_name);
        if values == remote.values {
            continue;
        }
        push_command(
            &mut commands.attribute_updates,
            metadata,
            "variant_update_attribute",
            CommandPayload::VariantUpdateAttribute(VariantUpdateAttribute {
                id: remote.id.clone(),
                name: Some(local.name.clone()),
                references: None,
                variant_values: Some(VariantValues { values }),
            }),
        );
    }

    commands
}

fn local_variant_attributes_file(resources: &ResourceMap) -> Option<VariantAttributesFile> {
    let content = resources
        .get(VARIANTS.file.file_path)?
        .payload
        .get("content")?
        .as_str()?;
    parse_variant_attributes_content(VARIANTS.file.file_path, content).ok()
}

fn local_variant_items(file: &VariantAttributesFile) -> Vec<VariantItem> {
    file.variants.iter().map(local_variant_item).collect()
}

fn local_variant_item(item: &LocalVariantItem) -> VariantItem {
    VariantItem {
        id: String::new(),
        name: item.name().to_string(),
        is_default: item.is_default(),
    }
}

fn remote_variant_items(projection: &JsonValue) -> Vec<VariantItem> {
    VARIANTS
        .entries(projection)
        .into_iter()
        .filter_map(|(id, value)| {
            let name = json_str(value, &["name"]);
            if name.is_empty() {
                return None;
            }
            Some(VariantItem {
                id,
                name,
                is_default: json_bool(value, &["isDefault", "is_default"]),
            })
        })
        .collect()
}

fn local_variant_attribute_items(file: &VariantAttributesFile) -> Vec<VariantAttributeItem> {
    file.attributes
        .iter()
        .map(local_variant_attribute_item)
        .collect()
}

fn local_variant_attribute_item(item: &LocalVariantAttributeItem) -> VariantAttributeItem {
    VariantAttributeItem {
        id: String::new(),
        name: item.name().to_string(),
        values: item.values().clone().into_iter().collect(),
    }
}

fn remote_variant_attribute_items(projection: &JsonValue) -> Vec<VariantAttributeItem> {
    let variants_by_id = remote_variant_items(projection)
        .into_iter()
        .map(|item| (item.id, item.name))
        .collect::<HashMap<_, _>>();
    let mut attributes = VARIANT_ATTRIBUTES
        .entries(projection)
        .into_iter()
        .filter_map(|(id, value)| {
            if json_bool(value, &["archived"]) {
                return None;
            }
            let name = json_str(value, &["name"]);
            if name.is_empty() {
                return None;
            }
            Some(VariantAttributeItem {
                id,
                name,
                values: HashMap::new(),
            })
        })
        .collect::<Vec<_>>();
    let attribute_index_by_id = attributes
        .iter()
        .enumerate()
        .map(|(idx, item)| (item.id.clone(), idx))
        .collect::<HashMap<_, _>>();

    for (variant_id, value) in VARIANT_ATTRIBUTE_VALUES.entries(projection) {
        if !variants_by_id.contains_key(&variant_id) {
            continue;
        }
        let Some(values) = value.get("values").and_then(JsonValue::as_object) else {
            continue;
        };
        for (attribute_id, attribute_value) in values {
            let Some(index) = attribute_index_by_id.get(attribute_id).copied() else {
                continue;
            };
            attributes[index].values.insert(
                variant_id.clone(),
                attribute_value.as_str().unwrap_or("").to_string(),
            );
        }
    }
    attributes
}

fn variant_attribute_values_with_ids(
    values: &HashMap<String, String>,
    variant_ids_by_name: &HashMap<String, String>,
) -> HashMap<String, String> {
    values
        .iter()
        .filter_map(|(variant_name, value)| {
            Some((
                variant_ids_by_name.get(variant_name)?.clone(),
                value.clone(),
            ))
        })
        .collect()
}

fn empty_attribute_references() -> AttributeReferences {
    AttributeReferences {
        topics: HashMap::new(),
        flow_steps: HashMap::new(),
        no_code_steps: HashMap::new(),
    }
}

pub(crate) fn attribute_values_json(values: Option<&AttributeValues>) -> JsonValue {
    let Some(values) = values else {
        return json!({});
    };
    if values.values.is_empty() {
        json!({})
    } else {
        json!({ "values": values.values })
    }
}

pub(crate) fn attribute_references_json(references: Option<&AttributeReferences>) -> JsonValue {
    let Some(references) = references else {
        return json!({});
    };
    let mut value = serde_json::Map::new();
    if !references.topics.is_empty() {
        value.insert("topics".to_string(), json!(references.topics));
    }
    if !references.flow_steps.is_empty() {
        value.insert("flow_steps".to_string(), json!(references.flow_steps));
    }
    if !references.no_code_steps.is_empty() {
        value.insert("no_code_steps".to_string(), json!(references.no_code_steps));
    }
    JsonValue::Object(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_types::{Resource, ResourceMap};

    #[test]
    fn local_variant_items_use_typed_file_model() {
        let file = parse_variant_attributes_content(
            VARIANTS.file.file_path,
            r#"
variants:
  - name: Control
    is_default: true
  - name: Treatment
attributes:
  - name: Channel
    values:
      Control: primary
      Treatment: secondary
"#,
        )
        .expect("variant attributes yaml");

        let variants = local_variant_items(&file);
        let attributes = local_variant_attribute_items(&file);

        assert_eq!(variants[0].name, "Control");
        assert!(variants[0].is_default);
        assert_eq!(attributes[0].name, "Channel");
        assert_eq!(
            attributes[0].values.get("Treatment"),
            Some(&"secondary".to_string())
        );
    }

    #[test]
    fn parse_errors_do_not_delete_remote_variants_or_attributes() {
        let mut resources = ResourceMap::new();
        resources.insert(
            VARIANTS.file.file_path.to_string(),
            Resource {
                resource_id: VARIANTS.file.resource_id.to_string(),
                name: VARIANTS.file.name.to_string(),
                file_path: VARIANTS.file.file_path.to_string(),
                payload: json!({
                    "content": "variants: definitely not a list\nattributes: []\n",
                }),
            },
        );
        let projection = json!({
            "variantManagement": {
                "variants": {
                    "ids": ["variant-control"],
                    "entities": {
                        "variant-control": {
                            "id": "variant-control",
                            "name": "Control",
                            "isDefault": true
                        }
                    }
                },
                "attributes": {
                    "ids": ["attr-channel"],
                    "entities": {
                        "attr-channel": {
                            "id": "attr-channel",
                            "name": "Channel",
                            "archived": false
                        }
                    }
                },
                "variantAttributeValues": {
                    "ids": ["variant-control"],
                    "entities": {
                        "variant-control": {
                            "values": {
                                "attr-channel": "primary"
                            }
                        }
                    }
                }
            }
        });

        let commands = variant_lifecycle_commands(&resources, &projection, &None);

        assert!(commands.variant_deletes.is_empty());
        assert!(commands.attribute_deletes.is_empty());
        assert!(commands.variant_creates.is_empty());
        assert!(commands.attribute_creates.is_empty());
        assert!(commands.variant_updates.is_empty());
        assert!(commands.attribute_updates.is_empty());
    }
}
