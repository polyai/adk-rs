use crate::python_functions::local_resource_content;
use crate::status_snapshot::StatusSnapshot;
use crate::{
    CoreError, DiscoveredResourceChanges, DiscoveredResourcePaths, MIGRATED_LEGACY_TOPIC_FILES,
    PROJECT_CONFIG_FILE, PYTHON_FLOW_IMPORT_STATUS_KEY_PREFIX, PYTHON_VARIANT_STATUS_KEY_PREFIX,
    ReferenceNameReplacement, STATUS_FILE, TypedResourceLifecycle,
    apply_reference_name_replacements, compute_modified_files_against_snapshot,
    compute_modified_files_against_snapshot_with_replacements, discover, find_project_root_with_fs,
    flatten_deleted_discovered_paths, flatten_discovered_paths_by_type_order,
    legacy_python_rules_reference_names, legacy_python_status_resource_content,
    legacy_python_status_resource_file_hash, legacy_python_status_resource_path,
    migrate_legacy_topic_files, migration_flags_from_status, project_config_contains_branch_id,
    project_config_yaml, recursive_file_paths, reference_name_from_logical_path, resources,
    stable_dedup,
};
use adk_io::{FileSystem, StdFileSystem, parse_multi_resource_path};
use adk_types::{DomainError, ProjectConfig, Resource, ResourceMap, StatusSummary};
use base64::Engine;
use std::collections::BTreeSet;
use std::path::Path;

const PYTHON_GEN_TEMPLATE_FILES: &[(&str, &str)] = &[
    (
        "__init__.py",
        include_str!("../python-gen-template/__init__.py"),
    ),
    (
        "attachment.py",
        include_str!("../python-gen-template/attachment.py"),
    ),
    (
        "conv_utils.py",
        include_str!("../python-gen-template/conv_utils.py"),
    ),
    (
        "conversation.py",
        include_str!("../python-gen-template/conversation.py"),
    ),
    (
        "decorators.py",
        include_str!("../python-gen-template/decorators.py"),
    ),
    (
        "external_events.py",
        include_str!("../python-gen-template/external_events.py"),
    ),
    ("flow.py", include_str!("../python-gen-template/flow.py")),
    (
        "history.py",
        include_str!("../python-gen-template/history.py"),
    ),
    (
        "log_utils.py",
        include_str!("../python-gen-template/log_utils.py"),
    ),
    (
        "memory.py",
        include_str!("../python-gen-template/memory.py"),
    ),
    (
        "secret_vault.py",
        include_str!("../python-gen-template/secret_vault.py"),
    ),
    ("sms.py", include_str!("../python-gen-template/sms.py")),
    (
        "value_extraction.py",
        include_str!("../python-gen-template/value_extraction.py"),
    ),
];

/// Local project workspace operations that do not require a platform client.
pub struct ProjectWorkspace<Fs = StdFileSystem> {
    pub(crate) fs: Fs,
}

impl ProjectWorkspace<StdFileSystem> {
    pub fn new() -> Self {
        Self::with_file_system(StdFileSystem)
    }
}

impl Default for ProjectWorkspace<StdFileSystem> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Fs: FileSystem> ProjectWorkspace<Fs> {
    pub fn with_file_system(fs: Fs) -> Self {
        Self { fs }
    }

    pub fn init_project(
        &self,
        base_path: &Path,
        region: String,
        account_id: String,
        project_id: String,
    ) -> Result<ProjectConfig, CoreError> {
        self.init_project_with_name(base_path, region, account_id, project_id, None)
    }

    pub fn init_project_with_name(
        &self,
        base_path: &Path,
        region: String,
        account_id: String,
        project_id: String,
        project_name: Option<String>,
    ) -> Result<ProjectConfig, CoreError> {
        let root = base_path.join(&account_id).join(&project_id);
        self.fs.create_dir_all(&root)?;
        let config = ProjectConfig {
            region,
            account_id,
            project_id,
            project_name,
            branch_id: "main".to_string(),
        };
        let serialized = project_config_yaml(&config)?;
        self.fs
            .write_string(&root.join(PROJECT_CONFIG_FILE), &serialized)?;
        self.write_python_gen_package(&root)?;
        Ok(config)
    }

