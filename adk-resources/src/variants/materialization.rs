use crate::CommandGenError;
use crate::materialization::insert_yaml_resource;
use crate::specs::{VARIANT_ATTRIBUTE_VALUES, VARIANT_ATTRIBUTES, VARIANTS};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::HashMap;

pub(crate) fn insert_variant_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(value) = variant_attributes_yaml(projection) {
        insert_yaml_resource(
            map,
            VARIANTS.file.file_path,
            VARIANTS.file.resource_id,
            VARIANTS.file.name,
            value,
        )?;
    }

    Ok(())
}

fn variant_attributes_yaml(projection: &Value) -> Option<Value> {
    let variants = VARIANTS.owned_entries(projection);
    let attributes = VARIANT_ATTRIBUTES.owned_entries(projection);
    if variants.is_empty() && attributes.is_empty() {
        return None;
    }

    let variant_names_by_id = variants
        .iter()
        .filter_map(|(id, variant)| {
            Some((
                id.clone(),
                variant
                    .get("name")?
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ))
        })
        .collect::<HashMap<_, _>>();
    let variant_yaml = variants
        .iter()
        .filter_map(|(_, variant)| {
            let name = variant.get("name")?.as_str()?;
            let mut variant_yaml = serde_json::Map::new();
            variant_yaml.insert("name".to_string(), Value::String(name.to_string()));
            if variant
                .get("isDefault")
                .or_else(|| variant.get("is_default"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                variant_yaml.insert("is_default".to_string(), Value::Bool(true));
            }
            Some(Value::Object(variant_yaml))
        })
        .collect::<Vec<_>>();
    let values_by_attribute =
        variant_attribute_values_by_attribute(projection, &variant_names_by_id);
    let attribute_yaml = attributes
        .iter()
        .filter(|(_, attribute)| {
            !attribute
                .get("archived")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|(id, attribute)| {
            let name = attribute.get("name")?.as_str()?;
            Some(serde_json::json!({
                "name": name,
                "values": values_by_attribute.get(id).cloned().unwrap_or_default(),
            }))
        })
        .collect::<Vec<_>>();
    Some(serde_json::json!({
        "variants": variant_yaml,
        "attributes": attribute_yaml,
    }))
}

fn variant_attribute_values_by_attribute(
    projection: &Value,
    variant_names_by_id: &HashMap<String, String>,
) -> HashMap<String, HashMap<String, String>> {
    let mut out: HashMap<String, HashMap<String, String>> = HashMap::new();
    for (variant_id, values) in VARIANT_ATTRIBUTE_VALUES.owned_entries(projection) {
        let Some(variant_name) = variant_names_by_id.get(&variant_id) else {
            continue;
        };
        let Some(values) = values.get("values").and_then(Value::as_object) else {
            continue;
        };
        for (attribute_id, value) in values {
            out.entry(attribute_id.clone()).or_default().insert(
                variant_name.clone(),
                value.as_str().unwrap_or("").to_string(),
            );
        }
    }
    out
}
