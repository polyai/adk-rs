use crate::{
    CoreError, DiscoveredResourceChanges, DiscoveredResourcePaths, PROJECT_CONFIG_FILE,
    canonical_path_inside_root, is_generated_path, project_config_yaml, recursive_file_paths,
    remove_path_if_outside_root,
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
            // `_gen` itself may be symlinked elsewhere, but a template-named
            // child must not be a symlink escape: writing through it could
            // overwrite a user file outside the resolved generated package root.
            remove_path_if_outside_root(&self.fs, &gen_dir, &path)?;
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

#[cfg(test)]
#[allow(clippy::disallowed_methods, clippy::disallowed_types)]
mod tests {
    use super::*;
    use adk_io::StdFileSystem;

    #[cfg(unix)]
    struct TempProjectDir {
        path: std::path::PathBuf,
    }

    #[cfg(unix)]
    impl TempProjectDir {
        fn new(name: &str) -> Self {
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("adk-core-{name}-{}-{suffix}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create temp project dir");
            Self { path }
        }
    }

    #[cfg(unix)]
    impl Drop for TempProjectDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn python_gen_content(file_name: &str) -> String {
        PYTHON_GEN_TEMPLATE_FILES
            .iter()
            .find(|(template_file, _)| *template_file == file_name)
            .map(|(_, content)| (*content).to_string())
            .expect("python gen template")
    }

    #[cfg(unix)]
    #[test]
    fn write_python_gen_package_replaces_template_symlink_escape_before_write() {
        let temp = TempProjectDir::new("workspace-template-symlink");
        let root = temp.path.as_path();
        std::fs::create_dir_all(root.join("_gen")).expect("create _gen");
        std::fs::create_dir_all(root.join("functions")).expect("create functions");
        std::fs::write(root.join("functions/lookup.py"), "keep me\n").expect("write target");
        std::os::unix::fs::symlink("../functions/lookup.py", root.join("_gen/conversation.pyi"))
            .expect("symlink template path outside _gen");

        ProjectWorkspace::with_file_system(StdFileSystem)
            .write_python_gen_package(root)
            .expect("write python gen");

        assert_eq!(
            std::fs::read_to_string(root.join("functions/lookup.py")).expect("read target"),
            "keep me\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("_gen/conversation.pyi")).expect("read template"),
            python_gen_content("conversation.pyi")
        );
        assert!(
            !std::fs::symlink_metadata(root.join("_gen/conversation.pyi"))
                .expect("template metadata")
                .file_type()
                .is_symlink()
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_python_gen_package_replaces_template_parent_symlink_escape_before_write() {
        let temp = TempProjectDir::new("workspace-template-parent-symlink");
        let root = temp.path.as_path();
        std::fs::create_dir_all(root.join("_gen")).expect("create _gen");
        std::fs::create_dir_all(root.join("functions")).expect("create functions");
        std::fs::write(root.join("functions/integration.pyi"), "keep me\n").expect("write target");
        std::os::unix::fs::symlink("../functions", root.join("_gen/integrations"))
            .expect("symlink template parent outside _gen");

        ProjectWorkspace::with_file_system(StdFileSystem)
            .write_python_gen_package(root)
            .expect("write python gen");

        assert_eq!(
            std::fs::read_to_string(root.join("functions/integration.pyi")).expect("read target"),
            "keep me\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("_gen/integrations/integration.pyi"))
                .expect("read template"),
            python_gen_content("integrations/integration.pyi")
        );
        assert!(
            !std::fs::symlink_metadata(root.join("_gen/integrations"))
                .expect("template parent metadata")
                .file_type()
                .is_symlink()
        );
    }
}
