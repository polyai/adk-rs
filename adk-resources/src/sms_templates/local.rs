use crate::local_parse::{
    NonEmptyString, ResourceParseErrors, ResourceParseResult, deserialize_yaml, duplicate_names,
};
use adk_protobuf::sms::{SmsEnvPhoneNumbers, SmsTemplateReferences, UpdateSmsEnvPhoneNumbers};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;
use std::collections::HashMap;

pub(crate) const SMS_TEMPLATES_FILE_PATH: &str = "config/sms_templates.yaml";
pub(crate) const SMS_TEMPLATE_ITEM_PREFIX: &str = "config/sms_templates.yaml/sms_templates/";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SmsTemplatesFile {
    sms_templates: Vec<SmsTemplate>,
}

impl SmsTemplatesFile {
    pub(crate) fn new(sms_templates: Vec<SmsTemplate>) -> Self {
        Self { sms_templates }
    }

    fn try_from_raw(path: &str, raw: RawSmsTemplatesFile) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        for duplicate in duplicate_names(raw.sms_templates.iter().map(|item| item.name())) {
            errors.push(
                &format!("{path}/sms_templates/{duplicate}"),
                format!("duplicate SMS template name '{duplicate}'."),
            );
        }
        if errors.is_empty() {
            Ok(Self {
                sms_templates: raw.sms_templates,
            })
        } else {
            Err(errors)
        }
    }
}

pub(crate) fn parse_sms_templates_file(
    path: &str,
    yaml: &Value,
) -> ResourceParseResult<SmsTemplatesFile> {
    let raw = deserialize_yaml::<RawSmsTemplatesFile>(path, yaml)?;
    SmsTemplatesFile::try_from_raw(path, raw)
}

pub(crate) fn parse_sms_templates_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<Vec<SmsTemplate>> {
    let yaml = serde_yaml_ng::from_str::<Value>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    if path == SMS_TEMPLATES_FILE_PATH {
        return deserialize_yaml::<RawSmsTemplatesFile>(path, &yaml)
            .map(|file| file.into_sms_templates());
    }
    if path.starts_with(SMS_TEMPLATE_ITEM_PREFIX) {
        return deserialize_yaml::<SmsTemplate>(path, &yaml).map(|template| vec![template]);
    }
    Ok(Vec::new())
}

#[derive(Debug, Clone, Deserialize)]
struct RawSmsTemplatesFile {
    #[serde(default)]
    sms_templates: Vec<SmsTemplate>,
}

impl RawSmsTemplatesFile {
    fn into_sms_templates(self) -> Vec<SmsTemplate> {
        self.sms_templates
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct SmsTemplate {
    name: NonEmptyString,
    text: NonEmptyString,
    #[serde(alias = "envPhoneNumbers")]
    env_phone_numbers: EnvPhoneNumbers,
    #[serde(
        default,
        alias = "refs",
        skip_serializing_if = "SmsReferences::is_empty"
    )]
    references: SmsReferences,
}

impl SmsTemplate {
    pub(super) fn new(
        name: String,
        text: String,
        env_phone_numbers: EnvPhoneNumbers,
    ) -> Result<Self, String> {
        Ok(Self {
            name: NonEmptyString::new(name)?,
            text: NonEmptyString::new(text)?,
            env_phone_numbers,
            references: SmsReferences::default(),
        })
    }

    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn text(&self) -> &str {
        self.text.as_str()
    }

    pub(crate) fn env_phone_numbers_proto(&self) -> SmsEnvPhoneNumbers {
        self.env_phone_numbers.to_create_proto()
    }

    pub(crate) fn env_phone_numbers_update_proto(&self) -> UpdateSmsEnvPhoneNumbers {
        self.env_phone_numbers.to_update_proto()
    }

    pub(super) fn env_phone_numbers(&self) -> &EnvPhoneNumbers {
        &self.env_phone_numbers
    }

    pub(crate) fn references_proto(&self) -> Option<SmsTemplateReferences> {
        self.references.to_proto()
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub(super) struct EnvPhoneNumbers {
    #[serde(default)]
    pub(super) sandbox: String,
    #[serde(default, alias = "preRelease")]
    pub(super) pre_release: String,
    #[serde(default)]
    pub(super) live: String,
}

impl EnvPhoneNumbers {
    pub(super) fn sandbox(&self) -> &str {
        &self.sandbox
    }

    pub(super) fn pre_release(&self) -> &str {
        &self.pre_release
    }

    pub(super) fn live(&self) -> &str {
        &self.live
    }

    fn to_create_proto(&self) -> SmsEnvPhoneNumbers {
        SmsEnvPhoneNumbers {
            sandbox: self.sandbox.clone(),
            pre_release: self.pre_release.clone(),
            live: self.live.clone(),
        }
    }

    fn to_update_proto(&self) -> UpdateSmsEnvPhoneNumbers {
        UpdateSmsEnvPhoneNumbers {
            sandbox: Some(self.sandbox.clone()),
            pre_release: Some(self.pre_release.clone()),
            live: Some(self.live.clone()),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct SmsReferences {
    #[serde(default, deserialize_with = "reference_map")]
    topics: HashMap<String, bool>,
    #[serde(default, deserialize_with = "reference_map")]
    flow_steps: HashMap<String, bool>,
    #[serde(default, deserialize_with = "reference_map")]
    variables: HashMap<String, bool>,
    #[serde(default, deserialize_with = "reference_map")]
    translations: HashMap<String, bool>,
}

impl SmsReferences {
    fn is_empty(&self) -> bool {
        self.topics.is_empty()
            && self.flow_steps.is_empty()
            && self.variables.is_empty()
            && self.translations.is_empty()
    }

    fn to_proto(&self) -> Option<SmsTemplateReferences> {
        (!self.is_empty()).then(|| SmsTemplateReferences {
            topics: self.topics.clone(),
            flow_steps: self.flow_steps.clone(),
            variables: self.variables.clone(),
            translations: self.translations.clone(),
        })
    }
}

fn reference_map<'de, D>(deserializer: D) -> Result<HashMap<String, bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?.unwrap_or(Value::Null);
    if let Some(items) = value.as_sequence() {
        return Ok(items
            .iter()
            .filter_map(|value| value.as_str().map(|key| (key.to_string(), true)))
            .collect());
    }
    if let Some(items) = value.as_mapping() {
        return Ok(items
            .iter()
            .filter_map(|(key, value)| {
                Some((key.as_str()?.to_string(), value.as_bool().unwrap_or(true)))
            })
            .collect());
    }
    Ok(HashMap::new())
}
