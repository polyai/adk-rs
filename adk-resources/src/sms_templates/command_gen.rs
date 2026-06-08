use crate::ids::stable_resource_id;
use crate::push_commands::CommandGroups;
use crate::{extract_entities_map, is_synthetic_local_resource_id, push_command};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::sms::{
    SmsCreateTemplate, SmsDeleteTemplate, SmsEnvPhoneNumbers, SmsTemplateReferences,
    SmsUpdateTemplate, UpdateSmsEnvPhoneNumbers,
};
use adk_protobuf::{Command, Metadata};
use adk_types::ResourceMap;
use serde_json::{self, Value as JsonValue};
use serde_yaml_ng::{Value as YamlValue, from_str};
use std::collections::{HashMap, HashSet};

pub(crate) fn sms_template_command_groups(
    resources: &ResourceMap,
    projection: &JsonValue,
    metadata: &Option<Metadata>,
) -> CommandGroups {
    let remote = remote_sms(projection);
    let mut deletes = Vec::new();
    let mut creates = Vec::new();
    let mut updates = Vec::new();
    let mut local_names = HashSet::new();

    {
        let mut queue = SmsItemQueue {
            projection,
            remote: &remote,
            metadata,
            local_names: &mut local_names,
            creates: &mut creates,
            updates: &mut updates,
        };

        for resource in resources.values() {
            let path = resource.file_path.as_str();
            let content = resource
                .payload
                .get("content")
                .and_then(JsonValue::as_str)
                .unwrap_or_default();

            if path == "config/sms_templates.yaml" {
                if let Ok(yaml) = from_str::<YamlValue>(content)
                    && let Some(items) = yaml.get("sms_templates").and_then(YamlValue::as_sequence)
                {
                    for item in items {
                        queue.queue(item, "local");
                    }
                }
                continue;
            }

            if path.starts_with("config/sms_templates.yaml/sms_templates/")
                && let Ok(yaml) = from_str::<YamlValue>(content)
            {
                queue.queue(&yaml, &resource.resource_id);
            }
        }
    }

    for (name, id) in &remote {
        if !local_names.contains(name) {
            push_command(
                &mut deletes,
                metadata,
                "sms_delete_template",
                CommandPayload::SmsDeleteTemplate(SmsDeleteTemplate { id: id.clone() }),
            );
        }
    }

    CommandGroups {
        deletes,
        creates,
        updates,
        post_updates: Vec::new(),
        cleanup_deletes: Vec::new(),
        post_deletes: Vec::new(),
    }
}

fn remote_sms(projection: &JsonValue) -> HashMap<String, String> {
    let entities = extract_entities_map(projection, &["sms", "templates", "entities"]);
    let mut sms = HashMap::new();
    for (id, value) in entities {
        if !value
            .get("active")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let name = value
            .get("name")
            .and_then(JsonValue::as_str)
            .unwrap_or(&id)
            .to_string();
        sms.insert(name, id);
    }
    sms
}

fn yaml_str(yaml: &YamlValue, key: &str) -> String {
    yaml.get(key)
        .and_then(YamlValue::as_str)
        .unwrap_or("")
        .to_string()
}

fn sms_env_phone_numbers(yaml: &YamlValue) -> SmsEnvPhoneNumbers {
    let env = yaml
        .get("env_phone_numbers")
        .or_else(|| yaml.get("envPhoneNumbers"));
    let env = match env {
        Some(value) => value,
        None => {
            return SmsEnvPhoneNumbers {
                sandbox: String::new(),
                pre_release: String::new(),
                live: String::new(),
            };
        }
    };
    let pre_release = non_empty(yaml_str(env, "pre_release"), yaml_str(env, "preRelease"));
    SmsEnvPhoneNumbers {
        sandbox: yaml_str(env, "sandbox"),
        pre_release,
        live: yaml_str(env, "live"),
    }
}

fn sms_env_update(yaml: &YamlValue) -> UpdateSmsEnvPhoneNumbers {
    let env = yaml
        .get("env_phone_numbers")
        .or_else(|| yaml.get("envPhoneNumbers"));
    let env = match env {
        Some(value) => value,
        None => {
            return UpdateSmsEnvPhoneNumbers {
                sandbox: None,
                pre_release: None,
                live: None,
            };
        }
    };
    let pre_release = non_empty(yaml_str(env, "pre_release"), yaml_str(env, "preRelease"));
    UpdateSmsEnvPhoneNumbers {
        sandbox: Some(yaml_str(env, "sandbox")),
        pre_release: Some(pre_release),
        live: Some(yaml_str(env, "live")),
    }
}

fn non_empty(left: String, right: String) -> String {
    if left.is_empty() { right } else { left }
}

