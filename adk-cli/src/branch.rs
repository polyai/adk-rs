use crate::{
    AdkService, BranchArgs, BranchCommands, BranchCreateArgs, BranchDeleteArgs, BranchMergeArgs,
    BranchSwitchArgs, CommonPathArgs, ProjectWorkspace, allow_inmemory_fallback,
    clean_error_message, console, emit_error, emit_inmemory_fallback_warning,
    emit_remote_service_error, ensure_project_loaded, http_service_for_path, local_service,
    parse_optional_json_arg, print_payload, prompt_branch_switch, prompt_confirm,
    prompt_multi_select, prompt_select, pull_projection_into_path, read_stdin_line,
    should_warn_inmemory_fallback,
};
use adk_api_client::PlatformClient;
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

macro_rules! with_remote_service {
    ($workspace:expr, $path:expr, $json_mode:expr, |$service:ident| $body:expr) => {{
        match http_service_for_path($workspace, $path) {
            Ok($service) => $body,
            Err(error) if allow_inmemory_fallback() => {
                if should_warn_inmemory_fallback(&error) {
                    emit_inmemory_fallback_warning($json_mode, &error);
                }
                let $service = local_service();
                $body
            }
            Err(error) => {
                emit_remote_service_error($json_mode, &error);
                ExitCode::from(1)
            }
        }
    }};
}

pub(crate) fn cmd_branch(args: BranchArgs) -> ExitCode {
    let workspace = ProjectWorkspace::new();
    match args.command {
        BranchCommands::List(a) => with_remote_service!(&workspace, &a.path, a.json, |service| {
            cmd_branch_list_with_service(&service, a)
        }),
        BranchCommands::Create(a) => with_remote_service!(&workspace, &a.path, a.json, |service| {
            cmd_branch_create_with_service(&service, a)
        }),
        BranchCommands::Switch(a) => {
            let projection_json = match parse_optional_json_arg(a.from_projection.as_deref()) {
                Ok(value) => value,
                Err(error) => {
                    emit_error(a.json || a.output_json_projection, &error);
                    return ExitCode::from(1);
                }
            };
            if projection_json.is_some() {
                let service = local_service();
                cmd_branch_switch_with_service(&service, a, projection_json)
            } else {
                with_remote_service!(
                    &workspace,
                    &a.path,
                    a.json || a.output_json_projection,
                    |service| cmd_branch_switch_with_service(&service, a, projection_json)
                )
            }
        }
        BranchCommands::Current(a) => with_remote_service!(&workspace, &a.path, a.json, |service| {
            cmd_branch_current_with_service(&service, a)
        }),
        BranchCommands::Delete(a) => with_remote_service!(&workspace, &a.path, a.json, |service| {
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            cmd_branch_delete(&service, a)
        }),
        BranchCommands::Merge(a) => with_remote_service!(&workspace, &a.path, a.json, |service| {
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            cmd_branch_merge(&service, a)
        }),
    }
}

fn cmd_branch_list_with_service<C: PlatformClient>(
    service: &AdkService<C>,
    a: CommonPathArgs,
) -> ExitCode {
    if !ensure_project_loaded(service, &a.path, a.json) {
        return ExitCode::from(1);
    }
    match (
        service.current_branch_name_optional(PathBuf::from(&a.path).as_path()),
        service.list_branch_map(PathBuf::from(&a.path).as_path()),
    ) {
        (Ok(current_branch), Ok(branches)) => {
            if a.json {
                println!(
                    "{}",
                    json!({"current_branch": current_branch, "branches": branches})
                );
                ExitCode::SUCCESS
            } else {
                print_branch_list(current_branch.as_deref(), branches.iter());
                if current_branch.is_none() {
                    console::warning(
                        "Current local branch does not exist in Agent Studio. It may have been deleted or merged.",
                    );
                }
                ExitCode::SUCCESS
            }
        }
        (Err(error), _) => {
            emit_error(a.json, &error.to_string());
            ExitCode::from(1)
        }
        (_, Err(error)) => {
            emit_error(a.json, &error.to_string());
            ExitCode::from(1)
        }
    }
}

