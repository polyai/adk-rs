//! Direct Python ADK vs Rust CLI parity checks for small local command contracts.
//!
//! These tests spawn both CLIs directly. They do not record HTTP traffic and do
//! not replay the saved httpmock cassettes.

mod support;

use std::process::Command;
use support::cli::{make_temp_project_dir as support_temp_project_dir, run_poly_offline};
use support::python_recordings::python_adk_bin;

fn run_python_poly(args: &[&str]) -> Option<std::process::Output> {
    let mut command = Command::new(python_adk_bin());
    if let Ok(cwd) = std::env::var("PYTHON_ADK_CWD") {
        command.current_dir(cwd);
    }
    let output = command.args(args).output().ok()?;
    Some(output)
}

fn run_rust(args: &[&str]) -> std::process::Output {
    run_poly_offline(args)
}

fn make_temp_project_dir() -> String {
    support_temp_project_dir("adk-rs-parity")
}

#[test]
fn parity_missing_project_error_against_python() {
    let missing_project_path = std::env::temp_dir().to_string_lossy().to_string();
    let Some(py) = run_python_poly(&["status", "--json", "--path", missing_project_path.as_str()])
    else {
        eprintln!("Skipping parity test: Python ADK CLI unavailable");
        return;
    };
    let rs = run_rust(&["status", "--json", "--path", missing_project_path.as_str()]);

    assert_eq!(rs.status.code(), py.status.code());
    let py_json: serde_json::Value = serde_json::from_slice(&py.stdout).expect("python json");
    let rs_json: serde_json::Value = serde_json::from_slice(&rs.stdout).expect("rust json");
    assert_eq!(rs_json.get("success"), py_json.get("success"));
    assert_eq!(
        rs_json.get("error").is_some(),
        py_json.get("error").is_some()
    );
}

#[test]
fn parity_invalid_subcommand_exit_code() {
    let Some(py) = run_python_poly(&["not-a-command"]) else {
        eprintln!("Skipping parity test: python CLI unavailable");
        return;
    };
    let rs = run_rust(&["not-a-command"]);
    assert_eq!(rs.status.code(), py.status.code());
}

#[test]
fn parity_version_flags_match_python() {
    for flag in ["-v", "--version"] {
        let Some(py) = run_python_poly(&[flag]) else {
            eprintln!("Skipping parity test: python CLI unavailable");
            return;
        };
        let rs = run_rust(&[flag]);
        assert_eq!(rs.status.code(), py.status.code());
        assert_eq!(
            String::from_utf8_lossy(&rs.stdout).trim(),
            String::from_utf8_lossy(&py.stdout).trim()
        );
    }

    let Some(py) = run_python_poly(&["-V"]) else {
        eprintln!("Skipping parity test: python CLI unavailable");
        return;
    };
    let rs = run_rust(&["-V"]);
    assert_eq!(rs.status.code(), py.status.code());
}

#[test]
fn parity_diff_hash_before_after_nonfatal_json() {
    let project_dir = make_temp_project_dir();
    let Some(py) = run_python_poly(&[
        "diff",
        "--json",
        "--path",
        &project_dir,
        "abc123",
        "--before",
        "main",
    ]) else {
        eprintln!("Skipping parity test: python CLI unavailable");
        return;
    };
    let rs = run_rust(&[
        "diff",
        "--json",
        "--path",
        &project_dir,
        "abc123",
        "--before",
        "main",
    ]);
    assert_eq!(rs.status.code(), py.status.code());
    let py_err = String::from_utf8_lossy(&py.stderr);
    let rs_err = String::from_utf8_lossy(&rs.stderr);
    assert_eq!(
        rs_err.contains("Cannot specify both hash and before/after versions."),
        py_err.contains("Cannot specify both hash and before/after versions.")
    );
}

#[test]
fn parity_completion_contains_poly_and_adk() {
    let Some(py) = run_python_poly(&["completion", "bash"]) else {
        eprintln!("Skipping parity test: python CLI unavailable");
        return;
    };
    let rs = run_rust(&["completion", "bash"]);
    assert_eq!(rs.status.code(), py.status.code());
    let py_out = String::from_utf8_lossy(&py.stdout);
    let rs_out = String::from_utf8_lossy(&rs.stdout);
    for token in ["poly", "adk"] {
        assert_eq!(rs_out.contains(token), py_out.contains(token));
    }
}
