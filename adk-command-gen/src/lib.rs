use adk_protobuf::agent::{RulesReferences, RulesUpdateRules};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::entities;
use adk_protobuf::functions::{FunctionParameter, FunctionUpdateLatencyControl};
use adk_protobuf::knowledge_base::{KnowledgeBaseCreateTopic, TopicReferences};
use adk_protobuf::variables::VariableReferences;
use adk_protobuf::{Command, Metadata};
use adk_types::{Resource, ResourceMap};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CommandGenError {
    #[error("{0}")]
    InvalidData(String),
}

mod push_flows;
mod push_functions;
mod push_single_file_resources;
mod push_topics;
mod push_variables;

pub fn projection_to_resource_map(projection: &Value) -> Result<ResourceMap, CommandGenError> {
    let mut map = ResourceMap::new();

    for (id, topic) in push_topics::topic_entries(projection) {
        let name = topic
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        let file_name = clean_name(&name).to_lowercase();
        let file_path = format!("topics/{file_name}.yaml");
        let content = serde_yaml::to_string(&serde_json::json!({
            "name": name,
            "enabled": topic.get("isActive").and_then(Value::as_bool).unwrap_or(true),
            "actions": topic.get("actions").and_then(Value::as_str).unwrap_or(""),
            "content": topic.get("content").and_then(Value::as_str).unwrap_or(""),
            "example_queries": topic.get("exampleQueries").and_then(Value::as_array).map(|arr| {
                arr.iter()
                    .filter_map(|x| x.get("query").and_then(Value::as_str).map(ToString::to_string))
                    .collect::<Vec<String>>()
            }).unwrap_or_default(),
        }))
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(&mut map, &file_path, &id, &name, content)?;
    }

    for (id, function) in push_functions::function_entries(projection) {
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        let file_name = clean_name(&name).to_lowercase();
        let file_path = format!("functions/{file_name}.py");
        let content = push_functions::function_raw_content(&function);
        insert_content_resource(&mut map, &file_path, &id, &name, content)?;
    }
    for kind in [
        push_functions::SpecialFunctionKind::Start,
        push_functions::SpecialFunctionKind::End,
    ] {
        if let Some((id, function)) = push_functions::special_function_entry(projection, kind) {
            let name = push_functions::special_function_name(kind).to_string();
            let file_path = format!("functions/{name}.py");
            let content = push_functions::function_raw_content(&function);
            insert_content_resource(&mut map, &file_path, &id, &name, content)?;
        }
    }

    for (id, flow) in flow_entries(projection) {
        insert_flow_resources(&mut map, &id, &flow)?;
    }

    let mut entity_yaml_list = Vec::new();
    for (id, entity) in entity_entries(projection) {
        let name = entity
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        entity_yaml_list.push(serde_json::json!({
            "name": name,
            "description": entity.get("description").and_then(Value::as_str).unwrap_or(""),
            "entity_type": to_snake_case(entity.get("type").and_then(Value::as_str).unwrap_or("")),
            "config": projection_entity_config(&entity),
        }));
    }
    if !entity_yaml_list.is_empty() {
        let content = serde_yaml::to_string(&serde_json::json!({ "entities": entity_yaml_list }))
            .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
        insert_content_resource(
            &mut map,
            "config/entities.yaml",
            "entities",
            "entities",
            content,
        )?;
    }

    // config/handoffs.yaml multi-resource file
    let mut handoff_yaml_list = Vec::new();
    for (_id, handoff) in handoff_entries(projection) {
        if !handoff
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            continue;
        }
        handoff_yaml_list.push(serde_json::json!({
            "name": handoff.get("name").and_then(Value::as_str).unwrap_or(""),
            "description": handoff.get("description").and_then(Value::as_str).unwrap_or(""),
            "is_default": handoff.get("isDefault").and_then(Value::as_bool).unwrap_or(false),
            "sip_config": handoff_sip_config_yaml(&handoff),
            "sip_headers": handoff_sip_headers_yaml(&handoff)
        }));
    }
    if !handoff_yaml_list.is_empty() {
        let content = serde_yaml::to_string(&serde_json::json!({ "handoffs": handoff_yaml_list }))
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
    for (_id, sms) in sms_entries(projection) {
        if !sms.get("active").and_then(Value::as_bool).unwrap_or(true) {
            continue;
        }
        sms_yaml_list.push(serde_json::json!({
                "name": sms.get("name").and_then(Value::as_str).unwrap_or(""),
                "text": sms.get("text").and_then(Value::as_str).unwrap_or(""),
                "env_phone_numbers": {
                    "sandbox": sms.get("envPhoneNumbers").and_then(|v| v.get("sandbox")).and_then(Value::as_str).unwrap_or(""),
                    "pre_release": sms.get("envPhoneNumbers").and_then(|v| v.get("preRelease")).and_then(Value::as_str).unwrap_or(""),
                    "live": sms.get("envPhoneNumbers").and_then(|v| v.get("live")).and_then(Value::as_str).unwrap_or(""),
                }
            }));
    }
    if !sms_yaml_list.is_empty() {
        let content = serde_yaml::to_string(&serde_json::json!({ "sms_templates": sms_yaml_list }))
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
    let global_function_names = push_functions::function_entries(projection)
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
    for (_id, pf) in phrase_filter_entries(projection) {
        let mut phrase = serde_json::json!({
            "name": pf.get("title").and_then(Value::as_str).unwrap_or(""),
            "description": pf.get("description").and_then(Value::as_str).unwrap_or(""),
            "regular_expressions": pf.get("regularExpressions").and_then(Value::as_array).cloned().unwrap_or_default(),
            "say_phrase": pf.get("sayPhrase").and_then(Value::as_bool).unwrap_or(false),
            "language_code": pf.get("languageCode").and_then(Value::as_str).unwrap_or(""),
        });
        if let Some(function_id) = pf
            .pointer("/references/globalFunctions")
            .or_else(|| pf.pointer("/references/global_functions"))
            .and_then(Value::as_object)
            .and_then(|refs| refs.keys().next())
        {
            phrase["function"] = Value::String(
                global_function_names
                    .get(function_id)
                    .filter(|name| !name.is_empty())
                    .cloned()
                    .unwrap_or_else(|| function_id.to_string()),
            );
        }
        phrase_yaml_list.push(phrase);
    }
    if !phrase_yaml_list.is_empty() {
        let content = serde_yaml::to_string(&serde_json::json!({
            "phrase_filtering": phrase_yaml_list
        }))
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
            personality.clone(),
        )?;
    }

    if let Some(role) = projection.pointer("/agentSettings/role") {
        insert_yaml_resource(
            &mut map,
            "agent_settings/role.yaml",
            "role",
            "role",
            role.clone(),
        )?;
    }

    if let Some(safety_filters) = projection.get("contentFilterSettings") {
        insert_yaml_resource(
            &mut map,
            "agent_settings/safety_filters.yaml",
            "safety_filters",
            "safety_filters",
            safety_filters.clone(),
        )?;
    }

    if let Some(voice_safety_filters) = projection.pointer("/channels/voice/config/safetyFilters") {
        insert_yaml_resource(
            &mut map,
            "voice/safety_filters.yaml",
            "voice_safety_filters",
            "voice_safety_filters",
            voice_safety_filters.clone(),
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
            asr_settings.clone(),
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
            serde_json::json!({
                "greeting": voice_greeting.unwrap_or_else(|| serde_json::json!({})),
                "style_prompt": voice_style_prompt.unwrap_or_else(|| serde_json::json!({})),
                "disclaimer_messages": voice_disclaimer
                    .map(|disclaimer| serde_json::json!([disclaimer]))
                    .unwrap_or_else(|| serde_json::json!([])),
            }),
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
                serde_json::json!({
                    "greeting": chat_greeting.unwrap_or_else(|| serde_json::json!({})),
                    "style_prompt": chat_style_prompt.unwrap_or_else(|| serde_json::json!({})),
                }),
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
                chat_safety_filters.clone(),
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
    Ok(map)
}

fn insert_flow_resources(
    map: &mut ResourceMap,
    flow_id: &str,
    flow: &Value,
) -> Result<(), CommandGenError> {
    let name = flow
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("flow")
        .to_string();
    let folder = clean_name(&name).to_lowercase();
    let start_step_id = flow.get("startStepId").and_then(Value::as_str);
    let steps = projection_nested_entities(flow, &["steps"]);
    let start_step = start_step_id.unwrap_or_default().to_string();

    let flow_config = serde_json::json!({
        "name": name,
        "description": flow.get("description").and_then(Value::as_str).unwrap_or(""),
        "start_step": start_step,
    });
    let flow_config_path = format!("flows/{folder}/flow_config.yaml");
    let flow_name = flow_config
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("flow")
        .to_string();
    insert_yaml_resource(
        map,
        &flow_config_path,
        flow.get("id").and_then(Value::as_str).unwrap_or(flow_id),
        &flow_name,
        flow_config,
    )?;

    for (id, step) in steps {
        let step_name = step
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        match step.get("type").and_then(Value::as_str).unwrap_or("") {
            "function_step" => {
                let function = step.get("function").unwrap_or(&Value::Null);
                let code = push_functions::function_raw_content(function);
                let file_path = format!(
                    "flows/{folder}/function_steps/{}.py",
                    clean_name(&step_name).to_lowercase()
                );
                insert_content_resource(map, &file_path, &id, &step_name, code)?;
            }
            "default_step" => insert_flow_step_resource(map, &folder, id, step, true)?,
            _ => insert_flow_step_resource(map, &folder, id, step, false)?,
        }
    }

    for (id, function) in projection_nested_entities(flow, &["transitionFunctions"])
        .into_iter()
        .chain(projection_nested_entities(flow, &["transition_functions"]))
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
            .unwrap_or(id.as_str())
            .to_string();
        let file_name = clean_name(&function_name).to_lowercase();
        let file_path = format!("flows/{folder}/functions/{file_name}.py");
        let content = push_functions::function_raw_content(&function);
        insert_content_resource(map, &file_path, &id, &function_name, content)?;
    }

    Ok(())
}

fn flow_entries(projection: &Value) -> Vec<(String, Value)> {
    projection_entities(projection, &["flows", "flows"])
}

fn projection_entities(root: &Value, path: &[&str]) -> Vec<(String, Value)> {
    let mut current = root;
    for key in path {
        let Some(next) = current.get(*key) else {
            return Vec::new();
        };
        current = next;
    }
    projection_entities_at(current)
}

fn projection_nested_entities(root: &Value, path: &[&str]) -> Vec<(String, Value)> {
    let mut current = root;
    for key in path {
        let Some(next) = current.get(*key) else {
            return Vec::new();
        };
        current = next;
    }
    projection_entities_at(current)
}

fn projection_entities_at(value: &Value) -> Vec<(String, Value)> {
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
            Some(serde_json::json!({
                "name": name,
                "is_default": variant.get("isDefault").or_else(|| variant.get("is_default")).and_then(Value::as_bool).unwrap_or(false),
            }))
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
                            "id": regex.get("id").and_then(Value::as_str).unwrap_or(""),
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
                Some(serde_json::json!({
                    "regex": regex,
                    "replacement": item.get("replacement").and_then(Value::as_str).unwrap_or(""),
                    "case_sensitive": item.get("caseSensitive").or_else(|| item.get("case_sensitive")).and_then(Value::as_bool).unwrap_or(false),
                    "language_code": item.get("languageCode").or_else(|| item.get("language_code")).and_then(Value::as_str).unwrap_or(""),
                    "description": item.get("description").and_then(Value::as_str).unwrap_or(""),
                    "position": item.get("position").and_then(Value::as_i64).unwrap_or(0),
                    "name": item.get("name").and_then(Value::as_str).unwrap_or(""),
                }))
            })
            .collect::<Vec<_>>()
    }))
}

