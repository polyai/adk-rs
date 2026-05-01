use std::process::Command;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn run_rust(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_poly"))
        .args(args)
        .output()
        .expect("run rust cli")
}

fn run_python_poly(args: &[&str]) -> Option<std::process::Output> {
    let poly = "/home/ben/adk/.venv/bin/poly";
    if !std::path::Path::new(poly).exists() {
        return None;
    }
    let output = Command::new(poly)
        .args(args)
        .current_dir("/home/ben/adk")
        .output()
        .ok()?;
    Some(output)
}

fn make_temp_project_dir() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("adk-rs-parity-{ts}"));
    fs::create_dir_all(&dir).expect("mkdir");
    fs::write(
        dir.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write config");
    dir.to_string_lossy().to_string()
}

#[test]
fn parity_missing_project_error_against_python() {
    let Some(py) = run_python_poly(&["status", "--json", "--path", "/tmp"]) else {
        eprintln!("Skipping parity test: `poly` executable not found in PATH");
        return;
    };
    let rs = run_rust(&["status", "--json", "--path", "/tmp"]);

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
