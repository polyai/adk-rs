use adk_types::{DiffMap, DomainError, ProjectConfig, ResourceMap};
use serde_json::{self, Value as JsonValue};
use serde_yaml_ng::{Mapping, Value as YamlValue, from_str, to_string};

mod pull;
mod push;
pub mod validation;
mod workspace;

use adk_io::{FileSystem, parse_multi_resource_path};
use adk_resources::current_status_hash_for_expected;
pub use adk_resources::{
    DiscoveredResourceChanges, DiscoveredResourcePaths, TypedResourceLifecycle,
};
use anyhow::Result;
use globset::{Glob, GlobSetBuilder};
pub use pull::{
    FileChange, PullInput, PullOutput, PullResourceMapInput, pull_from_filesystem,
    pull_resource_map_from_filesystem,
};
pub use push::{
    ChangedResourceMap, PushCommandPlan, PushInput, PushOutput, PushPlanInput,
    add_discovered_variable_resources_from_fs, plan_push_commands_from_changed_resources,
    plan_push_commands_from_resources, push_from_filesystem,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use thiserror::Error;
pub use workspace::ProjectWorkspace;

pub const PROJECT_CONFIG_FILE: &str = "project.yaml";
pub const STATUS_FILE: &str = "_gen/.agent_studio_config";
const MIGRATED_LEGACY_TOPIC_FILES: &str = "migrated_legacy_topic_files";
const MIGRATED_LEGACY_KEYPHRASE_BOOSTING_FILE: &str = "migrated_legacy_keyphrase_boosting_file";

pub(crate) fn is_status_metadata_path(path: &str) -> bool {
    path == STATUS_FILE
}

pub(crate) fn is_generated_path(path: &str) -> bool {
    path.starts_with("_gen/")
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("{0}")]
    Domain(#[from] DomainError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    CommandGeneration(#[from] adk_resources::CommandGenError),
}

pub fn project_config_yaml(cfg: &ProjectConfig) -> Result<String, CoreError> {
    let mut map = Mapping::new();
    map.insert(
        YamlValue::String("project_id".to_string()),
        YamlValue::String(cfg.project_id.clone()),
    );
    map.insert(
        YamlValue::String("account_id".to_string()),
        YamlValue::String(cfg.account_id.clone()),
    );
    map.insert(
        YamlValue::String("region".to_string()),
        YamlValue::String(cfg.region.clone()),
    );
    if let Some(project_name) = &cfg.project_name
        && !project_name.is_empty()
    {
        map.insert(
            YamlValue::String("project_name".to_string()),
            YamlValue::String(project_name.clone()),
        );
    }
    to_string(&YamlValue::Mapping(map)).map_err(|e| DomainError::InvalidData(e.to_string()).into())
}

pub fn project_config_contains_branch_id(raw: &str) -> bool {
    from_str::<YamlValue>(raw)
        .ok()
        .and_then(|value| match value {
            YamlValue::Mapping(mapping) => Some(mapping),
            _ => None,
        })
        .is_some_and(|mapping| mapping.contains_key("branch_id"))
}

pub fn migration_flags_from_status(
    status: &serde_json::Map<String, JsonValue>,
) -> BTreeSet<String> {
    status
        .get("migration_flags")
        .and_then(JsonValue::as_array)
        .map(|flags| {
            flags
                .iter()
                .filter_map(JsonValue::as_str)
                .filter(|flag| {
                    matches!(
                        *flag,
                        MIGRATED_LEGACY_TOPIC_FILES | MIGRATED_LEGACY_KEYPHRASE_BOOSTING_FILE
                    )
                })
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn migrate_legacy_topic_files<Fs: FileSystem>(
    fs: &Fs,
    project_root: &Path,
) -> Result<bool, CoreError> {
    let topics_dir = project_root.join("topics");
    if !fs.is_dir(&topics_dir) {
        return Ok(false);
    }

    let mut migrated_topics: std::collections::BTreeMap<PathBuf, YamlValue> =
        std::collections::BTreeMap::new();
    let mut old_files = Vec::new();
    let mut old_dirs = BTreeSet::new();

    for topic_path in recursive_file_paths(fs, &topics_dir)? {
        if !is_yaml_file(&topic_path) {
            continue;
        }
        let raw = fs.read_to_string(&topic_path)?;
        let Ok(parsed) = from_str::<YamlValue>(&raw) else {
            continue;
        };
        let YamlValue::Mapping(existing) = parsed else {
            continue;
        };
        if yaml_mapping_contains_key(&existing, "name") {
            continue;
        }

        let rel_path = topic_path.strip_prefix(&topics_dir).unwrap_or(&topic_path);
        let topic_name = rel_path
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");
        let clean_file_name = adk_resources::clean_name(&topic_name, true);
        let clean_file_path = topics_dir.join(format!("{clean_file_name}.yaml"));
        if migrated_topics.contains_key(&clean_file_path) {
            return Err(DomainError::InvalidData(format!(
                "Can't migrate legacy topic files: multiple topics with the same file name after cleaning: {clean_file_name}"
            ))
            .into());
        }

        let mut updated = Mapping::new();
        updated.insert(
            YamlValue::String("name".to_string()),
            YamlValue::String(topic_name),
        );
        for (key, value) in existing {
            updated.insert(key, value);
        }
        migrated_topics.insert(clean_file_path, YamlValue::Mapping(updated));
        old_files.push(topic_path.to_path_buf());
        if topic_path.parent() != Some(topics_dir.as_path())
            && let Some(parent) = topic_path.parent()
        {
            old_dirs.insert(parent.to_path_buf());
        }
    }

    for (path, content) in &migrated_topics {
        let serialized = to_string(content).map_err(|e| DomainError::InvalidData(e.to_string()))?;
        fs.write_string(path, &serialized)?;
    }
    for old_file in old_files {
        if !migrated_topics.contains_key(&old_file) {
            fs.remove_file(&old_file)?;
        }
    }
    let mut old_dirs = old_dirs.into_iter().collect::<Vec<_>>();
    old_dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for old_dir in old_dirs {
        if fs.is_dir(&old_dir) && fs.read_dir(&old_dir)?.is_empty() {
            fs.remove_dir(&old_dir)?;
        }
    }
    Ok(!migrated_topics.is_empty())
}

pub fn migrate_legacy_keyphrase_boosting_file<Fs: FileSystem>(
    fs: &Fs,
    project_root: &Path,
) -> Result<bool, CoreError> {
    let legacy_path = project_root.join(adk_resources::specs::LEGACY_KEYPHRASE_BOOSTING_FILE_PATH);
    if !fs.is_file(&legacy_path) {
        return Ok(false);
    }

    let canonical_path = project_root.join(adk_resources::specs::KEYPHRASE_BOOSTING_FILE.file_path);
    if fs.exists(&canonical_path) {
        return Err(DomainError::InvalidData(format!(
            "Can't migrate legacy keyphrase boosting file: canonical path already exists: {}",
            canonical_path.to_string_lossy()
        ))
        .into());
    }

    let raw = fs.read(&legacy_path)?;
    if let Some(parent) = canonical_path.parent() {
        fs.create_dir_all(parent)?;
    }
    fs.write(&canonical_path, &raw)?;
    fs.remove_file(&legacy_path)?;
    if let Some(parent) = legacy_path.parent()
        && fs.is_dir(parent)
        && fs.read_dir(parent)?.is_empty()
    {
        fs.remove_dir(parent)?;
    }
    Ok(true)
}

pub fn recursive_file_paths<Fs: FileSystem>(
    fs: &Fs,
    root: &Path,
) -> Result<Vec<PathBuf>, CoreError> {
    recursive_file_paths_with_ancestors(fs, root, &HashSet::new())
}

fn recursive_file_paths_with_ancestors<Fs: FileSystem>(
    fs: &Fs,
    root: &Path,
    ancestors: &HashSet<PathBuf>,
) -> Result<Vec<PathBuf>, CoreError> {
    let mut files = Vec::new();
    if !fs.is_dir(root) {
        return Ok(files);
    }

    // Follow symlinked resource directories, but stop if a link points back to an ancestor.
    let canonical_root = fs.canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    if ancestors.contains(&canonical_root) {
        return Ok(files);
    }
    let mut child_ancestors = ancestors.clone();
    child_ancestors.insert(canonical_root);

    for path in fs.read_dir(root)? {
        if fs.is_dir(&path) {
            files.extend(recursive_file_paths_with_ancestors(
                fs,
                &path,
                &child_ancestors,
            )?);
        } else if fs.is_file(&path) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn yaml_mapping_contains_key(mapping: &Mapping, key: &str) -> bool {
    mapping
        .keys()
        .any(|candidate| candidate.as_str() == Some(key))
}

pub fn sort_json_value_keys(value: &mut JsonValue) {
    match value {
        JsonValue::Object(object) => {
            let old = std::mem::take(object);
            let mut items = old.into_iter().collect::<Vec<_>>();
            items.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (key, mut value) in items {
                sort_json_value_keys(&mut value);
                object.insert(key, value);
            }
        }
        JsonValue::Array(items) => {
            for item in items {
                sort_json_value_keys(item);
            }
        }
        _ => {}
    }
}

fn is_yaml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "yaml" | "yml"))
}

pub fn normalize_format_file_patterns(root: &Path, files: &[String]) -> Vec<String> {
    files
        .iter()
        .map(|file| {
            let path = Path::new(file);
            let rel = if path.is_absolute() {
                path.strip_prefix(root).unwrap_or(path).to_path_buf()
            } else {
                path.to_path_buf()
            };
            rel.to_string_lossy().replace('\\', "/")
        })
        .collect()
}

pub fn build_file_matcher(patterns: &[String]) -> Result<globset::GlobSet, CoreError> {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        let glob = Glob::new(p).map_err(|e| DomainError::InvalidData(e.to_string()))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| DomainError::InvalidData(e.to_string()).into())
}

pub fn flatten_discovered_paths(paths: &DiscoveredResourcePaths) -> Vec<String> {
    let mut out: Vec<String> = paths.values().flat_map(|v| v.iter().cloned()).collect();
    out.sort();
    out
}

pub fn flatten_discovered_paths_by_type_order(paths: &DiscoveredResourcePaths) -> Vec<String> {
    let mut out = Vec::new();
    for type_name in adk_types::ORDERED_TYPE_NAMES {
        if let Some(type_paths) = paths.get(type_name) {
            out.extend(type_paths.iter().cloned());
        }
    }
    let known_types = adk_types::ORDERED_TYPE_NAMES
        .into_iter()
        .collect::<HashSet<_>>();
    let mut remaining = paths
        .iter()
        .filter(|(type_name, _)| !known_types.contains(type_name.as_str()))
        .flat_map(|(_, type_paths)| type_paths.iter().cloned())
        .collect::<Vec<_>>();
    remaining.sort();
    out.extend(remaining);
    out
}

fn ordered_discovered_paths_for_modifications(
    paths: &DiscoveredResourcePaths,
    file_paths: &HashSet<String>,
    logical_paths: &HashSet<String>,
) -> Vec<String> {
    flatten_discovered_paths_by_type_order(paths)
        .into_iter()
        .filter(|logical_path| {
            logical_paths.contains(logical_path)
                || file_paths.contains(&parse_multi_resource_path(logical_path).0)
        })
        .collect()
}

pub fn flatten_deleted_discovered_paths(paths: &DiscoveredResourcePaths) -> Vec<String> {
    let mut entries = Vec::new();
    for (type_name, logical_paths) in paths {
        for path in logical_paths {
            entries.push((deleted_status_type_rank(type_name), path.clone()));
        }
    }
    entries.sort_by(|(left_rank, left_path), (right_rank, right_path)| {
        left_rank
            .cmp(right_rank)
            .then_with(|| left_path.cmp(right_path))
    });
    entries.into_iter().map(|(_, path)| path).collect()
}

fn deleted_status_type_rank(type_name: &str) -> usize {
    match type_name {
        "VoiceStylePrompt" => 0,
        "SettingsPersonality" => 1,
        "VoiceSafetyFilters" => 2,
        "SettingsRole" => 3,
        "GeneralSafetyFilters" => 4,
        "VoiceDisclaimerMessage" => 5,
        "VoiceGreeting" => 6,
        "AsrSettings" => 7,
        "Entity" => 8,
        "PhraseFilter" => 9,
        "Handoff" => 10,
        "SMSTemplate" => 11,
        other => adk_types::ORDERED_TYPE_NAMES
            .iter()
            .position(|name| *name == other)
            .map(|position| position + 100)
            .unwrap_or(usize::MAX),
    }
}

pub fn stable_dedup(items: &mut Vec<String>) {
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.clone()));
}

