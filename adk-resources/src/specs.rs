//! Resource-family local file and projection facts.
//!
//! These describe how backend projection fragments map to local ADK files and
//! generated IDs for resource families that can be materialized or pushed.

use crate::projection::ProjectionCollection;
use serde_json::Value;

/// Fixed local file that represents one resource family or singleton resource.
#[derive(Clone, Copy)]
pub struct FileResourceSpec {
    pub file_path: &'static str,
    pub resource_id: &'static str,
    pub name: &'static str,
}

/// Aggregate YAML file plus the backend projection collection it represents.
#[derive(Clone, Copy)]
pub struct CollectionResourceSpec {
    pub file: FileResourceSpec,
    pub yaml_key: &'static str,
    projection: ProjectionCollection,
    pub id_prefix: &'static str,
}

impl CollectionResourceSpec {
    /// Returns ordered projection entries for this aggregate resource family.
    pub fn entries<'a>(&self, root: &'a Value) -> Vec<(String, &'a Value)> {
        self.projection.entries(root)
    }

    /// Returns cloned ordered projection entries for this aggregate resource family.
    pub fn owned_entries(&self, root: &Value) -> Vec<(String, Value)> {
        self.projection.owned_entries(root)
    }
}

pub const VARIANT_ATTRIBUTES_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "config/variant_attributes.yaml",
    resource_id: "variant_attributes",
    name: "variant_attributes",
};

pub const API_INTEGRATIONS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "config/api_integrations.yaml",
    resource_id: "api_integrations",
    name: "api_integrations",
};

pub const ENTITIES_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "config/entities.yaml",
    resource_id: "entities",
    name: "entities",
};

pub const ENTITY_ID_PREFIX: &str = "ENTITIES";

pub const KEYPHRASE_BOOSTING_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "voice/speech_recognition/keyphrase_boosting.yaml",
    resource_id: "keyphrase_boosting",
    name: "keyphrase_boosting",
};

pub const LEGACY_KEYPHRASE_BOOSTING_FILE_PATH: &str = "speech_recognition/keyphrase_boosting.yaml";

pub const TRANSCRIPT_CORRECTIONS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "voice/speech_recognition/transcript_corrections.yaml",
    resource_id: "transcript_corrections",
    name: "transcript_corrections",
};

pub const PRONUNCIATIONS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "voice/response_control/pronunciations.yaml",
    resource_id: "pronunciations",
    name: "pronunciations",
};

pub const AGENT_PERSONALITY_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "agent_settings/personality.yaml",
    resource_id: "personality",
    name: "personality",
};

pub const AGENT_ROLE_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "agent_settings/role.yaml",
    resource_id: "role",
    name: "role",
};

pub const AGENT_SAFETY_FILTERS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "agent_settings/safety_filters.yaml",
    resource_id: "safety_filters",
    name: "safety_filters",
};

pub const AGENT_RULES_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "agent_settings/rules.txt",
    resource_id: "rules",
    name: "rules",
};

pub const EXPERIMENTAL_CONFIG_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "agent_settings/experimental_config.json",
    resource_id: "experimental_config",
    name: "experimental_config",
};

pub const VOICE_SAFETY_FILTERS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "voice/safety_filters.yaml",
    resource_id: "voice_safety_filters",
    name: "voice_safety_filters",
};

pub const VOICE_CONFIGURATION_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "voice/configuration.yaml",
    resource_id: "voice_configuration",
    name: "voice_configuration",
};

pub const CHAT_CONFIGURATION_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "chat/configuration.yaml",
    resource_id: "chat_configuration",
    name: "chat_configuration",
};

pub const CHAT_SAFETY_FILTERS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "chat/safety_filters.yaml",
    resource_id: "chat_safety_filters",
    name: "chat_safety_filters",
};

pub const ASR_SETTINGS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "voice/speech_recognition/asr_settings.yaml",
    resource_id: "asr_settings",
    name: "asr_settings",
};

pub const VARIANTS: CollectionResourceSpec = CollectionResourceSpec {
    file: VARIANT_ATTRIBUTES_FILE,
    yaml_key: "variants",
    projection: ProjectionCollection::new(&["variantManagement", "variants"]),
    id_prefix: "VARIANTS",
};

pub const VARIANT_ATTRIBUTES: CollectionResourceSpec = CollectionResourceSpec {
    file: VARIANT_ATTRIBUTES_FILE,
    yaml_key: "attributes",
    projection: ProjectionCollection::new(&["variantManagement", "attributes"]),
    id_prefix: "VARIANT_ATTRIBUTES",
};

pub const VARIANT_ATTRIBUTE_VALUES: ProjectionCollection =
    ProjectionCollection::new(&["variantManagement", "variantAttributeValues"]);

pub const API_INTEGRATIONS: CollectionResourceSpec = CollectionResourceSpec {
    file: API_INTEGRATIONS_FILE,
    yaml_key: "api_integrations",
    projection: ProjectionCollection::new(&["apiIntegrations", "apiIntegrations"]),
    id_prefix: "API-INTEGRATION",
};

pub const KEYPHRASE_BOOSTING: CollectionResourceSpec = CollectionResourceSpec {
    file: KEYPHRASE_BOOSTING_FILE,
    yaml_key: "keyphrases",
    projection: ProjectionCollection::new(&["keyphraseBoosting", "keyphraseBoosting"]),
    id_prefix: "KEYPHRASE_BOOSTING",
};

pub const TRANSCRIPT_CORRECTIONS: CollectionResourceSpec = CollectionResourceSpec {
    file: TRANSCRIPT_CORRECTIONS_FILE,
    yaml_key: "corrections",
    projection: ProjectionCollection::new(&["transcriptCorrections", "transcriptCorrections"]),
    id_prefix: "TRANSCRIPT_CORRECTIONS",
};

pub const PRONUNCIATIONS: CollectionResourceSpec = CollectionResourceSpec {
    file: PRONUNCIATIONS_FILE,
    yaml_key: "pronunciations",
    projection: ProjectionCollection::new(&["pronunciations", "pronunciations"]),
    id_prefix: "PRONUNCIATIONS",
};
