use crate::{
    AdkService, DeploymentsArgs, DeploymentsCommands, DeploymentsPromoteArgs,
    DeploymentsRollbackArgs, DeploymentsShowArgs, console, emit_error, ensure_project_loaded,
    prompt_confirm,
};
use adk_api_client::PlatformClient;
use serde_json::json;
use std::process::ExitCode;

pub(crate) fn cmd_deployments<C: PlatformClient>(
    service: &AdkService<C>,
    args: DeploymentsArgs,
) -> ExitCode {
    match args.command {
        DeploymentsCommands::List(list_args) => {
            if !ensure_project_loaded(service, &list_args.path, list_args.json) {
                return ExitCode::from(1);
            }
            match service.list_deployments(&list_args.env) {
                Ok(deployments) => {
                    if deployments.versions.is_empty() {
                        if !list_args.json {
                            console::warning("No versions found.");
                        }
                        return ExitCode::SUCCESS;
                    }
                    let mut offset = list_args.offset;
                    if let Some(version_hash) = list_args.version_hash.as_deref() {
                        let prefix = version_hash.chars().take(9).collect::<String>();
                        let Some(idx) = deployments.versions.iter().position(|version| {
                            version
                                .get("version_hash")
                                .or_else(|| version.get("versionHash"))
                                .or_else(|| version.get("hash"))
                                .and_then(serde_json::Value::as_str)
                                .map(|hash| hash.starts_with(&prefix))
                                .unwrap_or(false)
                        }) else {
                            console::warning(format!("Version hash '{prefix}' not found."));
                            return ExitCode::SUCCESS;
                        };
                        offset = idx;
                    }
                    let versions = deployments
                        .versions
                        .into_iter()
                        .skip(offset)
                        .take(list_args.limit)
                        .collect::<Vec<_>>();
                    if list_args.json {
                        println!(
                            "{}",
                            json!({
                                "versions": versions,
                                "active_deployment_hashes": deployments.active_deployment_hashes
                            })
                        );
                    } else {
                        print_deployment_versions(
                            &versions,
                            &deployments.active_deployment_hashes,
                            list_args.details,
                        );
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    emit_error(list_args.json, &error.to_string());
                    ExitCode::from(1)
                }
            }
        }
        DeploymentsCommands::Show(show_args) => cmd_deployments_show(service, show_args),
        DeploymentsCommands::Promote(promote_args) => {
            cmd_deployments_promote(service, promote_args)
        }
        DeploymentsCommands::Rollback(rollback_args) => {
            cmd_deployments_rollback(service, rollback_args)
        }
    }
}

fn cmd_deployments_show<C: PlatformClient>(
    service: &AdkService<C>,
    args: DeploymentsShowArgs,
) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let deployments = match service.list_deployments(&args.env) {
        Ok(deployments) => deployments,
        Err(error) => {
            emit_error(args.json, &error.to_string());
            return ExitCode::from(1);
        }
    };
    if deployments.versions.is_empty() {
        emit_error(args.json, "No versions found.");
        return ExitCode::from(1);
    }
    let prefix = deployment_hash_prefix(&args.version_hash);
    let Some((version_idx, deployment)) = find_deployment_by_prefix(&deployments.versions, &prefix)
    else {
        emit_error(args.json, &format!("Version hash '{prefix}' not found."));
        return ExitCode::from(1);
    };
    let deployment = deployment.clone();
    let target_hash = deployment_hash(&deployment).unwrap_or_default().to_string();
    let predecessor_hash = deployments
        .versions
        .get(version_idx + 1)
        .and_then(deployment_hash)
        .map(ToString::to_string);
    let sandbox_versions = if args.env == "sandbox" {
        deployments.versions.clone()
    } else {
        match service.list_deployments("sandbox") {
            Ok(deployments) => deployments.versions,
            Err(error) => {
                emit_error(args.json, &error.to_string());
                return ExitCode::from(1);
            }
        }
    };
    let (included, is_rollback) =
        resolve_included_deployments(&sandbox_versions, &target_hash, predecessor_hash.as_deref());

    if args.json {
        println!(
            "{}",
            json!({
                "success": true,
                "deployment": deployment,
                "active_deployment_hashes": deployments.active_deployment_hashes,
                "included_deployments": included,
                "is_rollback": is_rollback,
            })
        );
    } else {
        print_deployment_show(
            &deployment,
            &deployments.active_deployment_hashes,
            &included,
            is_rollback,
        );
    }
    ExitCode::SUCCESS
}

