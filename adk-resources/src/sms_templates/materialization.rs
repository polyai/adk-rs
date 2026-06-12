use crate::materialization::to_yaml_string;
use crate::sms_templates::local::{
    EnvPhoneNumbers, SMS_TEMPLATES_FILE_PATH, SmsTemplate, SmsTemplatesFile,
};
use crate::{CommandGenError, extract_entities_vec};
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_sms_template_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let mut sms_templates = Vec::new();
    for (_id, sms) in sms_entries_vec(projection) {
        if !sms.get("active").and_then(Value::as_bool).unwrap_or(true) {
            continue;
        }
        sms_templates.push(local_sms_template_from_projection(&sms)?);
    }
    if sms_templates.is_empty() {
        return Ok(());
    }

    let content = to_yaml_string(&SmsTemplatesFile::new(sms_templates))
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
    crate::materialization::insert_content_resource(
        map,
        SMS_TEMPLATES_FILE_PATH,
        "sms_templates",
        "sms_templates",
        content,
    )
}

fn sms_entries_vec(projection: &Value) -> Vec<(String, Value)> {
    extract_entities_vec(projection, &["sms", "templates", "entities"])
}

fn local_sms_template_from_projection(sms: &Value) -> Result<SmsTemplate, CommandGenError> {
    SmsTemplate::new(
        json_str(sms, "name"),
        json_str(sms, "text"),
        EnvPhoneNumbers {
            sandbox: sms
                .get("envPhoneNumbers")
                .map(|env| json_str(env, "sandbox"))
                .unwrap_or_default(),
            pre_release: sms
                .get("envPhoneNumbers")
                .map(|env| json_str(env, "preRelease"))
                .unwrap_or_default(),
            live: sms
                .get("envPhoneNumbers")
                .map(|env| json_str(env, "live"))
                .unwrap_or_default(),
        },
    )
    .map_err(invalid_sms_template_projection)
}

fn json_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn invalid_sms_template_projection(error: String) -> CommandGenError {
    CommandGenError::InvalidData(format!("Invalid SMS template projection: {error}"))
}
