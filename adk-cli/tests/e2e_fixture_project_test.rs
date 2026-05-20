//! Integration tests that **spawn the `poly` binary** against on-disk trees copied from Python
//! `poly/tests/test_projects/`. For **in-process** domain tests on the same fixtures (no
//! subprocess), see `adk-core/tests/project_fixture_test.rs`.
//!
//! No network: real API credentials and base URLs are stripped from the spawned process.
//!
//! Fixture provenance: `adk/src/poly/tests/test_projects/` in the Python ADK repo.
//!
//! Each test documents related Python coverage (`poly/tests/project_test.py` or `cli_test.py`).
//! These are subprocess smoke tests; behavior parity lives in `adk-core/tests/project_fixture_test.rs`.

mod support;

use std::path::PathBuf;
use support::cli::{copy_dir_recursive, poly_offline_command, temp_dir};

/// Tests remove real credentials, keeping behavior deterministic in CI and on developer
/// machines that export API credentials.
fn poly_offline() -> std::process::Command {
    poly_offline_command()
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test_projects")
}

fn full_fixture_dir() -> PathBuf {
    fixtures_root().join("test_project")
}

fn empty_fixture_dir() -> PathBuf {
    fixtures_root().join("test_empty_project")
}

fn make_temp_copy_of_full_fixture() -> PathBuf {
    let dst = temp_dir("adk-rs-e2e-format");
    copy_dir_recursive(&full_fixture_dir(), &dst).expect("copy fixture tree");
    dst
}

/// Infrastructure: same on-disk trees as `poly/tests/project_test.py` (`TEST_DIR`, `EMPTY_PROJECT_DIR`).
/// Not a port of a single test; guards fixture sync (see `tests/fixtures/SYNC_FROM_PYTHON_ADK.txt`).
#[test]
fn fixture_trees_are_present() {
    assert!(
        full_fixture_dir().join("topics/topic_1.yaml").is_file(),
        "full fixture missing topics/topic_1.yaml; copy from Python poly/tests/test_projects"
    );
    assert!(
        full_fixture_dir().join("test_project.json").is_file(),
        "full fixture missing test_project.json"
    );
    assert!(
        empty_fixture_dir().join("project.yaml").is_file(),
        "empty fixture missing project.yaml"
    );
}

/// Related (CLI): `poly/tests/project_test.py` - `ProjectStatusTest` (exercises `project_status` / status summary).
/// See `adk-core` `status_with_empty_remote_marks_full_fixture_files_as_new` for in-process analog.
#[test]
fn status_json_succeeds_on_full_fixture() {
    let dir = full_fixture_dir();
    let output = poly_offline()
        .args(["status", "--json", "--path"])
        .arg(&dir)
        .output()
        .expect("spawn poly status");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout must be JSON");

    let new_files = payload
        .get("new_files")
        .and_then(|v| v.as_array())
        .expect("new_files array");
    // With no saved resource snapshot, every tracked file is "new".
    assert!(
        new_files.len() >= 40,
        "expected many new_files against empty remote, got {}",
        new_files.len()
    );

    let modified = payload
        .get("modified_files")
        .and_then(|v| v.as_array())
        .expect("modified_files array");
    let deleted = payload
        .get("deleted_files")
        .and_then(|v| v.as_array())
        .expect("deleted_files array");
    assert!(modified.is_empty(), "unexpected modified_files");
    assert!(deleted.is_empty(), "unexpected deleted_files");
}

/// Related (CLI): same empty tree as `poly/tests/project_test.py` - `DiscoverLocalResourcesTest.test_discover_local_resources_empty_project`.
#[test]
fn status_json_succeeds_on_empty_fixture() {
    let dir = empty_fixture_dir();
    let output = poly_offline()
        .args(["status", "--json", "--path"])
        .arg(&dir)
        .output()
        .expect("spawn poly status");

    assert_eq!(output.status.code(), Some(0));

    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout must be JSON");

    // Python status is local/snapshot based. With no saved resource snapshot, the empty fixture
    // has no typed ADK resources to classify as new.
    let new_files = payload
        .get("new_files")
        .and_then(|v| v.as_array())
        .expect("new_files array");
    assert!(new_files.is_empty(), "unexpected new_files: {new_files:?}");
    assert_eq!(
        payload
            .get("modified_files")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(0)
    );
    assert_eq!(
        payload
            .get("deleted_files")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(0)
    );
}

/// Related (CLI): `poly/tests/project_test.py` - `ValidateProjectTest.test_validate_project_valid` (happy path).
#[test]
fn validate_json_succeeds_on_full_fixture() {
    let dir = full_fixture_dir();
    let output = poly_offline()
        .args(["validate", "--json", "--path"])
        .arg(&dir)
        .output()
        .expect("spawn poly validate");

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout must be JSON");
    assert_eq!(payload.get("valid").and_then(|v| v.as_bool()), Some(true));
}

/// Related (CLI): `poly/tests/cli_test.py` - `FormatCommandTest` / `poly format --path <project>`.
#[test]
fn format_json_succeeds_on_full_fixture() {
    let dir = make_temp_copy_of_full_fixture();
    let output = poly_offline()
        .args(["format", "--json", "--path"])
        .arg(&dir)
        .output()
        .expect("spawn poly format");

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout must be JSON");
    assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
}