fn cmd_branch_create_with_service<C: PlatformClient>(
    service: &AdkService<C>,
    a: BranchCreateArgs,
) -> ExitCode {
    if !ensure_project_loaded(service, &a.path, a.json) {
        return ExitCode::from(1);
    }
    let branch_name_from_prompt;
    let branch_name = match a.branch_name.as_deref() {
        Some(branch_name) => branch_name,
        None if a.json => {
            emit_error(
                true,
                "branch create with --json requires a branch name argument.",
            );
            return ExitCode::from(1);
        }
        None => {
            let _ = console::prompt("Enter the name of the new branch: ");
            let _ = std::io::stdout().flush();
            branch_name_from_prompt = read_stdin_line().trim().to_string();
            if branch_name_from_prompt.is_empty() {
                emit_error(false, "branch create requires a branch name argument.");
                return ExitCode::from(1);
            }
            branch_name_from_prompt.as_str()
        }
    };
    let path = PathBuf::from(&a.path);
    if matches!(a.environment.as_deref(), Some("pre-release" | "live")) {
        if !a.force {
            match service.diff(path.as_path(), &[], None, None) {
                Ok(diffs) if !diffs.is_empty() => {
                    let changed_files = diffs
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(", ");
                    emit_error(
                        a.json,
                        &format!("Uncommitted changes on main branch: {changed_files}"),
                    );
                    return ExitCode::from(1);
                }
                Err(error) => {
                    emit_error(a.json, &error.to_string());
                    return ExitCode::from(1);
                }
                _ => {}
            }
        }
        if let Some(env_name) = a.environment.as_deref()
            && let Err(error) = service.pull_named(path.as_path(), env_name, true)
        {
            emit_error(a.json, &error.to_string());
            return ExitCode::from(1);
        }
    }
    match service.create_branch(path.as_path(), branch_name) {
        Ok(cfg) if matches!(a.environment.as_deref(), Some("pre-release" | "live")) => {
            match service.push(path.as_path(), true, true, false) {
                Ok(push) if push.success || push.message == "No changes detected" => print_payload(
                    a.json,
                    json!({"success": true, "branch_name": branch_name, "new_branch_id": cfg.branch_id}),
                ),
                Ok(push) => {
                    emit_error(a.json, &push.message);
                    ExitCode::from(1)
                }
                Err(error) => {
                    emit_error(a.json, &error.to_string());
                    ExitCode::from(1)
                }
            }
        }
        Ok(cfg) => {
            if a.json {
                print_payload(
                    true,
                    json!({"success": true, "branch_name": branch_name, "new_branch_id": cfg.branch_id}),
                )
            } else {
                console::success(format!(
                    "Branch '{branch_name}' created (ID: {})",
                    cfg.branch_id
                ));
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            emit_error(a.json, &error.to_string());
            ExitCode::from(1)
        }
    }
}

fn cmd_branch_current_with_service<C: PlatformClient>(
    service: &AdkService<C>,
    a: CommonPathArgs,
) -> ExitCode {
    if !ensure_project_loaded(service, &a.path, a.json) {
        return ExitCode::from(1);
    }
    let path = PathBuf::from(&a.path);
    match service.current_branch_name_optional(path.as_path()) {
        Ok(branch) => {
            if a.json {
                println!("{}", json!({"current_branch": branch}));
            } else if let Some(branch) = branch {
                console::plain(format!("[label]Current branch:[/label] {branch}"));
            } else {
                console::warning(
                    "Current local branch does not exist in Agent Studio. It may have been deleted or merged.",
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            let message = error.to_string();
            print_payload(
                a.json,
                json!({"success": false, "message": clean_error_message(&message)}),
            );
            ExitCode::SUCCESS
        }
    }
}

fn cmd_branch_switch_with_service<C: PlatformClient>(
    service: &AdkService<C>,
    a: BranchSwitchArgs,
    projection_json: Option<serde_json::Value>,
) -> ExitCode {
    if !ensure_project_loaded(service, &a.path, a.json) {
        return ExitCode::from(1);
    }
    let path = PathBuf::from(&a.path);
    let branch_name_from_prompt;
    let branch_name = match a.branch_name.as_deref() {
        Some(branch_name) => branch_name,
        None if a.json => {
            emit_error(
                true,
                "branch switch with --json requires a branch name argument.",
            );
            return ExitCode::from(1);
        }
        None => match prompt_branch_switch(service, path.as_path()) {
            Ok(Some(selected)) => {
                branch_name_from_prompt = selected;
                branch_name_from_prompt.as_str()
            }
            Ok(None) => return ExitCode::SUCCESS,
            Err(error) => {
                emit_error(false, &error);
                return ExitCode::from(1);
            }
        },
    };
    if !a.force {
        match service.diff(path.as_path(), &[], None, None) {
            Ok(diffs) if !diffs.is_empty() => {
                emit_error(
                    a.json,
                    "Cannot switch branches with uncommitted changes. Use --force to switch and discard changes.",
                );
                return ExitCode::from(1);
            }
            Ok(_) => {}
            Err(error) => {
                emit_error(a.json, &error.to_string());
                return ExitCode::from(1);
            }
        }
    }
    let mut output_projection = projection_json.clone();
    match service.set_branch(PathBuf::from(&a.path).as_path(), branch_name) {
        Ok(_cfg) => {
            if let Some(projection) = &projection_json
                && let Err(error) =
                    pull_projection_into_path(path.as_path(), projection, a.force, a.format)
            {
                emit_error(a.json, &error);
                return ExitCode::from(1);
            } else if projection_json.is_none()
                && let Err(error) =
                    service.pull_named_with_format(path.as_path(), branch_name, a.force, a.format)
            {
                emit_error(a.json, &error.to_string());
                return ExitCode::from(1);
            }
            if projection_json.is_none() && a.output_json_projection {
                match service.pull_projection_json_by_name(branch_name) {
                    Ok(projection) => output_projection = Some(projection),
                    Err(error) => {
                        emit_error(a.json, &error.to_string());
                        return ExitCode::from(1);
                    }
                }
            }
            let mut payload = json!({"success": true, "branch_name": branch_name});
            if a.output_json_projection {
                payload["projection"] = output_projection.unwrap_or(serde_json::Value::Null);
            }
            print_payload(a.json || a.output_json_projection, payload)
        }
        Err(error) => {
            emit_error(a.json, &error.to_string());
            ExitCode::from(1)
        }
    }
}

fn cmd_branch_delete<C: PlatformClient>(
    service: &AdkService<C>,
    args: BranchDeleteArgs,
) -> ExitCode {
    let path = PathBuf::from(&args.path);
    let current_branch = match service.current_branch_name(path.as_path()) {
        Ok(branch) => branch,
        Err(error) => {
            return print_branch_delete_error(args.json, &error.to_string());
        }
    };
    let branches = match service.list_branch_map(path.as_path()) {
        Ok(branches) => branches,
        Err(error) => {
            return print_branch_delete_error(args.json, &error.to_string());
        }
    };
    let deletable = branches
        .into_iter()
        .filter(|(name, branch_id)| name != "main" && branch_id != "main")
        .collect::<Vec<_>>();

    if let Some(branch_name) = args.branch_name.as_deref() {
        if !deletable.iter().any(|(name, _)| name == branch_name) {
            return print_branch_delete_error(
                args.json,
                &format!("Branch '{branch_name}' does not exist or cannot be deleted."),
            );
        }
        if !args.json {
            match prompt_confirm(&format!("Delete branch '{branch_name}'?")) {
                Ok(true) => {}
                Ok(false) => {
                    console::warning("Aborted.");
                    return ExitCode::SUCCESS;
                }
                Err(error) => {
                    emit_error(false, &error);
                    return ExitCode::from(1);
                }
            }
        }
        return delete_one_branch(service, path.as_path(), branch_name, &current_branch, args.json);
    }

    if args.json {
        emit_error(true, "branch delete with --json requires a branch name argument.");
        return ExitCode::from(1);
    }

    if deletable.is_empty() {
        console::plain("[muted]No deletable branches found.[/muted]");
        return ExitCode::SUCCESS;
    }

    let choices = deletable
        .iter()
        .map(|(name, _)| {
            let title = if name == &current_branch {
                format!("{name} (current)")
            } else {
                name.clone()
            };
            (name.clone(), title)
        })
        .collect::<Vec<_>>();
    let branch_names = match prompt_multi_select("Select branches to delete", &choices) {
        Ok(Some(branch_names)) => branch_names,
        Ok(None) => {
            console::warning("No branches selected. Exiting.");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            emit_error(false, &error);
            return ExitCode::from(1);
        }
    };
    let confirm_msg = format!(
        "Delete {} branch(es): {}?",
        branch_names.len(),
        branch_names.join(", ")
    );
    match prompt_confirm(&confirm_msg) {
        Ok(true) => {}
        Ok(false) => {
            console::warning("Aborted.");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            emit_error(false, &error);
            return ExitCode::from(1);
        }
    }

    let mut deleted_count = 0usize;
    let mut current_branch_deleted = false;
    for branch_name in branch_names {
        match service.delete_branch(path.as_path(), &branch_name) {
            Ok((true, switched_to)) => {
                deleted_count += 1;
                if branch_name == current_branch || switched_to.as_deref() == Some("main") {
                    current_branch_deleted = true;
                }
                console::plain(format!("  [muted]Deleted branch:[/muted] {branch_name}"));
                if switched_to.as_deref() == Some("main") {
                    console::info("Switched to branch 'main'.");
                }
            }
            Ok((false, _)) => console::error(format!("Failed to delete branch '{branch_name}'.")),
            Err(error) => console::error(clean_error_message(&error.to_string())),
        }
    }
    if deleted_count > 0 {
        console::success(format!("Deleted {deleted_count} branch(es)."));
    }
    if current_branch_deleted {
        tracing::debug!("deleted current branch and switched local config to main");
    }
    ExitCode::SUCCESS
}

fn delete_one_branch<C: PlatformClient>(
    service: &AdkService<C>,
    path: &Path,
    branch_name: &str,
    current_branch: &str,
    json_mode: bool,
) -> ExitCode {
    match service.delete_branch(path, branch_name) {
        Ok((deleted, switched_to)) => {
            if json_mode {
                let mut payload = json!({"success": deleted});
                if let Some(switched_to) = switched_to {
                    payload["switched_to"] = json!(switched_to);
                }
                println!("{payload}");
            } else if deleted {
                console::success(format!("Deleted branch: {branch_name}"));
                if branch_name == current_branch {
                    console::info("Switched to branch 'main'.");
                }
            } else {
                console::error(format!("Failed to delete branch '{branch_name}'."));
            }
            ExitCode::SUCCESS
        }
        Err(error) => print_branch_delete_error(json_mode, &error.to_string()),
    }
}

fn print_branch_delete_error(json_mode: bool, message: &str) -> ExitCode {
    let message = clean_error_message(message);
    if json_mode {
        println!("{}", json!({"success": false, "message": message}));
    } else {
        console::error(message);
    }
    ExitCode::SUCCESS
}

fn cmd_branch_merge<C: PlatformClient>(
    service: &AdkService<C>,
    args: BranchMergeArgs,
) -> ExitCode {
    let message = args.message.unwrap_or_default();
    if message.trim().is_empty() {
        emit_error(args.json, "Merge message is required.");
        return ExitCode::from(1);
    }
    if args.interactive && args.json {
        emit_error(args.json, "--interactive and --json cannot be used together.");
        return ExitCode::from(1);
    }
    let file_resolutions = match parse_branch_merge_resolutions(args.resolutions.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            emit_error(args.json, &format!("Failed to parse resolutions: {error}"));
            return ExitCode::from(1);
        }
    };
    let path = PathBuf::from(&args.path);
    let branch_name = service
        .current_branch_name(path.as_path())
        .unwrap_or_else(|_| "current branch".to_string());

    let first_result = match service.merge_branch(path.as_path(), &message, file_resolutions.clone())
    {
        Ok(result) => result,
        Err(error) => {
            emit_error(args.json, &error.to_string());
            return ExitCode::from(1);
        }
    };
    if args.json {
        return print_branch_merge_json_result(
            first_result.success,
            first_result.conflicts,
            first_result.errors,
        );
    }
    if first_result.success {
        print_branch_merge_success(&branch_name);
        return ExitCode::SUCCESS;
    }

    console::error(format!("Failed to merge branch '{branch_name}'."));
    print_branch_merge_errors(&first_result.errors);
    let mut enriched = enrich_branch_merge_conflicts(&first_result.conflicts);
    print_merge_conflict_table(&display_merge_conflicts(&enriched), "Merge conflicts");
    if !first_result.errors.is_empty() {
        return ExitCode::from(1);
    }
    if !args.interactive {
        console::plain(
            "Merge conflicts detected. To resolve:\n- Use 'poly branch merge -i <message>' to resolve conflicts interactively\n- Use 'poly branch merge --resolutions <file.json> <message>' to provide pre-defined resolutions\n- Merge manually on Agent Studio",
        );
        return ExitCode::from(1);
    }

    let mut existing_resolutions = branch_merge_existing_resolutions(&file_resolutions);
    loop {
        let Some(resolutions) =
            prompt_branch_merge_resolutions(&enriched, &existing_resolutions, &branch_name)
        else {
            console::warning("No resolutions provided. Exiting.");
            return ExitCode::from(1);
        };
        existing_resolutions = branch_merge_existing_resolutions(&Some(resolutions.clone()));
        let result = match service.merge_branch(path.as_path(), &message, Some(resolutions)) {
            Ok(result) => result,
            Err(error) => {
                emit_error(false, &error.to_string());
                return ExitCode::from(1);
            }
        };
        if result.success {
            print_branch_merge_success(&branch_name);
            return ExitCode::SUCCESS;
        }
        if !result.errors.is_empty() {
            console::error(format!(
                "Failed to merge branch '{branch_name}' after conflict resolution."
            ));
            print_branch_merge_errors(&result.errors);
            return ExitCode::from(1);
        }
        if result.conflicts.is_empty() {
            console::error(format!(
                "Failed to merge branch '{branch_name}' after conflict resolution (no conflicts or errors returned)."
            ));
            return ExitCode::from(1);
        }
        console::warning("Merge still blocked; resolve the remaining conflicts below.");
        enriched = enrich_branch_merge_conflicts(&result.conflicts);
        print_merge_conflict_table(&display_merge_conflicts(&enriched), "Remaining merge conflicts");
    }
}

fn print_branch_merge_json_result(
    success: bool,
    conflicts: Vec<serde_json::Value>,
    errors: Vec<serde_json::Value>,
) -> ExitCode {
    let mut payload = json!({"success": success});
    if !conflicts.is_empty() || !errors.is_empty() {
        payload["conflicts"] = json!(conflicts);
        payload["errors"] = json!(errors);
    }
    println!("{payload}");
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn print_branch_merge_success(branch_name: &str) {
    console::success(format!("Branch '{branch_name}' merged successfully."));
    console::info("Switched to \"main\" branch after merge.");
}

fn print_branch_merge_errors(errors: &[serde_json::Value]) {
    if errors.is_empty() {
        return;
    }
    console::plain("\n[error]Errors:[/error]");
    for error in errors {
        let path = error
            .get("path")
            .map(merge_value_to_string)
            .unwrap_or_else(|| "-".to_string());
        let message = error
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown error");
        console::error(format!("- {path}: {message}"));
    }
}

fn enrich_branch_merge_conflicts(conflicts: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut counts = BTreeMap::<String, usize>::new();
    for conflict in conflicts {
        let Some(path) = merge_conflict_path(conflict) else {
            continue;
        };
        if branch_merge_timestamp_path(&path) {
            continue;
        }
        *counts.entry(branch_merge_conflict_file_key(&path)).or_default() += 1;
    }

    conflicts
        .iter()
        .map(|conflict| {
            let Some(path) = merge_conflict_path(conflict) else {
                return conflict.clone();
            };
            if branch_merge_timestamp_path(&path) {
                return conflict.clone();
            }
            let base = conflict
                .get("baseValue")
                .map(merge_value_to_string)
                .unwrap_or_default();
            let theirs = conflict
                .get("theirsValue")
                .map(merge_value_to_string)
                .unwrap_or_default();
            let ours = conflict
                .get("oursValue")
                .map(merge_value_to_string)
                .unwrap_or_default();
            let merged = merge_strings_simple(&base, &theirs, &ours);
            let file_key = branch_merge_conflict_file_key(&path);
            let mut row = conflict.as_object().cloned().unwrap_or_default();
            row.insert("visual_path".to_string(), json!(path.join("/")));
            row.insert("merged_value".to_string(), json!(merged));
            row.insert(
                "can_auto_merge".to_string(),
                json!(!contains_merge_conflict(&merged)),
            );
            row.insert("file_key".to_string(), json!(file_key));
            row.insert(
                "conflicts_in_resource".to_string(),
                json!(counts.get(&branch_merge_conflict_file_key(&path)).copied().unwrap_or(1)),
            );
            serde_json::Value::Object(row)
        })
        .collect()
}

fn display_merge_conflicts(conflicts: &[serde_json::Value]) -> Vec<serde_json::Value> {
    conflicts
        .iter()
        .filter(|conflict| {
            merge_conflict_path(conflict).is_some_and(|path| !branch_merge_timestamp_path(&path))
        })
        .cloned()
        .collect()
}

fn print_merge_conflict_table(conflicts: &[serde_json::Value], title: &str) {
    if conflicts.is_empty() {
        return;
    }
    console::plain(format!("\n[label]{title}[/label]"));
    for conflict in conflicts {
        let visual = conflict
            .get("visual_path")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .or_else(|| merge_conflict_path(conflict).map(|path| path.join("/")))
            .unwrap_or_else(|| "<unknown>".to_string());
        let status = if conflict
            .get("can_auto_merge")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            "Auto-mergeable"
        } else {
            "Needs decision"
        };
        let count = conflict
            .get("conflicts_in_resource")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        let suffix = if count == 1 { "conflict" } else { "conflicts" };
        console::plain(format!("  - {visual} [{status}; {count} {suffix}]"));
    }
}

fn prompt_branch_merge_resolutions(
    conflicts: &[serde_json::Value],
    existing_resolutions: &BTreeMap<String, serde_json::Value>,
    branch_name: &str,
) -> Option<Vec<serde_json::Value>> {
    let mut resolutions = Vec::new();
    let mut index_in_resource = BTreeMap::<String, usize>::new();
    for conflict in conflicts {
        let Some(path) = merge_conflict_path(conflict) else {
            continue;
        };
        if branch_merge_timestamp_path(&path) {
            resolutions.push(json!({"path": path, "strategy": "theirs"}));
            continue;
        }

        let clean_path = conflict
            .get("visual_path")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| path.join("/"));
        let file_key = conflict
            .get("file_key")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| branch_merge_conflict_file_key(&path));
        let idx = {
            let entry = index_in_resource.entry(file_key.clone()).or_default();
            *entry += 1;
            *entry
        };
        let total = conflict
            .get("conflicts_in_resource")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        let merged = conflict
            .get("merged_value")
            .map(merge_value_to_string)
            .unwrap_or_else(|| {
                merge_strings_simple(
                    &conflict
                        .get("baseValue")
                        .map(merge_value_to_string)
                        .unwrap_or_default(),
                    &conflict
                        .get("theirsValue")
                        .map(merge_value_to_string)
                        .unwrap_or_default(),
                    &conflict
                        .get("oursValue")
                        .map(merge_value_to_string)
                        .unwrap_or_default(),
                )
            });
        let auto_mergeable = conflict
            .get("can_auto_merge")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or_else(|| !contains_merge_conflict(&merged));
        let existing_resolution = existing_resolutions.get(&clean_path);
        print_merge_conflict_prompt_header(MergeConflictPromptHeader {
            conflict,
            clean_path: &clean_path,
            file_key: &file_key,
            idx,
            total,
            auto_mergeable,
            branch_name,
            existing_resolution,
        });

        let mut choices = Vec::<(String, String)>::new();
        if existing_resolution.is_some() {
            choices.push(("existing".to_string(), "Use resolution".to_string()));
        }
        if auto_mergeable {
            choices.push(("merged".to_string(), "Accept auto-merge".to_string()));
        }
        choices.extend([
            ("ours".to_string(), "Use main".to_string()),
            (
                "theirs".to_string(),
                format!("Use branch - {branch_name}"),
            ),
            ("base".to_string(), "Use original (base)".to_string()),
            ("edit".to_string(), "Edit".to_string()),
        ]);

        loop {
            let answer = match prompt_select("Select resolution", &choices) {
                Ok(Some(answer)) => answer,
                Ok(None) => return None,
                Err(error) => {
                    console::error(error);
                    return None;
                }
            };
            match answer.as_str() {
                "existing" => {
                    if let Some(existing) = existing_resolution {
                        resolutions.push(existing.clone());
                        break;
                    }
                }
                "merged" => {
                    resolutions.push(json!({"path": path, "value": merged, "strategy": "theirs"}));
                    break;
                }
                "edit" => {
                    let edited = match prompt_or_edit_merge_value(conflict, &merged, &file_key) {
                        Ok(Some(edited)) => edited,
                        Ok(None) => return None,
                        Err(error) => {
                            console::warning(error);
                            continue;
                        }
                    };
                    if contains_merge_conflict(&edited) {
                        console::warning(
                            "Edited version still contains merge conflict markers. Resolve them before continuing.",
                        );
                        continue;
                    }
                    resolutions.push(json!({"path": path, "value": edited, "strategy": "theirs"}));
                    break;
                }
                strategy => {
                    resolutions.push(json!({"path": path, "strategy": strategy}));
                    break;
                }
            }
        }
    }
    Some(resolutions)
}