fn insert_flow_step_resource(
    map: &mut ResourceMap,
    folder: &str,
    id: String,
    step: Value,
    is_default: bool,
) -> Result<(), CommandGenError> {
    let name = step
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(id.as_str())
        .to_string();
    let mut value = serde_json::json!({
        "step_type": if is_default { "default_step" } else { "advanced_step" },
        "name": name,
        "prompt": step.get("prompt").and_then(Value::as_str).unwrap_or(""),
    });
    if let Some(position) = step.get("position") {
        value["position"] = position.clone();
    }
    if !is_default {
        value["asr_biasing"] = step
            .get("asrBiasing")
            .or_else(|| step.get("asr_biasing"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        value["dtmf_config"] = step
            .get("dtmfConfig")
            .or_else(|| step.get("dtmf_config"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
    } else {
        value["conditions"] = flow_conditions_yaml(&step);
        value["extracted_entities"] = step
            .pointer("/references/extractedEntities")
            .or_else(|| step.pointer("/references/extracted_entities"))
            .and_then(Value::as_object)
            .map(|refs| {
                refs.keys()
                    .cloned()
                    .map(Value::String)
                    .collect::<Vec<Value>>()
            })
            .map(Value::Array)
            .unwrap_or_else(|| Value::Array(vec![]));
    }
    let file_path = format!(
        "flows/{folder}/steps/{}.yaml",
        clean_name(&name).to_lowercase()
    );
    insert_yaml_resource(map, &file_path, &id, &name, value)
}

fn flow_conditions_yaml(step: &Value) -> Value {
    let items = step
        .get("conditions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|condition| {
            let config = condition.get("config")?;
            let case = config.get("$case").and_then(Value::as_str).unwrap_or("");
            if case != "exitFlowCondition" {
                return None;
            }
            let value = config.get("value").unwrap_or(&Value::Null);
            let details = value.get("details").unwrap_or(&Value::Null);
            let mut out = serde_json::json!({
                "name": details.get("label").and_then(Value::as_str).unwrap_or(""),
                "condition_type": "exit_flow_condition",
                "description": details.get("description").and_then(Value::as_str).unwrap_or(""),
                "required_entities": details.get("requiredEntities").and_then(Value::as_array).cloned().unwrap_or_default(),
            });
            if let Some(ingress) = details.get("ingressPosition").and_then(Value::as_str) {
                out["ingress_position"] = Value::String(ingress.to_string());
            }
            if let Some(position) = details.get("position") {
                out["position"] = position.clone();
            }
            if let Some(position) = value.get("exitFlowPosition") {
                out["exit_flow_position"] = position.clone();
            }
            Some(out)
        })
        .collect::<Vec<_>>();
    Value::Array(items)
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

fn insert_yaml_resource(
    map: &mut ResourceMap,
    file_path: &str,
    resource_id: &str,
    name: &str,
    value: Value,
) -> Result<(), CommandGenError> {
    let content =
        serde_yaml::to_string(&value).map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
    insert_content_resource(map, file_path, resource_id, name, content)
}

fn insert_content_resource(
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

fn projection_entity_config(entity: &Value) -> Value {
    if let Some(cfg) = entity.pointer("/config/value") {
        return cfg.clone();
    }
    if let Some(cfg) = entity.get("config") {
        return cfg.clone();
    }
    let entity_type = to_snake_case(entity.get("type").and_then(Value::as_str).unwrap_or(""));
    match entity_type.as_str() {
        "numeric" => entity
            .get("numberConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "alphanumeric" => entity
            .get("alphanumericConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "enum" => entity
            .get("multipleOptionsConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "date" => entity
            .get("dateConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "phone_number" => entity
            .get("phoneNumberConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "time" => entity
            .get("timeConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        _ => serde_json::json!({}),
    }
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

pub fn build_phase1_commands(resources: &ResourceMap, projection: &Value) -> Vec<Command> {
    build_phase1_commands_with_actor(resources, projection, None)
}

pub fn build_phase1_commands_with_actor(
    resources: &ResourceMap,
    projection: &Value,
    actor: Option<&str>,
) -> Vec<Command> {
    let metadata = command_metadata_with_actor(actor);

    let flow_groups = push_flows::flow_resource_command_groups(resources, projection, &metadata);
    let function_groups =
        push_functions::function_resource_command_groups(resources, projection, &metadata);
    let topic_groups = push_topics::topic_resource_command_groups(resources, projection, &metadata);
    let variable_groups =
        push_variables::variable_resource_command_groups(resources, projection, &metadata);
    let single_file_groups = push_single_file_resources::single_file_resource_command_groups(
        resources, projection, &metadata,
    );
    let push_single_file_resources::CommandGroups {
        deletes: variable_deletes,
        creates: variable_creates,
        updates: variable_updates,
        post_updates: variable_post_updates,
    } = variable_groups;

    let mut deletes: Vec<Command> = variable_deletes
        .into_iter()
        .chain(function_groups.deletes)
        .chain(topic_groups.deletes)
        .chain(flow_groups.deletes)
        .chain(single_file_groups.deletes)
        .collect();
    order_commands_with_priority(&mut deletes, DELETE_COMMAND_PRIORITY);

    let mut creates: Vec<Command> = variable_creates
        .into_iter()
        .chain(function_groups.creates)
        .chain(topic_groups.creates)
        .chain(flow_groups.creates)
        .chain(single_file_groups.creates)
        .collect();
    order_commands_with_priority(&mut creates, CREATE_COMMAND_PRIORITY);

    let mut updates: Vec<Command> = variable_updates
        .into_iter()
        .chain(function_groups.updates)
        .chain(topic_groups.updates)
        .chain(flow_groups.updates)
        .chain(single_file_groups.updates)
        .collect();
    order_commands_with_priority(&mut updates, UPDATE_COMMAND_PRIORITY);

    let mut out: Vec<Command> = Vec::new();
    out.extend(deletes);
    out.extend(creates);
    out.extend(updates);
    out.extend(variable_post_updates);
    out.extend(function_groups.post_updates);
    out.extend(topic_groups.post_updates);
    out.extend(flow_groups.post_updates);
    out.extend(single_file_groups.post_updates);
    out
}

const DELETE_COMMAND_PRIORITY: &[&str] = &[
    "variable_delete",
    "delete_start_function",
    "delete_end_function",
    "delete_function",
    "delete_flow_transition_function",
    "delete_topic",
    "handoff_delete",
    "sms_delete_template",
    "stop_keywords_delete",
    "entity_delete",
];

const CREATE_COMMAND_PRIORITY: &[&str] = &[
    "variable_create",
    "entity_create",
    "sms_create_template",
    "handoff_create",
    "create_start_function",
    "create_end_function",
    "create_function",
    "create_topic",
    "create_flow",
    "create_flow_transition_function",
    "create_step",
    "create_no_code_condition",
    "stop_keywords_create",
];

const UPDATE_COMMAND_PRIORITY: &[&str] = &[
    "variable_update",
    "entity_update",
    "update_rules",
    "update_start_function",
    "update_end_function",
    "update_function",
    "update_flow_transition_function",
    "update_topic",
    "sms_update_template",
    "handoff_update",
    "stop_keywords_update",
    "experimental_config_update_config",
];

fn order_commands_with_priority(commands: &mut [Command], priority: &[&str]) {
    commands.sort_by_key(|command| {
        priority
            .iter()
            .position(|value| *value == command.r#type.as_str())
            .unwrap_or(priority.len())
    });
}

fn command_metadata_with_actor(actor: Option<&str>) -> Option<Metadata> {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let created_by = actor
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(default_metadata_created_by);
    Some(Metadata {
        created_at: Some(prost_types::Timestamp {
            seconds: dur.as_secs() as i64,
            nanos: dur.subsec_nanos() as i32,
        }),
        created_by,
    })
}

pub(crate) fn default_metadata_created_by() -> String {
    "sdk-user".to_string()
}

pub(crate) fn push_command(
    out: &mut Vec<Command>,
    metadata: &Option<Metadata>,
    type_str: &str,
    payload: CommandPayload,
) {
    out.push(Command {
        r#type: type_str.to_string(),
        metadata: metadata.clone(),
        command_id: Uuid::new_v4().to_string(),
        payload: Some(payload),
    });
}

pub(crate) fn extract_variable_names_from_code(code: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in code.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut rest = line;
        while let Some(index) = rest.find("conv.state.") {
            let after = &rest[index + "conv.state.".len()..];
            let name_len = after
                .char_indices()
                .take_while(|(idx, ch)| {
                    if *idx == 0 {
                        ch.is_ascii_alphabetic() || *ch == '_'
                    } else {
                        ch.is_ascii_alphanumeric() || *ch == '_'
                    }
                })
                .map(|(idx, ch)| idx + ch.len_utf8())
                .last()
                .unwrap_or(0);
            if name_len > 0 {
                let name = &after[..name_len];
                let after_name = after[name_len..].trim_start();
                if !after_name.starts_with('(') {
                    names.push(name.to_string());
                }
            }
            rest = after;
        }
    }
    names.sort();
    names.dedup();
    names
}

pub(crate) fn entity_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["entities", "entities", "entities"])
}

fn handoff_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["handoff", "handoffs", "entities"])
}

fn sms_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["sms", "templates", "entities"])
}

fn phrase_filter_entries(projection: &Value) -> HashMap<String, Value> {
    extract_entities_map(projection, &["stopKeywords", "filters", "entities"])
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

pub(crate) fn rules_references_from_projection(projection: &Value) -> Option<RulesReferences> {
    let references = projection.pointer("/agentSettings/rules/references")?;
    let refs = RulesReferences {
        sms: json_bool_map(references.get("sms")),
        handoff: json_bool_map(references.get("handoff")),
        attributes: json_bool_map(references.get("attributes")),
        global_functions: json_bool_map(
            references
                .get("globalFunctions")
                .or_else(|| references.get("global_functions")),
        ),
        variables: json_bool_map(references.get("variables")),
        translations: json_bool_map(references.get("translations")),
    };
    if refs.sms.is_empty()
        && refs.handoff.is_empty()
        && refs.attributes.is_empty()
        && refs.global_functions.is_empty()
        && refs.variables.is_empty()
        && refs.translations.is_empty()
    {
        None
    } else {
        Some(refs)
    }
}

pub(crate) fn rules_references_from_behaviour(behaviour: &str) -> Option<RulesReferences> {
    let refs = RulesReferences {
        sms: extract_template_references(behaviour, "sms"),
        handoff: extract_template_references(behaviour, "ho"),
        attributes: extract_template_references(behaviour, "attr"),
        global_functions: extract_template_references(behaviour, "fn"),
        variables: extract_template_references(behaviour, "var"),
        translations: HashMap::new(),
    };
    if refs.sms.is_empty()
        && refs.handoff.is_empty()
        && refs.attributes.is_empty()
        && refs.global_functions.is_empty()
        && refs.variables.is_empty()
        && refs.translations.is_empty()
    {
        None
    } else {
        Some(refs)
    }
}

fn extract_template_references(behaviour: &str, prefix: &str) -> HashMap<String, bool> {
    let marker = format!("{{{{{prefix}:");
    let mut out = HashMap::new();
    let mut start = 0;
    while let Some(index) = behaviour[start..].find(&marker) {
        let value_start = start + index + marker.len();
        let tail = &behaviour[value_start..];
        let Some(end) = tail.find("}}") else {
            break;
        };
        let value = tail[..end].trim();
        if !value.is_empty() {
            out.insert(value.to_string(), true);
        }
        start = value_start + end + 2;
    }
    out
}

fn json_bool_map(value: Option<&Value>) -> HashMap<String, bool> {
    value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.as_bool().unwrap_or(true)))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn extract_entities_map(root: &Value, path: &[&str]) -> HashMap<String, Value> {
    let mut cur = root;
    for key in path {
        cur = match cur.get(*key) {
            Some(v) => v,
            None => return HashMap::new(),
        };
    }
    cur.as_object()
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default()
}

pub(crate) fn to_camel_case(s: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for ch in s.chars() {
        if ch == '_' || ch == '-' {
            upper = true;
            continue;
        }
        if upper {
            out.push(ch.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

pub(crate) fn clean_name(s: &str) -> String {
    let mut cleaned = String::new();
    let mut last_was_separator = true;
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            cleaned.push(ch);
            last_was_separator = false;
        } else if !last_was_separator {
            cleaned.push('_');
            last_was_separator = true;
        }
    }
    cleaned.trim_matches('_').to_string()
}

pub(crate) fn is_synthetic_local_resource_id(resource_id: &str) -> bool {
    let trimmed = resource_id.trim();
    trimmed.is_empty()
        || trimmed == "local"
        || trimmed.contains('/')
        || trimmed.ends_with(".yaml")
        || trimmed.ends_with(".yml")
        || trimmed.ends_with(".py")
}

pub(crate) fn random_resource_id(prefix: &str) -> String {
    let hex = Uuid::new_v4().simple().to_string();
    format!("{prefix}-{}", &hex[..8])
}

pub(crate) fn generated_replay_resource_id(kind: &str, name: &str, path: &str) -> Option<String> {
    // Replay tests map Python-recorded random IDs onto Rust-generated command payloads.
    let env_name = format!("POLY_ADK_GENERATED_{}_IDS", kind.to_ascii_uppercase());
    let mappings = env::var(env_name).ok()?;
    for raw in mappings.lines() {
        let Some((key, id)) = raw.split_once('=') else {
            continue;
        };
        if key == name || key == path {
            return Some(id.to_string());
        }
    }
    None
}

pub(crate) fn generated_or_stable_resource_id(
    kind: &str,
    prefix: &str,
    name: &str,
    path: &str,
) -> String {
    generated_replay_resource_id(kind, name, path).unwrap_or_else(|| {
        let mut hash = 0x811c9dc5_u32;
        for byte in format!("{name}\0{path}").bytes() {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
        format!("{prefix}-{hash:08x}")
    })
}

pub(crate) fn build_entity_create_config(
    entity_type: &str,
    config: Option<&serde_yaml::Value>,
) -> Option<entities::entity_create::Config> {
    match entity_type {
        "numeric" => Some(entities::entity_create::Config::Numeric(
            entities::NumberConfig {
                has_decimal: yaml_bool(config, "has_decimal", false),
                has_range: yaml_bool(config, "has_range", false),
                min: yaml_f32_opt(config, "min"),
                max: yaml_f32_opt(config, "max"),
            },
        )),
        "alphanumeric" => Some(entities::entity_create::Config::Alphanumeric(
            entities::AlphanumericConfig {
                enabled: yaml_bool(config, "enabled", true),
                validation_type: yaml_string(config, "validation_type"),
                regular_expression: yaml_string(config, "regular_expression"),
            },
        )),
        "enum" => Some(entities::entity_create::Config::Enum(
            entities::MultipleOptionsConfig {
                options: yaml_string_list(config, "options"),
            },
        )),
        "date" => Some(entities::entity_create::Config::Date(
            entities::DateConfig {
                relative_date: yaml_bool(config, "relative_date", false),
            },
        )),
        "phone_number" => Some(entities::entity_create::Config::PhoneNumber(
            entities::PhoneNumberConfig {
                enabled: yaml_bool(config, "enabled", true),
                country_codes: yaml_string_list(config, "country_codes"),
            },
        )),
        "time" => Some(entities::entity_create::Config::Time(
            entities::TimeConfig {
                enabled: yaml_bool(config, "enabled", true),
                start_time: yaml_string(config, "start_time"),
                end_time: yaml_string(config, "end_time"),
            },
        )),
        "address" => Some(entities::entity_create::Config::Address(
            entities::AddressConfig {},
        )),
        "free_text" => Some(entities::entity_create::Config::FreeText(
            entities::FreeTextConfig {},
        )),
        "name_config" => Some(entities::entity_create::Config::NameConfig(
            entities::NameConfig {},
        )),
        _ => None,
    }
}

pub(crate) fn build_entity_update_config(
    entity_type: &str,
    config: Option<&serde_yaml::Value>,
) -> Option<entities::entity_update::Config> {
    match entity_type {
        "numeric" => Some(entities::entity_update::Config::Numeric(
            entities::NumberConfig {
                has_decimal: yaml_bool(config, "has_decimal", false),
                has_range: yaml_bool(config, "has_range", false),
                min: yaml_f32_opt(config, "min"),
                max: yaml_f32_opt(config, "max"),
            },
        )),
        "alphanumeric" => Some(entities::entity_update::Config::Alphanumeric(
            entities::AlphanumericConfig {
                enabled: yaml_bool(config, "enabled", true),
                validation_type: yaml_string(config, "validation_type"),
                regular_expression: yaml_string(config, "regular_expression"),
            },
        )),
        "enum" => Some(entities::entity_update::Config::Enum(
            entities::MultipleOptionsConfig {
                options: yaml_string_list(config, "options"),
            },
        )),
        "date" => Some(entities::entity_update::Config::Date(
            entities::DateConfig {
                relative_date: yaml_bool(config, "relative_date", false),
            },
        )),
        "phone_number" => Some(entities::entity_update::Config::PhoneNumber(
            entities::PhoneNumberConfig {
                enabled: yaml_bool(config, "enabled", true),
                country_codes: yaml_string_list(config, "country_codes"),
            },
        )),
        "time" => Some(entities::entity_update::Config::Time(
            entities::TimeConfig {
                enabled: yaml_bool(config, "enabled", true),
                start_time: yaml_string(config, "start_time"),
                end_time: yaml_string(config, "end_time"),
            },
        )),
        "address" => Some(entities::entity_update::Config::Address(
            entities::AddressConfig {},
        )),
        "free_text" => Some(entities::entity_update::Config::FreeText(
            entities::FreeTextConfig {},
        )),
        "name_config" => Some(entities::entity_update::Config::NameConfig(
            entities::NameConfig {},
        )),
        _ => None,
    }
}

fn yaml_get<'a>(config: Option<&'a serde_yaml::Value>, key: &str) -> Option<&'a serde_yaml::Value> {
    config.and_then(|c| c.get(key))
}

fn yaml_bool(config: Option<&serde_yaml::Value>, key: &str, default: bool) -> bool {
    yaml_get(config, key)
        .and_then(serde_yaml::Value::as_bool)
        .unwrap_or(default)
}

fn yaml_string(config: Option<&serde_yaml::Value>, key: &str) -> String {
    yaml_get(config, key)
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn yaml_str(config: &serde_yaml::Value, key: &str) -> String {
    yaml_string(Some(config), key)
}

fn yaml_string_list(config: Option<&serde_yaml::Value>, key: &str) -> Vec<String> {
    yaml_get(config, key)
        .and_then(serde_yaml::Value::as_sequence)
        .map(|seq| {
            seq.iter()
                .filter_map(serde_yaml::Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn yaml_f32_opt(config: Option<&serde_yaml::Value>, key: &str) -> Option<f32> {
    yaml_get(config, key).and_then(|v| match v {
        serde_yaml::Value::Number(n) => n.as_f64().map(|x| x as f32),
        _ => None,
    })
}

pub fn command_to_json_summary(command: &Command) -> Value {
    let mut value = serde_json::json!({
        "type": command.r#type,
        "command_id": command.command_id,
    });
    if let Some(metadata) = &command.metadata {
        value["metadata"] = metadata_to_json(metadata);
    }
    if let Some(payload) = &command.payload {
        match payload {
            CommandPayload::DeleteFunction(delete) => {
                value["delete_function"] = serde_json::json!({ "id": delete.id });
            }
            CommandPayload::DeleteStartFunction(delete) => {
                value["delete_start_function"] = serde_json::json!({ "id": delete.id });
            }
            CommandPayload::DeleteEndFunction(delete) => {
                value["delete_end_function"] = serde_json::json!({ "id": delete.id });
            }
            CommandPayload::VariableDelete(delete) => {
                value["variable_delete"] = serde_json::json!({ "id": delete.id });
            }
            CommandPayload::VariableCreate(create) => {
                value["variable_create"] =
                    variable_command_to_json(&create.id, &create.name, create.references.as_ref());
            }
            CommandPayload::VariableUpdate(update) => {
                value["variable_update"] =
                    variable_command_to_json(&update.id, &update.name, update.references.as_ref());
            }
            CommandPayload::CreateStartFunction(create) => {
                value["create_start_function"] = special_function_create_to_json(
                    &create.id,
                    &create.name,
                    &create.description,
                    &create.parameters,
                    &create.code,
                    create.archived,
                    create.references.as_ref().map(|refs| &refs.variables),
                );
            }
            CommandPayload::CreateEndFunction(create) => {
                value["create_end_function"] = special_function_create_to_json(
                    &create.id,
                    &create.name,
                    &create.description,
                    &create.parameters,
                    &create.code,
                    create.archived,
                    create.references.as_ref().map(|refs| &refs.variables),
                );
            }
            CommandPayload::UpdateStartFunction(update) => {
                value["update_start_function"] = special_function_update_to_json(
                    &update.id,
                    update.description.as_deref(),
                    update.code.as_deref(),
                    update.references.as_ref().map(|refs| &refs.variables),
                );
            }
            CommandPayload::UpdateEndFunction(update) => {
                value["update_end_function"] = special_function_update_to_json(
                    &update.id,
                    update.description.as_deref(),
                    update.code.as_deref(),
                    update.references.as_ref().map(|refs| &refs.variables),
                );
            }
            CommandPayload::UpdateLatencyControl(update) => {
                value["update_latency_control"] = function_update_latency_control_to_json(update);
            }
            CommandPayload::CreateTopic(topic) => {
                value["create_topic"] = create_topic_to_json(topic);
            }
            CommandPayload::UpdateRules(update) => {
                value["update_rules"] = rules_update_to_json(update);
            }
            _ => {}
        }
        if let Some((key, payload_value)) =
            push_single_file_resources::payload_json_summary(payload)
        {
            value[key] = payload_value;
        }
        if let Some((key, payload_value)) = push_flows::payload_json_summary(payload) {
            value[key] = payload_value;
        }
    }
    value
}

fn metadata_to_json(metadata: &Metadata) -> Value {
    let created_at = metadata
        .created_at
        .as_ref()
        .map(|timestamp| format!("{}.{:09}Z", timestamp.seconds, timestamp.nanos))
        .unwrap_or_default();
    serde_json::json!({
        "created_at": created_at,
        "created_by": metadata.created_by,
    })
}

fn rules_update_to_json(update: &RulesUpdateRules) -> Value {
    let mut value = serde_json::Map::new();
    if let Some(behaviour) = &update.behaviour {
        value.insert("behaviour".to_string(), Value::String(behaviour.clone()));
    }
    if let Some(references) = &update.references {
        let references_json = rules_references_to_json(references);
        if references_json
            .as_object()
            .map(|object| !object.is_empty())
            .unwrap_or(false)
        {
            value.insert("references".to_string(), references_json);
        }
    }
    Value::Object(value)
}

fn special_function_create_to_json(
    id: &str,
    name: &str,
    description: &str,
    parameters: &[FunctionParameter],
    code: &str,
    archived: Option<bool>,
    variables: Option<&HashMap<String, bool>>,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(id.to_string()));
    value.insert("name".to_string(), Value::String(name.to_string()));
    if !description.is_empty() {
        value.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
    if !parameters.is_empty() {
        value.insert(
            "parameters".to_string(),
            Value::Array(
                parameters
                    .iter()
                    .map(function_parameter_to_json)
                    .collect::<Vec<_>>(),
            ),
        );
    }
    value.insert("code".to_string(), Value::String(code.to_string()));
    if archived == Some(true) {
        value.insert("archived".to_string(), Value::Bool(true));
    }
    let references = special_function_references_to_json(variables);
    if references
        .as_object()
        .is_some_and(|object| !object.is_empty())
    {
        value.insert("references".to_string(), references);
    }
    Value::Object(value)
}

fn special_function_update_to_json(
    id: &str,
    description: Option<&str>,
    code: Option<&str>,
    variables: Option<&HashMap<String, bool>>,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(id.to_string()));
    if let Some(description) = description {
        value.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
    if let Some(code) = code {
        value.insert("code".to_string(), Value::String(code.to_string()));
    }
    let references = special_function_references_to_json(variables);
    if references
        .as_object()
        .is_some_and(|object| !object.is_empty())
    {
        value.insert("references".to_string(), references);
    }
    Value::Object(value)
}

fn function_update_latency_control_to_json(update: &FunctionUpdateLatencyControl) -> Value {
    let mut value = serde_json::Map::new();
    value.insert(
        "function_id".to_string(),
        Value::String(update.function_id.clone()),
    );
    value.insert("enabled".to_string(), Value::Bool(update.enabled));
    if let Some(initial_delay) = update.initial_delay {
        value.insert(
            "initial_delay".to_string(),
            Value::Number(initial_delay.into()),
        );
    }
    if let Some(interval) = update.interval {
        value.insert("interval".to_string(), Value::Number(interval.into()));
    }
    if let Some(delay_responses) = &update.delay_responses {
        value.insert(
            "delay_responses".to_string(),
            Value::Array(
                delay_responses
                    .delay_responses
                    .iter()
                    .map(|response| {
                        serde_json::json!({
                            "id": response.id,
                            "message": response.message,
                            "duration": response.duration,
                        })
                    })
                    .collect(),
            ),
        );
    }
    Value::Object(value)
}

fn variable_command_to_json(
    id: &str,
    name: &str,
    references: Option<&VariableReferences>,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(id.to_string()));
    value.insert("name".to_string(), Value::String(name.to_string()));
    if let Some(references) = references {
        let references = variable_references_to_json(references);
        if references
            .as_object()
            .is_some_and(|object| !object.is_empty())
        {
            value.insert("references".to_string(), references);
        }
    }
    Value::Object(value)
}

fn variable_references_to_json(references: &VariableReferences) -> Value {
    let mut value = serde_json::Map::new();
    insert_bool_map(&mut value, "functions", &references.functions);
    insert_bool_map(&mut value, "delay_responses", &references.delay_responses);
    insert_bool_map(&mut value, "flow_steps", &references.flow_steps);
    insert_bool_map(
        &mut value,
        "flow_no_code_steps",
        &references.flow_no_code_steps,
    );
    insert_bool_map(&mut value, "flow_functions", &references.flow_functions);
    insert_bool_map(&mut value, "topics", &references.topics);
    insert_bool_map(&mut value, "behaviours", &references.behaviours);
    insert_bool_map(&mut value, "greetings", &references.greetings);
    insert_bool_map(&mut value, "roles", &references.roles);
    insert_bool_map(&mut value, "personalities", &references.personalities);
    insert_bool_map(&mut value, "sms", &references.sms);
    insert_bool_map(&mut value, "start_functions", &references.start_functions);
    insert_bool_map(&mut value, "end_functions", &references.end_functions);
    Value::Object(value)
}

fn function_parameter_to_json(parameter: &FunctionParameter) -> Value {
    serde_json::json!({
        "id": parameter.id.clone(),
        "name": parameter.name.clone(),
        "description": parameter.description.clone(),
        "type": parameter.r#type.clone(),
    })
}

fn special_function_references_to_json(variables: Option<&HashMap<String, bool>>) -> Value {
    let mut value = serde_json::Map::new();
    if let Some(variables) = variables {
        insert_bool_map(&mut value, "variables", variables);
    }
    Value::Object(value)
}

fn create_topic_to_json(topic: &KnowledgeBaseCreateTopic) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), Value::String(topic.id.clone()));
    value.insert("name".to_string(), Value::String(topic.name.clone()));
    value.insert("content".to_string(), Value::String(topic.content.clone()));
    value.insert("actions".to_string(), Value::String(topic.actions.clone()));
    if let Some(example_queries) = &topic.example_queries {
        value.insert(
            "example_queries".to_string(),
            serde_json::json!({ "queries": example_queries.queries.clone() }),
        );
    }
    value.insert(
        "references".to_string(),
        topic
            .references
            .as_ref()
            .map(topic_references_to_json)
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
    );
    if let Some(is_active) = topic.is_active {
        value.insert("is_active".to_string(), Value::Bool(is_active));
    }
    Value::Object(value)
}

fn topic_references_to_json(references: &TopicReferences) -> Value {
    let mut value = serde_json::Map::new();
    insert_bool_map(&mut value, "sms", &references.sms);
    insert_bool_map(&mut value, "handoff", &references.handoff);
    insert_bool_map(&mut value, "attributes", &references.attributes);
    insert_bool_map(&mut value, "globalFunctions", &references.global_functions);
    insert_bool_map(&mut value, "variables", &references.variables);
    insert_bool_map(&mut value, "translations", &references.translations);
    Value::Object(value)
}

fn rules_references_to_json(references: &RulesReferences) -> Value {
    let mut value = serde_json::Map::new();
    insert_bool_map(&mut value, "sms", &references.sms);
    insert_bool_map(&mut value, "handoff", &references.handoff);
    insert_bool_map(&mut value, "attributes", &references.attributes);
    insert_bool_map(&mut value, "globalFunctions", &references.global_functions);
    insert_bool_map(&mut value, "variables", &references.variables);
    insert_bool_map(&mut value, "translations", &references.translations);
    Value::Object(value)
}

fn insert_bool_map(
    target: &mut serde_json::Map<String, Value>,
    key: &str,
    source: &HashMap<String, bool>,
) {
    if source.is_empty() {
        return;
    }
    target.insert(key.to_string(), serde_json::json!(source));
}

#[cfg(test)]
mod parity_matrix_tests;
#[cfg(test)]
mod tests;
