use super::api_integrations::environments_json;
use super::transcript_corrections::{regular_expression_json, transcript_correction_json};
use super::variants::{attribute_references_json, attribute_values_json};
use adk_protobuf::agent::{DisclaimerMessageUpdateDisclaimerMessage, GreetingUpdateGreeting};
use adk_protobuf::asr_settings::AsrSettingsUpdateAsrSettings;
use adk_protobuf::channels::ChannelType;
use adk_protobuf::command::Payload as CommandPayload;
use adk_protobuf::content_filter_settings::{
    AzureContentFilter, AzureContentFilterCategory,
    ContentFilterSettingsUpdateContentFilterSettings,
};
use serde_json::{Value, json};

pub(super) fn payload_json_summary(payload: &CommandPayload) -> Option<(&'static str, Value)> {
    match payload {
        CommandPayload::VariantCreateVariant(msg) => Some((
            "variant_create_variant",
            json!({
                "id": msg.id,
                "name": msg.name,
                "attribute_values": attribute_values_json(msg.attribute_values.as_ref()),
            }),
        )),
        CommandPayload::VariantCreateAttribute(msg) => Some((
            "variant_create_attribute",
            json!({
                "id": msg.id,
                "name": msg.name,
                "references": attribute_references_json(msg.references.as_ref()),
                "variant_values": {
                    "values": msg
                        .variant_values
                        .as_ref()
                        .map(|values| json!(values.values))
                        .unwrap_or_else(|| json!({})),
                },
            }),
        )),
        CommandPayload::VariantDeleteVariant(msg) => Some((
            "variant_delete_variant",
            json!({
                "id": msg.id,
            }),
        )),
        CommandPayload::VariantDeleteAttribute(msg) => Some((
            "variant_delete_attribute",
            json!({
                "id": msg.id,
            }),
        )),
        CommandPayload::VariantSetDefaultVariant(msg) => Some((
            "variant_set_default_variant",
            json!({
                "id": msg.id,
            }),
        )),
        CommandPayload::VariantUpdateAttribute(msg) => {
            let mut value = serde_json::Map::new();
            value.insert("id".to_string(), json!(msg.id));
            if let Some(name) = &msg.name {
                value.insert("name".to_string(), json!(name));
            }
            if let Some(references) = &msg.references {
                value.insert(
                    "references".to_string(),
                    attribute_references_json(Some(references)),
                );
            }
            value.insert(
                "variant_values".to_string(),
                json!({
                    "values": msg
                        .variant_values
                        .as_ref()
                        .map(|values| json!(values.values))
                        .unwrap_or_else(|| json!({})),
                }),
            );
            Some(("variant_update_attribute", Value::Object(value)))
        }
        CommandPayload::CreateApiIntegration(msg) => Some((
            "create_api_integration",
            json!({
                "id": msg.id,
                "name": msg.name,
                "description": msg.description.clone().unwrap_or_default(),
                "environments": environments_json(msg.environments.as_ref()),
            }),
        )),
        CommandPayload::UpdateApiIntegration(msg) => Some((
            "update_api_integration",
            json!({
                "id": msg.id,
                "name": msg.name.clone().unwrap_or_default(),
                "description": msg.description.clone().unwrap_or_default(),
            }),
        )),
        CommandPayload::DeleteApiIntegration(msg) => Some((
            "delete_api_integration",
            json!({
                "id": msg.id,
            }),
        )),
        CommandPayload::UpdateApiIntegrationConfig(msg) => Some((
            "update_api_integration_config",
            json!({
                "id": msg.id,
                "environment": msg.environment,
                "base_url": msg.base_url.clone().unwrap_or_default(),
                "auth_type": msg.auth_type.clone().unwrap_or_default(),
            }),
        )),
        CommandPayload::CreateApiIntegrationOperation(msg) => Some((
            "create_api_integration_operation",
            json!({
                "id": msg.id,
                "integration_id": msg.integration_id,
                "name": msg.name,
                "method": msg.method,
                "resource": msg.resource,
            }),
        )),
        CommandPayload::UpdateApiIntegrationOperation(msg) => Some((
            "update_api_integration_operation",
            json!({
                "id": msg.id,
                "integration_id": msg.integration_id.clone().unwrap_or_default(),
                "name": msg.name.clone().unwrap_or_default(),
                "method": msg.method.clone().unwrap_or_default(),
                "resource": msg.resource.clone().unwrap_or_default(),
            }),
        )),
        CommandPayload::DeleteApiIntegrationOperation(msg) => Some((
            "delete_api_integration_operation",
            json!({
                "id": msg.id,
                "integration_id": msg.integration_id,
            }),
        )),
        CommandPayload::CreateKeyphraseBoosting(msg) => Some((
            "create_keyphrase_boosting",
            json!({
                "id": msg.id,
                "keyphrase": msg.keyphrase,
                "level": msg.level,
            }),
        )),
        CommandPayload::UpdateKeyphraseBoosting(msg) => Some((
            "update_keyphrase_boosting",
            json!({
                "id": msg.id,
                "keyphrase": msg.keyphrase.clone().unwrap_or_default(),
                "level": msg.level.clone().unwrap_or_default(),
            }),
        )),
        CommandPayload::DeleteKeyphraseBoosting(msg) => Some((
            "delete_keyphrase_boosting",
            json!({
                "id": msg.id,
            }),
        )),
        CommandPayload::CreateTranscriptCorrections(msg) => Some((
            "create_transcript_corrections",
            json!({
                "id": msg.id,
                "name": msg.name,
                "description": msg.description.clone().unwrap_or_default(),
                "regular_expressions": msg.regular_expressions.iter().map(regular_expression_json).collect::<Vec<_>>(),
            }),
        )),
        CommandPayload::UpdateTranscriptCorrections(msg) => Some((
            "update_transcript_corrections",
            json!({
                "data": {
                    "corrections": msg
                        .data
                        .as_ref()
                        .map(|data| {
                            data.corrections
                                .iter()
                                .map(transcript_correction_json)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                },
            }),
        )),
        CommandPayload::DeleteTranscriptCorrections(msg) => Some((
            "delete_transcript_corrections",
            json!({
                "transcript_corrections_id": msg.transcript_corrections_id,
            }),
        )),
        CommandPayload::PronunciationsCreatePronunciation(msg) => Some((
            "pronunciations_create_pronunciation",
            json!({
                "id": msg.id,
                "regex": msg.regex,
                "replacement": msg.replacement,
                "case_sensitive": msg.case_sensitive,
                "language_code": msg.language_code,
            }),
        )),
        CommandPayload::PronunciationsUpdatePronunciation(msg) => Some((
            "pronunciations_update_pronunciation",
            json!({
                "id": msg.id.clone().unwrap_or_default(),
                "regex": msg.regex.clone().unwrap_or_default(),
                "replacement": msg.replacement.clone().unwrap_or_default(),
                "case_sensitive": msg.case_sensitive.unwrap_or(false),
                "language_code": msg.language_code.clone().unwrap_or_default(),
                "description": msg.description.clone().unwrap_or_default(),
                "position": msg.position.unwrap_or(0),
                "name": msg.name.clone().unwrap_or_default(),
            }),
        )),
        CommandPayload::PronunciationsDeletePronunciation(msg) => Some((
            "pronunciations_delete_pronunciation",
            json!({
                "id": msg.id,
            }),
        )),
        CommandPayload::UpdatePersonality(msg) => Some((
            "update_personality",
            json!({
                "adjectives": {
                    "values": msg
                        .adjectives
                        .as_ref()
                        .map(|adjectives| json!(adjectives.values))
                        .unwrap_or_else(|| json!({})),
                },
                "custom": msg.custom.clone().unwrap_or_default(),
            }),
        )),
        CommandPayload::UpdateRole(msg) => Some((
            "update_role",
            json!({
                "value": msg.value.clone().unwrap_or_default(),
                "additional_info": msg.additional_info.clone().unwrap_or_default(),
                "custom": msg.custom.clone().unwrap_or_default(),
            }),
        )),
        CommandPayload::UpdateContentFilterSettings(msg) => Some((
            "update_content_filter_settings",
            content_filter_settings_json(msg),
        )),
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
        CommandPayload::VoiceChannelUpdateAsrSettings(msg) => Some((
            "voice_channel_update_asr_settings",
            json!({
                "asr_settings": msg
                    .asr_settings
                    .as_ref()
                    .map(asr_settings_json)
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

fn asr_settings_json(settings: &AsrSettingsUpdateAsrSettings) -> Value {
    json!({
        "barge_in": settings.barge_in.unwrap_or(false),
        "latency_config": {
            "interaction_style": settings
                .latency_config
                .as_ref()
                .map(|config| config.interaction_style.clone())
                .unwrap_or_default(),
        },
    })
}