struct MergeConflictPromptHeader<'a> {
    conflict: &'a serde_json::Value,
    clean_path: &'a str,
    file_key: &'a str,
    idx: usize,
    total: u64,
    auto_mergeable: bool,
    branch_name: &'a str,
    existing_resolution: Option<&'a serde_json::Value>,
}

fn print_merge_conflict_prompt_header(header: MergeConflictPromptHeader<'_>) {
    let MergeConflictPromptHeader {
        conflict,
        clean_path,
        file_key,
        idx,
        total,
        auto_mergeable,
        branch_name,
        existing_resolution,
    } = header;
    console::plain("\n[label]Resolve conflict[/label]");
    console::plain(format!("  Field: {clean_path}"));
    if total > 1 {
        console::plain(format!("  Resource: {file_key} (conflict {idx} of {total})"));
    }
    let status = if auto_mergeable {
        "Auto-mergeable"
    } else {
        "Needs decision"
    };
    console::plain(format!("  Status: {status}"));
    if let Some(existing) = existing_resolution {
        let display = existing
            .get("value")
            .map(merge_value_to_string)
            .or_else(|| {
                existing
                    .get("strategy")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string)
            })
            .unwrap_or_else(|| "resolution".to_string());
        console::plain(format!("  Resolution: {display}"));
    }
    if !merge_conflict_heavy(conflict) {
        console::plain(format!(
            "  Main: {}",
            conflict
                .get("oursValue")
                .map(merge_value_to_string)
                .unwrap_or_default()
        ));
        console::plain(format!(
            "  Branch ({branch_name}): {}",
            conflict
                .get("theirsValue")
                .map(merge_value_to_string)
                .unwrap_or_default()
        ));
        console::plain(format!(
            "  Original (base): {}",
            conflict
                .get("baseValue")
                .map(merge_value_to_string)
                .unwrap_or_default()
        ));
    } else {
        console::plain("[muted]Multiline or long values - choose a side, accept auto-merge, or use Edit to open your editor.[/muted]");
    }
}

