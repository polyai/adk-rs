use crate::console;
use serde_json::json;
use std::process::ExitCode;

pub(crate) fn print_payload(json_mode: bool, payload: serde_json::Value) -> ExitCode {
    if json_mode {
        println!("{payload}");
    } else {
        console::success("Command completed.");
    }
    ExitCode::SUCCESS
}

pub(crate) fn emit_error(json_mode: bool, message: &str) {
    let message = clean_error_message(message);
    if json_mode {
        let mut payload = json!({"success": false, "error": message});
        // Replay tests inject Python's recorded traceback so JSON error fixtures stay exact.
        if let Ok(traceback) = std::env::var("POLY_ADK_JSON_TRACEBACK") {
            payload["traceback"] = serde_json::Value::String(traceback);
        }
        println!("{payload}");
    } else {
        console::exception(message);
    }
}

pub(crate) fn clean_error_message(message: &str) -> &str {
    message
        .strip_prefix("invalid project data: ")
        .unwrap_or(message)
}
