use crate::{
    ReviewArgs, ReviewCommands, ReviewCreateArgs, console, emit_inmemory_fallback_warning,
    emit_remote_service_error, ensure_project_loaded, http_service_for_path, local_service,
    normalize_cli_file_args, should_warn_inmemory_fallback,
};
use adk_api_client::PlatformClient;
use adk_core::{AdkService, ProjectWorkspace};
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

macro_rules! with_remote_service {
    ($workspace:expr, $path:expr, $json_mode:expr, |$service:ident| $body:expr) => {{
        match http_service_for_path($workspace, $path) {
            Ok($service) => $body,
            Err(error) if crate::allow_inmemory_fallback() => {
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

pub(crate) fn cmd_review(args: ReviewArgs) -> ExitCode {
    match args.command {
        None => ExitCode::SUCCESS,
        Some(ReviewCommands::List(list)) => match github_list_diff_gists() {
            Ok(gists) => {
                if args.json || list.json {
                    println!("{}", json!(gists));
                } else if gists.is_empty() {
                    console::plain("[muted]No review gists found.[/muted]");
                } else if let Err(error) = prompt_open_review_gist(&gists) {
                    emit_review_message(false, &error);
                } else {
                    console::success("Opened gist.");
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                emit_review_message(args.json || list.json, &error);
                ExitCode::SUCCESS
            }
        },
        Some(ReviewCommands::Delete(delete)) => {
            let json_mode = args.json || delete.json;
            let delete_result = if let Some(id) = delete.id.as_deref() {
                github_delete_review_gist(id).map(usize::from)
            } else {
                prompt_delete_review_gists(json_mode)
            };
            match delete_result {
                Ok(deleted_count) => {
                    let deleted = deleted_count > 0;
                    if args.json || delete.json {
                        println!("{}", json!({"success": deleted}));
                    } else if deleted_count > 1 {
                        console::success(format!("Deleted {deleted_count} gists."));
                    } else if deleted {
                        console::success("Deleted gist.");
                    } else {
                        console::plain("[muted]No review gists found.[/muted]");
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    emit_review_message(args.json || delete.json, &error);
                    ExitCode::SUCCESS
                }
            }
        }
        Some(ReviewCommands::Create(create)) => {
            if create.hash.is_some() && (create.before.is_some() || create.after.is_some()) {
                console::error("Cannot specify both hash and before/after versions.");
                return ExitCode::SUCCESS;
            }
            let json_mode = args.json || create.json;
            let needs_remote =
                create.hash.is_some() || create.before.is_some() || create.after.is_some();
            if needs_remote {
                let workspace = ProjectWorkspace::new();
                with_remote_service!(&workspace, &args.path, json_mode, |service| {
                    cmd_review_create(&service, &args.path, args.json, create)
                })
            } else {
                let service = local_service();
                cmd_review_create(&service, &args.path, args.json, create)
            }
        }
    }
}

fn cmd_review_create<C: PlatformClient>(
    service: &AdkService<C>,
    path: &str,
    parent_json: bool,
    create: ReviewCreateArgs,
) -> ExitCode {
    let json_mode = parent_json || create.json;
    if !ensure_project_loaded(service, path, json_mode) {
        return ExitCode::from(1);
    }
    let after = create.hash.clone().or(create.after.clone());
    let root = PathBuf::from(path);
    let files = normalize_cli_file_args(root.as_path(), &create.files);
    let diffs = match service.diff(root.as_path(), &files, create.before.clone(), after.clone()) {
        Ok(diffs) => diffs,
        Err(_) => {
            if json_mode {
                println!(
                    "{}",
                    json!({"success": false, "message": "Failed to compute diffs."})
                );
            } else {
                console::plain("[muted]No changes detected.[/muted]");
            }
            return ExitCode::SUCCESS;
        }
    };
    if diffs.is_empty() {
        if json_mode {
            println!(
                "{}",
                json!({"success": false, "message": "No changes to review."})
            );
        } else {
            console::plain("[muted]No changes detected.[/muted]");
        }
        return ExitCode::SUCCESS;
    }
    let description = review_description(
        path,
        create.hash.as_deref(),
        create.before.as_deref(),
        after.as_deref(),
    );
    match github_create_review_gist(diffs.iter(), &description) {
        Ok(url) => {
            if json_mode {
                println!("{}", json!({"success": true, "link": url}));
            } else {
                console::success(format!("Gist created: {url}"));
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_review_message(json_mode, &error);
            ExitCode::SUCCESS
        }
    }
}

pub fn emit_review_message(json_mode: bool, message: &str) {
    if json_mode {
        println!("{}", json!({"success": false, "message": message}));
    } else {
        crate::console::error(message);
    }
}

pub fn github_list_diff_gists() -> Result<Vec<serde_json::Value>, String> {
    let client = github_client()?;
    let response = client
        .get("https://api.github.com/gists")
        .send()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(github_error_message(response));
    }
    let gists: Vec<serde_json::Value> = response.json().map_err(|e| e.to_string())?;
    Ok(gists
        .into_iter()
        .filter(|gist| {
            gist.get("files")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|files| files.keys().all(|name| name.ends_with(".diff")))
        })
        .map(|gist| {
            json!({
                "id": gist.get("id").cloned().unwrap_or(serde_json::Value::Null),
                "description": gist.get("description").cloned().unwrap_or_else(|| gist.get("id").cloned().unwrap_or(serde_json::Value::Null)),
                "created_at": gist.get("created_at").cloned().unwrap_or(serde_json::Value::Null),
                "html_url": gist.get("html_url").cloned().unwrap_or(serde_json::Value::Null),
            })
        })
        .collect())
}

pub fn github_create_review_gist<'a, I>(diffs: I, description: &str) -> Result<String, String>
where
    I: IntoIterator<Item = (&'a String, &'a String)>,
{
    let client = github_client()?;
    let files = diffs
        .into_iter()
        .filter(|(_, diff)| !diff.is_empty())
        .map(|(path, diff)| {
            (
                format!("{}.diff", path.replace(std::path::MAIN_SEPARATOR, "_")),
                json!({"content": diff}),
            )
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    let response = client
        .post("https://api.github.com/gists")
        .json(&json!({"description": description, "public": false, "files": files}))
        .send()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(github_error_message(response));
    }
    let payload: serde_json::Value = response.json().map_err(|e| e.to_string())?;
    payload
        .get("html_url")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "missing html_url in GitHub response".to_string())
}

pub fn prompt_open_review_gist(gists: &[serde_json::Value]) -> Result<(), String> {
    print_review_gist_choices(gists);
    crate::console::prompt("Select a gist to open: ").map_err(|e| e.to_string())?;
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut selection = String::new();
    std::io::stdin()
        .read_line(&mut selection)
        .map_err(|e| e.to_string())?;
    let selection = selection.trim();
    if selection.is_empty() {
        return Ok(());
    }
    let gist = select_gist(gists, selection)?;
    let url = gist
        .get("html_url")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "selected gist did not include an html_url".to_string())?;
    open_url(url)
}

pub fn prompt_delete_review_gists(json_mode: bool) -> Result<usize, String> {
    let gists = github_list_diff_gists()?;
    if gists.is_empty() {
        return Ok(0);
    }
    if json_mode {
        return Err("review delete requires --id when --json is used".to_string());
    }
    print_review_gist_choices(&gists);
    crate::console::prompt("Select gists to delete (comma-separated numbers or id prefixes): ")
        .map_err(|e| e.to_string())?;
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut selection = String::new();
    std::io::stdin()
        .read_line(&mut selection)
        .map_err(|e| e.to_string())?;
    let selection = selection.trim();
    if selection.is_empty() {
        return Ok(0);
    }

    let mut ids = Vec::new();
    for token in selection
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|token| !token.is_empty())
    {
        let gist = select_gist(&gists, token)?;
        let id = gist
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "selected gist did not include an id".to_string())?
            .to_string();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }

    for id in &ids {
        github_delete_review_gist_by_id(id)?;
    }
    Ok(ids.len())
}

pub fn github_delete_review_gist(gist_id: &str) -> Result<bool, String> {
    let gists = github_list_diff_gists()?;
    let id = select_gist(&gists, gist_id)?
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "matched gist did not include an id".to_string())?
        .to_string();
    github_delete_review_gist_by_id(&id)?;
    Ok(true)
}

pub fn review_description(
    base_path: &str,
    version_hash: Option<&str>,
    before: Option<&str>,
    after: Option<&str>,
) -> String {
    let path = PathBuf::from(base_path);
    let pieces = path
        .components()
        .rev()
        .take(2)
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let project_name = pieces.into_iter().rev().collect::<Vec<_>>().join("/");
    if let Some(hash) = version_hash {
        format!("Poly ADK: {project_name}: {hash}")
    } else if before.is_none() && after.is_none() {
        format!("Poly ADK: {project_name}: local -> remote")
    } else if let (Some(before), Some(after)) = (before, after) {
        format!("Poly ADK: {project_name}: {before} -> {after}")
    } else if let Some(after) = after {
        format!("Poly ADK: {project_name}: {after}")
    } else {
        format!("Poly ADK: {project_name}: {} -> local", before.unwrap_or(""))
    }
}

fn github_headers() -> Result<reqwest::header::HeaderMap, String> {
    let token = std::env::var("GITHUB_ACCESS_TOKEN").map_err(|_| {
        "GITHUB_ACCESS_TOKEN environment variable not set. Please set it to your GitHub personal access token with gist scope.".to_string()
    })?;
    let mut headers = reqwest::header::HeaderMap::new();
    let auth_value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|e| e.to_string())?;
    headers.insert(reqwest::header::AUTHORIZATION, auth_value);
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "X-GitHub-Api-Version",
        reqwest::header::HeaderValue::from_static("2022-11-28"),
    );
    Ok(headers)
}