fn prompt_or_edit_merge_value(
    conflict: &serde_json::Value,
    merged: &str,
    file_key: &str,
) -> Result<Option<String>, String> {
    if merge_conflict_heavy(conflict) {
        return edit_in_editor(
            merged,
            if merge_conflict_path(conflict)
                .and_then(|path| path.last().cloned())
                .as_deref()
                == Some("code")
            {
                ".py"
            } else {
                ".txt"
            },
            file_key,
        )
        .map(Some);
    }
    console::prompt("Custom resolution: ")
        .map_err(|error| format!("Failed to write prompt: {error}"))?;
    io::stdout()
        .flush()
        .map_err(|error| format!("Failed to write prompt: {error}"))?;
    let mut input = String::new();
    let bytes = io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("Failed to read custom resolution: {error}"))?;
    if bytes == 0 {
        Ok(None)
    } else {
        Ok(Some(input.trim_end_matches(['\r', '\n']).to_string()))
    }
}

fn edit_in_editor(initial: &str, extension: &str, filename: &str) -> Result<String, String> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let safe_name = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let path = std::env::temp_dir().join(format!(
        "{safe_name}_{}_merge{extension}",
        std::process::id()
    ));
    fs::write(&path, initial).map_err(|error| error.to_string())?;
    let mut parts = editor.split_whitespace();
    let Some(program) = parts.next() else {
        let _ = fs::remove_file(&path);
        return Err("Could not open the configured editor.".to_string());
    };
    let status = std::process::Command::new(program)
        .args(parts)
        .arg(&path)
        .status()
        .map_err(|_| {
            "Could not open the configured editor. Check your $EDITOR or $VISUAL setting, then try Edit again."
                .to_string()
        })?;
    if !status.success() {
        let _ = fs::remove_file(&path);
        return Err(
            "The editor exited with an error. Fix the issue and try Edit again, or choose another resolution."
                .to_string(),
        );
    }
    let edited = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let _ = fs::remove_file(&path);
    if edited == initial {
        Err("Editor closed without saving; choose another option or try Edit again.".to_string())
    } else {
        Ok(edited)
    }
}

