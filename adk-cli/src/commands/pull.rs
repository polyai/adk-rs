use crate::{
    AdkService, PlatformClient, PullArgs, emit_error, ensure_project_loaded,
    parse_optional_json_arg, pull_projection_into_path,
};
use adk_service::PullOutcome;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::ExitCode;

pub(crate) fn cmd_pull<C: PlatformClient>(service: &AdkService<C>, args: PullArgs) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let projection_json = match parse_optional_json_arg(args.from_projection.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            emit_error(args.json || args.output_json_projection, &error);
            return ExitCode::from(1);
        }
    };
    let path = PathBuf::from(&args.path);
    let mut output_projection = projection_json.clone();
    let pull_result = if let Some(projection) = &projection_json {
        pull_projection_into_path(path.as_path(), projection, args.force, args.format).map(
            |conflicts| PullOutcome {
                files_with_conflicts: conflicts,
                new_branch_name: None,
                new_branch_id: None,
            },
        )
    } else {
        if args.output_json_projection {
            match service.pull_projection_json() {
                Ok(projection) => output_projection = Some(projection),
                Err(error) => {
                    emit_error(args.json || args.output_json_projection, &error.to_string());
                    return ExitCode::from(1);
                }
            }
        }
        service
            .pull_detailed_with_format(path.as_path(), args.force, args.format)
            .map_err(|error| error.to_string())
    };
    match pull_result {
        Ok(outcome) => {
            let conflicts_empty = outcome.files_with_conflicts.is_empty();
            if args.json || args.output_json_projection {
                let mut payload = json!({
                    "success": conflicts_empty,
                    "files_with_conflicts": outcome.files_with_conflicts.clone(),
                });
                if let Some(new_branch_name) = outcome.new_branch_name {
                    payload["new_branch_name"] = Value::String(new_branch_name);
                }
                if let Some(new_branch_id) = outcome.new_branch_id {
                    payload["new_branch_id"] = Value::String(new_branch_id);
                }
                if args.output_json_projection {
                    payload["projection"] = output_projection.unwrap_or(serde_json::Value::Null);
                }
                println!("{}", payload);
            } else {
                crate::console::success("Pulled project.");
            }
            if conflicts_empty {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            emit_error(args.json || args.output_json_projection, &error);
            ExitCode::from(1)
        }
    }
}
