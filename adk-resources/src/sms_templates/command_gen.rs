use crate::ids::stable_resource_id;
use crate::push_commands::CommandGroups;
use crate::sms_templates::local::{
    SMS_TEMPLATE_ITEM_PREFIX, SMS_TEMPLATES_FILE_PATH, SmsTemplate, parse_sms_templates_content,
};
use crate::{
    PromptReferenceMaps, extract_entities_map, extract_template_references,
    is_synthetic_local_resource_id, prompt_reference_maps_from_projection, push_command,
    replace_resource_names_with_ids,
};
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::sms::{
    SmsCreateTemplate, SmsDeleteTemplate, SmsEnvPhoneNumbers, SmsTemplateReferences,
    SmsUpdateTemplate, UpdateSmsEnvPhoneNumbers,
};
use adk_protobuf::{Command, Metadata};
use adk_types::ResourceMap;
use serde_json::{self, Value as JsonValue};
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
    let Some(local_templates) = local_sms_template_resources(resources) else {
        return CommandGroups::default();
    };
    let prompt_reference_maps = prompt_reference_maps_from_projection(projection);

    {
        let mut builder = SmsTemplateCommandBuilder {
            projection,
            prompt_reference_maps: &prompt_reference_maps,
            remote: &remote,
            metadata,
            local_names: &mut local_names,
            creates: &mut creates,
            updates: &mut updates,
        };

        for local in &local_templates {
            builder.append_item(&local.template, &local.resource_id);
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

struct LocalSmsTemplateResource {
    resource_id: String,
    template: SmsTemplate,
}

fn local_sms_template_resources(resources: &ResourceMap) -> Option<Vec<LocalSmsTemplateResource>> {
    let mut templates = Vec::new();
    for resource in resources.values() {
        let path = resource.file_path.as_str();
        if path != SMS_TEMPLATES_FILE_PATH && !path.starts_with(SMS_TEMPLATE_ITEM_PREFIX) {
            continue;
        }
        let content = resource
            .payload
            .get("content")
            .and_then(JsonValue::as_str)
            .unwrap_or_default();
        let parsed_templates = parse_sms_templates_content(path, content).ok()?;
        let resource_id = if path == SMS_TEMPLATES_FILE_PATH {
            "local"
        } else {
            resource.resource_id.as_str()
        };
        templates.extend(
            parsed_templates
                .into_iter()
                .map(|template| LocalSmsTemplateResource {
                    resource_id: resource_id.to_string(),
                    template,
                }),
        );
    }
    Some(templates)
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

fn non_empty(left: String, right: String) -> String {
    if left.is_empty() { right } else { left }
}

fn sms_matches_remote(local: &SmsTemplate, text: &str, remote: &JsonValue) -> bool {
    if local.name() != remote.get("name").and_then(JsonValue::as_str).unwrap_or("") {
        return false;
    }
    if text != remote.get("text").and_then(JsonValue::as_str).unwrap_or("") {
        return false;
    }
    let local_env = local.env_phone_numbers();
    let remote_env = remote.get("envPhoneNumbers");
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
    local_env.sandbox()
        == remote_env
            .and_then(|env| env.get("sandbox"))
            .and_then(JsonValue::as_str)
            .unwrap_or("")
        && local_env.pre_release() == remote_pre
        && local_env.live()
            == remote_env
                .and_then(|env| env.get("live"))
                .and_then(JsonValue::as_str)
                .unwrap_or("")
}

fn sms_references_from_text(text: &str) -> SmsTemplateReferences {
    let mut variables = extract_template_references(text, "vrbl");
    variables.extend(extract_template_references(text, "var"));
    SmsTemplateReferences {
        topics: HashMap::new(),
        flow_steps: HashMap::new(),
        variables,
        translations: extract_template_references(text, "tn"),
    }
}

struct SmsTemplateCommandBuilder<'a> {
    projection: &'a JsonValue,
    prompt_reference_maps: &'a PromptReferenceMaps,
    remote: &'a HashMap<String, String>,
    metadata: &'a Option<Metadata>,
    local_names: &'a mut HashSet<String>,
    creates: &'a mut Vec<Command>,
    updates: &'a mut Vec<Command>,
}

impl SmsTemplateCommandBuilder<'_> {
    fn append_item(&mut self, template: &SmsTemplate, resource_id: &str) {
        let name = template.name().to_string();
        self.local_names.insert(name.clone());
        let id = self
            .remote
            .get(&name)
            .cloned()
            .or_else(|| {
                (!is_synthetic_local_resource_id(resource_id)).then_some(resource_id.to_string())
            })
            .unwrap_or_else(|| stable_resource_id("SMS_TEMPLATES", &name, SMS_TEMPLATES_FILE_PATH));
        let text =
            replace_resource_names_with_ids(template.text(), self.prompt_reference_maps, None);
        let env_create = template.env_phone_numbers_proto();
        let env_update = template.env_phone_numbers_update_proto();
        let references = sms_references_from_text(&text);
        if self.remote.contains_key(&name) {
            let sms_entities =
                extract_entities_map(self.projection, &["sms", "templates", "entities"]);
            if let Some(remote_id) = self.remote.get(&name)
                && let Some(remote) = sms_entities.get(remote_id.as_str())
                && sms_matches_remote(template, &text, remote)
            {
                return;
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
                    references: Some(references),
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
                    references: Some(references),
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