    pub fn load_project_config(&self, base_path: &Path) -> Result<ProjectConfig, CoreError> {
        let discovered = find_project_root_with_fs(&self.fs, base_path)
            .ok_or_else(|| DomainError::ConfigNotFound(base_path.to_string_lossy().to_string()))?;
        let config_path = discovered.join(PROJECT_CONFIG_FILE);
        if self.fs.exists(&config_path) {
            let raw = self.fs.read_to_string(&config_path)?;
            let mut config: ProjectConfig =
                serde_yaml::from_str(&raw).map_err(|e| DomainError::InvalidData(e.to_string()))?;
            if !project_config_contains_branch_id(&raw)
                && let Some(snapshot) = self.load_status_snapshot(&discovered)?
            {
                config.branch_id = snapshot.branch_id;
            }
            self.run_and_persist_project_migrations(&discovered)?;
            return Ok(config);
        }

        if let Some(snapshot) = self.load_status_snapshot(&discovered)? {
            let config = ProjectConfig {
                region: snapshot.region,
                account_id: snapshot.account_id,
                project_id: snapshot.project_id,
                project_name: snapshot.project_name,
                branch_id: snapshot.branch_id,
            };
            self.run_and_persist_project_migrations(&discovered)?;
            return Ok(config);
        }

        Err(DomainError::ConfigNotFound(discovered.to_string_lossy().to_string()).into())
    }

    pub(crate) fn write_python_gen_package(&self, project_root: &Path) -> Result<(), CoreError> {
        let gen_dir = project_root.join("_gen");
        self.fs.create_dir_all(&gen_dir)?;
        for path in self.fs.read_dir(&gen_dir)? {
            if path.extension().is_some_and(|extension| extension == "pyi") {
                self.fs.remove_file(&path)?;
            }
        }
        for (file_name, contents) in PYTHON_GEN_TEMPLATE_FILES {
            self.fs.write_string(&gen_dir.join(file_name), contents)?;
        }
        Ok(())
    }

    pub(crate) fn run_and_persist_project_migrations(
        &self,
        project_root: &Path,
    ) -> Result<BTreeSet<String>, CoreError> {
        let mut status = self.load_status_snapshot_json(project_root)?;
        let mut migration_flags = migration_flags_from_status(&status);
        if !migration_flags.contains(MIGRATED_LEGACY_TOPIC_FILES) {
            let had_status_snapshot = self.fs.exists(&project_root.join(STATUS_FILE));
            let migrated_files = migrate_legacy_topic_files(&self.fs, project_root)?;
            migration_flags.insert(MIGRATED_LEGACY_TOPIC_FILES.to_string());
            if had_status_snapshot || migrated_files {
                status.insert(
                    "migration_flags".to_string(),
                    serde_json::Value::Array(
                        migration_flags
                            .iter()
                            .cloned()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                );
                self.write_python_gen_package(project_root)?;
                self.write_status_snapshot_json(project_root, &status)?;
            }
        }
        Ok(migration_flags)
    }

    fn load_status_snapshot_json(
        &self,
        project_root: &Path,
    ) -> Result<serde_json::Map<String, serde_json::Value>, CoreError> {
        // Raw JSON is used only by migrations so unknown Python fields are
        // preserved byte-for-byte at the schema level.
        let status_path = project_root.join(STATUS_FILE);
        if !self.fs.exists(&status_path) {
            return Ok(serde_json::Map::new());
        }
        let encoded = self.fs.read(&status_path)?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| DomainError::InvalidData(e.to_string()))?;
        let value: serde_json::Value = serde_json::from_slice(&decoded)?;
        Ok(value.as_object().cloned().unwrap_or_default())
    }

