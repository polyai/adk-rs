use crate::{FormatArgs, console, emit_error, ensure_project_loaded, local_service};
use serde_json::json;
use std::path::PathBuf;
use std::process::{ExitCode, Stdio};
use std::time::{Duration, Instant};

pub(crate) fn cmd_format(args: FormatArgs) -> ExitCode {
    let service = local_service();
    if !ensure_project_loaded(&service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    match service.format_local_resources(
        PathBuf::from(&args.path).as_path(),
        &args.files,
        args.check,
    ) {
        Ok(changed_files) => {
            let format_success = !args.check || changed_files.is_empty();
            let ty_ran = args.ty && format_success;
            let (ty_returncode, ty_timed_out) = if ty_ran {
                run_ty_check(PathBuf::from(&args.path).as_path())
            } else {
                (None, false)
            };
            let success =
                format_success && !ty_timed_out && ty_returncode.is_none_or(|code| code == 0);
            if args.json {
                println!(
                    "{}",
                    json!({
                        "success": success,
                        "check_only": args.check,
                        "format_errors": [],
                        "affected": changed_files,
                        "ty_ran": ty_ran,
                        "ty_returncode": ty_returncode,
                        "ty_timed_out": ty_timed_out,
                    })
                );
            } else if args.check {
                if success {
                    console::success("Formatting check passed.");
                } else {
                    console::error("Formatting check failed.");
                }
            } else {
                console::success("Formatting completed.");
            }
            if ty_ran && ty_returncode.is_some_and(|code| code != 0) && !args.json {
                console::error("Type checking failed.");
            }
            if ty_timed_out && !args.json {
                console::error("Type checking timed out after 15s.");
            }
            if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            if args.json {
                println!(
                    "{}",
                    json!({
                        "success": false,
                        "check_only": args.check,
                        "format_errors": [error.to_string()],
                        "affected": [],
                        "ty_ran": false,
                        "ty_returncode": null,
                        "ty_timed_out": false,
                    })
                );
            } else {
                emit_error(false, &error.to_string());
            }
            ExitCode::from(1)
        }
    }
}

fn run_ty_check(path: &std::path::Path) -> (Option<i32>, bool) {
    let mut ty = std::process::Command::new("ty");
    ty.arg("check").current_dir(path);
    match output_with_timeout(&mut ty, Duration::from_secs(15)) {
        Ok((Some(output), false)) => (Some(output.status.code().unwrap_or(1)), false),
        Ok((None, true)) => (None, true),
        Ok(_) => (Some(1), false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Some(1), false),
        Err(_) => (Some(1), false),
    }
}

fn output_with_timeout(
    command: &mut std::process::Command,
    timeout: Duration,
) -> std::io::Result<(Option<std::process::Output>, bool)> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let start = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output().map(|output| (Some(output), false));
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Ok((None, true));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
