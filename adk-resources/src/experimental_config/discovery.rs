use crate::discover::DiscoverResources;
use crate::local_resources::is_file;
use crate::resource_utils::rel_under_root;
use std::path::Path;

// poly/resources/experimental_config.py
pub(crate) struct ExperimentalConfig;
impl DiscoverResources for ExperimentalConfig {
    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/experimental_config.json");
        if is_file(fs, &p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}
