use crate::clean_name;
use crate::flows::flow_entries;
use crate::projection::projection_entity_values;
use crate::specs::{AGENT_RULES_FILE, VARIANT_ATTRIBUTES};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub(crate) struct PromptReferenceMaps {
    global_id_to_name: HashMap<(String, String), String>,
    global_name_to_id: HashMap<(String, String), String>,
    flow_scoped_id_to_name: HashMap<String, HashMap<(String, String), String>>,
    flow_scoped_name_to_id: HashMap<String, HashMap<(String, String), String>>,
}

impl PromptReferenceMaps {
    fn insert_global(&mut self, prefix: &str, resource_id: &str, resource_name: &str) {
        if prefix.is_empty() || resource_id.is_empty() || resource_name.is_empty() {
            return;
        }
        self.global_id_to_name.insert(
            (prefix.to_string(), resource_id.to_string()),
            resource_name.to_string(),
        );
        self.global_name_to_id.insert(
            (prefix.to_string(), resource_name.to_string()),
            resource_id.to_string(),
        );
    }

    fn insert_flow_scoped(
        &mut self,
        flow_folder_name: &str,
        prefix: &str,
        resource_id: &str,
        resource_name: &str,
    ) {
        if flow_folder_name.is_empty()
            || prefix.is_empty()
            || resource_id.is_empty()
            || resource_name.is_empty()
        {
            return;
        }
        self.flow_scoped_id_to_name
            .entry(flow_folder_name.to_string())
            .or_default()
            .insert(
                (prefix.to_string(), resource_id.to_string()),
                resource_name.to_string(),
            );
        self.flow_scoped_name_to_id
            .entry(flow_folder_name.to_string())
            .or_default()
            .insert(
                (prefix.to_string(), resource_name.to_string()),
                resource_id.to_string(),
            );
    }

    fn id_to_name<'a>(
        &'a self,
        prefix: &str,
        value: &str,
        flow_folder_name: Option<&str>,
    ) -> Option<&'a str> {
        if let Some(name) = self
            .global_id_to_name
            .get(&(prefix.to_string(), value.to_string()))
        {
            return Some(name.as_str());
        }
        flow_folder_name.and_then(|flow_folder_name| {
            self.flow_scoped_id_to_name
                .get(flow_folder_name)
                .and_then(|map| map.get(&(prefix.to_string(), value.to_string())))
                .map(String::as_str)
        })
    }

    fn name_to_id<'a>(
        &'a self,
        prefix: &str,
        value: &str,
        flow_folder_name: Option<&str>,
    ) -> Option<&'a str> {
        if let Some(resource_id) = self
            .global_name_to_id
            .get(&(prefix.to_string(), value.to_string()))
        {
            return Some(resource_id.as_str());
        }
        flow_folder_name.and_then(|flow_folder_name| {
            self.flow_scoped_name_to_id
                .get(flow_folder_name)
                .and_then(|map| map.get(&(prefix.to_string(), value.to_string())))
                .map(String::as_str)
        })
    }
}

