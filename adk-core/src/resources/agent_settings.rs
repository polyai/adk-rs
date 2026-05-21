use crate::discover::DiscoverResources;
use crate::resource_utils::rel_under_root;
use crate::resources::common::is_file;
use std::path::Path;

// poly/resources/agent_settings.py
pub(crate) struct SettingsPersonality;
impl DiscoverResources for SettingsPersonality {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/personality.yaml");
        if is_file(&p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

pub(crate) struct SettingsRole;
impl DiscoverResources for SettingsRole {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/role.yaml");
        if is_file(&p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

pub(crate) struct SettingsRules;
impl DiscoverResources for SettingsRules {
    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/rules.txt");
        if is_file(&p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}
