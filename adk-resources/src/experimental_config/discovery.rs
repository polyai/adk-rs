use crate::discover::DiscoverResources;
use crate::local_resource_common::is_file;
use crate::resource_utils::rel_under_root;
use std::path::Path;

// poly/resources/experimental_config.py
pub(crate) struct ExperimentalConfig;
impl DiscoverResources for ExperimentalConfig {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/experimental_config.json");
        if is_file(&p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}