fn cmd_deployments_promote<C: PlatformClient>(
    service: &AdkService<C>,
    args: DeploymentsPromoteArgs,
) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let search_env = deployment_promote_search_env(&args.to_env);
    let deployments = match service.list_deployments(search_env) {
        Ok(deployments) => deployments,
        Err(error) => {
            emit_error(args.json, &error.to_string());
            return ExitCode::from(1);
        }
    };
    let selection = match select_deployment_for_promotion_or_exit(
        args.json,
        &deployments.versions,
        &deployments.active_deployment_hashes,
        &args.from_deployment,
        &args.to_env,
        args.message.as_deref(),
        search_env,
    ) {
        Ok(selection) => selection,
        Err(code) => return code,
    };
    let sandbox_versions = match deployment_promote_sandbox_versions_or_exit(
        service,
        &deployments.versions,
        search_env,
        args.json,
    ) {
        Ok(versions) => versions,
        Err(code) => return code,
    };
    let (included, is_rollback) = resolve_included_deployments(
        &sandbox_versions,
        &selection.from_hash,
        selection.predecessor_hash.as_deref(),
    );
    let mut result = deployment_promote_result_payload(&args.to_env, &selection, included);

    if !args.json {
        print_deployment_promote_preview(&args.to_env, &result, &selection, is_rollback);
    }

    if args.dry_run {
        result["dry_run"] = json!(true);
        return print_deployment_dry_run(args.json, result);
    }
    if let Err(code) = confirm_deployment_promote_or_exit(args.json, args.force) {
        return code;
    }

    finish_deployment_promote(
        service,
        &selection,
        &args.to_env,
        &args.from_deployment,
        args.json,
        &mut result,
    )
}

fn print_deployment_promote_preview(
    to_env: &str,
    result: &serde_json::Value,
    selection: &DeploymentPromotionSelection,
    is_rollback: bool,
) {
    let preview = deployment_promote_preview(to_env, result, selection, is_rollback);
    console::plain(format!(
        "Promoting hash [bold]{}[/bold] to [info]{}[/info]",
        preview.hash_prefix,
        to_env
    ));
    if let Some(note) = preview.note {
        console::plain(note);
    }
    if let (Some(label), Some(items)) = (preview.included_label, preview.included_items) {
        console::plain(format!("{label} ({}):", items.len()));
        print_deployment_versions(items, &indexmap::IndexMap::new(), false);
    }
}

#[derive(Debug, PartialEq)]
struct DeploymentPromotionPreview<'a> {
    hash_prefix: String,
    note: Option<String>,
    included_label: Option<&'static str>,
    included_items: Option<&'a [serde_json::Value]>,
}

fn deployment_promote_preview<'a>(
    to_env: &str,
    result: &'a serde_json::Value,
    selection: &DeploymentPromotionSelection,
    is_rollback: bool,
) -> DeploymentPromotionPreview<'a> {
    let included_items = result
        .get("included_deployments")
        .and_then(serde_json::Value::as_array)
        .filter(|items| !items.is_empty())
        .map(Vec::as_slice);
    DeploymentPromotionPreview {
        hash_prefix: deployment_hash_prefix(
            result
                .get("from_hash")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        ),
        note: deployment_promote_preview_note(to_env, selection, is_rollback),
        included_label: included_items.map(|_| {
            if is_rollback {
                "Reverting deployments"
            } else {
                "Included deployments"
            }
        }),
        included_items,
    }
}

fn deployment_promote_preview_note(
    to_env: &str,
    selection: &DeploymentPromotionSelection,
    is_rollback: bool,
) -> Option<String> {
    if is_rollback {
        Some(format!(
            "Rolling back to an earlier version: {}",
            selection.display_message
        ))
    } else if selection.predecessor_hash.is_none() {
        Some(format!("First deployment to {to_env}."))
    } else {
        None
    }
}