fn sms_matches_remote(local: &YamlValue, remote: &JsonValue) -> bool {
    if local.get("name").and_then(|v| v.as_str()).unwrap_or("")
        != remote.get("name").and_then(JsonValue::as_str).unwrap_or("")
    {
        return false;
    }
    if local.get("text").and_then(|v| v.as_str()).unwrap_or("")
        != remote.get("text").and_then(JsonValue::as_str).unwrap_or("")
    {
        return false;
    }
    let local_env = local
        .get("env_phone_numbers")
        .or_else(|| local.get("envPhoneNumbers"));
    let remote_env = remote.get("envPhoneNumbers");
    let local_pre = non_empty(
        local_env
            .and_then(|env| env.get("pre_release"))
            .and_then(YamlValue::as_str)
            .unwrap_or("")
            .to_string(),
        local_env
            .and_then(|env| env.get("preRelease"))
            .and_then(YamlValue::as_str)
            .unwrap_or("")
            .to_string(),
    );
    let remote_pre = non_empty(
        remote_env
            .and_then(|env| env.get("preRelease"))
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .to_string(),
        remote_env
            .and_then(|env| env.get("pre_release"))
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .to_string(),
    );
    local_env
        .and_then(|env| env.get("sandbox"))
        .and_then(YamlValue::as_str)
        .unwrap_or("")
        == remote_env
            .and_then(|env| env.get("sandbox"))
            .and_then(JsonValue::as_str)
            .unwrap_or("")
        && local_pre == remote_pre
        && local_env
            .and_then(|env| env.get("live"))
            .and_then(YamlValue::as_str)
            .unwrap_or("")
            == remote_env
                .and_then(|env| env.get("live"))
                .and_then(JsonValue::as_str)
                .unwrap_or("")
}

fn yaml_reference_map(yaml: Option<&YamlValue>) -> HashMap<String, bool> {
    let Some(yaml) = yaml else {
        return HashMap::new();
    };
    if let Some(items) = yaml.as_sequence() {
        return items
            .iter()
            .filter_map(|value| value.as_str().map(|key| (key.to_string(), true)))
            .collect();
    }
    if let Some(items) = yaml.as_mapping() {
        return items
            .iter()
            .filter_map(|(key, value)| {
                Some((key.as_str()?.to_string(), value.as_bool().unwrap_or(true)))
            })
            .collect();
    }
    HashMap::new()
}

fn json_reference_map(value: Option<&JsonValue>) -> HashMap<String, bool> {
    value
        .and_then(JsonValue::as_object)
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.as_bool().unwrap_or(true)))
                .collect()
        })
        .unwrap_or_default()
}

fn sms_references_from_yaml(yaml: &YamlValue) -> Option<SmsTemplateReferences> {
    let refs = yaml.get("references").or_else(|| yaml.get("refs"));
    let topics = yaml_reference_map(refs.and_then(|refs| refs.get("topics")));
    let flow_steps = yaml_reference_map(refs.and_then(|refs| refs.get("flow_steps")));
    let variables = yaml_reference_map(refs.and_then(|refs| refs.get("variables")));
    let translations = yaml_reference_map(refs.and_then(|refs| refs.get("translations")));
    if topics.is_empty() && flow_steps.is_empty() && variables.is_empty() && translations.is_empty()
    {
        return None;
    }
    Some(SmsTemplateReferences {
        topics,
        flow_steps,
        variables,
        translations,
    })
}

fn sms_references_from_remote(remote: Option<&JsonValue>) -> Option<SmsTemplateReferences> {
    let refs = remote.and_then(|value| value.get("references"))?;
    let topics = json_reference_map(refs.get("topics"));
    let flow_steps = json_reference_map(refs.get("flowSteps").or_else(|| refs.get("flow_steps")));
    let variables = json_reference_map(refs.get("variables"));
    let translations = json_reference_map(refs.get("translations"));
    if topics.is_empty() && flow_steps.is_empty() && variables.is_empty() && translations.is_empty()
    {
        return None;
    }
    Some(SmsTemplateReferences {
        topics,
        flow_steps,
        variables,
        translations,
    })
}

struct SmsItemQueue<'a> {
    projection: &'a JsonValue,
    remote: &'a HashMap<String, String>,
    metadata: &'a Option<Metadata>,
    local_names: &'a mut HashSet<String>,
    creates: &'a mut Vec<Command>,
    updates: &'a mut Vec<Command>,
}

