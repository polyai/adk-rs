use crate::{AdkService, RevertArgs, console, emit_error, ensure_project_loaded};
use adk_api_client::PlatformClient;
use serde_json::json;
use std::path::PathBuf;
use std::process::ExitCode;

pub(crate) fn cmd_revert<C: PlatformClient>(
    service: &AdkService<C>,
    args: RevertArgs,
) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let files: Vec<String> = args
        .files
        .iter()
        .map(|file| {
            let path = PathBuf::from(file);
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
            .to_string_lossy()
            .to_string()
        })
        .collect();
    match service.revert_changes(PathBuf::from(&args.path).as_path(), &files) {
        Ok(files_reverted) => {
            if args.json {
                println!(
                    "{}",
                    json!({"success": true, "files_reverted": files_reverted})
                );
            } else if files_reverted.is_empty() {
                console::plain("[muted]No changes to revert.[/muted]");
            } else {
                console::success("Changes reverted successfully.");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_error(args.json, &error.to_string());
            ExitCode::from(1)
        }
    }
}
