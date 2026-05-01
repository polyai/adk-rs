use adk_domain::{DiffMap, ResourceMap};
use serde::Serialize;
use sha2::{Digest, Sha256};
use similar::TextDiff;
use std::path::Path;

pub fn canonical_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value)
}

pub fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn diff_text(before: &str, after: &str) -> String {
    let diff = TextDiff::from_lines(before, after);
    diff.unified_diff().to_string()
}

pub fn diff_resources(before: &ResourceMap, after: &ResourceMap) -> DiffMap {
    let mut out = DiffMap::new();
    for (path, resource) in after {
        let new_payload =
            serde_json::to_string_pretty(&resource.payload).unwrap_or_else(|_| "{}".to_string());
        let old_payload = before
            .get(path)
            .and_then(|r| serde_json::to_string_pretty(&r.payload).ok())
            .unwrap_or_default();
        if old_payload != new_payload {
            out.insert(path.clone(), diff_text(&old_payload, &new_payload));
        }
    }
    out
}

pub fn parse_multi_resource_path(path: &str) -> (String, Option<String>) {
    // Python uses logical subpaths after `.yaml/` for multi-resource documents.
    let marker = ".yaml/";
    if let Some(idx) = path.find(marker) {
        let yaml_path = path[..idx + 5].to_string();
        let sub = path[idx + marker.len()..].to_string();
        return (yaml_path, Some(sub));
    }
    (path.to_string(), None)
}

pub fn normalize_rel_path(root: &Path, target: &Path) -> String {
    target
        .strip_prefix(root)
        .unwrap_or(target)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_resource_paths() {
        let (base, sub) = parse_multi_resource_path("config/entities.yaml/entities/foo");
        assert_eq!(base, "config/entities.yaml");
        assert_eq!(sub.as_deref(), Some("entities/foo"));
    }
}
