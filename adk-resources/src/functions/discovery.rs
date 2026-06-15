use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_dir, sorted_read_dir};
use crate::resource_utils::rel_under_root;
use std::path::Path;

// poly/resources/function.py
/// Validation parity: TODO(DEVP-319) audit Python Function.validate().
pub(crate) struct Function;
impl DiscoverResources for Function {
    const LOCAL_PATH: LocalResourcePath =
        LocalResourcePath::GlobSet(&["functions/*.py", "flows/*/functions/*.py"]);

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let flows = base_path.join("flows");
        if is_dir(fs, &flows)
            && let Some(flow_dirs) = sorted_read_dir(fs, &flows)
        {
            for flow_dir in flow_dirs {
                if !is_dir(fs, &flow_dir) {
                    continue;
                }
                let flow_functions = flow_dir.join("functions");
                if let Some(files) = sorted_read_dir(fs, &flow_functions) {
                    for f in files {
                        if f.extension().and_then(|e| e.to_str()) == Some("py") {
                            out.push(rel_under_root(base_path, &f));
                        }
                    }
                }
            }
        }
        let global_functions = base_path.join("functions");
        if is_dir(fs, &global_functions)
            && let Some(files) = sorted_read_dir(fs, &global_functions)
        {
            for f in files {
                if f.extension().and_then(|e| e.to_str()) == Some("py") {
                    out.push(rel_under_root(base_path, &f));
                }
            }
        }
        out
    }
}
