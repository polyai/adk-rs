use super::transcript_corrections::{regular_expression_json, transcript_correction_json};
use super::variants::{attribute_references_json, attribute_values_json};
use crate::api_integrations::environments_json;
use adk_protobuf::command::Payload as CommandPayload;
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
        _ => None,
    }
}
