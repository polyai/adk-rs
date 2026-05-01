use adk_core::AdkService;
use adk_platform_api::InMemoryPlatformClient;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfiStatusResponse {
    pub success: bool,
    pub modified_files: Vec<String>,
    pub new_files: Vec<String>,
    pub deleted_files: Vec<String>,
    pub files_with_conflicts: Vec<String>,
    pub error: Option<String>,
}

pub fn status_json(project_path: &str) -> String {
    let client = InMemoryPlatformClient::default();
    let service = AdkService::new(Box::new(client));
    let payload = match service.status(project_path.as_ref()) {
        Ok(summary) => FfiStatusResponse {
            success: true,
            modified_files: summary.modified_files,
            new_files: summary.new_files,
            deleted_files: summary.deleted_files,
            files_with_conflicts: summary.files_with_conflicts,
            error: None,
        },
        Err(err) => FfiStatusResponse {
            success: false,
            modified_files: vec![],
            new_files: vec![],
            deleted_files: vec![],
            files_with_conflicts: vec![],
            error: Some(err.to_string()),
        },
    };
    serde_json::to_string(&payload).unwrap_or_else(|_| "{\"success\":false}".to_string())
}
