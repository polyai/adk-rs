use crate::discover::DiscoverResources;
use crate::discover::resource_utils::rel_under_root;
use crate::resources::common::{is_dir, sorted_read_dir};
use std::path::Path;

// poly/resources/function.py
pub(crate) struct Function;
impl DiscoverResources for Function {
    const TYPE_NAME: &'static str = "Function";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let flows = base_path.join("flows");
        if is_dir(&flows)
            && let Some(flow_dirs) = sorted_read_dir(&flows)
        {
            for flow_dir in flow_dirs {
                if !is_dir(&flow_dir) {
                    continue;
                }
                let flow_functions = flow_dir.join("functions");
                if let Some(files) = sorted_read_dir(&flow_functions) {
                    for f in files {
                        if f.extension().and_then(|e| e.to_str()) == Some("py") {
                            out.push(rel_under_root(base_path, &f));
                        }
                    }
                }
            }
        }
        let global_functions = base_path.join("functions");
        if is_dir(&global_functions)
            && let Some(files) = sorted_read_dir(&global_functions)
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
