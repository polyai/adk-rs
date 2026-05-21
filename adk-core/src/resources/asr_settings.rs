use crate::discover::DiscoverResources;
use crate::discover::resource_utils::rel_under_root;
use crate::resources::common::is_file;
use std::path::Path;

// poly/resources/asr_settings.py
pub(crate) struct AsrSettings;
impl DiscoverResources for AsrSettings {
    const TYPE_NAME: &'static str = "AsrSettings";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("voice/speech_recognition/asr_settings.yaml");
        if is_file(&p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}
