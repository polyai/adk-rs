use crate::functions;
use crate::materialization::to_yaml_string;
use crate::phrase_filters::local::{PhraseFilterItem, PhraseFiltersFile};
use crate::{CommandGenError, extract_entities_vec};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::HashMap;

pub(crate) fn insert_phrase_filter_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let global_function_names = functions::function_entries(projection)
        .into_iter()
        .filter_map(|(id, function)| {
            Some((
                id,
                function
                    .get("name")?
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ))
        })
        .collect::<HashMap<_, _>>();
    let mut phrase_filters = Vec::new();
    for (_id, phrase_filter) in phrase_filter_entries_vec(projection) {
        let function = phrase_filter
            .pointer("/references/globalFunctions")
            .or_else(|| phrase_filter.pointer("/references/global_functions"))
            .and_then(Value::as_object)
            .and_then(|refs| refs.keys().next())
            .map(|function_id| {
                global_function_names
                    .get(function_id)
                    .filter(|name| !name.is_empty())
                    .cloned()
                    .unwrap_or_else(|| function_id.to_string())
            });
        if let Some(item) = local_phrase_filter_from_projection(&phrase_filter, function)? {
            phrase_filters.push(item);
        }
    }
    if phrase_filters.is_empty() {
        return Ok(());
    }

    let content = to_yaml_string(&PhraseFiltersFile::new(phrase_filters))
        .map_err(|error| CommandGenError::InvalidData(error.to_string()))?;
    crate::materialization::insert_content_resource(
        map,
        "voice/response_control/phrase_filtering.yaml",
        "phrase_filtering",
        "phrase_filtering",
        content,
    )
}

fn phrase_filter_entries_vec(projection: &Value) -> Vec<(String, Value)> {
    extract_entities_vec(projection, &["stopKeywords", "filters", "entities"])
}

fn local_phrase_filter_from_projection(
    phrase_filter: &Value,
    function: Option<String>,
) -> Result<Option<PhraseFilterItem>, CommandGenError> {
    let Some(name) = phrase_filter.get("title").and_then(Value::as_str) else {
        return Ok(None);
    };
    PhraseFilterItem::new(
        name.to_string(),
        json_str(phrase_filter, "description"),
        phrase_filter
            .get("regularExpressions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
        phrase_filter
            .get("sayPhrase")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        json_str(phrase_filter, "languageCode"),
        function,
    )
    .map(Some)
    .map_err(invalid_phrase_filter_projection)
}

fn json_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn invalid_phrase_filter_projection(error: String) -> CommandGenError {
    CommandGenError::InvalidData(format!("Invalid phrase filter projection: {error}"))
}