fn branch_merge_existing_resolutions(
    resolutions: &Option<Vec<serde_json::Value>>,
) -> BTreeMap<String, serde_json::Value> {
    resolutions
        .as_ref()
        .into_iter()
        .flatten()
        .filter_map(|resolution| {
            let path = merge_conflict_path(resolution)?;
            Some((path.join("/"), resolution.clone()))
        })
        .collect()
}

fn merge_conflict_path(value: &serde_json::Value) -> Option<Vec<String>> {
    value
        .get("path")
        .and_then(serde_json::Value::as_array)
        .map(|items| items.iter().map(merge_value_to_string).collect())
}

fn branch_merge_timestamp_path(path: &[String]) -> bool {
    path.last()
        .is_some_and(|field| field == "updatedAt" || field == "createdAt")
}

fn branch_merge_conflict_file_key(path: &[String]) -> String {
    if path.len() <= 1 {
        path.join("/")
    } else {
        path[..path.len() - 1].join("/")
    }
}

fn merge_conflict_heavy(conflict: &serde_json::Value) -> bool {
    ["baseValue", "theirsValue", "oursValue"].iter().any(|key| {
        conflict
            .get(*key)
            .map(merge_value_to_string)
            .is_some_and(|value| value.contains('\n') || value.len() > 800)
    })
}

