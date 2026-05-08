//! Integration tests that **spawn the `poly` binary** against on-disk trees copied from Python
//! `poly/tests/test_projects/`. For **in-process** domain tests on the same fixtures (no
//! subprocess), see `adk-core/tests/project_fixture_test.rs`.
//!
//! No network: `POLY_ADK_KEY` is stripped so the CLI falls back to `InMemoryPlatformClient`.
//!
//! Fixture provenance: `adk/src/poly/tests/test_projects/` in the Python ADK repo.
//!
//! Each test documents related Python coverage (`poly/tests/project_test.py` or `cli_test.py`).
//! These are subprocess smoke tests; behavior parity lives in `adk-core/tests/project_fixture_test.rs`.

use std::path::PathBuf;
use std::process::Command;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn poly_bin() -> &'static str {
    env!("CARGO_BIN_EXE_poly")
}

/// Without `POLY_ADK_KEY`, `poly` uses `InMemoryPlatformClient` (see `service_for_path`). Tests
/// remove the key so behavior is deterministic in CI and on developer machines that export API
/// credentials.
fn poly_offline() -> Command {
    let mut cmd = Command::new(poly_bin());
    cmd.env_remove("POLY_ADK_KEY");
    cmd.env("POLY_ADK_ALLOW_INMEMORY_FALLBACK", "1");
    cmd
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
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dst = std::env::temp_dir().join(format!("adk-rs-e2e-format-{ts}"));
    copy_dir_recursive(&full_fixture_dir(), &dst).expect("copy fixture tree");
    dst
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(src_path, dst_path)?;
        }
    }
    Ok(())
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
    // In-memory remote is empty: every tracked file is "new".
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

    // The Python "empty" fixture still ships `empty_project.json` (snapshot / export). The
    // Rust scanner treats every file under the root as a resource except `project.yaml` and
    // `_gen/`, so we see exactly that file as "new" vs an empty in-memory remote.
    let new_files = payload
        .get("new_files")
        .and_then(|v| v.as_array())
        .expect("new_files array");
    assert_eq!(new_files.len(), 1);
    assert_eq!(
        new_files[0].as_str(),
        Some("empty_project.json"),
        "unexpected new_files: {new_files:?}"
    );
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

/// Related (CLI): `poly/tests/project_test.py` - `GetDiffsTest` (invokes `get_diffs`).
#[test]
fn diff_json_reports_changes_on_full_fixture() {
    let dir = full_fixture_dir();
    let output = poly_offline()
        .args(["diff", "--json", "--path"])
        .arg(&dir)
        .output()
        .expect("spawn poly diff");

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout must be JSON");
    assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
    let diffs = payload
        .get("diffs")
        .and_then(|v| v.as_object())
        .expect("diffs object");
    assert!(
        diffs.len() >= 40,
        "expected many file diffs vs empty remote, got {}",
        diffs.len()
    );
}

/// Related (CLI): basic `chat` JSON contract against offline in-memory client.
#[test]
fn chat_json_succeeds_on_full_fixture() {
    let dir = full_fixture_dir();
    let output = poly_offline()
        .args(["chat", "--json", "--path"])
        .arg(&dir)
        .args(["--message", "hello"])
        .output()
        .expect("spawn poly chat");

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout must be JSON");
    let conversation = payload
        .get("conversations")
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .expect("conversation entry");
    assert_eq!(
        conversation.get("conversation_id").and_then(|v| v.as_str()),
        Some("local-conversation")
    );
    let turns = conversation
        .get("turns")
        .and_then(|v| v.as_array())
        .expect("turns array");
    assert_eq!(turns.len(), 2);
    assert_eq!(
        turns[1].get("input").and_then(|v| v.as_str()),
        Some("hello")
    );
}