impl SmsItemQueue<'_> {
    fn queue(&mut self, yaml: &YamlValue, resource_id: &str) {
        let name = yaml_str(yaml, "name");
        if name.is_empty() {
            return;
        }
        self.local_names.insert(name.clone());
        let id = self
            .remote
            .get(&name)
            .cloned()
            .or_else(|| {
                (!is_synthetic_local_resource_id(resource_id)).then_some(resource_id.to_string())
            })
            .unwrap_or_else(|| {
                stable_resource_id("SMS_TEMPLATES", &name, "config/sms_templates.yaml")
            });
        let text = yaml_str(yaml, "text");
        let env_create = sms_env_phone_numbers(yaml);
        let env_update = sms_env_update(yaml);
        let local_refs = sms_references_from_yaml(yaml);
        if self.remote.contains_key(&name) {
            let sms_entities =
                extract_entities_map(self.projection, &["sms", "templates", "entities"]);
            let mut remote_template: Option<&JsonValue> = None;
            if let Some(remote_id) = self.remote.get(&name)
                && let Some(remote) = sms_entities.get(remote_id.as_str())
            {
                remote_template = Some(remote);
                if sms_matches_remote(yaml, remote) {
                    return;
                }
            }
            push_command(
                self.updates,
                self.metadata,
                "sms_update_template",
                CommandPayload::SmsUpdateTemplate(SmsUpdateTemplate {
                    id: id.clone(),
                    name: Some(name.clone()),
                    text: Some(text),
                    env_phone_numbers: Some(env_update),
                    references: local_refs
                        .clone()
                        .or_else(|| sms_references_from_remote(remote_template)),
                    active: Some(true),
                }),
            );
        } else {
            push_command(
                self.creates,
                self.metadata,
                "sms_create_template",
                CommandPayload::SmsCreateTemplate(SmsCreateTemplate {
                    id: id.clone(),
                    name: name.clone(),
                    text,
                    env_phone_numbers: Some(env_create),
                    references: local_refs,
                    active: true,
                }),
            );
        }
    }
}

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, JsonValue)> {
    match payload {
        CommandPayload::SmsDeleteTemplate(delete) => Some((
            "sms_delete_template",
            serde_json::json!({
                "id": delete.id,
            }),
        )),
        CommandPayload::SmsCreateTemplate(create) => Some((
            "sms_create_template",
            serde_json::json!({
                "id": create.id,
                "name": create.name,
                "text": create.text,
                "env_phone_numbers": sms_env_json(create.env_phone_numbers.as_ref()),
                "references": sms_references_json(create.references.as_ref()),
                "active": create.active,
            }),
        )),
        CommandPayload::SmsUpdateTemplate(update) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), JsonValue::String(update.id.clone()));
            value.insert(
                "name".to_string(),
                JsonValue::String(update.name.clone().unwrap_or_default()),
            );
            value.insert(
                "text".to_string(),
                JsonValue::String(update.text.clone().unwrap_or_default()),
            );
            value.insert(
                "env_phone_numbers".to_string(),
                sms_env_update_json(update.env_phone_numbers.as_ref()),
            );
            if update.references.is_some() {
                value.insert(
                    "references".to_string(),
                    sms_references_json(update.references.as_ref()),
                );
            }
            value.insert(
                "active".to_string(),
                JsonValue::Bool(update.active.unwrap_or(false)),
            );
            Some(("sms_update_template", JsonValue::Object(value)))
        }
        _ => None,
    }
}

fn sms_env_json(env: Option<&SmsEnvPhoneNumbers>) -> JsonValue {
    let Some(env) = env else {
        return serde_json::json!({});
    };
    let mut value = serde_json::Map::new();
    if !env.sandbox.is_empty() {
        value.insert(
            "sandbox".to_string(),
            JsonValue::String(env.sandbox.clone()),
        );
    }
    if !env.pre_release.is_empty() {
        value.insert(
            "pre_release".to_string(),
            JsonValue::String(env.pre_release.clone()),
        );
    }
    if !env.live.is_empty() {
        value.insert("live".to_string(), JsonValue::String(env.live.clone()));
    }
    JsonValue::Object(value)
}

fn sms_env_update_json(env: Option<&UpdateSmsEnvPhoneNumbers>) -> JsonValue {
    let Some(env) = env else {
        return serde_json::json!({});
    };
    let mut value = serde_json::Map::new();
    if let Some(sandbox) = &env.sandbox {
        value.insert("sandbox".to_string(), JsonValue::String(sandbox.clone()));
    }
    if let Some(pre_release) = &env.pre_release {
        value.insert(
            "pre_release".to_string(),
            JsonValue::String(pre_release.clone()),
        );
    }
    if let Some(live) = &env.live {
        value.insert("live".to_string(), JsonValue::String(live.clone()));
    }
    JsonValue::Object(value)
}

fn sms_references_json(references: Option<&SmsTemplateReferences>) -> JsonValue {
    let Some(references) = references else {
        return serde_json::json!({});
    };
    let mut value = serde_json::Map::new();
    if !references.topics.is_empty() {
        value.insert("topics".to_string(), serde_json::json!(references.topics));
    }
    if !references.flow_steps.is_empty() {
        value.insert(
            "flow_steps".to_string(),
            serde_json::json!(references.flow_steps),
        );
    }
    if !references.variables.is_empty() {
        value.insert(
            "variables".to_string(),
            serde_json::json!(references.variables),
        );
    }
    if !references.translations.is_empty() {
        value.insert(
            "translations".to_string(),
            serde_json::json!(references.translations),
        );
    }
    JsonValue::Object(value)
}

#[cfg(test)]
#[path = "command_gen_tests.rs"]
mod command_gen_tests;