pub(crate) fn prompt_reference_maps_from_projection(projection: &Value) -> PromptReferenceMaps {
    let mut maps = PromptReferenceMaps::default();

    for (id, function) in projection_entity_values(projection, &["functions", "functions"]) {
        if function
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str());
        maps.insert_global("fn", &id, name);
        if let Some(resource_id) = function.get("id").and_then(Value::as_str) {
            maps.insert_global("fn", resource_id, name);
        }
    }

    for (id, sms) in projection_entity_values(projection, &["sms", "templates"]) {
        if !sms.get("active").and_then(Value::as_bool).unwrap_or(true) {
            continue;
        }
        let name = sms
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str());
        maps.insert_global("twilio_sms", &id, name);
        if let Some(resource_id) = sms.get("id").and_then(Value::as_str) {
            maps.insert_global("twilio_sms", resource_id, name);
        }
    }

    for (id, handoff) in projection_entity_values(projection, &["handoff", "handoffs"]) {
        if !handoff
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            continue;
        }
        let name = handoff
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str());
        maps.insert_global("ho", &id, name);
        if let Some(resource_id) = handoff.get("id").and_then(Value::as_str) {
            maps.insert_global("ho", resource_id, name);
        }
    }

    for (id, variable) in projection_entity_values(projection, &["variables", "variables"]) {
        if variable
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let name = variable
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str());
        maps.insert_global("vrbl", &id, name);
        maps.insert_global("var", &id, name);
        if let Some(resource_id) = variable.get("id").and_then(Value::as_str) {
            maps.insert_global("vrbl", resource_id, name);
            maps.insert_global("var", resource_id, name);
        }
    }

    for (id, entity) in projection_entity_values(projection, &["entities", "entities"]) {
        if entity
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let name = entity
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str());
        maps.insert_global("entity", &id, name);
        if let Some(resource_id) = entity.get("id").and_then(Value::as_str) {
            maps.insert_global("entity", resource_id, name);
        }
    }

    let translations = projection_entity_values(projection, &["translations", "translations"]);
    for (id, translation) in translations {
        if translation
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let name = translation
            .get("translationKey")
            .or_else(|| translation.get("translation_key"))
            .or_else(|| translation.get("name"))
            .and_then(Value::as_str)
            .unwrap_or(id.as_str());
        maps.insert_global("tn", &id, name);
        if let Some(resource_id) = translation.get("id").and_then(Value::as_str) {
            maps.insert_global("tn", resource_id, name);
        }
    }

    for (id, attribute) in VARIANT_ATTRIBUTES.owned_entries(projection) {
        if attribute
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let name = attribute
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str());
        maps.insert_global("attr", &id, name);
        if let Some(resource_id) = attribute.get("id").and_then(Value::as_str) {
            maps.insert_global("attr", resource_id, name);
        }
    }

    for (flow_id, flow) in flow_entries(projection) {
        let flow_name = flow
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(flow_id.as_str());
        let flow_folder_name = clean_name(flow_name, true);
        for (transition_id, function) in projection_entity_values(&flow, &["transitionFunctions"])
            .into_iter()
            .chain(projection_entity_values(&flow, &["transition_functions"]))
        {
            if function
                .get("archived")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                continue;
            }
            let function_name = function
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(transition_id.as_str());
            maps.insert_flow_scoped(&flow_folder_name, "ft", &transition_id, function_name);
            if let Some(resource_id) = function.get("id").and_then(Value::as_str) {
                maps.insert_flow_scoped(&flow_folder_name, "ft", resource_id, function_name);
            }
        }
    }

    maps
}

pub(crate) fn replace_resource_ids_with_names(
    prompt: &str,
    prompt_reference_maps: &PromptReferenceMaps,
    flow_folder_name: Option<&str>,
) -> String {
    replace_prompt_references_with_lookup(prompt, |prefix, value| {
        prompt_reference_maps.id_to_name(prefix, value, flow_folder_name)
    })
}

pub(crate) fn replace_resource_names_with_ids(
    prompt: &str,
    prompt_reference_maps: &PromptReferenceMaps,
    flow_folder_name: Option<&str>,
) -> String {
    replace_prompt_references_with_lookup(prompt, |prefix, value| {
        prompt_reference_maps.name_to_id(prefix, value, flow_folder_name)
    })
}

fn replace_prompt_references_with_lookup<'a, F>(prompt: &str, mut lookup: F) -> String
where
    F: FnMut(&str, &str) -> Option<&'a str>,
{
    let mut out = String::with_capacity(prompt.len());
    let mut start = 0usize;
    while let Some(open_relative) = prompt[start..].find("{{") {
        let open = start + open_relative;
        out.push_str(&prompt[start..open]);
        let content_start = open + 2;
        let Some(close_relative) = prompt[content_start..].find("}}") else {
            out.push_str(&prompt[open..]);
            return out;
        };
        let close = content_start + close_relative;
        let reference = &prompt[content_start..close];
        if let Some((prefix, value)) = reference.split_once(':')
            && let Some(replacement) = lookup(prefix, value)
        {
            out.push_str("{{");
            out.push_str(prefix);
            out.push(':');
            out.push_str(replacement);
            out.push_str("}}");
        } else {
            out.push_str(&prompt[open..close + 2]);
        }
        start = close + 2;
    }
    out.push_str(&prompt[start..]);
    out
}

