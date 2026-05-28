use crate::discover::DiscoverResources;
use crate::local_resources::is_file;
use crate::resource_utils::rel_under_root;
use std::path::Path;

// poly/resources/agent_settings.py
pub(crate) struct SettingsPersonality;
impl DiscoverResources for SettingsPersonality {
    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/personality.yaml");
        if is_file(fs, &p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

pub(crate) struct SettingsRole;
impl DiscoverResources for SettingsRole {
    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/role.yaml");
        if is_file(fs, &p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

pub(crate) struct SettingsRules;
impl DiscoverResources for SettingsRules {
    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/rules.txt");
        if is_file(fs, &p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}