fn merge_value_to_string(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn merge_strings_simple(base: &str, theirs: &str, ours: &str) -> String {
    if ours == theirs {
        ours.to_string()
    } else if base == ours {
        theirs.to_string()
    } else if base == theirs {
        ours.to_string()
    } else {
        let mut out = String::new();
        out.push_str("<<<<<<<\n");
        out.push_str(theirs);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("=======\n");
        out.push_str(ours);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(">>>>>>>\n");
        out
    }
}

fn contains_merge_conflict(value: &str) -> bool {
    let mut has_start = false;
    let mut has_middle = false;
    for line in value.lines() {
        if line.contains("<<<<<<<") {
            has_start = true;
        } else if has_start && line.contains("=======") {
            has_middle = true;
        } else if has_middle && line.contains(">>>>>>>") {
            return true;
        }
    }
    false
}

fn print_branch_list<'a, I>(current_branch: Option<&str>, branches: I)
where
    I: IntoIterator<Item = (&'a String, &'a String)>,
{
    console::plain("[label]Branches:[/label]");
    for (name, branch_id) in branches {
        let marker = if current_branch.is_some_and(|current| name == current || branch_id == current)
        {
            "*"
        } else {
            " "
        };
        if marker == "*" {
            console::plain(format!("[success]{marker} {name}[/success] [muted]({branch_id})[/muted]"));
        } else {
            console::plain(format!("{marker} {name} [muted]({branch_id})[/muted]"));
        }
    }
}

fn parse_branch_merge_resolutions(
    raw: Option<&str>,
) -> Result<Option<Vec<serde_json::Value>>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let raw = raw.trim();
    let content = if raw == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| e.to_string())?;
        buf
    } else if raw.starts_with('[') {
        raw.to_string()
    } else {
        let maybe_path = PathBuf::from(raw);
        fs::read_to_string(&maybe_path).map_err(|e| e.to_string())?
    };
    let parsed: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let array = parsed
        .as_array()
        .cloned()
        .ok_or_else(|| "merge resolutions must be a JSON array".to_string())?;
    Ok(Some(array))
}
