use crate::{
    CoreError, DiscoveredResourceChanges, DiscoveredResourcePaths, PROJECT_CONFIG_FILE,
    is_generated_path, project_config_yaml, recursive_file_paths,
};
use adk_io::FileSystem;
use adk_resources::local_resource_content;
use adk_types::{ProjectConfig, Resource, ResourceMap};
use std::path::Path;

pub(crate) const PYTHON_GEN_TEMPLATE_FILES: &[(&str, &str)] = &[
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