    fn write_status_snapshot_json(
        &self,
        project_root: &Path,
        status: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), CoreError> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(status)?);
        let status_path = project_root.join(STATUS_FILE);
        if let Some(parent) = status_path.parent() {
            self.fs.create_dir_all(parent)?;
        }
        self.fs.write_string(&status_path, &encoded)?;
        Ok(())
    }

    pub(crate) fn load_status_snapshot(
        &self,
        project_root: &Path,
    ) -> Result<Option<StatusSnapshot>, CoreError> {
        let status_path = project_root.join(STATUS_FILE);
        if !self.fs.exists(&status_path) {
            return Ok(None);
        }
        let encoded = self.fs.read(&status_path)?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| DomainError::InvalidData(e.to_string()))?;
        Ok(Some(serde_json::from_slice(&decoded)?))
    }

    pub(crate) fn write_status_snapshot(
        &self,
        project_root: &Path,
        snapshot: &StatusSnapshot,
    ) -> Result<(), CoreError> {
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(snapshot)?);
        let status_path = project_root.join(STATUS_FILE);
        if let Some(parent) = status_path.parent() {
            self.fs.create_dir_all(parent)?;
        }
        self.fs.write_string(&status_path, &encoded)?;
        Ok(())
    }

    pub(crate) fn write_project_config(
        &self,
        root: &Path,
        cfg: &ProjectConfig,
    ) -> Result<(), CoreError> {
        let project_root = find_project_root_with_fs(&self.fs, root)
            .ok_or_else(|| DomainError::ConfigNotFound(root.to_string_lossy().to_string()))?;
        let serialized = project_config_yaml(cfg)?;
        self.fs
            .write_string(&project_root.join(PROJECT_CONFIG_FILE), &serialized)?;
        self.write_project_config_status_snapshot(&project_root, cfg)?;
        Ok(())
    }

    fn write_project_config_status_snapshot(
        &self,
        project_root: &Path,
        cfg: &ProjectConfig,
    ) -> Result<(), CoreError> {
        let mut snapshot = self.load_status_snapshot(project_root)?.unwrap_or_default();
        snapshot.region = cfg.region.clone();
        snapshot.account_id = cfg.account_id.clone();
        snapshot.project_id = cfg.project_id.clone();
        snapshot.project_name = cfg.project_name.clone();
        snapshot.branch_id = cfg.branch_id.clone();
        self.write_status_snapshot(project_root, &snapshot)
    }

    pub fn collect_local_resources(&self, root: &Path) -> Result<ResourceMap, CoreError> {
        let mut map = ResourceMap::new();
        for file in recursive_file_paths(&self.fs, root)? {
            let rel = file
                .strip_prefix(root)
                .unwrap_or(file.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            if rel == PROJECT_CONFIG_FILE || rel == STATUS_FILE || rel.starts_with("_gen/") {
                continue;
            }
            let content = self.fs.read_to_string(file.as_path()).unwrap_or_default();
            let payload = serde_json::json!({
                "content": local_resource_content(&rel, &content),
            });
            map.insert(
                rel.clone(),
                Resource {
                    resource_id: rel.clone(),
                    name: rel.clone(),
                    file_path: rel,
                    payload,
                },
            );
        }
        Ok(map)
    }

    /// Typed discovery matching Python `AgentStudioProject.discover_local_resources()`.
    pub fn discover_local_resources(&self, root: &Path) -> indexmap::IndexMap<String, Vec<String>> {
        discover::discover_local_resources(root)
    }

    /// Typed parity helper matching Python `find_new_kept_deleted` semantics at path level.
    pub fn find_new_kept_deleted(
        &self,
        discovered_resources: &DiscoveredResourcePaths,
        existing_resources: &DiscoveredResourcePaths,
    ) -> DiscoveredResourceChanges {
        resources::find_new_kept_deleted(discovered_resources, existing_resources)
    }

    pub fn typed_resource_lifecycle(
        &self,
        root: &Path,
    ) -> Result<Vec<TypedResourceLifecycle>, CoreError> {
        let discovered = self.discover_local_resources(root);
        let existing_resource_ids = self.load_status_snapshot_resource_ids(root)?;
        Ok(resources::build_typed_resource_lifecycle(
            &discovered,
            &existing_resource_ids,
        ))
    }

    pub fn status(&self, root: &Path) -> Result<StatusSummary, CoreError> {
        self.load_project_config(root)?;
        let mut local = self.collect_local_resources(root)?;
        let mut summary = StatusSummary {
            conflict_detection_available: true,
            files_with_conflicts: self.detect_conflict_files(root)?,
            ..StatusSummary::default()
        };

        let existing_typed = self
            .load_status_snapshot_discovered_resources(root)?
            .unwrap_or_else(resources::empty_discovered_resource_paths);
        let discovered_typed = self.discover_local_resources(root);
        let typed_changes = self.find_new_kept_deleted(&discovered_typed, &existing_typed);
        summary.new_files = flatten_discovered_paths_by_type_order(&typed_changes.new_resources);
        summary.deleted_files = flatten_deleted_discovered_paths(&typed_changes.deleted_resources);

        if let Some(snapshot_hashes) = self.load_status_snapshot_file_hashes(root)? {
            summary.modified_files = compute_modified_files_against_snapshot(
                root,
                &typed_changes.kept_resources,
                &snapshot_hashes,
            )?;
            let replacements = self
                .deleted_resource_reference_replacements(root, &typed_changes.deleted_resources)?;
            if !replacements.is_empty() {
                apply_reference_name_replacements(&mut local, &replacements);
                summary.modified_files.extend(
                    compute_modified_files_against_snapshot_with_replacements(
                        root,
                        &typed_changes.kept_resources,
                        &snapshot_hashes,
                        &replacements,
                    )?,
                );
                stable_dedup(&mut summary.modified_files);
            }
        } else {
            summary.modified_files =
                flatten_discovered_paths_by_type_order(&typed_changes.kept_resources);
        }

        Ok(summary)
    }

    pub(crate) fn load_status_snapshot_discovered_resources(
        &self,
        root: &Path,
    ) -> Result<Option<DiscoveredResourcePaths>, CoreError> {
        let Some(snapshot) = self.load_status_snapshot(root)? else {
            return Ok(None);
        };

        let mut discovered = resources::empty_discovered_resource_paths();
        for (resource_name, resource_entries) in &snapshot.resources {
            let Some(type_name) = discover::resource_name_to_type_name(resource_name) else {
                continue;
            };
            let mut paths = Vec::new();
            for (idx, resource_data) in resource_entries.values().enumerate() {
                let resource_value = resource_data.as_value();
                let file_path = resource_data
                    .file_path()
                    .map(|path| path.replace('\\', "/"))
                    .or_else(|| {
                        legacy_python_status_resource_path(resource_name, &resource_value, idx)
                    });
                if let Some(file_path) = file_path {
                    paths.push(file_path);
                }
            }
            paths.sort();
            paths.dedup();
            discovered.insert(type_name.to_string(), paths);
        }
        Ok(Some(discovered))
    }

    pub(crate) fn deleted_resource_reference_replacements(
        &self,
        root: &Path,
        deleted_resources: &DiscoveredResourcePaths,
    ) -> Result<Vec<ReferenceNameReplacement>, CoreError> {
        let existing_resource_ids = self.load_status_snapshot_resource_ids(root)?;
        let mut replacements = Vec::new();
        for (type_name, paths) in deleted_resources {
            let Some(prefix) = resources::type_name_to_resource_prefix(type_name) else {
                continue;
            };
            for logical_path in paths {
                let Some(resource_id) = existing_resource_ids.get(logical_path) else {
                    continue;
                };
                let name = reference_name_from_logical_path(logical_path);
                if !resource_id.is_empty() && !name.is_empty() && resource_id != &name {
                    replacements.push(ReferenceNameReplacement {
                        prefix: prefix.to_string(),
                        id: resource_id.clone(),
                        name,
                    });
                }
            }
        }
        Ok(replacements)
    }

    pub(crate) fn detect_conflict_files(&self, root: &Path) -> Result<Vec<String>, CoreError> {
        let mut conflicts = Vec::new();
        for path in recursive_file_paths(&self.fs, root)? {
            let content = match self.fs.read_to_string(&path) {
                Ok(content) => content,
                Err(_) => continue,
            };
            if content.contains("<<<<<<<")
                && content.contains("=======")
                && content.contains(">>>>>>>")
            {
                conflicts.push(path.to_string_lossy().to_string());
            }
        }
        Ok(conflicts)
    }

    pub(crate) fn load_status_snapshot_file_hashes(
        &self,
        root: &Path,
    ) -> Result<Option<indexmap::IndexMap<String, String>>, CoreError> {
        let Some(snapshot) = self.load_status_snapshot(root)? else {
            return Ok(None);
        };
        let mut out = indexmap::IndexMap::new();
        if !snapshot.resources.is_empty() {
            let mut found_resource_hash = false;
            let rules_reference_names = legacy_python_rules_reference_names(&snapshot.resources);
            for (resource_name, entries) in &snapshot.resources {
                for (idx, payload) in entries.values().enumerate() {
                    let payload_value = payload.as_value();
                    if resource_name == "flow_config"
                        && let Some(flow_name) = payload.name()
                        && let Some(flow_id) = payload.resource_id()
                    {
                        out.insert(
                            format!(
                                "{PYTHON_FLOW_IMPORT_STATUS_KEY_PREFIX}{}",
                                discover::clean_name(flow_name, true)
                            ),
                            flow_id.to_string(),
                        );
                    }
                    if resource_name == "variants"
                        && let Some(variant_name) = payload.name()
                        && let Some(variant_id) = payload.resource_id()
                    {
                        out.insert(
                            format!("{PYTHON_VARIANT_STATUS_KEY_PREFIX}{variant_name}"),
                            variant_id.to_string(),
                        );
                    }
                    let Some(logical_path) =
                        legacy_python_status_resource_path(resource_name, &payload_value, idx)
                    else {
                        continue;
                    };
                    let (file_path, _) = parse_multi_resource_path(&logical_path);
                    if out.contains_key(&file_path) {
                        continue;
                    }
                    if let Some(hash) = legacy_python_status_resource_file_hash(
                        &self.fs,
                        root,
                        resource_name,
                        &file_path,
                        &payload_value,
                        &rules_reference_names,
                    ) {
                        out.insert(file_path, hash);
                        found_resource_hash = true;
                    }
                }
            }
            if found_resource_hash {
                return Ok(Some(out));
            }
        }

        if snapshot.file_structure_info.is_empty() {
            return Ok(if out.is_empty() { None } else { Some(out) });
        }
        for (file_path, info) in &snapshot.file_structure_info {
            if file_path.contains("variant_attributes.yaml/variants/")
                && !info.resource_name.is_empty()
                && !info.resource_id.is_empty()
            {
                out.insert(
                    format!("{PYTHON_VARIANT_STATUS_KEY_PREFIX}{}", info.resource_name),
                    info.resource_id.clone(),
                );
            }
            if info.hash.is_empty() {
                continue;
            }
            out.insert(file_path.replace('\\', "/"), info.hash.clone());
        }
        Ok(Some(out))
    }

    pub(crate) fn load_status_snapshot_resource_ids(
        &self,
        root: &Path,
    ) -> Result<indexmap::IndexMap<String, String>, CoreError> {
        let Some(snapshot) = self.load_status_snapshot(root)? else {
            return Ok(indexmap::IndexMap::new());
        };
        let mut ids = indexmap::IndexMap::new();
        for (resource_name, entries) in &snapshot.resources {
            for (idx, payload) in entries.values().enumerate() {
                let payload_value = payload.as_value();
                let path = payload
                    .file_path()
                    .map(|path| path.replace('\\', "/"))
                    .or_else(|| {
                        legacy_python_status_resource_path(resource_name, &payload_value, idx)
                    });
                let Some(id) = payload.resource_id() else {
                    continue;
                };
                if let Some(path) = path {
                    ids.insert(path, id.to_string());
                }
            }
        }
        Ok(ids)
    }

    pub(crate) fn load_status_snapshot_resource_metadata(
        &self,
        root: &Path,
    ) -> Result<indexmap::IndexMap<String, (String, String)>, CoreError> {
        let Some(snapshot) = self.load_status_snapshot(root)? else {
            return Ok(indexmap::IndexMap::new());
        };
        let mut metadata = indexmap::IndexMap::new();
        for (resource_name, entries) in &snapshot.resources {
            for (idx, payload) in entries.values().enumerate() {
                let payload_value = payload.as_value();
                let Some(path) = payload
                    .file_path()
                    .map(|path| path.replace('\\', "/"))
                    .or_else(|| {
                        legacy_python_status_resource_path(resource_name, &payload_value, idx)
                    })
                else {
                    continue;
                };
                let Some(id) = payload.resource_id() else {
                    continue;
                };
                let name = payload.name().unwrap_or_default().to_string();
                let item = (id.to_string(), name);
                metadata.insert(path.clone(), item.clone());
                metadata
                    .entry(parse_multi_resource_path(&path).0)
                    .or_insert(item);
            }
        }
        Ok(metadata)
    }

    pub(crate) fn load_status_snapshot_resource_map(
        &self,
        root: &Path,
    ) -> Result<Option<ResourceMap>, CoreError> {
        let Some(snapshot) = self.load_status_snapshot(root)? else {
            return Ok(None);
        };

        let mut map = ResourceMap::new();
        for (resource_name, entries) in &snapshot.resources {
            for (idx, payload) in entries.values().enumerate() {
                let payload_value = payload.as_value();
                let Some(path) = payload
                    .file_path()
                    .map(|path| path.replace('\\', "/"))
                    .or_else(|| {
                        legacy_python_status_resource_path(resource_name, &payload_value, idx)
                    })
                else {
                    continue;
                };
                let (file_path, _) = parse_multi_resource_path(&path);
                let Some(content) =
                    legacy_python_status_resource_content(resource_name, &payload_value)
                else {
                    continue;
                };
                let resource_id = payload.resource_id().unwrap_or_default().to_string();
                let name = payload.name().unwrap_or_default().to_string();
                map.insert(
                    file_path.clone(),
                    Resource {
                        resource_id,
                        name,
                        file_path,
                        payload: serde_json::json!({ "content": content }),
                    },
                );
            }
        }

        if map.is_empty() {
            Ok(None)
        } else {
            Ok(Some(map))
        }
    }
}
