use crate::{
    CoreError, DiscoveredResourceChanges, DiscoveredResourcePaths, PROJECT_CONFIG_FILE,
    canonical_path_inside_root, is_generated_path, project_config_yaml, recursive_file_paths,
};
use adk_io::FileSystem;
use adk_resources::local_resource_content;
use adk_types::{ProjectConfig, Resource, ResourceMap};
use std::collections::HashSet;
use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/python_gen_template_files.rs"));

/// Local project workspace operations that do not require a platform client.
pub struct ProjectWorkspace<Fs> {
    pub(crate) fs: Fs,
}

impl<Fs: FileSystem> ProjectWorkspace<Fs> {
    pub fn with_file_system(fs: Fs) -> Self {
        Self { fs }
    }

    pub fn file_system(&self) -> &Fs {
        &self.fs
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

    pub fn write_python_gen_package(&self, project_root: &Path) -> Result<(), CoreError> {
        let gen_dir = project_root.join("_gen");
        self.fs.create_dir_all(&gen_dir)?;
        let mut cleaned_stubs = HashSet::new();
        let template_files: HashSet<&str> = PYTHON_GEN_TEMPLATE_FILES
            .iter()
            .map(|(file_name, _)| *file_name)
            .collect();
        for path in recursive_file_paths(&self.fs, &gen_dir)? {
            let rel = path
                .strip_prefix(&gen_dir)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            let is_generated_stub = path
                .extension()
                .is_some_and(|extension| matches!(extension.to_str(), Some("py" | "pyi")));
            if !is_generated_stub || template_files.contains(rel.as_str()) {
                continue;
            }

            // `_gen` itself may be a symlink to a user-chosen generated package
            // location, but child symlinks inside it must not let stale-stub
            // cleanup delete files outside that resolved package root. The
            // canonical path also deduplicates aliases from internal symlinks.
            let Some(canonical_path) = canonical_path_inside_root(&self.fs, &gen_dir, &path) else {
                continue;
            };
            if !cleaned_stubs.insert(canonical_path) {
                continue;
            }
            self.fs.remove_file(&path)?;
        }
        for (file_name, contents) in PYTHON_GEN_TEMPLATE_FILES {
            let path = gen_dir.join(file_name);
            if let Some(parent) = path.parent() {
                self.fs.create_dir_all(parent)?;
            }
            self.fs.write_string(&path, contents)?;
        }
        Ok(())
    }

    pub fn collect_local_resources(&self, root: &Path) -> Result<ResourceMap, CoreError> {
        collect_local_resources_from_fs(&self.fs, root)
    }

    /// Typed discovery matching Python `AgentStudioProject.discover_local_resources()`.
    pub fn discover_local_resources(&self, root: &Path) -> indexmap::IndexMap<String, Vec<String>> {
        adk_resources::discover_local_resources(&self.fs, root)
    }

    /// Typed parity helper matching Python `find_new_kept_deleted` semantics at path level.
    pub fn find_new_kept_deleted(
        &self,
        discovered_resources: &DiscoveredResourcePaths,
        existing_resources: &DiscoveredResourcePaths,
    ) -> DiscoveredResourceChanges {
        adk_resources::find_new_kept_deleted(discovered_resources, existing_resources)
    }

    pub fn detect_conflict_files(&self, root: &Path) -> Result<Vec<String>, CoreError> {
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
}

pub(crate) fn collect_local_resources_from_fs<Fs: FileSystem>(
    fs: &Fs,
    root: &Path,
) -> Result<ResourceMap, CoreError> {
    let mut map = ResourceMap::new();
    for file in recursive_file_paths(fs, root)? {
        let rel = file
            .strip_prefix(root)
            .unwrap_or(file.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        if rel == PROJECT_CONFIG_FILE || is_generated_path(&rel) {
            continue;
        }
        let content = fs.read_to_string(file.as_path()).unwrap_or_default();
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
