use adk_core::ProjectWorkspace;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfiStatusResponse {
    pub success: bool,
    pub conflict_detection_available: bool,
    pub modified_files: Vec<String>,
    pub new_files: Vec<String>,
    pub deleted_files: Vec<String>,
    pub files_with_conflicts: Vec<String>,
    pub error: Option<String>,
}

pub fn status_json(project_path: &str) -> String {
    let workspace = ProjectWorkspace::new();
    let payload = match workspace.status(project_path.as_ref()) {
        Ok(summary) => FfiStatusResponse {
            success: true,
            conflict_detection_available: summary.conflict_detection_available,
            modified_files: summary.modified_files,
            new_files: summary.new_files,
            deleted_files: summary.deleted_files,
            files_with_conflicts: summary.files_with_conflicts,
            error: None,
        },
        Err(err) => FfiStatusResponse {
            success: false,
            conflict_detection_available: false,
            modified_files: vec![],
            new_files: vec![],
            deleted_files: vec![],
            files_with_conflicts: vec![],
            error: Some(err.to_string()),
        },
    };
    serde_json::to_string(&payload).unwrap_or_else(|_| "{\"success\":false}".to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_methods)]

    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_project_dir() -> String {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("adk-rs-ffi-{ts}"));
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(
            dir.join("project.yaml"),
            "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
        )
        .expect("write config");
        dir.to_string_lossy().to_string()
    }

    #[test]
    fn ffi_status_json_returns_success_payload_shape() {
        let project_dir = make_temp_project_dir();
        let raw = status_json(&project_dir);
        let payload: serde_json::Value = serde_json::from_str(&raw).expect("json payload");
        assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            payload
                .get("conflict_detection_available")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert!(payload.get("modified_files").is_some());
        assert!(payload.get("new_files").is_some());
        assert!(payload.get("deleted_files").is_some());
        assert!(payload.get("files_with_conflicts").is_some());
    }

    #[test]
    fn ffi_status_json_reports_missing_project() {
        let raw = status_json("/tmp/definitely-not-an-adk-project");
        let payload: serde_json::Value = serde_json::from_str(&raw).expect("json payload");
        assert_eq!(
            payload.get("success").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            payload
                .get("conflict_detection_available")
                .and_then(|v| v.as_bool()),
            Some(false)
        );
        assert!(
            payload
                .get("error")
                .and_then(|v| v.as_str())
                .is_some_and(|message| message.contains("project configuration not found"))
        );
        for key in [
            "modified_files",
            "new_files",
            "deleted_files",
            "files_with_conflicts",
        ] {
            assert!(
                payload
                    .get(key)
                    .and_then(|v| v.as_array())
                    .is_some_and(Vec::is_empty),
                "{key} should be an empty array"
            );
        }
    }
}
