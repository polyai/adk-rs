use crate::{
    AdkService, DiffArgs, console, emit_error, ensure_project_loaded, normalize_cli_file_args,
};
use adk_api_client::PlatformClient;
use serde_json::json;
use std::path::PathBuf;
use std::process::ExitCode;

pub(crate) fn cmd_diff<C: PlatformClient>(service: &AdkService<C>, args: DiffArgs) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    if args.hash.is_some() && (args.before.is_some() || args.after.is_some()) {
        console::error("Cannot specify both hash and before/after versions.");
        return ExitCode::SUCCESS;
    }
    let named_diff = args.hash.is_some() || args.before.is_some() || args.after.is_some();
    let before_main_local =
        args.before.as_deref() == Some("main") && args.hash.is_none() && args.after.is_none();
    let after = args.hash.or(args.after);
    let root = PathBuf::from(args.path);
    let files = normalize_cli_file_args(root.as_path(), &args.files);
    match service.diff(root.as_path(), &files, args.before, after) {
        Ok(diffs) => {
            if diffs.is_empty() {
                if args.json {
                    let message = if before_main_local {
                        "Failed to compute diffs."
                    } else {
                        "No changes detected"
                    };
                    println!("{}", json!({"success": false, "message": message}));
                } else {
                    console::plain("[muted]No changes detected.[/muted]");
                }
                return ExitCode::SUCCESS;
            }
            if args.json {
                println!("{}", json!({"success": true, "diffs": diffs}));
            } else {
                for (path, diff) in diffs {
                    console::plain(format!("[label]=== {path} ===[/label]\n{diff}"));
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if args.json && named_diff {
                println!(
                    "{}",
                    json!({"success": false, "message": "Failed to compute diffs."})
                );
                ExitCode::SUCCESS
            } else {
                emit_error(args.json, &error.to_string());
                ExitCode::from(1)
            }
        }
    }
}
