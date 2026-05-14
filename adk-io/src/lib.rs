use adk_types::{DiffMap, ResourceMap};
use serde::Serialize;
use sha2::{Digest, Sha256};
use similar::TextDiff;
use std::path::Path;

mod fs;

pub use fs::{FileSystem, MemoryFileSystem, StdFileSystem};

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
    format!("--- original\n+++ updated\n{}", diff.unified_diff())
        .lines()
        .filter(|line| *line != "\\ No newline at end of file")
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end_matches('\n')
        .to_string()
}

pub fn diff_resources(before: &ResourceMap, after: &ResourceMap) -> DiffMap {
    let mut out = DiffMap::new();
    let mut all_paths = std::collections::BTreeSet::new();
    all_paths.extend(before.keys().cloned());
    all_paths.extend(after.keys().cloned());
    for path in all_paths {
        let after_content = resource_content(after.get(&path));
        let before_content = resource_content(before.get(&path));
        if before_content != after_content {
            out.insert(path, diff_text(&before_content, &after_content));
        }
    }
    out
}

fn resource_content(resource: Option<&adk_types::Resource>) -> String {
    resource
        .and_then(|r| r.payload.get("content").and_then(|v| v.as_str()))
        .unwrap_or_default()
        .to_string()
}

pub fn parse_multi_resource_path(path: &str) -> (String, Option<String>) {
    match parse_multi_resource_path_strict(path) {
        Ok((yaml_file_path, segments)) => (yaml_file_path, Some(segments.join("/"))),
        Err(_) => (path.to_string(), None),
    }
}

pub fn parse_multi_resource_path_strict(path: &str) -> Result<(String, Vec<String>), String> {
    let path = path.replace('\\', "/");
    let (absolute, mut parts) = normalize_parts(&path);
    if parts.is_empty() {
        return Err(format!(
            "Invalid multi-resource path (expected path to .yaml file): {path}"
        ));
    }
    let yaml_idx = parts
        .iter()
        .position(|part| part.ends_with(".yaml") || part.ends_with(".yml"))
        .ok_or_else(|| {
            format!("Invalid multi-resource path (expected path to .yaml file): {path}")
        })?;
    if yaml_idx >= parts.len() - 1 {
        return Err(format!(
            "Invalid multi-resource path (expected segments after .yaml file): {path}"
        ));
    }
    if parts[0].ends_with(':') {
        parts[0].push('/');
    }
    let base_parts = &parts[..=yaml_idx];
    let yaml_file_path = if absolute {
        format!("/{}", base_parts.join("/"))
    } else {
        base_parts.join("/")
    };
    let segments = parts[yaml_idx + 1..].to_vec();
    Ok((yaml_file_path, segments))
}

fn normalize_parts(path: &str) -> (bool, Vec<String>) {
    let absolute = path.starts_with('/');
    let mut parts: Vec<String> = Vec::new();
    for raw in path.split('/') {
        if raw.is_empty() || raw == "." {
            continue;
        }
        if raw == ".." {
            if let Some(last) = parts.last()
                && last != ".."
                && !last.ends_with(':')
            {
                parts.pop();
                continue;
            }
            parts.push("..".to_string());
            continue;
        }
        parts.push(raw.to_string());
    }
    (absolute, parts)
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
    use adk_types::Resource;

    #[test]
    fn parses_multi_resource_paths() {
        let (base, sub) = parse_multi_resource_path("config/entities.yaml/entities/foo");
        assert_eq!(base, "config/entities.yaml");
        assert_eq!(sub.as_deref(), Some("entities/foo"));
    }

    #[test]
    fn parses_multi_resource_paths_with_windows_separators() {
        let (base, sub) = parse_multi_resource_path("config\\entities.yaml\\entities\\foo");
        assert_eq!(base, "config/entities.yaml");
        assert_eq!(sub.as_deref(), Some("entities/foo"));
    }

    #[test]
    fn strict_parser_supports_yml_extension() {
        let (base, segments) =
            parse_multi_resource_path_strict("config/entities.yml/entities/foo").expect("valid");
        assert_eq!(base, "config/entities.yml");
        assert_eq!(segments, vec!["entities".to_string(), "foo".to_string()]);
    }

    #[test]
    fn strict_parser_rejects_paths_without_subresource_segments() {
        let err = parse_multi_resource_path_strict("config/entities.yaml").expect_err("invalid");
        assert!(err.contains("expected segments after .yaml file"));
    }

    #[test]
    fn diff_resources_includes_deletions_from_before_map() {
        let mut before = ResourceMap::new();
        before.insert(
            "topics/topic_1.yaml".to_string(),
            Resource {
                resource_id: "TOPIC-topic_1".to_string(),
                name: "topic_1".to_string(),
                file_path: "topics/topic_1.yaml".to_string(),
                payload: serde_json::json!({"content": "name: topic 1\n"}),
            },
        );
        let after = ResourceMap::new();
        let diffs = diff_resources(&before, &after);
        assert_eq!(diffs.len(), 1);
        assert!(diffs.contains_key("topics/topic_1.yaml"));
    }
}