pub(crate) fn rewrite_materialized_prompt_references(
    map: &mut ResourceMap,
    prompt_reference_maps: &PromptReferenceMaps,
) {
    for resource in map.values_mut() {
        let file_path = resource.file_path.as_str();
        if !materialized_prompt_reference_file(file_path) {
            continue;
        }
        let Some(content) = resource
            .payload
            .get("content")
            .and_then(Value::as_str)
            .map(ToString::to_string)
        else {
            continue;
        };
        let flow_folder_name = flow_folder_name_for_path(file_path);
        let updated =
            replace_resource_ids_with_names(&content, prompt_reference_maps, flow_folder_name);
        if updated != content {
            resource.payload["content"] = Value::String(updated);
        }
    }
}

fn materialized_prompt_reference_file(file_path: &str) -> bool {
    file_path == AGENT_RULES_FILE.file_path
        || file_path == "config/sms_templates.yaml"
        || (file_path.starts_with("topics/") && file_path.ends_with(".yaml"))
        || (file_path.starts_with("flows/")
            && file_path.contains("/steps/")
            && file_path.ends_with(".yaml"))
}

fn flow_folder_name_for_path(file_path: &str) -> Option<&str> {
    let mut parts = file_path.split('/');
    if parts.next() == Some("flows") {
        parts.next()
    } else {
        None
    }
}

#[derive(Debug, Default)]
pub(crate) struct FlowImportPathMaps {
    id_to_flow_folder: HashMap<String, String>,
    flow_folder_to_id: HashMap<String, String>,
}

pub(crate) fn flow_import_path_maps_from_projection(projection: &Value) -> FlowImportPathMaps {
    let mut maps = FlowImportPathMaps::default();
    for (flow_key, flow) in flow_entries(projection) {
        let flow_id = flow
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(flow_key.as_str());
        let flow_name = flow.get("name").and_then(Value::as_str).unwrap_or(flow_id);
        maps.insert(flow_id, flow_name);
        if flow_id != flow_key {
            maps.insert(flow_key.as_str(), flow_name);
        }
    }
    maps
}

impl FlowImportPathMaps {
    fn insert(&mut self, flow_id: &str, flow_name: &str) {
        let cleaned_id = clean_name(flow_id, true);
        let cleaned_name = clean_name(flow_name, true);
        if cleaned_id.is_empty() || cleaned_name.is_empty() {
            return;
        }
        self.id_to_flow_folder
            .insert(cleaned_id.clone(), cleaned_name.clone());
        self.flow_folder_to_id.insert(cleaned_name, cleaned_id);
    }
}

pub(crate) fn replace_flow_import_ids_with_names(
    code: &str,
    flow_import_path_maps: &FlowImportPathMaps,
) -> String {
    let mut normalized = code.to_string();
    for (flow_id, flow_folder) in &flow_import_path_maps.id_to_flow_folder {
        normalized = normalized.replace(
            &format!("functions.{flow_id}"),
            &format!("flows.{flow_folder}.functions"),
        );
    }
    normalized
}