pub fn normalize_flow_resources_for_diff(
    resources: &mut ResourceMap,
    reference: Option<&ResourceMap>,
) {
    let step_ids = reference
        .map(flow_step_ids_by_folder_and_name)
        .unwrap_or_default();
    for (path, resource) in resources.iter_mut() {
        let Some(content) = resource
            .payload
            .get("content")
            .and_then(JsonValue::as_str)
            .map(ToString::to_string)
        else {
            continue;
        };
        let normalized = if path.starts_with("flows/") && path.ends_with("/flow_config.yaml") {
            canonical_flow_config_for_diff(path, &content, &step_ids)
        } else if path.starts_with("flows/") && path.contains("/steps/") && path.ends_with(".yaml")
        {
            canonical_flow_step_for_diff(&content)
        } else if path.starts_with("flows/")
            && path.contains("/function_steps/")
            && path.ends_with(".py")
        {
            Some(strip_generated_flow_function_imports(&content))
        } else {
            None
        };
        if let Some(normalized) = normalized {
            resource.payload = serde_json::json!({ "content": normalized });
        }
    }
}

fn flow_step_ids_by_folder_and_name(resources: &ResourceMap) -> HashMap<(String, String), String> {
    resources
        .iter()
        .filter_map(|(path, resource)| {
            if !(path.starts_with("flows/") && path.contains("/steps/") && path.ends_with(".yaml"))
            {
                return None;
            }
            let folder = path.split('/').nth(1)?.to_string();
            let content = resource.payload.get("content")?.as_str()?;
            let yaml = from_str::<YamlValue>(content).ok()?;
            let name = yaml.get("name")?.as_str()?.to_string();
            Some(((folder, name), resource.resource_id.clone()))
        })
        .collect()
}

