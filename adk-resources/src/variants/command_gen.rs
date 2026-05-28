use crate::command_gen::local_file_helpers::{
    json_bool, json_str, resource_yaml, yaml_bool, yaml_sequence, yaml_string_map,
};
use crate::ids::stable_resource_id;
use crate::specs::{VARIANT_ATTRIBUTE_VALUES, VARIANT_ATTRIBUTES, VARIANTS};
use crate::{push_command, yaml_str};
use adk_protobuf::Metadata;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::variant::{
    AttributeReferences, AttributeValues, VariantCreateAttribute, VariantCreateVariant,
    VariantDeleteAttribute, VariantDeleteVariant, VariantSetDefaultVariant, VariantUpdateAttribute,
    VariantValues,
};
use adk_types::ResourceMap;
use serde_json::{Value, json};
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
    projection: &Value,
    metadata: &Option<Metadata>,
) -> VariantLifecycleCommands {
    let Some(yaml) = resource_yaml(resources, VARIANTS.file.file_path) else {
        return VariantLifecycleCommands::default();
    };

    let remote_variants = remote_variant_items(projection);
    let remote_attributes = remote_variant_attribute_items(projection);
    let mut local_variants = local_variant_items(&yaml);
    let mut local_attributes = local_variant_attribute_items(&yaml);

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

fn local_variant_items(yaml: &serde_yaml::Value) -> Vec<VariantItem> {
    yaml_sequence(yaml, VARIANTS.yaml_key)
        .into_iter()
        .filter_map(|item| {
            let name = yaml_str(item, "name");
            if name.is_empty() {
                return None;
            }
            Some(VariantItem {
                id: String::new(),
                name,
                is_default: yaml_bool(item, "is_default"),
            })
        })
        .collect()
}

fn remote_variant_items(projection: &Value) -> Vec<VariantItem> {
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

fn local_variant_attribute_items(yaml: &serde_yaml::Value) -> Vec<VariantAttributeItem> {
    yaml_sequence(yaml, VARIANT_ATTRIBUTES.yaml_key)
        .into_iter()
        .filter_map(|item| {
            let name = yaml_str(item, "name");
            if name.is_empty() {
                return None;
            }
            Some(VariantAttributeItem {
                id: String::new(),
                name,
                values: yaml_string_map(item.get("values")),
            })
        })
        .collect()
}

fn remote_variant_attribute_items(projection: &Value) -> Vec<VariantAttributeItem> {
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
        let Some(values) = value.get("values").and_then(Value::as_object) else {
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

pub(crate) fn attribute_values_json(values: Option<&AttributeValues>) -> Value {
    let Some(values) = values else {
        return json!({});
    };
    if values.values.is_empty() {
        json!({})
    } else {
        json!({ "values": values.values })
    }
}

pub(crate) fn attribute_references_json(references: Option<&AttributeReferences>) -> Value {
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
    Value::Object(value)
}
