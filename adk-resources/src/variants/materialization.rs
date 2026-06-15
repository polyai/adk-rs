use crate::CommandGenError;
use crate::materialization::to_yaml_string;
use crate::specs::{VARIANT_ATTRIBUTE_VALUES, VARIANT_ATTRIBUTES, VARIANTS};
use crate::variants::local::{VariantAttributeItem, VariantAttributesFile, VariantItem};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub(crate) fn insert_variant_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let Some(value) = variant_attributes_file(projection)? else {
        return Ok(());
    };

    let content =
        to_yaml_string(&value).map_err(|error| CommandGenError::InvalidData(error.to_string()))?;
    crate::materialization::insert_content_resource(
        map,
        VARIANTS.file.file_path,
        VARIANTS.file.resource_id,
        VARIANTS.file.name,
        content,
    )
}

fn variant_attributes_file(
    projection: &Value,
) -> Result<Option<VariantAttributesFile>, CommandGenError> {
    let variants = VARIANTS.owned_entries(projection);
    let attributes = VARIANT_ATTRIBUTES.owned_entries(projection);
    if variants.is_empty() && attributes.is_empty() {
        return Ok(None);
    }

    let variant_names_by_id = variant_names_by_id(&variants);
    let local_variants = variants
        .iter()
        .filter_map(|(_, variant)| local_variant_from_projection(variant).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    let values_by_attribute =
        variant_attribute_values_by_attribute(projection, &variant_names_by_id);
    let local_attributes = attributes
        .iter()
        .filter(|(_, attribute)| {
            !attribute
                .get("archived")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|(id, attribute)| {
            local_variant_attribute_from_projection(
                attribute,
                values_by_attribute.get(id).cloned().unwrap_or_default(),
            )
            .transpose()
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(VariantAttributesFile::new(
        local_variants,
        local_attributes,
    )))
}

fn variant_names_by_id(variants: &[(String, Value)]) -> HashMap<String, String> {
    variants
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
        .collect()
}

fn local_variant_from_projection(variant: &Value) -> Result<Option<VariantItem>, CommandGenError> {
    let Some(name) = variant.get("name").and_then(Value::as_str) else {
        return Ok(None);
    };
    VariantItem::new(
        name.to_string(),
        variant
            .get("isDefault")
            .or_else(|| variant.get("is_default"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    )
    .map(Some)
    .map_err(invalid_variant_projection)
}

fn local_variant_attribute_from_projection(
    attribute: &Value,
    values: BTreeMap<String, String>,
) -> Result<Option<VariantAttributeItem>, CommandGenError> {
    let Some(name) = attribute.get("name").and_then(Value::as_str) else {
        return Ok(None);
    };
    VariantAttributeItem::new(name.to_string(), values)
        .map(Some)
        .map_err(invalid_variant_projection)
}

fn variant_attribute_values_by_attribute(
    projection: &Value,
    variant_names_by_id: &HashMap<String, String>,
) -> HashMap<String, BTreeMap<String, String>> {
    let mut out: HashMap<String, BTreeMap<String, String>> = HashMap::new();
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

fn invalid_variant_projection(error: String) -> CommandGenError {
    CommandGenError::InvalidData(format!("Invalid variant projection: {error}"))
}
