use adk_types::Resource;

pub(crate) fn local_resource(path: &str, name: &str, content: &str) -> Resource {
    Resource {
        resource_id: "local".to_string(),
        name: name.to_string(),
        file_path: path.to_string(),
        payload: serde_json::json!({ "content": content }),
    }
}
