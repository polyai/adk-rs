use crate::discover::DiscoverResources;
use crate::resource_utils::rel_under_root;
use crate::resources::common::{is_dir, is_file, read_yaml_mapping, sorted_read_dir};
use std::path::Path;

fn flow_start_step_name(flow_dir: &Path) -> Option<String> {
    let yaml = read_yaml_mapping(&flow_dir.join("flow_config.yaml"))?;
    yaml.get("start_step")
        .and_then(|value| value.as_str())
        .map(|value| value.strip_prefix("STEP-").unwrap_or(value).to_string())
}

fn step_file_stem(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(ToString::to_string)
}

// poly/resources/flows.py
pub(crate) struct FlowStep;
impl DiscoverResources for FlowStep {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let flows_path = base_path.join("flows");
        if !is_dir(&flows_path) {
            return vec![];
        }
        let mut out = Vec::new();
        if let Some(flow_dirs) = sorted_read_dir(&flows_path) {
            for flow_dir in flow_dirs {
                if !is_dir(&flow_dir) {
                    continue;
                }
                let steps_path = flow_dir.join("steps");
                if let Some(files) = sorted_read_dir(&steps_path) {
                    let start_step = flow_start_step_name(&flow_dir);
                    let mut step_files = files
                        .into_iter()
                        .filter(|f| f.extension().and_then(|e| e.to_str()) == Some("yaml"))
                        .collect::<Vec<_>>();
                    step_files.sort_by(|left, right| {
                        let left_is_start = step_file_stem(left)
                            .as_deref()
                            .is_some_and(|name| Some(name) == start_step.as_deref());
                        let right_is_start = step_file_stem(right)
                            .as_deref()
                            .is_some_and(|name| Some(name) == start_step.as_deref());
                        right_is_start
                            .cmp(&left_is_start)
                            .then_with(|| left.cmp(right))
                    });
                    for f in step_files {
                        out.push(rel_under_root(base_path, &f));
                    }
                }
            }
        }
        out
    }
}

pub(crate) struct FunctionStep;
impl DiscoverResources for FunctionStep {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let flows_path = base_path.join("flows");
        if !is_dir(&flows_path) {
            return vec![];
        }
        let mut out = Vec::new();
        if let Some(flow_dirs) = sorted_read_dir(&flows_path) {
            for flow_dir in flow_dirs {
                if !is_dir(&flow_dir) {
                    continue;
                }
                let function_steps_path = flow_dir.join("function_steps");
                if let Some(files) = sorted_read_dir(&function_steps_path) {
                    for f in files {
                        if f.extension().and_then(|e| e.to_str()) == Some("py") {
                            out.push(rel_under_root(base_path, &f));
                        }
                    }
                }
            }
        }
        out
    }
}

pub(crate) struct FlowConfig;
impl DiscoverResources for FlowConfig {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let flows_path = base_path.join("flows");
        if !is_dir(&flows_path) {
            return vec![];
        }
        let mut out = Vec::new();
        if let Some(flow_dirs) = sorted_read_dir(&flows_path) {
            for flow_dir in flow_dirs {
                if !is_dir(&flow_dir) {
                    continue;
                }
                let cfg = flow_dir.join("flow_config.yaml");
                if is_file(&cfg) {
                    out.push(rel_under_root(base_path, &cfg));
                }
            }
        }
        out
    }
}
