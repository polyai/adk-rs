use crate::discover::DiscoverResources;
use crate::local_resources::is_file;
use crate::resource_utils::rel_under_root;
use std::path::Path;

// poly/resources/safety_filters.py
pub(crate) struct GeneralSafetyFilters;
impl DiscoverResources for GeneralSafetyFilters {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/safety_filters.yaml");
        if is_file(&p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}