fn canonical_flow_config_for_diff(
    path: &str,
    content: &str,
    step_ids: &HashMap<(String, String), String>,
) -> Option<String> {
    let yaml = from_str::<YamlValue>(content).ok()?;
    let name = yaml
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let description = yaml
        .get("description")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let mut start_step = yaml
        .get("start_step")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    if !start_step.starts_with("STEP-")
        && let Some(folder) = path.split('/').nth(1)
        && let Some(id) = step_ids.get(&(folder.to_string(), start_step.clone()))
    {
        start_step = id.clone();
    }
    Some(format!(
        "name: {name}\ndescription: {description}\nstart_step: {start_step}\n"
    ))
}

fn canonical_flow_step_for_diff(content: &str) -> Option<String> {
    let yaml = from_str::<YamlValue>(content).ok()?;
    let step_type = yaml
        .get("step_type")
        .and_then(|value| value.as_str())
        .unwrap_or("advanced_step");
    if step_type == "default_step" {
        Some(canonical_default_step_for_diff(&yaml))
    } else {
        Some(canonical_advanced_step_for_diff(&yaml))
    }
}

fn canonical_advanced_step_for_diff(yaml: &YamlValue) -> String {
    let name = yaml_string_value(yaml, "name");
    let prompt = yaml_string_value(yaml, "prompt");
    let asr = yaml.get("asr_biasing").or_else(|| yaml.get("asrBiasing"));
    let dtmf = yaml.get("dtmf_config").or_else(|| yaml.get("dtmfConfig"));
    let mut out = String::new();
    out.push_str("step_type: advanced_step\n");
    out.push_str(&format!("name: {name}\n"));
    out.push_str("asr_biasing:\n");
    for (key, value) in [
        (
            "is_enabled",
            yaml_bool_value(asr, &["is_enabled", "isEnabled"], false),
        ),
        (
            "alphanumeric",
            yaml_bool_value(asr, &["alphanumeric"], false),
        ),
        (
            "name_spelling",
            yaml_bool_value(asr, &["name_spelling", "nameSpelling"], false),
        ),
        ("numeric", yaml_bool_value(asr, &["numeric"], false)),
        (
            "party_size",
            yaml_bool_value(asr, &["party_size", "partySize"], false),
        ),
        (
            "precise_date",
            yaml_bool_value(asr, &["precise_date", "preciseDate"], false),
        ),
        (
            "relative_date",
            yaml_bool_value(asr, &["relative_date", "relativeDate"], false),
        ),
        (
            "single_number",
            yaml_bool_value(asr, &["single_number", "singleNumber"], false),
        ),
        ("time", yaml_bool_value(asr, &["time"], false)),
        ("yes_no", yaml_bool_value(asr, &["yes_no", "yesNo"], false)),
        ("address", yaml_bool_value(asr, &["address"], false)),
    ] {
        out.push_str(&format!("  {key}: {value}\n"));
    }
    let keywords = yaml_string_sequence(asr.and_then(|value| {
        value
            .get("custom_keywords")
            .or_else(|| value.get("customKeywords"))
    }));
    if keywords.is_empty() {
        out.push_str("  custom_keywords: []\n");
    } else {
        out.push_str("  custom_keywords:\n");
        for keyword in keywords {
            out.push_str(&format!("  - {keyword}\n"));
        }
    }
    out.push_str("dtmf_config:\n");
    out.push_str(&format!(
        "  is_enabled: {}\n",
        yaml_bool_value(dtmf, &["is_enabled", "isEnabled"], false)
    ));
    out.push_str(&format!(
        "  inter_digit_timeout: {}\n",
        yaml_i64_value(dtmf, &["inter_digit_timeout", "interDigitTimeout"], 0)
    ));
    out.push_str(&format!(
        "  max_digits: {}\n",
        yaml_i64_value(dtmf, &["max_digits", "maxDigits"], 0)
    ));
    out.push_str(&format!(
        "  end_key: '{}'\n",
        yaml_string_value_from(dtmf, &["end_key", "endKey"])
    ));
    out.push_str(&format!(
        "  collect_while_agent_speaking: {}\n",
        yaml_bool_value(
            dtmf,
            &["collect_while_agent_speaking", "collectWhileAgentSpeaking"],
            false
        )
    ));
    out.push_str(&format!(
        "  is_pii: {}\n",
        yaml_bool_value(dtmf, &["is_pii", "isPii"], false)
    ));
    out.push_str(&format!("prompt: {prompt}\n"));
    out
}

