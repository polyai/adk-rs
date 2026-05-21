use adk_io::{FileSystem, StdFileSystem};
use serde_yaml::Value;
use std::path::{Path, PathBuf};

pub(crate) fn read_yaml_mapping(path: &Path) -> Option<serde_yaml::Mapping> {
    let raw = StdFileSystem.read_to_string(path).ok()?;
    let v: Value = serde_yaml::from_str(&raw).ok()?;
    match v {
        Value::Mapping(m) => Some(m),
        _ => None,
    }
}

pub(crate) fn sorted_read_dir(dir: &Path) -> Option<Vec<PathBuf>> {
    StdFileSystem.read_dir(dir).ok()
}

pub(crate) fn is_file(path: impl AsRef<Path>) -> bool {
    StdFileSystem.is_file(path.as_ref())
}

pub(crate) fn is_dir(path: impl AsRef<Path>) -> bool {
    StdFileSystem.is_dir(path.as_ref())
}
