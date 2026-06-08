use crate::{ReviewArgs, ReviewCommands};
use serde_json::json;
use std::process::ExitCode;

pub(crate) fn cmd_review(args: ReviewArgs) -> ExitCode {
    let json_mode = match &args.command {
        ReviewCommands::Create(create) => args.json || create.json,
        ReviewCommands::List(list) => args.json || list.json,
        ReviewCommands::Delete(delete) => args.json || delete.json,
    };
    emit_review_message(
        json_mode,
        "The review subcommand is not implemented in adk-rs yet.",
    );
    ExitCode::from(1)
}

pub fn emit_review_message(json_mode: bool, message: &str) {
    if json_mode {
        println!("{}", json!({"success": false, "message": message}));
    } else {
        crate::console::error(message);
    }
}
