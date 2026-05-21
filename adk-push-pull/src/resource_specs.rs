//! Local file and projection facts shared by pull materialization and push replay.
//!
//! These are adk-push-pull details rather than global resource type metadata:
//! they describe how backend projection fragments map to local ADK files and
//! replay IDs for the resources this crate can materialize or push.

use crate::projection::ProjectionCollection;
use serde_json::Value;

#[derive(Clone, Copy)]
pub(crate) struct FileResourceSpec {
    pub(crate) file_path: &'static str,
    pub(crate) resource_id: &'static str,
    pub(crate) name: &'static str,
}

#[derive(Clone, Copy)]
pub(crate) struct CollectionResourceSpec {
    pub(crate) file: FileResourceSpec,
    pub(crate) yaml_key: &'static str,
    projection: ProjectionCollection,
    pub(crate) replay_kind: &'static str,
    pub(crate) id_prefix: &'static str,
}

impl CollectionResourceSpec {
    pub(crate) fn entries<'a>(&self, root: &'a Value) -> Vec<(String, &'a Value)> {
        self.projection.entries(root)
    }

    pub(crate) fn owned_entries(&self, root: &Value) -> Vec<(String, Value)> {
        self.projection.owned_entries(root)
    }
}

pub(crate) const VARIANT_ATTRIBUTES_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "config/variant_attributes.yaml",
    resource_id: "variant_attributes",
    name: "variant_attributes",
};

pub(crate) const API_INTEGRATIONS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "config/api_integrations.yaml",
    resource_id: "api_integrations",
    name: "api_integrations",
};

pub(crate) const ENTITIES_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "config/entities.yaml",
    resource_id: "entities",
    name: "entities",
};

pub(crate) const ENTITY_REPLAY_KIND: &str = "entity";
pub(crate) const ENTITY_ID_PREFIX: &str = "ENTITIES";

pub(crate) const KEYPHRASE_BOOSTING_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "voice/speech_recognition/keyphrase_boosting.yaml",
    resource_id: "keyphrase_boosting",
    name: "keyphrase_boosting",
};

pub(crate) const LEGACY_KEYPHRASE_BOOSTING_FILE_PATH: &str =
    "speech_recognition/keyphrase_boosting.yaml";

pub(crate) const TRANSCRIPT_CORRECTIONS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "voice/speech_recognition/transcript_corrections.yaml",
    resource_id: "transcript_corrections",
    name: "transcript_corrections",
};

pub(crate) const PRONUNCIATIONS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "voice/response_control/pronunciations.yaml",
    resource_id: "pronunciations",
    name: "pronunciations",
};

pub(crate) const AGENT_PERSONALITY_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "agent_settings/personality.yaml",
    resource_id: "personality",
    name: "personality",
};

pub(crate) const AGENT_ROLE_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "agent_settings/role.yaml",
    resource_id: "role",
    name: "role",
};

pub(crate) const AGENT_SAFETY_FILTERS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "agent_settings/safety_filters.yaml",
    resource_id: "safety_filters",
    name: "safety_filters",
};

pub(crate) const AGENT_RULES_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "agent_settings/rules.txt",
    resource_id: "rules",
    name: "rules",
};

pub(crate) const VOICE_SAFETY_FILTERS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "voice/safety_filters.yaml",
    resource_id: "voice_safety_filters",
    name: "voice_safety_filters",
};

pub(crate) const VOICE_CONFIGURATION_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "voice/configuration.yaml",
    resource_id: "voice_configuration",
    name: "voice_configuration",
};

pub(crate) const CHAT_CONFIGURATION_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "chat/configuration.yaml",
    resource_id: "chat_configuration",
    name: "chat_configuration",
};

pub(crate) const CHAT_SAFETY_FILTERS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "chat/safety_filters.yaml",
    resource_id: "chat_safety_filters",
    name: "chat_safety_filters",
};

pub(crate) const ASR_SETTINGS_FILE: FileResourceSpec = FileResourceSpec {
    file_path: "voice/speech_recognition/asr_settings.yaml",
    resource_id: "asr_settings",
    name: "asr_settings",
};

pub(crate) const VARIANTS: CollectionResourceSpec = CollectionResourceSpec {
    file: VARIANT_ATTRIBUTES_FILE,
    yaml_key: "variants",
    projection: ProjectionCollection::new(&["variantManagement", "variants"]),
    replay_kind: "variant",
    id_prefix: "VARIANTS",
};

pub(crate) const VARIANT_ATTRIBUTES: CollectionResourceSpec = CollectionResourceSpec {
    file: VARIANT_ATTRIBUTES_FILE,
    yaml_key: "attributes",
    projection: ProjectionCollection::new(&["variantManagement", "attributes"]),
    replay_kind: "variant_attribute",
    id_prefix: "VARIANT_ATTRIBUTES",
};

pub(crate) const VARIANT_ATTRIBUTE_VALUES: ProjectionCollection =
    ProjectionCollection::new(&["variantManagement", "variantAttributeValues"]);

pub(crate) const API_INTEGRATIONS: CollectionResourceSpec = CollectionResourceSpec {
    file: API_INTEGRATIONS_FILE,
    yaml_key: "api_integrations",
    projection: ProjectionCollection::new(&["apiIntegrations", "apiIntegrations"]),
    replay_kind: "api_integration",
    id_prefix: "API-INTEGRATION",
};

pub(crate) const API_INTEGRATION_OPERATION_REPLAY_KIND: &str = "api_integration_operation";

pub(crate) const KEYPHRASE_BOOSTING: CollectionResourceSpec = CollectionResourceSpec {
    file: KEYPHRASE_BOOSTING_FILE,
    yaml_key: "keyphrases",
    projection: ProjectionCollection::new(&["keyphraseBoosting", "keyphraseBoosting"]),
    replay_kind: "keyphrase_boosting",
    id_prefix: "KEYPHRASE_BOOSTING",
};

pub(crate) const TRANSCRIPT_CORRECTIONS: CollectionResourceSpec = CollectionResourceSpec {
    file: TRANSCRIPT_CORRECTIONS_FILE,
    yaml_key: "corrections",
    projection: ProjectionCollection::new(&["transcriptCorrections", "transcriptCorrections"]),
    replay_kind: "transcript_corrections",
    id_prefix: "TRANSCRIPT_CORRECTIONS",
};

pub(crate) const PRONUNCIATIONS: CollectionResourceSpec = CollectionResourceSpec {
    file: PRONUNCIATIONS_FILE,
    yaml_key: "pronunciations",
    projection: ProjectionCollection::new(&["pronunciations", "pronunciations"]),
    replay_kind: "pronunciations",
    id_prefix: "PRONUNCIATIONS",
};
