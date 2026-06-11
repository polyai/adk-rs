use crate::discover::{DiscoverResources, LocalResourcePath};
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

    fn validate_local_yaml(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_local_yaml(path, yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(path: &str, yaml: &Value, errors: &mut Vec<String>) {
    let interaction_style = yaml
        .get("interaction_style")
        .and_then(Value::as_str)
        .unwrap_or("balanced");
    if !matches!(
        interaction_style,
        "balanced" | "precise" | "swift" | "sonic" | "turbo"
    ) {
        errors.push(format!(
            "Validation error in {path}: Invalid interaction_style '{interaction_style}'. Must be one of: balanced, precise, swift, sonic, turbo"
        ));
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

        validate_local_yaml(
            "voice/speech_recognition/asr_settings.yaml",
            &yaml,
            &mut errors,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("Invalid interaction_style 'warp'"))
        );
    }
}
