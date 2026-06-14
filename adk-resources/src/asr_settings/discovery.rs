use crate::asr_settings::local::{AsrSettingsFile, parse_asr_settings};
use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{ParseLocalResource, ResourceParseErrors};
use crate::local_resources::is_file;
use crate::resource_utils::rel_under_root;
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/asr_settings.py
/// Validation parity: implemented against Python AsrSettings.validate().
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

    fn append_local_resource_errors(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        <Self as ParseLocalResource>::append_parse_errors(path, yaml, errors);
    }
}

#[cfg(test)]
pub(crate) fn append_parse_errors(path: &str, yaml: &Value, errors: &mut Vec<String>) {
    <AsrSettings as ParseLocalResource>::append_parse_errors(path, yaml, errors);
}

impl ParseLocalResource for AsrSettings {
    type Parsed = AsrSettingsFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> Result<Self::Parsed, ResourceParseErrors> {
        parse_asr_settings(path, yaml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    #[test]
    fn validates_python_asr_interaction_style_rule() {
        let yaml = from_str::<Value>("barge_in: false\ninteraction_style: warp\n")
            .expect("ASR settings YAML");
        let mut errors = Vec::new();

        append_parse_errors(
            "voice/speech_recognition/asr_settings.yaml",
            &yaml,
            &mut errors,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("unknown variant `warp`"))
        );
    }
}
