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
    let search_env = if args.to_env == "live" {
        "pre-release"
    } else {
        "sandbox"
    };
    let deployments = match service.list_deployments(search_env) {
        Ok(deployments) => deployments,
        Err(error) => {
            emit_error(args.json, &error.to_string());
            return ExitCode::from(1);
        }
    };
    let deployment_hash_or_alias = deployments
        .active_deployment_hashes
        .get(&args.from_deployment)
        .map(String::as_str)
        .unwrap_or(args.from_deployment.as_str());
    let prefix = deployment_hash_prefix(deployment_hash_or_alias);
    let Some((_, deployment)) = find_deployment_by_prefix(&deployments.versions, &prefix) else {
        return print_deployment_json_or_error(
            args.json,
            json!({
                "success": false,
                "to_env": args.to_env,
                "error": format!("Deployment '{}' not found in {search_env}.", args.from_deployment),
            }),
        );
    };
    let deployment = deployment.clone();
    let Some(deployment_id) = deployment_id(&deployment).map(ToString::to_string) else {
        emit_error(args.json, "Selected deployment does not include an id.");
        return ExitCode::from(1);
    };
    let from_hash = deployment_hash(&deployment).unwrap_or_default().to_string();
    let deployment_message = deployment_message(&deployment).unwrap_or("");
    let message = args
        .message
        .clone()
        .unwrap_or_else(|| deployment_message.to_string());
    let predecessor_hash = deployments
        .active_deployment_hashes
        .get(&args.to_env)
        .map(String::as_str);
    let sandbox_versions = if search_env == "sandbox" {
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
        resolve_included_deployments(&sandbox_versions, &from_hash, predecessor_hash);
    let mut result = json!({
        "success": false,
        "to_env": args.to_env,
        "from_hash": from_hash,
        "message": message,
        "included_deployments": included,
    });

    if !args.json {
        console::plain(format!(
            "Promoting hash [bold]{}[/bold] to [info]{}[/info]",
            result
                .get("from_hash")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .chars()
                .take(9)
                .collect::<String>(),
            args.to_env
        ));
        if is_rollback {
            console::plain(format!(
                "Rolling back to an earlier version: {}",
                deployment_message_or_dash(&deployment)
            ));
        } else if predecessor_hash.is_none() {
            console::plain(format!("First deployment to {}.", args.to_env));
        }
        if let Some(items) = result
            .get("included_deployments")
            .and_then(serde_json::Value::as_array)
            && !items.is_empty()
        {
            let label = if is_rollback {
                "Reverting deployments"
            } else {
                "Included deployments"
            };
            console::plain(format!("{label} ({}):", items.len()));
            print_deployment_versions(items, &indexmap::IndexMap::new(), false);
        }
    }

    if args.dry_run {
        result["dry_run"] = json!(true);
        return print_deployment_dry_run(args.json, result);
    }
    if !args.json && !args.force {
        match prompt_confirm("Confirm Deployment?") {
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

    match service.promote_deployment(
        &deployment_id,
        &args.to_env,
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
                    "Deployment {} promoted to {}.",
                    args.from_deployment, args.to_env
                ));
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if args.json {
                result["error"] = json!(error.to_string());
                println!("{result}");
            } else {
                emit_error(false, &format!("Failed to promote deployment: {error}"));
            }
            ExitCode::from(1)
        }
    }
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
