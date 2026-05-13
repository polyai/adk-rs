//! Mirrors `poly/resources/resource_utils.py` helpers used by resource discovery.

use fancy_regex::Regex as FancyRegex;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static NON_LETTER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^\w\s]").expect("valid regex"));
static SPACES_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("valid regex"));
static MULTI_UNDERSCORE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"_+").expect("valid"));

/// Same behavior as `resource_utils.clean_name` in Python.
pub fn clean_name(name: &str, lowercase: bool) -> String {
    let mut name = if lowercase {
        name.to_lowercase()
    } else {
        name.to_string()
    };
    name = NON_LETTER_REGEX.replace_all(&name, " ").to_string();
    name = SPACES_REGEX.replace_all(&name, "_").to_string();
    name = MULTI_UNDERSCORE_REGEX.replace_all(&name, "_").to_string();
    name.trim_matches('_').to_string()
}

/// Same as Python `CONV_STATE_DOT_NAME`; uses `fancy-regex` for look-around.
static CONV_STATE_DOT_NAME: LazyLock<FancyRegex> = LazyLock::new(|| {
    FancyRegex::new(r"(?<![\w.])conv\.state\.([a-zA-Z_][a-zA-Z0-9_]*)\b(?!\s*\()")
        .expect("valid regex")
});

/// Same as `resource_utils.remove_comments_from_code`.
pub fn remove_comments_from_code(code: &str) -> String {
    code.lines()
        .filter_map(|line| line.split('#').next().map(str::trim))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Same as `resource_utils.extract_variable_names_from_code`.
pub fn extract_variable_names_from_code(code: &str) -> Vec<String> {
    if code.is_empty() {
        return Vec::new();
    }
    let cleaned = remove_comments_from_code(code);
    let mut names: Vec<String> = Vec::new();
    for cap in CONV_STATE_DOT_NAME.captures_iter(&cleaned) {
        let Ok(cap) = cap else { continue };
        if let Some(m) = cap.get(1) {
            names.push(m.as_str().to_string());
        }
    }
    names.sort_unstable();
    names.dedup();
    names
}

/// Normalize a path under `root` to a `/`-separated relative string (stable keys).
pub fn rel_under_root(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn join_under_root(root: &Path, segments: &[&str]) -> PathBuf {
    let mut p = root.to_path_buf();
    for s in segments {
        p.push(s);
    }
    p
}