fn canonical_default_step_for_diff(yaml: &YamlValue) -> String {
    let name = yaml_string_value(yaml, "name");
    let prompt = yaml_string_value(yaml, "prompt");
    let mut out = String::new();
    out.push_str("step_type: default_step\n");
    out.push_str(&format!("name: {name}\n"));
    out.push_str("conditions:\n");
    if let Some(conditions) = yaml.get("conditions").and_then(|value| value.as_sequence()) {
        for condition in conditions {
            out.push_str(&format!(
                "- name: {}\n",
                yaml_string_value(condition, "name")
            ));
            out.push_str(&format!(
                "  condition_type: {}\n",
                yaml_string_value(condition, "condition_type")
            ));
            out.push_str(&format!(
                "  description: {}\n",
                yaml_string_value(condition, "description")
            ));
            let required = yaml_string_sequence(condition.get("required_entities"));
            if required.is_empty() {
                out.push_str("  required_entities: []\n");
            } else {
                out.push_str("  required_entities:\n");
                for entity in required {
                    out.push_str(&format!("  - {entity}\n"));
                }
            }
        }
    }
    let extracted = yaml_string_sequence(yaml.get("extracted_entities"));
    if extracted.is_empty() {
        out.push_str("extracted_entities: []\n");
    } else {
        out.push_str("extracted_entities:\n");
        for entity in extracted {
            out.push_str(&format!("- {entity}\n"));
        }
    }
    out.push_str(&format!("prompt: {prompt}\n"));
    out
}

