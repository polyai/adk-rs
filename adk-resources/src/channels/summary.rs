use adk_protobuf::agent::{DisclaimerMessageUpdateDisclaimerMessage, GreetingUpdateGreeting};
use adk_protobuf::channels::ChannelType;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::content_filter_settings::{
    AzureContentFilter, AzureContentFilterCategory,
    ContentFilterSettingsUpdateContentFilterSettings,
};
use serde_json::{Value, json};

pub(crate) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    match payload {
        CommandPayload::ChannelUpdateGreeting(msg) => Some((
            "channel_update_greeting",
            channel_payload_json(
                msg.channel_type,
                "greeting",
                msg.greeting
                    .as_ref()
                    .map(greeting_json)
                    .unwrap_or_else(|| json!({})),
            ),
        )),
        CommandPayload::ChannelUpdateStylePrompt(msg) => Some((
            "channel_update_style_prompt",
            channel_payload_json(
                msg.channel_type,
                "style_prompt",
                msg.style_prompt
                    .as_ref()
                    .map(|style_prompt| json!({ "prompt": style_prompt.prompt }))
                    .unwrap_or_else(|| json!({})),
            ),
        )),
        CommandPayload::ChannelUpdateSafetyFilters(msg) => Some((
            "channel_update_safety_filters",
            channel_payload_json(
                msg.channel_type,
                "safety_filters",
                msg.safety_filters
                    .as_ref()
                    .map(content_filter_settings_json)
                    .unwrap_or_else(|| json!({})),
            ),
        )),
        CommandPayload::VoiceChannelUpdateDisclaimer(msg) => Some((
            "voice_channel_update_disclaimer",
            json!({
                "disclaimer": msg
                    .disclaimer
                    .as_ref()
                    .map(disclaimer_json)
                    .unwrap_or_else(|| json!({})),
            }),
        )),
        _ => None,
    }
}

fn content_filter_settings_json(
    settings: &ContentFilterSettingsUpdateContentFilterSettings,
) -> Value {
    let mut value = serde_json::Map::new();
    value.insert(
        "type".to_string(),
        Value::String(settings.r#type.clone().unwrap_or_default()),
    );
    value.insert(
        "disabled".to_string(),
        Value::Bool(settings.disabled.unwrap_or(false)),
    );
    if let Some(azure_config) = &settings.azure_config {
        value.insert(
            "azure_config".to_string(),
            azure_content_filter_json(azure_config),
        );
    }
    Value::Object(value)
}

fn azure_content_filter_json(filter: &AzureContentFilter) -> Value {
    let mut value = serde_json::Map::new();
    if let Some(hate) = &filter.hate {
        value.insert("hate".to_string(), content_filter_category_json(hate));
    }
    if let Some(self_harm) = &filter.self_harm {
        value.insert(
            "self_harm".to_string(),
            content_filter_category_json(self_harm),
        );
    }
    if let Some(sexual) = &filter.sexual {
        value.insert("sexual".to_string(), content_filter_category_json(sexual));
    }
    if let Some(violence) = &filter.violence {
        value.insert(
            "violence".to_string(),
            content_filter_category_json(violence),
        );
    }
    Value::Object(value)
}

fn content_filter_category_json(category: &AzureContentFilterCategory) -> Value {
    let mut value = serde_json::Map::new();
    if category.is_active {
        value.insert("is_active".to_string(), Value::Bool(true));
    }
    value.insert(
        "precision".to_string(),
        Value::String(category.precision.clone()),
    );
    Value::Object(value)
}

fn channel_payload_json(channel_type: i32, payload_key: &str, payload: Value) -> Value {
    let mut value = serde_json::Map::new();
    if channel_type == ChannelType::WebChat as i32 {
        value.insert(
            "channel_type".to_string(),
            Value::String("WEB_CHAT".to_string()),
        );
    }
    value.insert(payload_key.to_string(), payload);
    Value::Object(value)
}

fn greeting_json(greeting: &GreetingUpdateGreeting) -> Value {
    json!({
        "welcome_message": greeting.welcome_message.clone().unwrap_or_default(),
        "language_code": greeting.language_code,
    })
}

fn disclaimer_json(disclaimer: &DisclaimerMessageUpdateDisclaimerMessage) -> Value {
    json!({
        "message": disclaimer.message.clone().unwrap_or_default(),
        "is_enabled": disclaimer.is_enabled.unwrap_or(false),
        "language_code": disclaimer.language_code,
    })
}
