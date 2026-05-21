use crate::discover::DiscoverResources;
use crate::discover::resource_utils::rel_under_root;
use crate::resources::common::is_file;
use std::path::Path;

// poly/resources/safety_filters.py
pub(crate) struct GeneralSafetyFilters;
impl DiscoverResources for GeneralSafetyFilters {
    const TYPE_NAME: &'static str = "GeneralSafetyFilters";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/safety_filters.yaml");
        if is_file(&p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}
