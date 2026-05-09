//! Cheap checks for the saved Python ADK recording fixtures.
//!
//! This test does not run the Rust CLI against the recordings. The ignored
//! `record_python_adk_httpmock_fixtures_test` target regenerates them; the
//! `replay_python_adk_httpmock_fixtures_test` target exercises Rust against the
//! saved cassettes.

mod support;

use serde_yaml::Value;
use std::fs;
use std::path::Path;
use support::python_recordings::{SCENARIOS, fixture_dir};

#[test]
fn saved_python_adk_recording_fixtures_are_complete_and_portable() {
    let fixture_dir = fixture_dir();
    for scenario in SCENARIOS {
        let commands_path = fixture_dir.join(format!("{scenario}.commands.yaml"));
        let httpmock_path = fixture_dir.join(format!("{scenario}.httpmock.yaml"));
        assert!(
            commands_path.exists(),
            "missing command manifest for {scenario}: {}",
            commands_path.display()
        );
        assert!(
            httpmock_path.exists(),
            "missing httpmock cassette for {scenario}: {}",
            httpmock_path.display()
        );

        let manifest_text = fs::read_to_string(&commands_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", commands_path.display()));
        assert_portable_fixture_text(&commands_path, &manifest_text);

        let manifest: Value = serde_yaml::from_str(&manifest_text)
            .unwrap_or_else(|error| panic!("parse {}: {error}", commands_path.display()));
        let expected_httpmock = format!("{scenario}.httpmock.yaml");
        assert_eq!(
            manifest.get("httpmock_recording").and_then(Value::as_str),
            Some(expected_httpmock.as_str()),
            "manifest points at the wrong cassette: {}",
            commands_path.display()
        );
        assert!(
            manifest
                .get("workflows")
                .and_then(Value::as_sequence)
                .is_some_and(|items| !items.is_empty()),
            "manifest has no workflows: {}",
            commands_path.display()
        );

        let cassette_text = fs::read_to_string(&httpmock_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", httpmock_path.display()));
        assert!(
            !cassette_text.contains("X-API-KEY") && !cassette_text.contains("x-api-key"),
            "cassette appears to contain an API key header: {}",
            httpmock_path.display()
        );
    }
}

fn assert_portable_fixture_text(path: &Path, text: &str) {
    for forbidden in ["/home/", "/Users/", "/tmp/", ".venv"] {
        assert!(
            !text.contains(forbidden),
            "fixture contains machine-specific text {forbidden:?}: {}",
            path.display()
        );
    }
}
