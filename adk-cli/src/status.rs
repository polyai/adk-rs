use crate::{ProjectWorkspace, StatusArgs, console, emit_error};
use serde_json::json;
use std::path::PathBuf;
use std::process::ExitCode;

pub(crate) fn cmd_status(workspace: &ProjectWorkspace, args: StatusArgs) -> ExitCode {
    if workspace
        .load_project_config(PathBuf::from(&args.path).as_path())
        .is_err()
    {
        emit_error(
            args.json,
            "No project configuration found. Run poly init to initialize a project.",
        );
        return ExitCode::from(1);
    }
    let root = PathBuf::from(&args.path);
    match workspace.status(root.as_path()) {
        Ok(summary) => {
            if args.json {
                println!(
                    "{}",
                    json!({
                        "files_with_conflicts": absolutize_status_paths(&root, summary.files_with_conflicts),
                        "modified_files": absolutize_status_paths(&root, summary.modified_files),
                        "new_files": absolutize_status_paths(&root, summary.new_files),
                        "deleted_files": absolutize_status_paths(&root, summary.deleted_files)
                    })
                );
            } else {
                print_status_summary(
                    &summary.files_with_conflicts,
                    &summary.modified_files,
                    &summary.new_files,
                    &summary.deleted_files,
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_error(args.json, &error.to_string());
            ExitCode::from(1)
        }
    }
}

fn print_status_summary(
    files_with_conflicts: &[String],
    modified_files: &[String],
    new_files: &[String],
    deleted_files: &[String],
) {
    let has_changes = !files_with_conflicts.is_empty()
        || !modified_files.is_empty()
        || !new_files.is_empty()
        || !deleted_files.is_empty();

    if !has_changes {
        console::plain("[muted]No changes detected.[/muted]");
        return;
    }

    print_status_group("Files with conflicts", files_with_conflicts);
    print_status_group("Modified files", modified_files);
    print_status_group("New files", new_files);
    print_status_group("Deleted files", deleted_files);
}

fn print_status_group(label: &str, files: &[String]) {
    if files.is_empty() {
        return;
    }
    let style = status_label_style(label);
    console::plain(format!("[label]{label}:[/label]"));
    for file in files {
        console::plain(format!("  - [{style}]{file}[/{style}]"));
    }
}

fn status_label_style(label: &str) -> &'static str {
    match label {
        "Files with conflicts" => "filename.conflict",
        "Modified files" => "filename.modified",
        "New files" => "filename.new",
        "Deleted files" => "filename.deleted",
        _ => "label",
    }
}

fn absolutize_status_paths(root: &std::path::Path, paths: Vec<String>) -> Vec<String> {
    let root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(root)
    };
    paths
        .into_iter()
        .map(|path| {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
            .to_string_lossy()
            .to_string()
        })
        .collect()
}
