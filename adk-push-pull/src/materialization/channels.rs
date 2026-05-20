use super::insert_yaml_resource;
use crate::CommandGenError;
use adk_types::ResourceMap;
use serde_json::Value;

pub(super) fn insert_channel_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    if let Some(voice_safety_filters) = projection.pointer("/channels/voice/config/safetyFilters") {
        insert_yaml_resource(
            map,
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
            map,
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
            map,
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
                map,
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
                map,
                "chat/safety_filters.yaml",
                "chat_safety_filters",
                "chat_safety_filters",
                safety_filters_yaml(chat_safety_filters, true),
            )?;
        }
    }

    Ok(())
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

pub(super) fn safety_filters_yaml(settings: &Value, include_enabled: bool) -> Value {
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
