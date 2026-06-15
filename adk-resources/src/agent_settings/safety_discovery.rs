use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{ParseLocalResource, ResourceParseResult};
use crate::local_resources::is_file;
use crate::resource_utils::rel_under_root;
use crate::safety_filters::{SafetyFilterMode, SafetyFilters, parse_safety_filters};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/safety_filters.py
/// Validation parity: implemented against Python GeneralSafetyFilters.validate().
pub(crate) struct GeneralSafetyFilters;
impl DiscoverResources for GeneralSafetyFilters {
    const LOCAL_PATH: LocalResourcePath =
        LocalResourcePath::File(crate::specs::AGENT_SAFETY_FILTERS_FILE.file_path);

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

impl ParseLocalResource for GeneralSafetyFilters {
    type Parsed = SafetyFilters;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_safety_filters(path, yaml, SafetyFilterMode::General)
    }
}
