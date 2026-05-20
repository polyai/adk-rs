use crate::{ValidateArgs, console, emit_error, ensure_project_loaded, local_service};
use serde_json::json;
use std::path::PathBuf;
use std::process::ExitCode;

pub(crate) fn cmd_validate(args: ValidateArgs) -> ExitCode {
    let service = local_service();
    if !ensure_project_loaded(&service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    match service.validate_local_resources(PathBuf::from(args.path).as_path()) {
        Ok(errors) => {
            let valid = errors.is_empty();
            if args.json {
                println!("{}", json!({"valid": valid, "errors": errors}));
            } else if valid {
                console::success("Project configuration is valid.");
            } else {
                for e in &errors {
                    console::plain_stderr(e);
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_error(args.json, &error.to_string());
            ExitCode::from(1)
        }
    }
}
