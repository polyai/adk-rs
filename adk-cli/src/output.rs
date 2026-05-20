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
        println!("{}", json!({"success": false, "error": message}));
    } else {
        console::exception(message);
    }
}

pub(crate) fn clean_error_message(message: &str) -> &str {
    message
        .strip_prefix("invalid project data: ")
        .unwrap_or(message)
}