fn confirm_deployment_promote_or_exit(json_mode: bool, force: bool) -> Result<(), ExitCode> {
    if json_mode || force {
        return Ok(());
    }
    match prompt_confirm("Confirm Deployment?") {
        Ok(true) => Ok(()),
        Ok(false) => {
            console::warning("Aborted.");
            Err(ExitCode::SUCCESS)
        }
        Err(error) => {
            emit_error(false, &error);
            Err(ExitCode::from(1))
        }
    }
}

fn finish_deployment_promote<C: PlatformClient>(
    service: &AdkService<C>,
    selection: &DeploymentPromotionSelection,
    to_env: &str,
    from_deployment: &str,
    json_mode: bool,
    result: &mut serde_json::Value,
) -> ExitCode {
    match service.promote_deployment(
        &selection.deployment_id,
        to_env,
        result
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
    ) {
        Ok(_) => {
            result["success"] = json!(true);
            if json_mode {
                println!("{result}");
            } else {
                console::success(format!(
                    "Deployment {} promoted to {}.",
                    from_deployment, to_env
                ));
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if json_mode {
                result["error"] = json!(error.to_string());
                println!("{result}");
            } else {
                emit_error(false, &format!("Failed to promote deployment: {error}"));
            }
            ExitCode::from(1)
        }
    }
}

#[derive(Debug, PartialEq)]
struct DeploymentPromotionSelection {
    deployment_id: String,
    from_hash: String,
    message: String,
    predecessor_hash: Option<String>,
    display_message: String,
}

#[derive(Debug, PartialEq, Eq)]
enum DeploymentPromotionSelectionError {
    NotFound {
        requested: String,
        search_env: &'static str,
    },
    MissingId,
}

fn deployment_promote_search_env(to_env: &str) -> &'static str {
    if to_env == "live" {
        "pre-release"
    } else {
        "sandbox"
    }
}

fn deployment_hash_or_active_alias<'a>(
    active_deployment_hashes: &'a indexmap::IndexMap<String, String>,
    requested: &'a str,
) -> &'a str {
    active_deployment_hashes
        .get(requested)
        .map(String::as_str)
        .unwrap_or(requested)
}

fn select_deployment_for_promotion(
    deployments: &[serde_json::Value],
    active_deployment_hashes: &indexmap::IndexMap<String, String>,
    from_deployment: &str,
    to_env: &str,
    message: Option<&str>,
    search_env: &'static str,
) -> Result<DeploymentPromotionSelection, DeploymentPromotionSelectionError> {
    let deployment_hash_or_alias =
        deployment_hash_or_active_alias(active_deployment_hashes, from_deployment);
    let prefix = deployment_hash_prefix(deployment_hash_or_alias);
    let Some((_, deployment)) = find_deployment_by_prefix(deployments, &prefix) else {
        return Err(DeploymentPromotionSelectionError::NotFound {
            requested: from_deployment.to_string(),
            search_env,
        });
    };
    let Some(deployment_id) = deployment_id(deployment).map(ToString::to_string) else {
        return Err(DeploymentPromotionSelectionError::MissingId);
    };
    let deployment_message = deployment_message(deployment).unwrap_or("");
    Ok(DeploymentPromotionSelection {
        deployment_id,
        from_hash: deployment_hash(deployment).unwrap_or_default().to_string(),
        message: message.unwrap_or(deployment_message).to_string(),
        predecessor_hash: active_deployment_hashes.get(to_env).cloned(),
        display_message: deployment_message_or_dash(deployment).to_string(),
    })
}

fn select_deployment_for_promotion_or_exit(
    json_mode: bool,
    deployments: &[serde_json::Value],
    active_deployment_hashes: &indexmap::IndexMap<String, String>,
    from_deployment: &str,
    to_env: &str,
    message: Option<&str>,
    search_env: &'static str,
) -> Result<DeploymentPromotionSelection, ExitCode> {
    match select_deployment_for_promotion(
        deployments,
        active_deployment_hashes,
        from_deployment,
        to_env,
        message,
        search_env,
    ) {
        Ok(selection) => Ok(selection),
        Err(DeploymentPromotionSelectionError::NotFound {
            requested,
            search_env,
        }) => Err(print_deployment_json_or_error(
            json_mode,
            json!({
                "success": false,
                "to_env": to_env,
                "error": format!("Deployment '{requested}' not found in {search_env}."),
            }),
        )),
        Err(DeploymentPromotionSelectionError::MissingId) => {
            emit_error(json_mode, "Selected deployment does not include an id.");
            Err(ExitCode::from(1))
        }
    }
}