fn strip_generated_flow_function_imports(content: &str) -> String {
    let mut lines = content.lines().collect::<Vec<_>>();
    while lines
        .first()
        .is_some_and(|line| line.trim().is_empty() || line.starts_with("from _gen import"))
    {
        lines.remove(0);
    }
    format!("{}\n", lines.join("\n"))
        .trim_end_matches('\n')
        .to_string()
}

fn yaml_string_value(yaml: &YamlValue, key: &str) -> String {
    yaml.get(key)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

fn yaml_string_value_from(yaml: Option<&YamlValue>, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| {
            yaml.and_then(|value| value.get(*key))
                .and_then(|value| value.as_str())
        })
        .unwrap_or_default()
        .to_string()
}

fn yaml_bool_value(yaml: Option<&YamlValue>, keys: &[&str], default: bool) -> bool {
    keys.iter()
        .find_map(|key| {
            yaml.and_then(|value| value.get(*key))
                .and_then(|value| value.as_bool())
        })
        .unwrap_or(default)
}

fn yaml_i64_value(yaml: Option<&YamlValue>, keys: &[&str], default: i64) -> i64 {
    keys.iter()
        .find_map(|key| {
            yaml.and_then(|value| value.get(*key))
                .and_then(|value| value.as_i64())
        })
        .unwrap_or(default)
}