pub(crate) fn replace_flow_import_names_with_ids(
    code: &str,
    flow_import_path_maps: &FlowImportPathMaps,
) -> String {
    let mut normalized = code.to_string();
    for (flow_folder, flow_id) in &flow_import_path_maps.flow_folder_to_id {
        normalized = normalized.replace(
            &format!("flows.{flow_folder}.functions"),
            &format!("functions.{flow_id}"),
        );
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_reference_maps_round_trip_python_reference_prefixes() {
        let projection = serde_json::json!({
            "functions": {
                "functions": {
                    "entities": {
                        "FUNCTION-start_verification": {
                            "id": "FUNCTION-start_verification",
                            "name": "start_verification",
                            "archived": false
                        }
                    }
                }
            },
            "sms": {
                "templates": {
                    "entities": {
                        "SMS_TEMPLATE-123": {
                            "id": "SMS_TEMPLATE-123",
                            "name": "test_template",
                            "active": true
                        }
                    }
                }
            },
            "handoff": {
                "handoffs": {
                    "entities": {
                        "handoff-1": {
                            "id": "handoff-1",
                            "name": "default",
                            "active": true
                        }
                    }
                }
            },
            "variantManagement": {
                "attributes": {
                    "entities": {
                        "attr-customer-name": {
                            "id": "attr-customer-name",
                            "name": "customer-name",
                            "archived": false
                        }
                    }
                }
            },
            "entities": {
                "entities": {
                    "entities": {
                        "ENTITY-customer_name": {
                            "id": "ENTITY-customer_name",
                            "name": "customer_name"
                        }
                    }
                }
            },
            "variables": {
                "variables": {
                    "entities": {
                        "VAR-customer_name": {
                            "id": "VAR-customer_name",
                            "name": "customer_name"
                        }
                    }
                }
            },
            "translations": {
                "translations": {
                    "entities": {
                        "translation-1": {
                            "id": "translation-1",
                            "translationKey": "greeting"
                        }
                    }
                }
            },
            "flows": {
                "flows": {
                    "entities": {
                        "FLOW-address": {
                            "id": "FLOW-address",
                            "name": "Address Flow",
                            "transitionFunctions": {
                                "entities": {
                                    "FUNCTION-determine_language": {
                                        "id": "FUNCTION-determine_language",
                                        "name": "determine_language",
                                        "archived": false
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        let maps = prompt_reference_maps_from_projection(&projection);
        let cases = [
            ("fn", "FUNCTION-start_verification", "start_verification"),
            ("twilio_sms", "SMS_TEMPLATE-123", "test_template"),
            ("ho", "handoff-1", "default"),
            ("attr", "attr-customer-name", "customer-name"),
            ("ft", "FUNCTION-determine_language", "determine_language"),
            ("entity", "ENTITY-customer_name", "customer_name"),
            ("vrbl", "VAR-customer_name", "customer_name"),
            ("tn", "translation-1", "greeting"),
        ];
        let id_prompt = cases
            .iter()
            .map(|(prefix, id, _)| format!("{{{{{prefix}:{id}}}}}"))
            .collect::<Vec<_>>()
            .join(" ");
        let named_prompt = cases
            .iter()
            .map(|(prefix, _, name)| format!("{{{{{prefix}:{name}}}}}"))
            .collect::<Vec<_>>()
            .join(" ");

        assert_eq!(
            replace_resource_ids_with_names(&id_prompt, &maps, Some("address_flow")),
            named_prompt
        );
        assert_eq!(
            replace_resource_names_with_ids(&named_prompt, &maps, Some("address_flow")),
            id_prompt
        );
    }

    #[test]
    fn prompt_reference_maps_preserve_variable_alias() {
        let projection = serde_json::json!({
            "variables": {
                "variables": {
                    "entities": {
                        "VAR-customer_name": {
                            "id": "VAR-customer_name",
                            "name": "customer_name"
                        }
                    }
                }
            }
        });
        let maps = prompt_reference_maps_from_projection(&projection);

        assert_eq!(
            replace_resource_ids_with_names("{{var:VAR-customer_name}}", &maps, None),
            "{{var:customer_name}}"
        );
        assert_eq!(
            replace_resource_names_with_ids("{{var:customer_name}}", &maps, None),
            "{{var:VAR-customer_name}}"
        );
    }
}
