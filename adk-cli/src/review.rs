use serde_json::json;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

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
