//! Platform projection to local resource materialization.

mod entities;
mod flows;
mod functions;
mod references;
mod topics;

pub(crate) use references::{
    FlowImportPathMaps, PromptReferenceMaps, flow_import_path_maps_from_projection,
    prompt_reference_maps_from_projection, replace_flow_import_ids_with_names,
    replace_flow_import_names_with_ids, replace_resource_names_with_ids,
    rewrite_materialized_prompt_references,
};

use crate::yaml_resources::{
    EnvPhoneNumbersYaml, HandoffYaml, HandoffsYaml, PhraseFilterYaml, PhraseFilteringYaml,
    SmsTemplateYaml, SmsTemplatesYaml, to_yaml_string,
};
use crate::{CommandGenError, command_gen, extract_entities_vec};
use adk_types::{Resource, ResourceMap};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

// Define mapping from projection to resources in file system.
pub fn projection_to_resource_map(projection: &Value) -> Result<ResourceMap, CommandGenError> {
    let mut map = ResourceMap::new();
    let prompt_reference_maps = prompt_reference_maps_from_projection(projection);
    let flow_import_path_maps = flow_import_path_maps_from_projection(projection);

    topics::insert_topic_resources(&mut map, projection)?;
    functions::insert_function_resources(&mut map, projection, &flow_import_path_maps)?;
    flows::insert_flow_resources(&mut map, projection, &flow_import_path_maps)?;
    entities::insert_entity_resources(&mut map, projection)?;

    // config/handoffs.yaml multi-resource file
    let mut handoff_yaml_list = Vec::new();
    for (_id, handoff) in handoff_entries_vec(projection) {
        if !handoff
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            continue;
        }
        handoff_yaml_list.push(HandoffYaml {
            name: handoff
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            description: handoff
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            is_default: handoff
                .get("isDefault")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            sip_config: handoff_sip_config_yaml(&handoff),
            sip_headers: handoff_sip_headers_yaml(&handoff),
        });
    }
    if !handoff_yaml_list.is_empty() {
        let content = to_yaml_string(&HandoffsYaml {
            handoffs: handoff_yaml_list,
        })
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(
            &mut map,
            "config/handoffs.yaml",
            "handoffs",
            "handoffs",
            content,
        )?;
    }

    // config/sms_templates.yaml multi-resource file
    let mut sms_yaml_list = Vec::new();
    for (_id, sms) in sms_entries_vec(projection) {
        if !sms.get("active").and_then(Value::as_bool).unwrap_or(true) {
            continue;
        }
        sms_yaml_list.push(SmsTemplateYaml {
            name: sms
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            text: sms
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            env_phone_numbers: EnvPhoneNumbersYaml {
                sandbox: sms
                    .get("envPhoneNumbers")
                    .and_then(|v| v.get("sandbox"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                pre_release: sms
                    .get("envPhoneNumbers")
                    .and_then(|v| v.get("preRelease"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                live: sms
                    .get("envPhoneNumbers")
                    .and_then(|v| v.get("live"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            },
        });
    }
    if !sms_yaml_list.is_empty() {
        let content = to_yaml_string(&SmsTemplatesYaml {
            sms_templates: sms_yaml_list,
        })
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(
            &mut map,
            "config/sms_templates.yaml",
            "sms_templates",
            "sms_templates",
            content,
        )?;
    }

    // phrase filters
    let global_function_names = command_gen::functions::function_entries(projection)
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
    let mut phrase_yaml_list = Vec::new();
    for (_id, pf) in phrase_filter_entries_vec(projection) {
        let function = pf
            .pointer("/references/globalFunctions")
            .or_else(|| pf.pointer("/references/global_functions"))
            .and_then(Value::as_object)
            .and_then(|refs| refs.keys().next())
            .map(|function_id| {
                global_function_names
                    .get(function_id)
                    .filter(|name| !name.is_empty())
                    .cloned()
                    .unwrap_or_else(|| function_id.to_string())
            });
        phrase_yaml_list.push(PhraseFilterYaml {
            name: pf
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            description: pf
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            regular_expressions: pf
                .get("regularExpressions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            say_phrase: pf
                .get("sayPhrase")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            language_code: pf
                .get("languageCode")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            function,
        });
    }
    if !phrase_yaml_list.is_empty() {
        let content = to_yaml_string(&PhraseFilteringYaml {
            phrase_filtering: phrase_yaml_list,
        })
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(
            &mut map,
            "voice/response_control/phrase_filtering.yaml",
            "phrase_filtering",
            "phrase_filtering",
            content,
        )?;
    }

    if let Some(features) = experimental_features(projection) {
        let content = serde_json::to_string_pretty(&features)
            .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(
            &mut map,
            "agent_settings/experimental_config.json",
            "experimental_config",
            "experimental_config",
            content,
        )?;
    }

    if let Some(value) = variant_attributes_yaml(projection) {
        insert_yaml_resource(
            &mut map,
            "config/variant_attributes.yaml",
            "variant_attributes",
            "variant_attributes",
            value,
        )?;
    }

    if let Some(value) = api_integrations_yaml(projection) {
        insert_yaml_resource(
            &mut map,
            "config/api_integrations.yaml",
            "api_integrations",
            "api_integrations",
            value,
        )?;
    }

    if let Some(value) = keyphrase_boosting_yaml(projection) {
        insert_yaml_resource(
            &mut map,
            "voice/speech_recognition/keyphrase_boosting.yaml",
            "keyphrase_boosting",
            "keyphrase_boosting",
            value,
        )?;
    }

    if let Some(value) = transcript_corrections_yaml(projection) {
        insert_yaml_resource(
            &mut map,
            "voice/speech_recognition/transcript_corrections.yaml",
            "transcript_corrections",
            "transcript_corrections",
            value,
        )?;
    }

    if let Some(value) = pronunciations_yaml(projection) {
        insert_yaml_resource(
            &mut map,
            "voice/response_control/pronunciations.yaml",
            "pronunciations",
            "pronunciations",
            value,
        )?;
    }

    if let Some(personality) = projection.pointer("/agentSettings/personality") {
        insert_yaml_resource(
            &mut map,
            "agent_settings/personality.yaml",
            "personality",
            "personality",
            personality_yaml(personality),
        )?;
    }

    if let Some(role) = projection.pointer("/agentSettings/role") {
        insert_yaml_resource(
            &mut map,
            "agent_settings/role.yaml",
            "role",
            "role",
            role_yaml(role),
        )?;
    }

    if let Some(safety_filters) = projection.get("contentFilterSettings") {
        insert_yaml_resource(
            &mut map,
            "agent_settings/safety_filters.yaml",
            "safety_filters",
            "safety_filters",
            safety_filters_yaml(safety_filters, false),
        )?;
    }

    if let Some(voice_safety_filters) = projection.pointer("/channels/voice/config/safetyFilters") {
        insert_yaml_resource(
            &mut map,
            "voice/safety_filters.yaml",
            "voice_safety_filters",
            "voice_safety_filters",
            safety_filters_yaml(voice_safety_filters, true),
        )?;
    }

    if let Some(asr_settings) = projection
        .pointer("/channels/voice/asrSettings")
        .or_else(|| projection.get("asrSettings"))
    {
        insert_yaml_resource(
            &mut map,
            "voice/speech_recognition/asr_settings.yaml",
            "asr_settings",
            "asr_settings",
            asr_settings_yaml(asr_settings),
        )?;
    }

    let voice_greeting = projection
        .pointer("/channels/voice/config/greeting")
        .cloned();
    let voice_style_prompt = projection
        .pointer("/channels/voice/config/stylePrompt")
        .cloned();
    let voice_disclaimer = projection.pointer("/channels/voice/disclaimer").cloned();
    if voice_greeting.is_some() || voice_style_prompt.is_some() || voice_disclaimer.is_some() {
        insert_yaml_resource(
            &mut map,
            "voice/configuration.yaml",
            "voice_configuration",
            "voice_configuration",
            channel_configuration_yaml(
                voice_greeting.as_ref(),
                voice_style_prompt.as_ref(),
                voice_disclaimer.as_ref(),
            ),
        )?;
    }

    if web_chat_channel_is_created(projection.pointer("/channels/webChat")) {
        let chat_greeting = projection
            .pointer("/channels/webChat/config/greeting")
            .cloned();
        let chat_style_prompt = projection
            .pointer("/channels/webChat/config/stylePrompt")
            .cloned();
        if chat_greeting.is_some() || chat_style_prompt.is_some() {
            insert_yaml_resource(
                &mut map,
                "chat/configuration.yaml",
                "chat_configuration",
                "chat_configuration",
                channel_configuration_yaml(
                    chat_greeting.as_ref(),
                    chat_style_prompt.as_ref(),
                    None,
                ),
            )?;
        }
        if let Some(chat_safety_filters) =
            projection.pointer("/channels/webChat/config/safetyFilters")
        {
            insert_yaml_resource(
                &mut map,
                "chat/safety_filters.yaml",
                "chat_safety_filters",
                "chat_safety_filters",
                safety_filters_yaml(chat_safety_filters, true),
            )?;
        }
    }

    if let Some(behaviour) = projection
        .pointer("/agentSettings/rules/behaviour")
        .and_then(Value::as_str)
    {
        insert_content_resource(
            &mut map,
            "agent_settings/rules.txt",
            "rules",
            "rules",
            behaviour.to_string(),
        )?;
    }
    rewrite_materialized_prompt_references(&mut map, &prompt_reference_maps);
    Ok(map)
}

pub(super) fn projection_entities(root: &Value, path: &[&str]) -> Vec<(String, Value)> {
    let mut current = root;
    for key in path {
        let Some(next) = current.get(*key) else {
            return Vec::new();
        };
        current = next;
    }
    projection_entities_at(current)
}

pub(super) fn projection_nested_entities(root: &Value, path: &[&str]) -> Vec<(String, Value)> {
    let mut current = root;
    for key in path {
        let Some(next) = current.get(*key) else {
            return Vec::new();
        };
        current = next;
    }
    projection_entities_at(current)
}

pub(super) fn projection_entities_at(value: &Value) -> Vec<(String, Value)> {
    let Some(entities) = value.get("entities").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(ids) = value.get("ids").and_then(Value::as_array) {
        for id in ids.iter().filter_map(Value::as_str) {
            if let Some(entity) = entities.get(id) {
                out.push((id.to_string(), entity.clone()));
                seen.insert(id.to_string());
            }
        }
    }
    let mut remaining = entities
        .iter()
        .filter(|(id, _)| !seen.contains(*id))
        .collect::<Vec<_>>();
    remaining.sort_by_key(|(left, _)| *left);
    out.extend(
        remaining
            .into_iter()
            .map(|(id, entity)| (id.clone(), entity.clone())),
    );
    out
}

fn variant_attributes_yaml(projection: &Value) -> Option<Value> {
    let variants = projection_entities(projection, &["variantManagement", "variants"]);
    let attributes = projection_entities(projection, &["variantManagement", "attributes"]);
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
    for (variant_id, values) in
        projection_entities(projection, &["variantManagement", "variantAttributeValues"])
    {
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

fn api_integrations_yaml(projection: &Value) -> Option<Value> {
    let integrations = projection_entities(projection, &["apiIntegrations", "apiIntegrations"]);
    if integrations.is_empty() {
        return None;
    }
    let integrations = integrations
        .iter()
        .filter_map(|(_, integration)| {
            let name = integration.get("name")?.as_str()?;
            Some(serde_json::json!({
                "name": name,
                "description": integration.get("description").and_then(Value::as_str).unwrap_or(""),
                "environments": api_integration_environments_yaml(integration),
                "operations": api_integration_operations_yaml(integration),
            }))
        })
        .collect::<Vec<_>>();
    Some(serde_json::json!({ "api_integrations": integrations }))
}

fn api_integration_environments_yaml(integration: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for (yaml_key, source_keys) in [
        ("sandbox", &["sandbox"][..]),
        (
            "pre-release",
            &["pre-release", "preRelease", "pre_release"][..],
        ),
        ("live", &["live"][..]),
    ] {
        if let Some(env) = source_keys.iter().find_map(|key| {
            integration
                .get("environments")
                .and_then(|envs| envs.get(*key))
        }) {
            out.insert(
                yaml_key.to_string(),
                serde_json::json!({
                    "base_url": env.get("baseUrl").or_else(|| env.get("base_url")).and_then(Value::as_str).unwrap_or(""),
                    "auth_type": env.get("authType").or_else(|| env.get("auth_type")).and_then(Value::as_str).unwrap_or(""),
                }),
            );
        }
    }
    Value::Object(out)
}

fn api_integration_operations_yaml(integration: &Value) -> Value {
    let operations = integration
        .get("operations")
        .map(projection_entities_at)
        .unwrap_or_default();
    let operations = if operations.is_empty() {
        integration
            .get("operations")
            .and_then(Value::as_object)
            .map(|object| {
                let mut items = object
                    .iter()
                    .map(|(id, value)| (id.clone(), value.clone()))
                    .collect::<Vec<_>>();
                items.sort_by(|(left, _), (right, _)| left.cmp(right));
                items
            })
            .unwrap_or_default()
    } else {
        operations
    };
    Value::Array(
        operations
            .into_iter()
            .filter_map(|(_, operation)| {
                let name = operation.get("name")?.as_str()?;
                Some(serde_json::json!({
                    "name": name,
                    "method": operation.get("method").and_then(Value::as_str).unwrap_or(""),
                    "resource": operation.get("resource").and_then(Value::as_str).unwrap_or(""),
                }))
            })
            .collect(),
    )
}

fn keyphrase_boosting_yaml(projection: &Value) -> Option<Value> {
    let keyphrases = projection_entities(projection, &["keyphraseBoosting", "keyphraseBoosting"]);
    if keyphrases.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "keyphrases": keyphrases
            .iter()
            .filter_map(|(_, item)| {
                Some(serde_json::json!({
                    "keyphrase": item.get("keyphrase")?.as_str()?,
                    "level": item.get("level").and_then(Value::as_str).unwrap_or(""),
                }))
            })
            .collect::<Vec<_>>()
    }))
}

fn transcript_corrections_yaml(projection: &Value) -> Option<Value> {
    let corrections = projection_entities(
        projection,
        &["transcriptCorrections", "transcriptCorrections"],
    );
    if corrections.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "corrections": corrections
            .iter()
            .filter_map(|(_, correction)| {
                let name = correction.get("name")?.as_str()?;
                Some(serde_json::json!({
                    "name": name,
                    "description": correction.get("description").and_then(Value::as_str).unwrap_or(""),
                    "regular_expressions": correction
                        .get("regularExpressions")
                        .or_else(|| correction.get("regular_expressions"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|regex| serde_json::json!({
                            "regular_expression": regex.get("regularExpression").or_else(|| regex.get("regular_expression")).and_then(Value::as_str).unwrap_or(""),
                            "replacement": regex.get("replacement").and_then(Value::as_str).unwrap_or(""),
                            "replacement_type": regex.get("replacementType").or_else(|| regex.get("replacement_type")).and_then(Value::as_str).unwrap_or(""),
                        }))
                        .collect::<Vec<_>>(),
                }))
            })
            .collect::<Vec<_>>()
    }))
}

fn pronunciations_yaml(projection: &Value) -> Option<Value> {
    let mut pronunciations = projection_entities(projection, &["pronunciations", "pronunciations"]);
    if pronunciations.is_empty() {
        return None;
    }
    pronunciations
        .sort_by_key(|(_, item)| item.get("position").and_then(Value::as_i64).unwrap_or(0));
    Some(serde_json::json!({
        "pronunciations": pronunciations
            .iter()
            .filter_map(|(_, item)| {
                let regex = item.get("regex")?.as_str()?;
                let mut pronunciation = serde_json::Map::new();
                pronunciation.insert("regex".to_string(), Value::String(regex.to_string()));
                pronunciation.insert(
                    "replacement".to_string(),
                    Value::String(
                        item.get("replacement")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    ),
                );
                pronunciation.insert(
                    "case_sensitive".to_string(),
                    Value::Bool(
                        item.get("caseSensitive")
                            .or_else(|| item.get("case_sensitive"))
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    ),
                );
                insert_non_empty_string(
                    &mut pronunciation,
                    "language_code",
                    item.get("languageCode")
                        .or_else(|| item.get("language_code"))
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                );
                insert_non_empty_string(
                    &mut pronunciation,
                    "description",
                    item.get("description").and_then(Value::as_str).unwrap_or(""),
                );
                Some(Value::Object(pronunciation))
            })
            .collect::<Vec<_>>()
    }))
}

fn insert_non_empty_string(map: &mut serde_json::Map<String, Value>, key: &str, value: &str) {
    if !value.is_empty() {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn asr_settings_yaml(settings: &Value) -> Value {
    let latency_config = settings
        .get("latencyConfig")
        .or_else(|| settings.get("latency_config"));
    serde_json::json!({
        "barge_in": settings
            .get("bargeIn")
            .or_else(|| settings.get("barge_in"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "interaction_style": latency_config
            .and_then(|config| {
                config
                    .get("interactionStyle")
                    .or_else(|| config.get("interaction_style"))
            })
            .or_else(|| {
                settings
                    .get("interactionStyle")
                    .or_else(|| settings.get("interaction_style"))
            })
            .and_then(Value::as_str)
            .unwrap_or("balanced"),
    })
}

fn web_chat_channel_is_created(channel: Option<&Value>) -> bool {
    let Some(channel) = channel else {
        return false;
    };
    match channel.get("status") {
        Some(Value::Bool(status)) => *status,
        Some(Value::Number(status)) => status.as_i64().is_some_and(|status| status != 0),
        Some(Value::String(status)) => {
            !matches!(status.as_str(), "" | "0" | "NOT_CREATED" | "not_created")
        }
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}

fn safety_filters_yaml(settings: &Value, include_enabled: bool) -> Value {
    let azure_config = settings
        .get("azureConfig")
        .or_else(|| settings.get("azure_config"))
        .unwrap_or(&Value::Null);
    let mut categories = serde_json::Map::new();
    for (yaml_key, backend_keys) in [
        ("violence", ["violence", "violence"]),
        ("hate", ["hate", "hate"]),
        ("sexual", ["sexual", "sexual"]),
        ("self_harm", ["selfHarm", "self_harm"]),
    ] {
        let category = backend_keys
            .iter()
            .find_map(|key| azure_config.get(*key))
            .map(safety_filter_category_yaml)
            .unwrap_or_else(|| serde_json::json!({}));
        categories.insert(yaml_key.to_string(), category);
    }

    let mut value = serde_json::Map::new();
    if include_enabled {
        value.insert(
            "enabled".to_string(),
            Value::Bool(
                !settings
                    .get("disabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
        );
    }
    value.insert("categories".to_string(), Value::Object(categories));
    Value::Object(value)
}

fn safety_filter_category_yaml(category: &Value) -> Value {
    serde_json::json!({
        "enabled": category
            .get("isActive")
            .or_else(|| category.get("is_active"))
            .and_then(Value::as_bool),
        "level": safety_filter_precision_level(
            category
                .get("precision")
                .and_then(Value::as_str)
                .unwrap_or_default()
        ),
    })
}

fn safety_filter_precision_level(precision: &str) -> String {
    match precision {
        "LOOSE" => "lenient".to_string(),
        "MEDIUM" => "medium".to_string(),
        "STRICT" => "strict".to_string(),
        value => value.to_ascii_lowercase(),
    }
}

fn channel_configuration_yaml(
    greeting: Option<&Value>,
    style_prompt: Option<&Value>,
    disclaimer: Option<&Value>,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert(
        "greeting".to_string(),
        greeting
            .map(channel_greeting_yaml)
            .unwrap_or_else(|| serde_json::json!({})),
    );
    value.insert(
        "style_prompt".to_string(),
        style_prompt
            .map(channel_style_prompt_yaml)
            .unwrap_or_else(|| serde_json::json!({})),
    );
    if let Some(disclaimer) = disclaimer {
        value.insert(
            "disclaimer_messages".to_string(),
            channel_disclaimer_yaml(disclaimer),
        );
    }
    Value::Object(value)
}

fn channel_greeting_yaml(greeting: &Value) -> Value {
    serde_json::json!({
        "welcome_message": greeting
            .get("welcomeMessage")
            .or_else(|| greeting.get("welcome_message"))
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "language_code": greeting
            .get("languageCode")
            .or_else(|| greeting.get("language_code"))
            .and_then(Value::as_str)
            .unwrap_or("en-GB"),
    })
}

fn channel_style_prompt_yaml(style_prompt: &Value) -> Value {
    serde_json::json!({
        "prompt": style_prompt
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    })
}

fn channel_disclaimer_yaml(disclaimer: &Value) -> Value {
    serde_json::json!({
        "message": disclaimer
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "enabled": disclaimer
            .get("isEnabled")
            .or_else(|| disclaimer.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "language_code": disclaimer
            .get("languageCode")
            .or_else(|| disclaimer.get("language_code"))
            .and_then(Value::as_str)
            .unwrap_or("en-GB"),
    })
}

fn personality_yaml(personality: &Value) -> Value {
    let adjectives = personality
        .pointer("/adjectives/values")
        .or_else(|| personality.get("adjectives"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    serde_json::json!({
        "adjectives": adjectives,
        "custom": personality
            .get("custom")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    })
}

fn role_yaml(role: &Value) -> Value {
    serde_json::json!({
        "value": role
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "additional_info": role
            .get("additionalInfo")
            .or_else(|| role.get("additional_info"))
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "custom": role
            .get("custom")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    })
}

pub(super) fn insert_yaml_resource(
    map: &mut ResourceMap,
    file_path: &str,
    resource_id: &str,
    name: &str,
    value: impl Serialize,
) -> Result<(), CommandGenError> {
    let content =
        to_yaml_string(&value).map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
    insert_content_resource(map, file_path, resource_id, name, content)
}

pub(super) fn insert_content_resource(
    map: &mut ResourceMap,
    file_path: &str,
    resource_id: &str,
    name: &str,
    content: String,
) -> Result<(), CommandGenError> {
    insert_resource(
        map,
        Resource {
            resource_id: resource_id.to_string(),
            name: name.to_string(),
            file_path: file_path.to_string(),
            payload: serde_json::json!({ "content": content }),
        },
    )
}

fn insert_resource(map: &mut ResourceMap, resource: Resource) -> Result<(), CommandGenError> {
    if map.contains_key(&resource.file_path) {
        return Err(CommandGenError::InvalidData(format!(
            "Duplicate resource file path found: {} for resource {}\nPlease rename the resource to avoid conflicts.",
            resource.file_path, resource.name
        )));
    }
    map.insert(resource.file_path.clone(), resource);
    Ok(())
}

fn handoff_sip_config_yaml(handoff: &Value) -> Value {
    let sip_config = handoff.get("sipConfig");
    let config = sip_config
        .and_then(|v| v.get("config"))
        .or(sip_config)
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(case) = config.get("$case").and_then(Value::as_str) {
        let value = config.get("value").unwrap_or(&Value::Null);
        return match case {
            "invite" => serde_json::json!({
                "method": "invite",
                "phone_number": value.get("phoneNumber").and_then(Value::as_str).unwrap_or(""),
                "outbound_endpoint": value.get("outboundEndpoint").and_then(Value::as_str).unwrap_or(""),
                "outbound_encryption": value.get("outboundEncryption").and_then(Value::as_str).unwrap_or(""),
            }),
            "refer" => serde_json::json!({
                "method": "refer",
                "phone_number": value.get("phoneNumber").and_then(Value::as_str).unwrap_or(""),
            }),
            _ => serde_json::json!({ "method": "bye" }),
        };
    }
    if let Some(invite) = config.get("invite") {
        return serde_json::json!({
            "method": "invite",
            "phone_number": invite.get("phoneNumber").and_then(Value::as_str).unwrap_or(""),
            "outbound_endpoint": invite.get("outboundEndpoint").and_then(Value::as_str).unwrap_or(""),
            "outbound_encryption": invite.get("outboundEncryption").and_then(Value::as_str).unwrap_or(""),
        });
    }
    if let Some(refer) = config.get("refer") {
        return serde_json::json!({
            "method": "refer",
            "phone_number": refer.get("phoneNumber").and_then(Value::as_str).unwrap_or(""),
        });
    }
    serde_json::json!({ "method": "bye" })
}

fn handoff_sip_headers_yaml(handoff: &Value) -> Value {
    let headers = handoff
        .get("sipHeaders")
        .and_then(|v| v.get("headers").or(Some(v)))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let yaml_headers: Vec<Value> = headers
        .iter()
        .map(|h| {
            serde_json::json!({
                "key": h.get("key").and_then(Value::as_str).unwrap_or(""),
                "value": h.get("value").and_then(Value::as_str).unwrap_or(""),
            })
        })
        .collect();
    Value::Array(yaml_headers)
}

fn handoff_entries_vec(projection: &Value) -> Vec<(String, Value)> {
    extract_entities_vec(projection, &["handoff", "handoffs", "entities"])
}

fn sms_entries_vec(projection: &Value) -> Vec<(String, Value)> {
    extract_entities_vec(projection, &["sms", "templates", "entities"])
}

fn phrase_filter_entries_vec(projection: &Value) -> Vec<(String, Value)> {
    extract_entities_vec(projection, &["stopKeywords", "filters", "entities"])
}

fn experimental_features(projection: &Value) -> Option<Value> {
    Some(
        projection
            .get("experimentalConfig")?
            .get("experimentalConfigs")?
            .get("entities")?
            .get("default")?
            .get("features")?
            .clone(),
    )
}
