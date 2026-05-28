use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::is_file;
use crate::resource_utils::rel_under_root;
use std::path::Path;

// poly/resources/asr_settings.py
pub(crate) struct AsrSettings;
impl DiscoverResources for AsrSettings {
    const LOCAL_PATH: LocalResourcePath =
        LocalResourcePath::File(crate::specs::ASR_SETTINGS_FILE.file_path);

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let p = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if is_file(fs, &p) {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}
