use crate::discover::DiscoverResources;
use crate::discover::resource_utils::{
    extract_variable_names_from_code, join_under_root, rel_under_root,
};
use crate::resources::common::{is_dir, sorted_read_dir};
use adk_io::{FileSystem, StdFileSystem};
use std::path::{Path, PathBuf};

// poly/resources/variable.py
pub(crate) struct Variable;
impl DiscoverResources for Variable {
    const TYPE_NAME: &'static str = "Variable";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let mut function_files: Vec<PathBuf> = Vec::new();
        let global_functions = base_path.join("functions");
        if is_dir(&global_functions)
            && let Some(files) = sorted_read_dir(&global_functions)
        {
            for f in files {
                if f.extension().and_then(|e| e.to_str()) == Some("py") {
                    function_files.push(f);
                }
            }
        }
        let flows_path = base_path.join("flows");
        if is_dir(&flows_path)
            && let Some(flow_dirs) = sorted_read_dir(&flows_path)
        {
            for flow_dir in flow_dirs {
                if !is_dir(&flow_dir) {
                    continue;
                }
                for sub in ["functions", "function_steps"] {
                    let d = flow_dir.join(sub);
                    if let Some(files) = sorted_read_dir(&d) {
                        for f in files {
                            if f.extension().and_then(|e| e.to_str()) == Some("py") {
                                function_files.push(f);
                            }
                        }
                    }
                }
            }
        }
        if function_files.is_empty() {
            return vec![];
        }
        let mut names = std::collections::HashSet::new();
        for function_file in function_files {
            let Ok(code) = StdFileSystem.read_to_string(&function_file) else {
                continue;
            };
            for v in extract_variable_names_from_code(&code) {
                names.insert(v);
            }
        }
        let mut out: Vec<String> = names
            .into_iter()
            .map(|n| rel_under_root(base_path, &join_under_root(base_path, &["variables", &n])))
            .collect();
        out.sort_unstable();
        out
    }
}
