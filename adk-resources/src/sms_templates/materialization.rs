use crate::yaml_resources::{
    EnvPhoneNumbersYaml, SmsTemplateYaml, SmsTemplatesYaml, to_yaml_string,
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
        sms_templates.push(SmsTemplateYaml {
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
                    .and_then(|value| value.get("sandbox"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                pre_release: sms
                    .get("envPhoneNumbers")
                    .and_then(|value| value.get("preRelease"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                live: sms
                    .get("envPhoneNumbers")
                    .and_then(|value| value.get("live"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            },
        });
    }
    if sms_templates.is_empty() {
        return Ok(());
    }

    let content = to_yaml_string(&SmsTemplatesYaml { sms_templates })
        .map_err(|e| CommandGenError::InvalidData(e.to_string()))?;
    crate::materialization::insert_content_resource(
        map,
        "config/sms_templates.yaml",
        "sms_templates",
        "sms_templates",
        content,
    )
}

fn sms_entries_vec(projection: &Value) -> Vec<(String, Value)> {
    extract_entities_vec(projection, &["sms", "templates", "entities"])
}
