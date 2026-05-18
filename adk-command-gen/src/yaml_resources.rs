use serde::Serialize;
use serde_json::Value;

// These structs mirror the Python ADK `to_yaml_dict()` methods. Field order is
// user-visible because ADK project files are often checked into Git.
pub(crate) fn to_yaml_string<T: Serialize>(value: &T) -> Result<String, serde_yaml::Error> {
    serde_yaml::to_string(value)
}

fn is_empty(value: &str) -> bool {
    value.is_empty()
}

fn is_none_or_empty(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(str::is_empty)
}

#[derive(Serialize)]
pub(crate) struct TopicYaml {
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) actions: String,
    pub(crate) content: String,
    pub(crate) example_queries: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct EntitiesYaml {
    pub(crate) entities: Vec<EntityYaml>,
}

#[derive(Serialize)]
pub(crate) struct EntityYaml {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) entity_type: String,
    pub(crate) config: Value,
}

#[derive(Serialize)]
pub(crate) struct HandoffsYaml {
    pub(crate) handoffs: Vec<HandoffYaml>,
}

#[derive(Serialize)]
pub(crate) struct HandoffYaml {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) is_default: bool,
    pub(crate) sip_config: Value,
    pub(crate) sip_headers: Value,
}

#[derive(Serialize)]
pub(crate) struct SmsTemplatesYaml {
    pub(crate) sms_templates: Vec<SmsTemplateYaml>,
}

#[derive(Serialize)]
pub(crate) struct SmsTemplateYaml {
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) env_phone_numbers: EnvPhoneNumbersYaml,
}

#[derive(Serialize)]
pub(crate) struct EnvPhoneNumbersYaml {
    pub(crate) sandbox: String,
    pub(crate) pre_release: String,
    pub(crate) live: String,
}

#[derive(Serialize)]
pub(crate) struct PhraseFilteringYaml {
    pub(crate) phrase_filtering: Vec<PhraseFilterYaml>,
}

#[derive(Serialize)]
pub(crate) struct PhraseFilterYaml {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "is_empty")]
    pub(crate) description: String,
    pub(crate) regular_expressions: Vec<Value>,
    pub(crate) say_phrase: bool,
    #[serde(skip_serializing_if = "is_empty")]
    pub(crate) language_code: String,
    #[serde(skip_serializing_if = "is_none_or_empty")]
    pub(crate) function: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct FlowConfigYaml {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) start_step: String,
}

#[derive(Serialize)]
pub(crate) struct FlowStepYaml {
    pub(crate) step_type: String,
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) asr_biasing: Option<AsrBiasingYaml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dtmf_config: Option<DtmfConfigYaml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) conditions: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) extracted_entities: Option<Value>,
    pub(crate) prompt: String,
}

#[derive(Serialize)]
pub(crate) struct AsrBiasingYaml {
    pub(crate) is_enabled: bool,
    pub(crate) alphanumeric: bool,
    pub(crate) name_spelling: bool,
    pub(crate) numeric: bool,
    pub(crate) party_size: bool,
    pub(crate) precise_date: bool,
    pub(crate) relative_date: bool,
    pub(crate) single_number: bool,
    pub(crate) time: bool,
    pub(crate) yes_no: bool,
    pub(crate) address: bool,
    pub(crate) custom_keywords: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct DtmfConfigYaml {
    pub(crate) is_enabled: bool,
    pub(crate) inter_digit_timeout: i32,
    pub(crate) max_digits: i32,
    pub(crate) end_key: String,
    pub(crate) collect_while_agent_speaking: bool,
    pub(crate) is_pii: bool,
}
