use adk_types::RESOURCE_TYPE_REGISTRY;
use indexmap::{IndexMap, IndexSet};
use std::collections::BTreeSet;

pub type DiscoveredResourcePaths = IndexMap<String, Vec<String>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredResourceChanges {
    pub new_resources: DiscoveredResourcePaths,
    pub kept_resources: DiscoveredResourcePaths,
    pub deleted_resources: DiscoveredResourcePaths,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedResourceLifecycle {
    pub type_name: String,
    pub file_path: String,
    pub resource_id: String,
    pub resource_prefix: Option<String>,
    pub is_existing: bool,
}

pub fn empty_discovered_resource_paths() -> DiscoveredResourcePaths {
    let mut out = DiscoveredResourcePaths::new();
    for d in RESOURCE_TYPE_REGISTRY {
        out.insert(d.type_name.to_string(), Vec::new());
    }
    out
}

pub fn type_name_to_resource_prefix(type_name: &str) -> Option<&'static str> {
    adk_types::descriptor_by_type_name(type_name).and_then(|d| d.id_prefix)
}

/// Builds local lifecycle rows for typed resource files.
///
/// Existing resources keep the server ids recorded in the status snapshot. Newly
/// discovered resources get deterministic ids hashed from their resource type and
/// logical path so status, diff, and push can reason about them before creation.
pub fn build_typed_resource_lifecycle(
    discovered: &DiscoveredResourcePaths,
    existing_resource_ids: &indexmap::IndexMap<String, String>,
) -> Vec<TypedResourceLifecycle> {
    let mut out = Vec::new();
    for (type_name, paths) in discovered {
        let resource_prefix = type_name_to_resource_prefix(type_name).map(str::to_string);
        for path in paths {
            if let Some(existing_id) = existing_resource_ids.get(path) {
                out.push(TypedResourceLifecycle {
                    type_name: type_name.clone(),
                    file_path: path.clone(),
                    resource_id: existing_id.clone(),
                    resource_prefix: resource_prefix.clone(),
                    is_existing: true,
                });
            } else {
                let digest = adk_io::compute_hash(&format!("{type_name}:{path}"));
                let short = &digest[..8];
                let generated_id = resource_prefix
                    .as_ref()
                    .map(|p| format!("{p}-{short}"))
                    .unwrap_or_else(|| format!("{}-{short}", type_name.to_uppercase()));
                out.push(TypedResourceLifecycle {
                    type_name: type_name.clone(),
                    file_path: path.clone(),
                    resource_id: generated_id,
                    resource_prefix: resource_prefix.clone(),
                    is_existing: false,
                });
            }
        }
    }
    out.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    out
}

/// Mirrors Python `AgentStudioProject.find_new_kept_deleted` at a typed path level.
/// Compares logical path lists per resource type and returns new/kept/deleted paths.
pub fn find_new_kept_deleted(
    discovered_resources: &DiscoveredResourcePaths,
    existing_resources: &DiscoveredResourcePaths,
) -> DiscoveredResourceChanges {
    let mut resource_types: IndexSet<String> = IndexSet::new();
    resource_types.extend(discovered_resources.keys().cloned());
    resource_types.extend(existing_resources.keys().cloned());

    let mut new_resources: DiscoveredResourcePaths = IndexMap::new();
    let mut kept_resources: DiscoveredResourcePaths = IndexMap::new();
    let mut deleted_resources: DiscoveredResourcePaths = IndexMap::new();

    for resource_type in resource_types {
        let discovered = discovered_resources
            .get(&resource_type)
            .cloned()
            .unwrap_or_default();
        let existing = existing_resources
            .get(&resource_type)
            .cloned()
            .unwrap_or_default();

        let discovered_set: BTreeSet<String> = discovered.iter().cloned().collect();
        let existing_set: BTreeSet<String> = existing.iter().cloned().collect();

        let new_paths: Vec<String> = discovered
            .iter()
            .filter(|path| !existing_set.contains(*path))
            .cloned()
            .collect();
        let kept_paths: Vec<String> = discovered
            .iter()
            .filter(|path| existing_set.contains(*path))
            .cloned()
            .collect();
        let deleted_paths: Vec<String> = existing
            .iter()
            .filter(|path| !discovered_set.contains(*path))
            .cloned()
            .collect();

        new_resources.insert(resource_type.clone(), new_paths);
        kept_resources.insert(resource_type.clone(), kept_paths);
        deleted_resources.insert(resource_type, deleted_paths);
    }

    DiscoveredResourceChanges {
        new_resources,
        kept_resources,
        deleted_resources,
    }
}