fn yaml_string_sequence(yaml: Option<&YamlValue>) -> Vec<String> {
    yaml.and_then(|value| value.as_sequence())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .map(ToString::to_string)
        .collect()
}

pub fn normalize_function_references_in_rules(resources: &mut ResourceMap) {
    let replacements = resources
        .values()
        .filter(|resource| {
            resource.file_path.starts_with("functions/") && resource.file_path.ends_with(".py")
        })
        .map(|resource| {
            let name = resource
                .file_path
                .split('/')
                .next_back()
                .unwrap_or(&resource.name)
                .trim_end_matches(".py")
                .to_string();
            (resource.resource_id.clone(), name)
        })
        .filter(|(id, name)| !id.is_empty() && id != name)
        .collect::<Vec<_>>();
    if replacements.is_empty() {
        return;
    }
    let Some(rules) = resources.get_mut("agent_settings/rules.txt") else {
        return;
    };
    let Some(content) = rules
        .payload
        .get("content")
        .and_then(JsonValue::as_str)
        .map(ToString::to_string)
    else {
        return;
    };
    let mut normalized = content;
    for (id, name) in replacements {
        normalized = normalized.replace(&format!("{{{{fn:{id}}}}}"), &format!("{{{{fn:{name}}}}}"));
    }
    rules.payload["content"] = JsonValue::String(normalized);
}

#[derive(Debug, Clone)]
pub struct ReferenceNameReplacement {
    pub prefix: String,
    pub id: String,
    pub name: String,
}

pub fn apply_reference_name_replacements(
    resources: &mut ResourceMap,
    replacements: &[ReferenceNameReplacement],
) {
    if replacements.is_empty() {
        return;
    }
    for resource in resources.values_mut() {
        let Some(content) = resource
            .payload
            .get("content")
            .and_then(JsonValue::as_str)
            .map(ToString::to_string)
        else {
            continue;
        };
        let normalized = replace_reference_ids_with_names(&content, replacements);
        if normalized != content {
            resource.payload["content"] = JsonValue::String(normalized);
        }
    }
}

fn replace_reference_ids_with_names(
    content: &str,
    replacements: &[ReferenceNameReplacement],
) -> String {
    let mut normalized = content.to_string();
    for replacement in replacements {
        normalized = normalized.replace(
            &format!("{{{{{}:{}}}}}", replacement.prefix, replacement.id),
            &format!("{{{{{}:{}}}}}", replacement.prefix, replacement.name),
        );
    }
    normalized
}

pub fn reference_name_from_logical_path(logical_path: &str) -> String {
    let (_, resource_suffix) = parse_multi_resource_path(logical_path);
    let source = resource_suffix.as_deref().unwrap_or(logical_path);
    let leaf = source.rsplit('/').next().unwrap_or(source);
    Path::new(leaf)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(leaf)
        .to_string()
}

pub fn absolutize_deleted_diff_keys(
    root: &Path,
    diffs: &mut DiffMap,
    deleted_file_paths: &HashSet<String>,
) {
    if deleted_file_paths.is_empty() {
        return;
    }
    let mut replacements = Vec::new();
    for path in deleted_file_paths {
        if let Some(diff) = diffs.shift_remove(path) {
            replacements.push((root.join(path).to_string_lossy().to_string(), diff));
        }
    }
    for (path, diff) in replacements {
        diffs.insert(path, diff);
    }
}

pub fn compute_modified_files_against_snapshot<Fs: FileSystem>(
    fs: &Fs,
    root: &Path,
    kept_resources: &DiscoveredResourcePaths,
    snapshot_hashes: &indexmap::IndexMap<String, String>,
) -> Result<Vec<String>, CoreError> {
    let kept_paths = flatten_discovered_paths(kept_resources);
    let mut modified_file_paths = HashSet::new();
    let mut modified_logical_paths = HashSet::new();
    for logical_path in kept_paths {
        let (file_path, _) = parse_multi_resource_path(&logical_path);
        let (hash_path, expected_hash) =
            if let Some(expected_hash) = snapshot_hashes.get(&logical_path) {
                (logical_path.as_str(), expected_hash)
            } else if let Some(expected_hash) = snapshot_hashes.get(&file_path) {
                (file_path.as_str(), expected_hash)
            } else {
                continue;
            };
        let current_path = root.join(&file_path);
        let current_content = fs.read_to_string(&current_path).unwrap_or_default();
        let current_hash = current_status_hash_for_expected(
            hash_path,
            &current_content,
            expected_hash,
            snapshot_hashes,
        );
        if current_hash != *expected_hash {
            if hash_path == logical_path {
                modified_logical_paths.insert(logical_path);
            } else {
                modified_file_paths.insert(file_path);
            }
        }
    }
    Ok(ordered_discovered_paths_for_modifications(
        kept_resources,
        &modified_file_paths,
        &modified_logical_paths,
    ))
}

