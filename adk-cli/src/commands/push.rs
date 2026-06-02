use crate::{
    AdkService, PushArgs, console, emit_error, ensure_project_loaded, parse_optional_json_arg,
};
use adk_api_client::PlatformClient;
use serde_json::json;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn validate_inline_projection_arg(args: &PushArgs) -> Option<String> {
    let raw = args.from_projection.as_deref()?;
    if raw.trim() == "-" {
        return None;
    }
    parse_optional_json_arg(Some(raw)).err()
}

pub(crate) fn cmd_push<C: PlatformClient>(service: &AdkService<C>, args: PushArgs) -> ExitCode {
    let json_mode = args.json || args.output_json_commands;
    if !ensure_project_loaded(service, &args.path, json_mode) {
        return ExitCode::from(1);
    }
    let projection_json = match parse_optional_json_arg(args.from_projection.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            emit_error(json_mode, &error);
            return ExitCode::from(1);
        }
    };
    let path = PathBuf::from(&args.path);
    if args.format
        && let Err(error) = service.format_local_resources(path.as_path(), &[], false)
    {
        emit_error(json_mode, &error.to_string());
        return ExitCode::from(1);
    }
    let current_branch = match service.current_branch(path.as_path()) {
        Ok(branch) => branch,
        Err(error) => {
            emit_error(json_mode, &error.to_string());
            return ExitCode::from(1);
        }
    };
    if current_branch == "main" && !args.dry_run && projection_json.is_none() {
        let branch_name = generated_adk_branch_name();
        match service.push_main_to_new_branch(
            path.as_path(),
            &branch_name,
            args.force,
            args.skip_validation,
        ) {
            Ok((cfg, push_result)) => {
                if args.json || args.output_json_commands {
                    let mut payload = json!({
                        "success": push_result.success,
                        "message": push_result.message,
                        "dry_run": false,
                    });
                    if args.output_json_commands {
                        payload["commands"] = json!(push_result.commands);
                    }
                    if push_result.success {
                        payload["new_branch_id"] = json!(cfg.branch_id);
                        payload["switched_to"] = json!(branch_name);
                    }
                    println!("{payload}");
                } else if push_result.success {
                    console::success("Push successful.");
                } else {
                    console::error(&push_result.message);
                }
                return if push_result.success {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(1)
                };
            }
            Err(error) => {
                emit_error(json_mode, &error.to_string());
                return ExitCode::from(1);
            }
        }
    }
    match service.push_with_options(
        path.as_path(),
        args.force,
        args.skip_validation,
        args.dry_run,
        projection_json.as_ref(),
    ) {
        Ok(push_result) => {
            if args.json || args.output_json_commands {
                let mut payload = json!({
                    "success": push_result.success,
                    "message": push_result.message,
                    "dry_run": args.dry_run,
                });
                if args.output_json_commands {
                    payload["commands"] = json!(push_result.commands);
                }
                println!("{}", payload);
            } else if push_result.success {
                console::success("Push successful.");
            } else {
                console::error(&push_result.message);
            }
            if push_result.success {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            emit_error(args.json || args.output_json_commands, &error.to_string());
            ExitCode::from(1)
        }
    }
}

fn generated_adk_branch_name() -> String {
    // Replay tests set this so Rust emits the branch name recorded from Python.
    if let Ok(name) = std::env::var("POLY_ADK_GENERATED_BRANCH_NAME")
        && !name.trim().is_empty()
    {
        return name;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    let suffix = format!("{:09x}", nanos & 0xfffffffff);
    format!("ADK-{}-{}", &suffix[..5], &suffix[5..9])
}
