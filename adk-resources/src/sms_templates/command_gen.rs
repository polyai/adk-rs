use crate::command_gen::CommandGroups;
use crate::ids::stable_resource_id;
use crate::{extract_entities_map, is_synthetic_local_resource_id, push_command};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::sms::{
    SmsCreateTemplate, SmsDeleteTemplate, SmsEnvPhoneNumbers, SmsTemplateReferences,
    SmsUpdateTemplate, UpdateSmsEnvPhoneNumbers,
};
use adk_protobuf::{Command, Metadata};
use adk_types::ResourceMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(crate) fn sms_template_command_groups(
    resources: &ResourceMap,
    projection: &Value,
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
                .and_then(Value::as_str)
                .unwrap_or_default();

            if path == "config/sms_templates.yaml" {
                if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content)
                    && let Some(items) = yaml
                        .get("sms_templates")
                        .and_then(serde_yaml::Value::as_sequence)
                {
                    for item in items {
                        queue.queue(item, "local");
                    }
                }
                continue;
            }

            if path.starts_with("config/sms_templates.yaml/sms_templates/")
                && let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(content)
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
    }
}

fn remote_sms(projection: &Value) -> HashMap<String, String> {
    let entities = extract_entities_map(projection, &["sms", "templates", "entities"]);
    let mut sms = HashMap::new();
    for (id, value) in entities {
        if !value
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        sms.insert(name, id);
    }
    sms
}

fn yaml_str(yaml: &serde_yaml::Value, key: &str) -> String {
    yaml.get(key)
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn sms_env_phone_numbers(yaml: &serde_yaml::Value) -> SmsEnvPhoneNumbers {
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

fn sms_env_update(yaml: &serde_yaml::Value) -> UpdateSmsEnvPhoneNumbers {
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

fn sms_matches_remote(local: &serde_yaml::Value, remote: &Value) -> bool {
    if local.get("name").and_then(|v| v.as_str()).unwrap_or("")
        != remote.get("name").and_then(Value::as_str).unwrap_or("")
    {
        return false;
    }
    if local.get("text").and_then(|v| v.as_str()).unwrap_or("")
        != remote.get("text").and_then(Value::as_str).unwrap_or("")
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
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("")
            .to_string(),
        local_env
            .and_then(|env| env.get("preRelease"))
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("")
            .to_string(),
    );
    let remote_pre = non_empty(
        remote_env
            .and_then(|env| env.get("preRelease"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        remote_env
            .and_then(|env| env.get("pre_release"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    );
    local_env
        .and_then(|env| env.get("sandbox"))
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("")
        == remote_env
            .and_then(|env| env.get("sandbox"))
            .and_then(Value::as_str)
            .unwrap_or("")
        && local_pre == remote_pre
        && local_env
            .and_then(|env| env.get("live"))
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("")
            == remote_env
                .and_then(|env| env.get("live"))
                .and_then(Value::as_str)
                .unwrap_or("")
}

fn yaml_reference_map(yaml: Option<&serde_yaml::Value>) -> HashMap<String, bool> {
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

fn json_reference_map(value: Option<&Value>) -> HashMap<String, bool> {
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

fn sms_references_from_yaml(yaml: &serde_yaml::Value) -> Option<SmsTemplateReferences> {
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

fn sms_references_from_remote(remote: Option<&Value>) -> Option<SmsTemplateReferences> {
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
    projection: &'a Value,
    remote: &'a HashMap<String, String>,
    metadata: &'a Option<Metadata>,
    local_names: &'a mut HashSet<String>,
    creates: &'a mut Vec<Command>,
    updates: &'a mut Vec<Command>,
}

impl SmsItemQueue<'_> {
    fn queue(&mut self, yaml: &serde_yaml::Value, resource_id: &str) {
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
            let mut remote_template: Option<&Value> = None;
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

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
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
            value.insert("id".to_string(), Value::String(update.id.clone()));
            value.insert(
                "name".to_string(),
                Value::String(update.name.clone().unwrap_or_default()),
            );
            value.insert(
                "text".to_string(),
                Value::String(update.text.clone().unwrap_or_default()),
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
                Value::Bool(update.active.unwrap_or(false)),
            );
            Some(("sms_update_template", Value::Object(value)))
        }
        _ => None,
    }
}

fn sms_env_json(env: Option<&SmsEnvPhoneNumbers>) -> Value {
    let Some(env) = env else {
        return serde_json::json!({});
    };
    let mut value = serde_json::Map::new();
    if !env.sandbox.is_empty() {
        value.insert("sandbox".to_string(), Value::String(env.sandbox.clone()));
    }
    if !env.pre_release.is_empty() {
        value.insert(
            "pre_release".to_string(),
            Value::String(env.pre_release.clone()),
        );
    }
    if !env.live.is_empty() {
        value.insert("live".to_string(), Value::String(env.live.clone()));
    }
    Value::Object(value)
}

fn sms_env_update_json(env: Option<&UpdateSmsEnvPhoneNumbers>) -> Value {
    let Some(env) = env else {
        return serde_json::json!({});
    };
    let mut value = serde_json::Map::new();
    if let Some(sandbox) = &env.sandbox {
        value.insert("sandbox".to_string(), Value::String(sandbox.clone()));
    }
    if let Some(pre_release) = &env.pre_release {
        value.insert(
            "pre_release".to_string(),
            Value::String(pre_release.clone()),
        );
    }
    if let Some(live) = &env.live {
        value.insert("live".to_string(), Value::String(live.clone()));
    }
    Value::Object(value)
}

fn sms_references_json(references: Option<&SmsTemplateReferences>) -> Value {
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
    Value::Object(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_types::Resource;
    use indexmap::IndexMap;

    fn map_with(resources: Vec<(String, Resource)>) -> ResourceMap {
        let mut map: ResourceMap = IndexMap::new();
        for (path, resource) in resources {
            map.insert(path, resource);
        }
        map
    }

    fn flatten(groups: CommandGroups) -> Vec<Command> {
        groups
            .deletes
            .into_iter()
            .chain(groups.creates)
            .chain(groups.updates)
            .chain(groups.post_updates)
            .collect()
    }

    #[test]
    fn sms_create_populates_references_from_yaml() {
        let sms_yaml = r#"
name: Welcome
text: hi
references:
  topics:
    topic-1: true
  flow_steps:
    flow-1: true
  variables:
    var-1: true
  translations:
    tr-1: true
"#;
        let resources = map_with(vec![(
            "config/sms_templates.yaml/sms_templates/Welcome".into(),
            Resource {
                resource_id: "twilio_sms-1".into(),
                name: "Welcome".into(),
                file_path: "config/sms_templates.yaml/sms_templates/Welcome".into(),
                payload: serde_json::json!({ "content": sms_yaml }),
            },
        )]);
        let projection = serde_json::json!({});
        let commands = flatten(sms_template_command_groups(&resources, &projection, &None));
        let create = commands
            .iter()
            .find(|command| command.r#type == "sms_create_template")
            .expect("sms create command");
        match &create.payload {
            Some(CommandPayload::SmsCreateTemplate(message)) => {
                let refs = message.references.as_ref().expect("references");
                assert!(refs.topics.get("topic-1").copied().unwrap_or(false));
                assert!(refs.flow_steps.get("flow-1").copied().unwrap_or(false));
                assert!(refs.variables.get("var-1").copied().unwrap_or(false));
                assert!(refs.translations.get("tr-1").copied().unwrap_or(false));
            }
            _ => panic!("unexpected payload variant for SMS create command"),
        }
    }
}
