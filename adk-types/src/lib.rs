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
];

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
