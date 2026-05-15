use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Typed envelope for Python's base64-encoded `_gen/.agent_studio_config`.
///
/// Resource payloads intentionally stay open-ended so Rust can round-trip
/// Python-authored status files without needing to know every resource field.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct StatusSnapshot {
    #[serde(default, deserialize_with = "default_if_null")]
    pub region: String,
    #[serde(default, deserialize_with = "default_if_null")]
    pub account_id: String,
    #[serde(default, deserialize_with = "default_if_null")]
    pub project_id: String,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default, deserialize_with = "default_if_null")]
    pub resources: IndexMap<String, IndexMap<String, StatusResourcePayload>>,
    #[serde(default)]
    pub last_updated: Option<String>,
    #[serde(default, deserialize_with = "default_if_null")]
    pub file_structure_info: IndexMap<String, FileStructureEntry>,
    #[serde(
        default = "default_branch",
        deserialize_with = "default_branch_if_null"
    )]
    pub branch_id: String,
    #[serde(default, deserialize_with = "default_if_null")]
    pub migration_flags: Vec<String>,
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

fn default_branch() -> String {
    "main".to_string()
}

fn default_if_null<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

fn default_branch_if_null<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_else(default_branch))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct StatusResourcePayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(default, flatten)]
    pub fields: Map<String, Value>,
}

impl StatusResourcePayload {
    pub fn from_value(value: Value) -> Self {
        serde_json::from_value(value).unwrap_or_default()
    }

    pub fn as_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn resource_id(&self) -> Option<&str> {
        self.resource_id.as_deref()
    }

    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct FileStructureEntry {
    #[serde(default, rename = "type")]
    pub resource_type: String,
    #[serde(default)]
    pub resource_id: String,
    #[serde(default)]
    pub resource_name: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_snapshot_preserves_unknown_top_level_and_payload_fields() {
        let raw = serde_json::json!({
            "region": "eu-west-1",
            "account_id": "acct",
            "project_id": "proj",
            "resources": {
                "functions": {
                    "fn-1": {
                        "name": "Lookup",
                        "resource_id": "fn-1",
                        "file_path": "functions/lookup.py",
                        "code": "def lookup(conv):\n    return None\n",
                        "python_only": {"still": true}
                    }
                }
            },
            "file_structure_info": {
                "functions/lookup.py": {
                    "type": "functions",
                    "resource_id": "fn-1",
                    "resource_name": "Lookup",
                    "hash": "abc",
                    "python_only": 7
                }
            },
            "future_python_field": "kept"
        });

        let snapshot: StatusSnapshot = serde_json::from_value(raw).expect("typed status snapshot");
        let encoded = serde_json::to_value(&snapshot).expect("serialize status snapshot");

        assert_eq!(encoded["future_python_field"], "kept");
        assert_eq!(
            encoded["resources"]["functions"]["fn-1"]["python_only"]["still"],
            true
        );
        assert_eq!(
            encoded["file_structure_info"]["functions/lookup.py"]["python_only"],
            7
        );
    }
}
