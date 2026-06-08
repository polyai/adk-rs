use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type ResourceMap = IndexMap<String, Resource>;
pub type DiffMap = IndexMap<String, String>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectConfig {
    pub region: String,
    pub account_id: String,
    pub project_id: String,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default = "default_branch")]
    pub branch_id: String,
}

fn default_branch() -> String {
    "main".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectStatus {
    #[serde(default)]
    pub resources: IndexMap<String, IndexMap<String, serde_json::Value>>,
    pub last_updated: Option<DateTime<Utc>>,
    #[serde(default = "default_branch")]
    pub branch_id: String,
    #[serde(default)]
    pub project_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resource {
    pub resource_id: String,
    pub name: String,
    pub file_path: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StatusSummary {
    #[serde(default)]
    pub conflict_detection_available: bool,
    pub files_with_conflicts: Vec<String>,
    pub modified_files: Vec<String>,
    pub new_files: Vec<String>,
    pub deleted_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PushResult {
    pub success: bool,
    pub message: String,
    #[serde(default)]
    pub commands: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeploymentList {
    #[serde(default)]
    pub versions: Vec<serde_json::Value>,
    #[serde(default)]
    pub active_deployment_hashes: IndexMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchDescriptor {
    pub name: String,
    pub branch_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BranchMergeResult {
    pub success: bool,
    #[serde(default)]
    pub conflicts: Vec<serde_json::Value>,
    #[serde(default)]
    pub errors: Vec<serde_json::Value>,
    #[serde(default)]
    pub sequence: Option<String>,
}

/// Page returned by the Data API `GET /v1/agents/{agentId}/conversations` endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationListResponse {
    pub conversations: Vec<ConversationSummary>,
    pub count: usize,
    pub limit: usize,
    pub offset: usize,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Public-facing conversation summary from the Data API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub conversation_id: String,
    pub account_id: String,
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_duration: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polyai_duration: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_progress: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_destination: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poly_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_summary: Option<ConversationShortSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_url: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Conversation detail from the Data API, including transcript turns when available.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDetail {
    #[serde(flatten)]
    pub summary: ConversationSummary,
    #[serde(default)]
    pub turns: Vec<ConversationTurn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_events: Option<serde_json::Value>,
}

/// Transcript turn object embedded in a conversation detail response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationTurn {
    #[serde(
        default,
        alias = "userInput",
        alias = "input",
        skip_serializing_if = "Option::is_none"
    )]
    pub user_input: Option<String>,
    #[serde(
        default,
        alias = "agentResponse",
        alias = "response",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent_response: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Summary text returned by the Data API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConversationShortSummary {
    Text(String),
    Object {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        heading: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceTypeDescriptor {
    pub type_name: &'static str,
    pub status_resource_name: &'static str,
    pub id_prefix: Option<&'static str>,
}

pub const RESOURCE_TYPE_REGISTRY: &[ResourceTypeDescriptor] = &[
    ResourceTypeDescriptor {
        type_name: "ApiIntegration",
        status_resource_name: "api_integration",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "Function",
        status_resource_name: "functions",
        id_prefix: Some("fn"),
    },
    ResourceTypeDescriptor {
        type_name: "Topic",
        status_resource_name: "topics",
        id_prefix: Some("topic"),
    },
    ResourceTypeDescriptor {
        type_name: "SettingsPersonality",
        status_resource_name: "personality",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "SettingsRole",
        status_resource_name: "role",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "SettingsRules",
        status_resource_name: "rules",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "FlowStep",
        status_resource_name: "flow_steps",
        id_prefix: Some("step"),
    },
    ResourceTypeDescriptor {
        type_name: "FunctionStep",
        status_resource_name: "function_steps",
        id_prefix: Some("step"),
    },
    ResourceTypeDescriptor {
        type_name: "FlowConfig",
        status_resource_name: "flow_config",
        id_prefix: Some("flow"),
    },
    ResourceTypeDescriptor {
        type_name: "Entity",
        status_resource_name: "entities",
        id_prefix: Some("entity"),
    },
    ResourceTypeDescriptor {
        type_name: "ExperimentalConfig",
        status_resource_name: "experimental_config",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "GeneralSafetyFilters",
        status_resource_name: "safety_filters",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "SMSTemplate",
        status_resource_name: "sms_templates",
        id_prefix: Some("sms"),
    },
    ResourceTypeDescriptor {
        type_name: "Handoff",
        status_resource_name: "handoffs",
        id_prefix: Some("ho"),
    },
    ResourceTypeDescriptor {
        type_name: "Variant",
        status_resource_name: "variants",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "VariantAttribute",
        status_resource_name: "variant_attributes",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "Variable",
        status_resource_name: "variables",
        id_prefix: Some("var"),
    },
    ResourceTypeDescriptor {
        type_name: "VoiceGreeting",
        status_resource_name: "voice_greeting",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "VoiceSafetyFilters",
        status_resource_name: "voice_safety_filters",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "VoiceStylePrompt",
        status_resource_name: "voice_style_prompt",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "VoiceDisclaimerMessage",
        status_resource_name: "voice_disclaimer",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "ChatGreeting",
        status_resource_name: "chat_greeting",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "ChatSafetyFilters",
        status_resource_name: "chat_safety_filters",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "ChatStylePrompt",
        status_resource_name: "chat_style_prompt",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "KeyphraseBoosting",
        status_resource_name: "keyphrase_boosting",
        id_prefix: Some("kp"),
    },
    ResourceTypeDescriptor {
        type_name: "TranscriptCorrection",
        status_resource_name: "transcript_corrections",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "AsrSettings",
        status_resource_name: "asr_settings",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "PhraseFilter",
        status_resource_name: "phrase_filtering",
        id_prefix: Some("sk"),
    },
    ResourceTypeDescriptor {
        type_name: "Pronunciation",
        status_resource_name: "pronunciations",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "Translation",
        status_resource_name: "translations",
        id_prefix: Some("tn"),
    },
    ResourceTypeDescriptor {
        type_name: "DefaultLanguage",
        status_resource_name: "default_language",
        id_prefix: None,
    },
    ResourceTypeDescriptor {
        type_name: "AdditionalLanguage",
        status_resource_name: "additional_languages",
        id_prefix: None,
    },
];

const fn ordered_type_names_from_registry() -> [&'static str; RESOURCE_TYPE_REGISTRY.len()] {
    let mut out = [""; RESOURCE_TYPE_REGISTRY.len()];
    let mut index = 0;
    while index < RESOURCE_TYPE_REGISTRY.len() {
        out[index] = RESOURCE_TYPE_REGISTRY[index].type_name;
        index += 1;
    }
    out
}

pub const ORDERED_TYPE_NAMES: [&str; RESOURCE_TYPE_REGISTRY.len()] =
    ordered_type_names_from_registry();

pub fn descriptor_by_type_name(name: &str) -> Option<&'static ResourceTypeDescriptor> {
    RESOURCE_TYPE_REGISTRY.iter().find(|d| d.type_name == name)
}

pub fn descriptor_by_status_name(name: &str) -> Option<&'static ResourceTypeDescriptor> {
    RESOURCE_TYPE_REGISTRY
        .iter()
        .find(|d| d.status_resource_name == name)
}

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("project configuration not found at {0}")]
    ConfigNotFound(String),
    #[error("invalid project data: {0}")]
    InvalidData(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_type_names_match_registry_order() {
        let expected_type_names = RESOURCE_TYPE_REGISTRY
            .iter()
            .map(|descriptor| descriptor.type_name)
            .collect::<Vec<_>>();
        assert_eq!(
            ORDERED_TYPE_NAMES.as_slice(),
            expected_type_names.as_slice()
        );
    }

    #[test]
    fn descriptor_lookups_cover_the_registry() {
        for descriptor in RESOURCE_TYPE_REGISTRY {
            assert_eq!(
                descriptor_by_type_name(descriptor.type_name),
                Some(descriptor)
            );
            assert_eq!(
                descriptor_by_status_name(descriptor.status_resource_name),
                Some(descriptor)
            );
        }
        assert_eq!(descriptor_by_type_name("DoesNotExist"), None);
        assert_eq!(descriptor_by_status_name("does_not_exist"), None);
    }

    #[test]
    fn resource_type_registry_names_are_unique() {
        let mut type_names = std::collections::BTreeSet::new();
        let mut status_names = std::collections::BTreeSet::new();
        for descriptor in RESOURCE_TYPE_REGISTRY {
            assert!(
                type_names.insert(descriptor.type_name),
                "duplicate resource type name: {}",
                descriptor.type_name
            );
            assert!(
                status_names.insert(descriptor.status_resource_name),
                "duplicate resource status name: {}",
                descriptor.status_resource_name
            );
        }
    }

    #[test]
    fn conversation_list_uses_data_api_camel_case_identity_shape() {
        let raw = serde_json::json!({
            "conversations": [{
                "conversationId": "KA-123",
                "accountId": "acct",
                "projectId": "proj",
                "duration": 90,
                "shortSummary": "{\"heading\":\"Test call\"}",
                "customField": "kept"
            }],
            "count": 1,
            "limit": 50,
            "offset": 0
        });

        let response: ConversationListResponse =
            serde_json::from_value(raw).expect("deserialize Data API conversations response");

        let conversation = &response.conversations[0];
        assert_eq!(conversation.conversation_id, "KA-123");
        assert_eq!(conversation.account_id, "acct");
        assert_eq!(conversation.project_id, "proj");
        assert_eq!(conversation.duration, Some(90));
        assert_eq!(
            conversation.short_summary,
            Some(ConversationShortSummary::Text(
                "{\"heading\":\"Test call\"}".to_string()
            ))
        );
        assert_eq!(
            conversation.extra.get("customField"),
            Some(&serde_json::json!("kept"))
        );

        let serialized =
            serde_json::to_value(response).expect("serialize Data API conversations response");
        assert_eq!(
            serialized["conversations"][0]["conversationId"],
            serde_json::json!("KA-123")
        );
        assert!(serialized["conversations"][0].get("createdAt").is_none());
    }

    #[test]
    fn conversation_list_rejects_legacy_conversations_api_identity_shape() {
        let raw = serde_json::json!({
            "conversations": [{
                "id": "CA-123",
                "account_id": "acct",
                "project_id": "proj"
            }],
            "count": 1,
            "limit": 50,
            "offset": 0
        });

        let error = serde_json::from_value::<ConversationListResponse>(raw)
            .expect_err("legacy Conversations API payload should not match Data API DTOs");
        assert!(error.to_string().contains("conversationId"));
    }

    #[test]
    fn project_config_branch_defaults_to_main_on_deserialize() {
        let raw = serde_json::json!({
            "region": "eu-west-1",
            "account_id": "acc",
            "project_id": "proj"
        });
        let cfg: ProjectConfig = serde_json::from_value(raw).expect("deserialize project config");
        assert_eq!(cfg.branch_id, "main");
        assert_eq!(cfg.project_name, None);
    }

    #[test]
    fn project_status_branch_defaults_to_main_on_deserialize() {
        let raw = serde_json::json!({
            "resources": {},
            "last_updated": null
        });
        let status: ProjectStatus =
            serde_json::from_value(raw).expect("deserialize project status");
        assert_eq!(status.branch_id, "main");
        assert!(status.resources.is_empty());
        assert_eq!(status.project_name, None);
    }

    #[test]
    fn status_summary_default_is_empty_lists() {
        let summary = StatusSummary::default();
        assert!(!summary.conflict_detection_available);
        assert!(summary.files_with_conflicts.is_empty());
        assert!(summary.modified_files.is_empty());
        assert!(summary.new_files.is_empty());
        assert!(summary.deleted_files.is_empty());
    }
}
