use crate::discover::DiscoverResources;
use crate::local_resources::is_file;
use crate::resource_utils::rel_under_root;
use std::path::Path;

// poly/resources/asr_settings.py
pub(crate) struct AsrSettings;
impl DiscoverResources for AsrSettings {
    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let p = base_path.join("voice/speech_recognition/asr_settings.yaml");
        if is_file(fs, &p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}
