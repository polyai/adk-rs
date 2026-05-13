use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub schema_version: u8,
    #[serde(default)]
    pub source: Option<ManifestSource>,
    #[serde(default)]
    pub replay_notes: Vec<String>,
    pub httpmock_recording: String,
    pub workflows: Vec<Workflow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestSource {
    pub implementation: String,
    pub recorder: String,
    pub server: String,
    pub poly_binary: String,
    pub region: String,
    pub account_id: String,
    pub project_id: String,
    pub project_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub mutates_real_server: bool,
    #[serde(default)]
    pub cleanup: Vec<String>,
    pub steps: Vec<WorkflowStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WorkflowStep {
    Tagged(TaggedWorkflowStep),
    LegacyCommand(CommandRecord),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaggedWorkflowStep {
    Command(CommandRecord),
    FileEdit(FileEditRecord),
    FileAssertion(FileAssertionRecord),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRecord {
    pub name: String,
    pub argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
    #[serde(default)]
    pub exit_code: i32,
    #[serde(default)]
    pub stdout_json: Option<Value>,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEditRecord {
    pub name: String,
    pub operation: String,
    pub path: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub replacement: Option<String>,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAssertionRecord {
    pub name: String,
    pub path: String,
    pub exists: bool,
    #[serde(default)]
    pub contains: Vec<String>,
}