fn github_client() -> Result<reqwest::blocking::Client, String> {
    let headers = github_headers()?;
    reqwest::blocking::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| e.to_string())
}

fn print_review_gist_choices(gists: &[serde_json::Value]) {
    for (idx, gist) in gists.iter().enumerate() {
        crate::console::plain(format!(
            "{}. {}  {}",
            idx + 1,
            gist.get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
            gist.get("description")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
        ));
    }
}

fn select_gist<'a>(
    gists: &'a [serde_json::Value],
    selection: &str,
) -> Result<&'a serde_json::Value, String> {
    if let Ok(index) = selection.parse::<usize>()
        && (1..=gists.len()).contains(&index)
    {
        return Ok(&gists[index - 1]);
    }
    gists
        .iter()
        .find(|gist| {
            gist.get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|id| id.starts_with(selection))
        })
        .ok_or_else(|| format!("No review gist found matching '{selection}'."))
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    let status = command.status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to open browser: {status}"))
    }
}

fn github_delete_review_gist_by_id(id: &str) -> Result<(), String> {
    let client = github_client()?;
    let response = client
        .delete(format!("https://api.github.com/gists/{id}"))
        .send()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(github_error_message(response));
    }
    Ok(())
}

fn github_error_message(response: reqwest::blocking::Response) -> String {
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return "Failed to make the gist. Your token must be missing gist permissions. Update and try again!".to_string();
    }
    let status = response.status();
    let body = response.text().unwrap_or_default();
    format!("GitHub API error: status={status} body={body}")
}