pub fn compute_modified_files_against_snapshot_with_replacements<Fs: FileSystem>(
    fs: &Fs,
    root: &Path,
    kept_resources: &DiscoveredResourcePaths,
    snapshot_hashes: &indexmap::IndexMap<String, String>,
    replacements: &[ReferenceNameReplacement],
) -> Result<Vec<String>, CoreError> {
    if replacements.is_empty() {
        return Ok(Vec::new());
    }
    let kept_paths = flatten_discovered_paths(kept_resources);
    let mut modified_file_paths = HashSet::new();
    let mut modified_logical_paths = HashSet::new();
    for logical_path in kept_paths {
        let (file_path, _) = parse_multi_resource_path(&logical_path);
        let (hash_path, expected_hash) =
            if let Some(expected_hash) = snapshot_hashes.get(&logical_path) {
                (logical_path.as_str(), expected_hash)
            } else if let Some(expected_hash) = snapshot_hashes.get(&file_path) {
                (file_path.as_str(), expected_hash)
            } else {
                continue;
            };
        let current_path = root.join(&file_path);
        let current_content = fs.read_to_string(&current_path).unwrap_or_default();
        let normalized_content = replace_reference_ids_with_names(&current_content, replacements);
        if normalized_content == current_content {
            continue;
        }
        let current_hash = current_status_hash_for_expected(
            hash_path,
            &normalized_content,
            expected_hash,
            snapshot_hashes,
        );
        if current_hash != *expected_hash {
            if hash_path == logical_path {
                modified_logical_paths.insert(logical_path);
            } else {
                modified_file_paths.insert(file_path);
            }
        }
    }
    Ok(ordered_discovered_paths_for_modifications(
        kept_resources,
        &modified_file_paths,
        &modified_logical_paths,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_and_load_project_can_use_memory_filesystem() {
        let fs = adk_io::MemoryFileSystem::new();
        let workspace = ProjectWorkspace::with_file_system(fs.clone());
        let base = Path::new("workspace");

        let config = workspace
            .init_project(
                base,
                "dev".to_string(),
                "acct".to_string(),
                "proj".to_string(),
            )
            .expect("init project");

        let root = base.join("acct").join("proj");
        assert_eq!(config.branch_id, "main");
        assert!(fs.is_file(&root.join(PROJECT_CONFIG_FILE)));
        assert!(fs.is_file(&root.join("_gen/decorators.py")));

        let loaded: ProjectConfig = serde_yaml_ng::from_str(
            &fs.read_to_string(&root.join(PROJECT_CONFIG_FILE))
                .expect("read project config"),
        )
        .expect("parse project config");
        assert_eq!(loaded.account_id, "acct");
        assert_eq!(loaded.project_id, "proj");
        assert_eq!(loaded.region, "dev");
    }

    #[test]
    fn typed_discovery_uses_configured_filesystem() {
        let fs = adk_io::MemoryFileSystem::new();
        fs.write_string(
            Path::new("workspace/topics/support.yaml"),
            "name: Support\nenabled: true\nactions: Help.\ncontent: Hi.\nexample_queries: []\n",
        )
        .expect("write topic");
        fs.write_string(
            Path::new("workspace/functions/greet.py"),
            "def greet(conv):\n    conv.state.customer_name = 'Ada'\n",
        )
        .expect("write function");

        let workspace = ProjectWorkspace::with_file_system(fs);
        let discovered = workspace.discover_local_resources(Path::new("workspace"));

        assert_eq!(
            discovered.get("Topic").cloned().unwrap_or_default(),
            vec!["topics/support.yaml".to_string()]
        );
        assert_eq!(
            discovered.get("Variable").cloned().unwrap_or_default(),
            vec!["variables/customer_name".to_string()]
        );
    }
}