fn deployment_promote_sandbox_versions_or_exit<C: PlatformClient>(
    service: &AdkService<C>,
    search_deployments: &[serde_json::Value],
    search_env: &str,
    json_mode: bool,
) -> Result<Vec<serde_json::Value>, ExitCode> {
    if search_env == "sandbox" {
        return Ok(search_deployments.to_vec());
    }
    match service.list_deployments("sandbox") {
        Ok(deployments) => Ok(deployments.versions),
        Err(error) => {
            emit_error(json_mode, &error.to_string());
            Err(ExitCode::from(1))
        }
    }
}

fn deployment_promote_result_payload(
    to_env: &str,
    selection: &DeploymentPromotionSelection,
    included_deployments: Vec<serde_json::Value>,
) -> serde_json::Value {
    json!({
        "success": false,
        "to_env": to_env,
        "from_hash": &selection.from_hash,
        "message": &selection.message,
        "included_deployments": included_deployments,
    })
}

fn cmd_deployments_rollback<C: PlatformClient>(
    service: &AdkService<C>,
    args: DeploymentsRollbackArgs,
) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let deployments = match service.list_deployments("sandbox") {
        Ok(deployments) => deployments,
        Err(error) => {
            emit_error(args.json, &error.to_string());
            return ExitCode::from(1);
        }
    };
    let deployment_hash_or_alias = deployments
        .active_deployment_hashes
        .get(&args.to_deployment)
        .map(String::as_str)
        .unwrap_or(args.to_deployment.as_str());
    let prefix = deployment_hash_prefix(deployment_hash_or_alias);
    let Some((_, deployment)) = find_deployment_by_prefix(&deployments.versions, &prefix) else {
        return print_deployment_json_or_error(
            args.json,
            json!({
                "success": false,
                "error": format!("Deployment '{}' not found in sandbox.", args.to_deployment),
            }),
        );
    };
    let deployment = deployment.clone();
    let Some(deployment_id) = deployment_id(&deployment).map(ToString::to_string) else {
        emit_error(args.json, "Selected deployment does not include an id.");
        return ExitCode::from(1);
    };
    let target_hash = deployment_hash(&deployment).unwrap_or_default().to_string();
    let deployment_message = deployment_message(&deployment).unwrap_or("");
    let message = args
        .message
        .clone()
        .unwrap_or_else(|| deployment_message.to_string());
    let current_sandbox_hash = deployments
        .active_deployment_hashes
        .get("sandbox")
        .map(String::as_str);
    let (reverted, _) =
        resolve_included_deployments(
            &deployments.versions,
            current_sandbox_hash.unwrap_or(""),
            Some(&target_hash),
        );
    let mut result = json!({
        "success": false,
        "target_hash": target_hash,
        "message": message,
        "reverted_deployments": reverted,
    });

    if !args.json {
        console::plain(format!(
            "Rolling back sandbox to deployment '[bold]{}[/bold]: {}'",
            result
                .get("target_hash")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .chars()
                .take(9)
                .collect::<String>(),
            deployment_message_or_dash(&deployment)
        ));
        if let Some(items) = result
            .get("reverted_deployments")
            .and_then(serde_json::Value::as_array)
            && !items.is_empty()
        {
            console::plain(format!("Reverting deployments ({}):", items.len()));
            print_deployment_versions(items, &indexmap::IndexMap::new(), false);
        }
    }

    if args.dry_run {
        result["dry_run"] = json!(true);
        return print_deployment_dry_run(args.json, result);
    }
    if !args.json && !args.force {
        match prompt_confirm("Confirm Rollback?") {
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

    match service.rollback_deployment(
        &deployment_id,
        result
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
    ) {
        Ok(_) => {
            result["success"] = json!(true);
            if args.json {
                println!("{result}");
            } else {
                console::success(format!(
                    "Sandbox rolled back to deployment {}.",
                    args.to_deployment
                ));
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if args.json {
                result["error"] = json!(error.to_string());
                println!("{result}");
            } else {
                emit_error(false, &format!("Failed to rollback deployment: {error}"));
            }
            ExitCode::from(1)
        }
    }
}

fn print_deployment_json_or_error(json_mode: bool, payload: serde_json::Value) -> ExitCode {
    if json_mode {
        println!("{payload}");
    } else {
        emit_error(
            false,
            payload
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Deployment command failed."),
        );
    }
    ExitCode::from(1)
}

fn print_deployment_dry_run(json_mode: bool, payload: serde_json::Value) -> ExitCode {
    if json_mode {
        println!("{payload}");
    } else {
        console::plain("[muted]Dry run - no changes were made.[/muted]");
    }
    ExitCode::SUCCESS
}

fn find_deployment_by_prefix<'a>(
    deployments: &'a [serde_json::Value],
    prefix: &str,
) -> Option<(usize, &'a serde_json::Value)> {
    deployments.iter().enumerate().find(|(_, deployment)| {
        deployment_hash(deployment)
            .map(|hash| hash.chars().take(9).collect::<String>() == prefix)
            .unwrap_or(false)
    })
}

fn deployment_hash_prefix(hash: &str) -> String {
    hash.chars().take(9).collect()
}

fn deployment_hash(deployment: &serde_json::Value) -> Option<&str> {
    string_field(deployment, &["version_hash", "versionHash", "hash"])
}

fn deployment_id(deployment: &serde_json::Value) -> Option<&str> {
    string_field(deployment, &["id", "deployment_id", "deploymentId"])
}

fn deployment_message(deployment: &serde_json::Value) -> Option<&str> {
    deployment
        .pointer("/deployment_metadata/deployment_message")
        .and_then(serde_json::Value::as_str)
        .filter(|message| !message.is_empty())
}

fn deployment_message_or_dash(deployment: &serde_json::Value) -> &str {
    deployment_message(deployment).unwrap_or("-")
}

fn resolve_included_deployments(
    sandbox_versions: &[serde_json::Value],
    target_hash: &str,
    predecessor_hash: Option<&str>,
) -> (Vec<serde_json::Value>, bool) {
    let Some(target_idx) = sandbox_versions
        .iter()
        .position(|version| deployment_hash(version) == Some(target_hash))
    else {
        return (vec![], false);
    };
    let Some(predecessor_hash) = predecessor_hash.filter(|hash| !hash.is_empty()) else {
        return (sandbox_versions[target_idx..].to_vec(), false);
    };
    let Some(pred_idx) = sandbox_versions
        .iter()
        .position(|version| deployment_hash(version) == Some(predecessor_hash))
    else {
        return (sandbox_versions[target_idx..].to_vec(), false);
    };
    if pred_idx < target_idx {
        (sandbox_versions[pred_idx..target_idx].to_vec(), true)
    } else {
        (sandbox_versions[target_idx..pred_idx].to_vec(), false)
    }
}

fn print_deployment_show(
    deployment: &serde_json::Value,
    active_deployment_hashes: &indexmap::IndexMap<String, String>,
    included_deployments: &[serde_json::Value],
    is_rollback: bool,
) {
    console::plain("[label]Deployment:[/label]");
    print_deployment_version_details(deployment, active_deployment_hashes);
    if included_deployments.is_empty() {
        return;
    }
    let label = if is_rollback {
        "Reverted deployments"
    } else {
        "Included deployments"
    };
    console::plain(format!("[label]{label}:[/label]"));
    print_deployment_versions(
        included_deployments,
        &indexmap::IndexMap::new(),
        false,
    );
}

fn print_deployment_versions(
    versions: &[serde_json::Value],
    active_deployment_hashes: &indexmap::IndexMap<String, String>,
    details: bool,
) {
    console::plain("[label]Deployment versions:[/label]");
    for version in versions {
        if details {
            print_deployment_version_details(version, active_deployment_hashes);
        } else {
            console::plain(format!(
                "  - {}",
                describe_deployment_version(version, active_deployment_hashes)
            ));
        }
    }
}

fn describe_deployment_version(
    version: &serde_json::Value,
    active_deployment_hashes: &indexmap::IndexMap<String, String>,
) -> String {
    let hash = string_field(version, &["version_hash", "versionHash", "hash"])
        .unwrap_or("unknown hash");
    let created = string_field(version, &["created_at", "createdAt", "artifact_version"]);
    let author = string_field(version, &["created_by", "createdBy"]);
    let message = version
        .pointer("/deployment_metadata/deployment_message")
        .and_then(serde_json::Value::as_str)
        .filter(|message| !message.is_empty());
    let deployment_type = version
        .pointer("/deployment_metadata/deployment_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");

    let mut details = vec![deployment_type.to_string(), hash.chars().take(9).collect()];
    if let Some(created) = created {
        details.push(created.to_string());
    }
    if let Some(author) = author {
        details.push(author.to_string());
    }
    if let Some(message) = message {
        details.push(message.to_string());
    }
    let badges = deployment_active_badges(hash, active_deployment_hashes);
    if !badges.is_empty() {
        details.push(badges);
    }
    details.join(" | ")
}

fn print_deployment_version_details(
    version: &serde_json::Value,
    active_deployment_hashes: &indexmap::IndexMap<String, String>,
) {
    let hash = string_field(version, &["version_hash", "versionHash", "hash"]).unwrap_or("");
    let deployment_type = version
        .pointer("/deployment_metadata/deployment_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");
    let badges = deployment_active_badges(hash, active_deployment_hashes);
    let label = if badges.is_empty() {
        format!("({deployment_type}) {}", hash.chars().take(9).collect::<String>())
    } else {
        format!(
            "({deployment_type}) {} {badges}",
            hash.chars().take(9).collect::<String>()
        )
    };
    console::plain(format!("  {label}"));
    console::plain(format!(
        "    Date: {}",
        string_field(version, &["created_at", "createdAt"]).unwrap_or("-")
    ));
    console::plain(format!(
        "    By: {}",
        string_field(version, &["created_by", "createdBy"]).unwrap_or("-")
    ));
    console::plain(format!(
        "    Deployment ID: {}",
        string_field(version, &["id"]).unwrap_or("-")
    ));
    console::plain(format!(
        "    Artifact Version: {}",
        string_field(version, &["artifact_version", "artifactVersion"]).unwrap_or("-")
    ));
    console::plain(format!(
        "    Lambda Deployment Version: {}",
        string_field(version, &["function_deployment_version", "lambdaDeploymentVersion"])
            .unwrap_or("-")
    ));
    console::plain(format!(
        "    Client Environment: {}",
        string_field(version, &["client_env", "clientEnv"]).unwrap_or("-")
    ));
    console::plain(format!(
        "    Message: {}",
        version
            .pointer("/deployment_metadata/deployment_message")
            .and_then(serde_json::Value::as_str)
            .filter(|message| !message.is_empty())
            .unwrap_or("-")
    ));
}

fn deployment_active_badges(
    version_hash: &str,
    active_deployment_hashes: &indexmap::IndexMap<String, String>,
) -> String {
    active_deployment_hashes
        .iter()
        .filter_map(|(env, active_hash)| (active_hash == version_hash).then_some(env.as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn string_field<'a>(value: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(serde_json::Value::as_str))
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deployment(id: &str, hash: &str, message: &str) -> serde_json::Value {
        json!({
            "id": id,
            "version_hash": hash,
            "deployment_metadata": {
                "deployment_message": message,
            }
        })
    }

    #[test]
    fn deployment_promote_search_env_matches_python_flow() {
        assert_eq!(deployment_promote_search_env("pre-release"), "sandbox");
        assert_eq!(deployment_promote_search_env("live"), "pre-release");
    }

    #[test]
    fn deployment_promote_selection_resolves_active_alias_and_message_override() {
        let deployments = vec![
            deployment("dep-new", "abc123456xyz", "original"),
            deployment("dep-old", "def789012xyz", "previous"),
        ];
        let mut active = indexmap::IndexMap::new();
        active.insert("sandbox".to_string(), "abc123456xyz".to_string());
        active.insert("pre-release".to_string(), "def789012xyz".to_string());

        let selection = select_deployment_for_promotion(
            &deployments,
            &active,
            "sandbox",
            "pre-release",
            Some("release notes"),
            "sandbox",
        )
        .expect("selection");

        assert_eq!(selection.deployment_id, "dep-new");
        assert_eq!(selection.from_hash, "abc123456xyz");
        assert_eq!(selection.message, "release notes");
        assert_eq!(
            selection.predecessor_hash.as_deref(),
            Some("def789012xyz")
        );
        assert_eq!(selection.display_message, "original");
    }

    #[test]
    fn deployment_promote_selection_defaults_to_deployment_message() {
        let deployments = vec![deployment("dep-new", "abc123456xyz", "original")];
        let active = indexmap::IndexMap::new();

        let selection = select_deployment_for_promotion(
            &deployments,
            &active,
            "abc123456",
            "pre-release",
            None,
            "sandbox",
        )
        .expect("selection");

        assert_eq!(selection.message, "original");
        assert_eq!(selection.predecessor_hash, None);
    }

    #[test]
    fn deployment_promote_selection_reports_not_found_in_search_env() {
        let active = indexmap::IndexMap::new();

        assert_eq!(
            select_deployment_for_promotion(
                &[],
                &active,
                "missing",
                "live",
                None,
                "pre-release",
            ),
            Err(DeploymentPromotionSelectionError::NotFound {
                requested: "missing".to_string(),
                search_env: "pre-release",
            })
        );
    }

    #[test]
    fn deployment_promote_selection_rejects_deployment_without_id() {
        let deployments = vec![json!({"version_hash": "abc123456xyz"})];
        let active = indexmap::IndexMap::new();

        assert_eq!(
            select_deployment_for_promotion(
                &deployments,
                &active,
                "abc123456",
                "pre-release",
                None,
                "sandbox",
            ),
            Err(DeploymentPromotionSelectionError::MissingId)
        );
    }

    #[test]
    fn deployment_promote_result_payload_keeps_cli_json_contract() {
        let selection = DeploymentPromotionSelection {
            deployment_id: "dep-new".to_string(),
            from_hash: "abc123456xyz".to_string(),
            message: "release notes".to_string(),
            predecessor_hash: Some("def789012xyz".to_string()),
            display_message: "release notes".to_string(),
        };

        assert_eq!(
            deployment_promote_result_payload(
                "pre-release",
                &selection,
                vec![deployment("dep-new", "abc123456xyz", "release notes")]
            ),
            json!({
                "success": false,
                "to_env": "pre-release",
                "from_hash": "abc123456xyz",
                "message": "release notes",
                "included_deployments": [
                    deployment("dep-new", "abc123456xyz", "release notes")
                ],
            })
        );
    }

    #[test]
    fn deployment_promote_confirmation_skips_prompt_for_noninteractive_modes() {
        assert_eq!(confirm_deployment_promote_or_exit(true, false), Ok(()));
        assert_eq!(confirm_deployment_promote_or_exit(false, true), Ok(()));
    }

    #[test]
    fn deployment_promote_preview_handles_forward_and_rollback_shapes() {
        let selection = DeploymentPromotionSelection {
            deployment_id: "dep-new".to_string(),
            from_hash: "abc123456xyz".to_string(),
            message: "release notes".to_string(),
            predecessor_hash: None,
            display_message: "release notes".to_string(),
        };
        let result = deployment_promote_result_payload(
            "pre-release",
            &selection,
            vec![deployment("dep-new", "abc123456xyz", "release notes")],
        );

        let preview = deployment_promote_preview("pre-release", &result, &selection, false);
        assert_eq!(preview.hash_prefix, "abc123456");
        assert_eq!(
            preview.note.as_deref(),
            Some("First deployment to pre-release.")
        );
        assert_eq!(preview.included_label, Some("Included deployments"));
        assert_eq!(preview.included_items.expect("included items").len(), 1);

        let rollback_selection = DeploymentPromotionSelection {
            predecessor_hash: Some("def789012xyz".to_string()),
            ..selection
        };
        let rollback_preview =
            deployment_promote_preview("pre-release", &result, &rollback_selection, true);
        assert_eq!(
            rollback_preview.note.as_deref(),
            Some("Rolling back to an earlier version: release notes")
        );
        assert_eq!(rollback_preview.included_label, Some("Reverting deployments"));
    }
}
