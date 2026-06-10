use adk_api_client::{PlatformClient, ProjectionSnapshot};
use adk_core::*;
use adk_io::{FileSystem, StdFileSystem, diff_resources, parse_multi_resource_path};
use adk_resources::{
    FileStructureEntry, ResourceStatusPayloadInput, StatusResourcePayload, StatusSnapshot,
    clean_name, current_status_hash_for_expected, extract_variable_names_from_code,
    flow_folder_name, legacy_python_snapshot_hashes, local_resource_content,
    normalize_legacy_python_status_function_resources, resource_file_content,
    resource_status_file_hash, resource_status_payload,
};
use adk_types::{
    BranchDescriptor, BranchMergeResult, ConversationDetail, ConversationListResponse,
    DeploymentList, DiffMap, DomainError, ProjectConfig, PushResult, Resource, ResourceMap,
    StatusSummary,
};
use serde_json::{self, Value as JsonValue};
use serde_yaml_ng::{Value as YamlValue, from_str, to_string};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("{0}")]
    Core(#[from] adk_core::CoreError),
    #[error("{0}")]
    Api(#[from] adk_api_client::ApiError),
}

impl From<DomainError> for ServiceError {
    fn from(error: DomainError) -> Self {
        adk_core::CoreError::from(error).into()
    }
}

impl From<std::io::Error> for ServiceError {
    fn from(error: std::io::Error) -> Self {
        adk_core::CoreError::from(error).into()
    }
}

impl From<serde_json::Error> for ServiceError {
    fn from(error: serde_json::Error) -> Self {
        adk_core::CoreError::from(error).into()
    }
}

impl From<adk_resources::CommandGenError> for ServiceError {
    fn from(error: adk_resources::CommandGenError) -> Self {
        adk_core::CoreError::from(error).into()
    }
}

pub struct AdkService<C, Fs = StdFileSystem> {
    client: C,
    workspace: ProjectWorkspace<Fs>,
}

pub struct PullOutcome {
    pub files_with_conflicts: Vec<String>,
    pub new_branch_name: Option<String>,
    pub new_branch_id: Option<String>,
}

enum PushChangeSet {
    FullSnapshot(ResourceMap),
    ChangedOnly(ChangedResourceMap),
}

impl PushChangeSet {
    fn is_empty(&self) -> bool {
        match self {
            Self::FullSnapshot(resources) => resources.is_empty(),
            Self::ChangedOnly(resources) => resources.is_empty(),
        }
    }
}

impl<C: PlatformClient> AdkService<C, StdFileSystem> {
    pub fn new(client: C) -> Self {
        Self::with_file_system(client, StdFileSystem)
    }
}

impl<C: PlatformClient, Fs: FileSystem> AdkService<C, Fs> {
    pub fn with_file_system(client: C, fs: Fs) -> Self {
        Self {
            client,
            workspace: ProjectWorkspace::with_file_system(fs),
        }
    }

    pub fn init_project(
        &self,
        base_path: &Path,
        region: String,
        account_id: String,
        project_id: String,
    ) -> Result<ProjectConfig, ServiceError> {
        Ok(self
            .workspace
            .init_project(base_path, region, account_id, project_id)?)
    }

    pub fn init_project_with_name(
        &self,
        base_path: &Path,
        region: String,
        account_id: String,
        project_id: String,
        project_name: Option<String>,
    ) -> Result<ProjectConfig, ServiceError> {
        Ok(self.workspace.init_project_with_name(
            base_path,
            region,
            account_id,
            project_id,
            project_name,
        )?)
    }

    pub fn load_project_config(&self, base_path: &Path) -> Result<ProjectConfig, ServiceError> {
        Ok(self.workspace.load_project_config(base_path)?)
    }

    pub fn collect_local_resources(&self, root: &Path) -> Result<ResourceMap, ServiceError> {
        Ok(self.workspace.collect_local_resources(root)?)
    }

    /// Typed discovery matching Python `AgentStudioProject.discover_local_resources()`:
    /// logical paths per resource type, keyed by Python class name (`Topic`, `Entity`, ...).
    pub fn discover_local_resources(&self, root: &Path) -> indexmap::IndexMap<String, Vec<String>> {
        self.workspace.discover_local_resources(root)
    }

    /// Typed parity helper matching Python `find_new_kept_deleted` semantics at path level.
    pub fn find_new_kept_deleted(
        &self,
        discovered_resources: &DiscoveredResourcePaths,
        existing_resources: &DiscoveredResourcePaths,
    ) -> DiscoveredResourceChanges {
        self.workspace
            .find_new_kept_deleted(discovered_resources, existing_resources)
    }

    pub fn typed_resource_lifecycle(
        &self,
        root: &Path,
    ) -> Result<Vec<TypedResourceLifecycle>, ServiceError> {
        Ok(self.workspace.typed_resource_lifecycle(root)?)
    }

    pub fn status(&self, root: &Path) -> Result<StatusSummary, ServiceError> {
        Ok(self.workspace.status(root)?)
    }

    /// Computes resource diffs for either local project changes or named states.
    ///
    /// With `before` or `after` set, this resolves both sides as named states
    /// such as a deployment version, environment, branch snapshot, or `local`.
    /// If `before` is omitted, it derives the previous deployment version for
    /// `after`, matching the Python CLI's `--after` behavior. Both sides are then
    /// normalized before diffing so flow imports and function references compare
    /// by the same logical identity.
    ///
    /// With neither side set, this compares the current local resources against
    /// the best available baseline: replay state, typed status snapshot, or a
    /// fresh remote pull. Typed snapshots are used to limit the diff to resources
    /// that are new, deleted, or hash-modified, which avoids reporting
    /// materialization-only churn. `files` is applied last as a path/glob filter.
    pub fn diff(
        &self,
        root: &Path,
        files: &[String],
        before: Option<String>,
        after: Option<String>,
    ) -> Result<DiffMap, ServiceError> {
        if before.is_some() || after.is_some() {
            let mut before_name = before.unwrap_or_default();
            let mut after_name = after.unwrap_or_default();
            if before_name.is_empty() {
                let client_env = if after_name == "pre-release" || after_name == "live" {
                    after_name.as_str()
                } else {
                    "sandbox"
                };
                let deployments = self.client.list_deployments(client_env)?;
                if let Some(active_hash) = deployments.active_deployment_hashes.get(&after_name) {
                    after_name = active_hash.clone();
                }
                if deployments.versions.is_empty() {
                    return Err(DomainError::InvalidData("No versions found.".to_string()).into());
                }
                let after_prefix = after_name
                    .chars()
                    .take(9)
                    .collect::<String>()
                    .to_lowercase();
                let version_idx = deployments.versions.iter().position(|version| {
                    version
                        .get("version_hash")
                        .or_else(|| version.get("versionHash"))
                        .or_else(|| version.get("hash"))
                        .and_then(JsonValue::as_str)
                        .map(|v| {
                            v.chars().take(9).collect::<String>().to_lowercase() == after_prefix
                        })
                        .unwrap_or(false)
                });
                let Some(version_idx) = version_idx else {
                    return Err(DomainError::InvalidData(format!(
                        "Version hash '{after_name}' not found."
                    ))
                    .into());
                };
                if version_idx == deployments.versions.len() - 1 {
                    return Err(
                        DomainError::InvalidData("No previous version found.".to_string()).into(),
                    );
                }
                let previous = &deployments.versions[version_idx + 1];
                before_name = previous
                    .get("version_hash")
                    .or_else(|| previous.get("versionHash"))
                    .or_else(|| previous.get("hash"))
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .chars()
                    .take(9)
                    .collect::<String>();
            }
            if after_name.is_empty() {
                after_name = "local".to_string();
            }
            let mut before_state = self.resolve_named_state(root, &before_name)?;
            let mut after_state = self.resolve_named_state(root, &after_name)?;
            normalize_flow_resources_for_diff(&mut before_state, None);
            normalize_flow_resources_for_diff(&mut after_state, Some(&before_state));
            if before_name != "local" {
                normalize_function_references_in_rules(&mut before_state);
                normalize_function_references_in_rules(&mut after_state);
            }
            let mut diffs = diff_resources(&before_state, &after_state);
            if !files.is_empty() {
                let matcher = build_file_matcher(files)?;
                diffs.retain(|k, _| matcher.is_match(k));
            }
            return Ok(diffs);
        }
        let mut local = self.collect_local_resources(root)?;
        let snapshot_hashes = self.load_status_snapshot_file_hashes(root)?;
        let using_legacy_python_snapshot = snapshot_hashes
            .as_ref()
            .is_some_and(legacy_python_snapshot_hashes);
        let status_snapshot_resources = self.workspace.load_status_snapshot_resource_map(root)?;
        let using_status_snapshot_resources = status_snapshot_resources.is_some();
        let remote = if let Some(resources) = self.load_replay_state_resources(root)? {
            resources
        } else if let Some(resources) = status_snapshot_resources {
            resources
        } else {
            self.client.pull_resources()?
        };
        let mut changed_file_paths = None;
        let mut deleted_file_paths = HashSet::new();

        if let Some(existing_typed) = self.load_status_snapshot_discovered_resources(root)? {
            let discovered_typed = self.discover_local_resources(root);
            let typed_changes = self.find_new_kept_deleted(&discovered_typed, &existing_typed);
            deleted_file_paths = flatten_discovered_paths(&typed_changes.deleted_resources)
                .iter()
                .map(|p| parse_multi_resource_path(p).0)
                .collect();
            let replacements = self
                .deleted_resource_reference_replacements(root, &typed_changes.deleted_resources)?;
            if !replacements.is_empty() {
                apply_reference_name_replacements(&mut local, &replacements);
            }
            let mut changed_paths = flatten_discovered_paths(&typed_changes.new_resources)
                .into_iter()
                .chain(flatten_discovered_paths(&typed_changes.deleted_resources))
                .collect::<Vec<_>>();
            if let Some(snapshot_hashes) = snapshot_hashes.as_ref() {
                changed_paths.extend(compute_modified_files_against_snapshot(
                    root,
                    &typed_changes.kept_resources,
                    snapshot_hashes,
                )?);
                changed_paths.extend(compute_modified_files_against_snapshot_with_replacements(
                    root,
                    &typed_changes.kept_resources,
                    snapshot_hashes,
                    &replacements,
                )?);
            }

            if changed_paths.is_empty() {
                changed_file_paths = Some(HashSet::new());
            } else {
                changed_file_paths = Some(
                    changed_paths
                        .iter()
                        .map(|p| parse_multi_resource_path(p).0)
                        .collect(),
                );
            }
        }

        let mut remote = remote;
        if (using_legacy_python_snapshot || using_status_snapshot_resources)
            && let Some(snapshot_hashes) = snapshot_hashes.as_ref()
        {
            normalize_legacy_python_status_function_resources(&mut local, snapshot_hashes);
        }
        normalize_flow_resources_for_diff(&mut remote, None);
        normalize_flow_resources_for_diff(&mut local, Some(&remote));
        let mut diffs = diff_resources(&remote, &local);

        if let Some(changed_file_paths) = changed_file_paths {
            if changed_file_paths.is_empty() {
                diffs.clear();
            } else {
                diffs.retain(|k, _| changed_file_paths.contains(k));
            }
        }

        if !files.is_empty() {
            let matcher = build_file_matcher(files)?;
            diffs.retain(|k, _| matcher.is_match(k));
        }
        absolutize_deleted_diff_keys(root, &mut diffs, &deleted_file_paths);
        Ok(diffs)
    }

    pub fn push(
        &self,
        root: &Path,
        force: bool,
        skip_validation: bool,
        dry_run: bool,
    ) -> Result<PushResult, ServiceError> {
        self.push_with_options(root, force, skip_validation, dry_run, None)
    }

    /// Pushes local project changes to Agent Studio with explicit CLI options.
    ///
    /// This is the full push implementation behind the simpler `push` wrapper.
    /// Unless `force` is set, it refuses to continue when conflict markers are
    /// present. Unless `skip_validation` is set, it runs local semantic and
    /// Python validation before generating commands. `dry_run` routes the command
    /// batch through preview endpoints instead of mutating remote state.
    ///
    /// `projection` lets tests and offline workflows provide the remote Agent
    /// Studio state directly; otherwise the platform client supplies it. On a
    /// successful real push, the method persists replay/status baselines so later
    /// `status`, `diff`, and conflict checks compare against the state that was
    /// just accepted remotely.
    pub fn push_with_options(
        &self,
        root: &Path,
        force: bool,
        skip_validation: bool,
        dry_run: bool,
        projection: Option<&JsonValue>,
    ) -> Result<PushResult, ServiceError> {
        if !force {
            let conflicted = self.detect_conflict_files(root)?;
            if !conflicted.is_empty() {
                let conflicts = conflicted.join("\n- ");
                return Ok(PushResult {
                    success: false,
                    message: format!(
                        "Merge conflicts detected in the following files:\n- {conflicts}\nPlease resolve the conflicts and try again."
                    ),
                    commands: vec![],
                });
            }
        }
        if !skip_validation {
            let validation_errors = self.validate_local_resources(root)?;
            if !validation_errors.is_empty() {
                return Ok(PushResult {
                    success: false,
                    message: format!(
                        "Validation errors detected:\n{}",
                        validation_errors.join("\n")
                    ),
                    commands: vec![],
                });
            }
        }
        let mut persistent_local = self.collect_local_resources(root)?;
        self.apply_deleted_reference_names(root, &mut persistent_local)?;
        if let Some(mut changes) =
            self.push_resource_map_for_status_changes(root, &persistent_local, projection)?
        {
            if changes.is_empty() {
                return Ok(PushResult {
                    success: false,
                    message: "No changes detected".to_string(),
                    commands: vec![],
                });
            }
            let plan = match &mut changes {
                PushChangeSet::FullSnapshot(resources) => {
                    self.add_discovered_variable_resources(root, resources);
                    self.plan_push_resource_map(resources, projection)?
                }
                PushChangeSet::ChangedOnly(resources) => {
                    self.add_variable_resources_for_changed_resources(resources.as_resources_mut());
                    self.plan_changed_push_resource_map(resources, projection)?
                }
            };
            let result = self.push_command_plan(plan, dry_run)?;
            if result.success && !dry_run {
                self.client.record_successful_push(&persistent_local)?;
                self.save_replay_state_resources(root, &persistent_local)?;
                self.write_status_snapshot_from_resources(root, &persistent_local)?;
            }
            return Ok(result);
        }
        let mut local = persistent_local.clone();
        self.add_discovered_variable_resources(root, &mut local);
        let plan = self.plan_push_resource_map(&local, projection)?;
        let result = self.push_command_plan(plan, dry_run)?;
        if result.success && !dry_run {
            self.client.record_successful_push(&persistent_local)?;
            self.save_replay_state_resources(root, &persistent_local)?;
            self.write_status_snapshot_from_resources(root, &persistent_local)?;
        }
        Ok(result)
    }

    fn projection_snapshot_for_push(
        &self,
        projection: Option<&JsonValue>,
    ) -> Result<ProjectionSnapshot, ServiceError> {
        if let Some(projection) = projection {
            return Ok(ProjectionSnapshot {
                projection: projection.clone(),
                last_known_sequence: 0,
            });
        }
        Ok(self.client.pull_projection_snapshot()?)
    }

    fn plan_push_resource_map(
        &self,
        resources: &ResourceMap,
        projection: Option<&JsonValue>,
    ) -> Result<PushCommandPlan, ServiceError> {
        let snapshot = self.projection_snapshot_for_push(projection)?;
        Ok(plan_push_commands_from_resources(
            resources,
            PushPlanInput {
                projection: snapshot.projection,
                last_known_sequence: snapshot.last_known_sequence,
                created_by: self.client.command_user_override(),
                current_time: None,
            },
        )?)
    }

    fn plan_changed_push_resource_map(
        &self,
        resources: &ChangedResourceMap,
        projection: Option<&JsonValue>,
    ) -> Result<PushCommandPlan, ServiceError> {
        let snapshot = self.projection_snapshot_for_push(projection)?;
        Ok(plan_push_commands_from_changed_resources(
            resources,
            PushPlanInput {
                projection: snapshot.projection,
                last_known_sequence: snapshot.last_known_sequence,
                created_by: self.client.command_user_override(),
                current_time: None,
            },
        )?)
    }

    fn push_command_plan(
        &self,
        plan: PushCommandPlan,
        dry_run: bool,
    ) -> Result<PushResult, ServiceError> {
        if plan.commands.is_empty() {
            return Ok(PushResult {
                success: false,
                message: "No changes detected".to_string(),
                commands: vec![],
            });
        }
        if dry_run {
            return Ok(PushResult {
                success: true,
                message: "Dry run completed. No changes were pushed.".to_string(),
                commands: plan.command_summaries,
            });
        }
        Ok(self.client.push_command_batch(&plan.command_batch_bytes)?)
    }

    fn push_command_plan_to_branch(
        &self,
        branch_id: &str,
        plan: PushCommandPlan,
    ) -> Result<PushResult, ServiceError> {
        if plan.commands.is_empty() {
            return Ok(PushResult {
                success: false,
                message: "No changes detected".to_string(),
                commands: vec![],
            });
        }
        Ok(self
            .client
            .push_command_batch_to_branch(branch_id, &plan.command_batch_bytes)?)
    }

    fn push_resource_map_for_status_changes(
        &self,
        root: &Path,
        persistent_local: &ResourceMap,
        projection: Option<&JsonValue>,
    ) -> Result<Option<PushChangeSet>, ServiceError> {
        if projection.is_some() {
            return Ok(None);
        }
        let Some(existing_typed) = self.load_status_snapshot_discovered_resources(root)? else {
            return Ok(None);
        };
        let discovered_typed = self.discover_local_resources(root);
        let typed_changes = self.find_new_kept_deleted(&discovered_typed, &existing_typed);
        let requires_full_snapshot = !typed_changes.deleted_resources.values().all(Vec::is_empty);
        let mut changed_paths = flatten_discovered_paths(&typed_changes.new_resources)
            .into_iter()
            .chain(flatten_discovered_paths(&typed_changes.deleted_resources))
            .collect::<Vec<_>>();

        if let Some(snapshot_hashes) = self.load_status_snapshot_file_hashes(root)? {
            changed_paths.extend(compute_modified_files_against_snapshot(
                root,
                &typed_changes.kept_resources,
                &snapshot_hashes,
            )?);
            let replacements = self
                .deleted_resource_reference_replacements(root, &typed_changes.deleted_resources)?;
            changed_paths.extend(compute_modified_files_against_snapshot_with_replacements(
                root,
                &typed_changes.kept_resources,
                &snapshot_hashes,
                &replacements,
            )?);
        }

        if changed_paths.is_empty() {
            return Ok(Some(PushChangeSet::ChangedOnly(
                ChangedResourceMap::default(),
            )));
        }

        let changed_file_paths = changed_paths
            .into_iter()
            .map(|path| parse_multi_resource_path(&path).0)
            .collect::<BTreeSet<_>>();
        let metadata = self
            .workspace
            .load_status_snapshot_resource_metadata(root)?;
        let mut resources = if requires_full_snapshot {
            persistent_local.clone()
        } else {
            ResourceMap::new()
        };
        for file_path in changed_file_paths {
            if let Some(resource) = persistent_local.get(&file_path) {
                let mut resource = resource.clone();
                if let Some((id, name)) = metadata.get(&file_path) {
                    resource.resource_id = id.clone();
                    if !name.is_empty() {
                        resource.name = name.clone();
                    }
                }
                resources.insert(file_path, resource);
            } else {
                resources.shift_remove(&file_path);
            }
        }
        if requires_full_snapshot {
            Ok(Some(PushChangeSet::FullSnapshot(resources)))
        } else {
            Ok(Some(PushChangeSet::ChangedOnly(ChangedResourceMap::new(
                resources,
            ))))
        }
    }

    fn add_discovered_variable_resources(&self, root: &Path, local: &mut ResourceMap) {
        let project_root = find_project_root_with_fs(self.workspace.file_system(), root)
            .unwrap_or_else(|| root.to_path_buf());
        add_discovered_variable_resources_from_fs(
            self.workspace.file_system(),
            &project_root,
            local,
        );
    }

    fn add_variable_resources_for_changed_resources(&self, local: &mut ResourceMap) {
        let variable_names = local
            .values()
            .filter_map(|resource| resource.payload.get("content").and_then(JsonValue::as_str))
            .flat_map(extract_variable_names_from_code)
            .collect::<BTreeSet<_>>();
        for name in variable_names {
            let logical_path = format!("variables/{name}");
            if local.contains_key(&logical_path) {
                continue;
            }
            local.insert(
                logical_path.clone(),
                Resource {
                    resource_id: "local".to_string(),
                    name,
                    file_path: logical_path,
                    payload: serde_json::json!({ "content": "" }),
                },
            );
        }
    }

    pub fn push_main_to_new_branch(
        &self,
        root: &Path,
        branch_name: &str,
        force: bool,
        skip_validation: bool,
    ) -> Result<(ProjectConfig, PushResult), ServiceError> {
        if !force {
            let conflicted = self.detect_conflict_files(root)?;
            if !conflicted.is_empty() {
                let conflicts = conflicted.join("\n- ");
                return Ok((
                    self.load_project_config(root)?,
                    PushResult {
                        success: false,
                        message: format!(
                            "Merge conflicts detected in the following files:\n- {conflicts}\nPlease resolve the conflicts and try again."
                        ),
                        commands: vec![],
                    },
                ));
            }
        }
        if !skip_validation {
            let validation_errors = self.validate_local_resources(root)?;
            if !validation_errors.is_empty() {
                return Ok((
                    self.load_project_config(root)?,
                    PushResult {
                        success: false,
                        message: format!(
                            "Validation errors detected:\n{}",
                            validation_errors.join("\n")
                        ),
                        commands: vec![],
                    },
                ));
            }
        }
        let mut persistent_local = self.collect_local_resources(root)?;
        self.apply_deleted_reference_names(root, &mut persistent_local)?;
        let mut local = self
            .push_resource_map_for_status_changes(root, &persistent_local, None)?
            .and_then(|changes| match changes {
                PushChangeSet::FullSnapshot(resources) => Some(resources),
                PushChangeSet::ChangedOnly(_) => None,
            })
            .unwrap_or_else(|| persistent_local.clone());
        self.add_discovered_variable_resources(root, &mut local);
        let (branch_id, snapshot) = self.client.create_branch_from_main(branch_name)?;
        let plan = plan_push_commands_from_resources(
            &local,
            PushPlanInput {
                projection: snapshot.projection,
                last_known_sequence: snapshot.last_known_sequence,
                created_by: self.client.command_user_override(),
                current_time: None,
            },
        )?;
        let push_result = self.push_command_plan_to_branch(&branch_id, plan)?;
        let mut cfg = self.load_project_config(root)?;
        if push_result.success {
            cfg.branch_id = branch_id;
            self.write_project_config(root, &cfg)?;
            self.client.record_successful_push(&persistent_local)?;
            self.save_replay_state_resources(root, &persistent_local)?;
            self.write_status_snapshot_from_resources(root, &persistent_local)?;
        }
        Ok((cfg, push_result))
    }

    pub fn pull(&self, root: &Path, force: bool) -> Result<Vec<String>, ServiceError> {
        self.pull_with_format(root, force, false)
    }

    pub fn pull_with_format(
        &self,
        root: &Path,
        force: bool,
        format: bool,
    ) -> Result<Vec<String>, ServiceError> {
        Ok(self
            .pull_detailed_with_format(root, force, format)?
            .files_with_conflicts)
    }

    pub fn pull_resource_map_with_format(
        &self,
        root: &Path,
        resources: ResourceMap,
        force: bool,
        format: bool,
    ) -> Result<Vec<String>, ServiceError> {
        self.write_pulled_resources(root, resources, force, format)
    }

    pub fn pull_detailed_with_format(
        &self,
        root: &Path,
        force: bool,
        format: bool,
    ) -> Result<PullOutcome, ServiceError> {
        let remote = if let Some(resources) = self.load_replay_state_resources(root)? {
            resources
        } else {
            let (resources, new_branch) = self.pull_resources_with_branch_reconciliation(root)?;
            let conflicts = self.write_pulled_resources(root, resources, force, format)?;
            return Ok(PullOutcome {
                files_with_conflicts: conflicts,
                new_branch_name: new_branch.as_ref().map(|branch| branch.name.clone()),
                new_branch_id: new_branch.map(|branch| branch.branch_id),
            });
        };
        let conflicts = self.write_pulled_resources(root, remote, force, format)?;
        Ok(PullOutcome {
            files_with_conflicts: conflicts,
            new_branch_name: None,
            new_branch_id: None,
        })
    }

    pub fn pull_projection_json(&self) -> Result<JsonValue, ServiceError> {
        Ok(self.client.pull_projection_json()?)
    }

    pub fn pull_projection_json_by_name(&self, name: &str) -> Result<JsonValue, ServiceError> {
        Ok(self.client.pull_projection_json_by_name(name)?)
    }

    pub fn pull_named(
        &self,
        root: &Path,
        name: &str,
        force: bool,
    ) -> Result<Vec<String>, ServiceError> {
        self.pull_named_with_format(root, name, force, false)
    }

    pub fn pull_named_with_format(
        &self,
        root: &Path,
        name: &str,
        force: bool,
        format: bool,
    ) -> Result<Vec<String>, ServiceError> {
        let remote = if let Some(resources) = self.load_replay_state_resources(root)? {
            resources
        } else {
            self.client.pull_resources_by_name(name)?
        };
        self.write_pulled_resources(root, remote, force, format)
    }

    fn pull_resources_with_branch_reconciliation(
        &self,
        root: &Path,
    ) -> Result<(ResourceMap, Option<BranchDescriptor>), ServiceError> {
        let cfg = self.load_project_config(root)?;
        if cfg.branch_id == "main" {
            return Ok((self.client.pull_resources()?, None));
        }
        let branches = self.client.list_branches()?;
        if branches
            .iter()
            .any(|branch| branch.branch_id == cfg.branch_id || branch.name == cfg.branch_id)
        {
            return Ok((self.client.pull_resources()?, None));
        }
        let Some(branch) = branches
            .iter()
            .find(|branch| branch.name == "main" || branch.branch_id == "main")
            .or_else(|| branches.first())
            .cloned()
        else {
            return Ok((self.client.pull_resources()?, None));
        };
        let mut updated = cfg;
        updated.branch_id = branch.branch_id.clone();
        self.write_project_config(root, &updated)?;
        Ok((
            self.client.pull_resources_by_name(&branch.name)?,
            Some(branch),
        ))
    }

    fn pull_status_hashes_for_file<'a>(
        path: &str,
        snapshot_hashes: &'a indexmap::IndexMap<String, String>,
    ) -> Option<Vec<(&'a str, &'a str)>> {
        let entries = snapshot_hashes
            .iter()
            .filter_map(|(hash_path, expected_hash)| {
                let hash_path = hash_path.as_str();
                if hash_path == path
                    || (hash_path.contains(".yaml/")
                        && parse_multi_resource_path(hash_path).0 == path)
                {
                    Some((hash_path, expected_hash.as_str()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if entries.is_empty() {
            None
        } else {
            Some(entries)
        }
    }

    fn status_hashes_for_content(
        status_hashes: &[(&str, &str)],
        content: &str,
        snapshot_hashes: &indexmap::IndexMap<String, String>,
    ) -> Vec<String> {
        status_hashes
            .iter()
            .map(|(hash_path, expected_hash)| {
                current_status_hash_for_expected(hash_path, content, expected_hash, snapshot_hashes)
            })
            .collect()
    }

    fn status_hashes_changed(status_hashes: &[(&str, &str)], current_hashes: &[String]) -> bool {
        status_hashes
            .iter()
            .zip(current_hashes)
            .any(|((_, expected_hash), current_hash)| current_hash != expected_hash)
    }

    fn status_resource_paths(status_hashes: &[(&str, &str)]) -> BTreeSet<String> {
        status_hashes
            .iter()
            .filter_map(|(hash_path, _)| {
                parse_multi_resource_path(hash_path)
                    .1
                    .map(|_| (*hash_path).to_string())
            })
            .collect()
    }

    fn yaml_status_resource_paths(path: &str, content: &str) -> Option<BTreeSet<String>> {
        let yaml = from_str::<YamlValue>(content).ok()?;
        let mapping = yaml.as_mapping()?;
        let mut paths = BTreeSet::new();
        for (key, value) in mapping {
            let Some(top_level_name) = key.as_str() else {
                continue;
            };
            if let Some(items) = value.as_sequence() {
                for (index, item) in items.iter().enumerate() {
                    if top_level_name == "pronunciations" {
                        paths.insert(format!("{path}/{top_level_name}/{index}"));
                        continue;
                    }
                    if let Some(name) = item.get("name").and_then(YamlValue::as_str) {
                        let name = clean_name(name, false);
                        paths.insert(format!("{path}/{top_level_name}/{name}"));
                    }
                }
            } else {
                paths.insert(format!("{path}/{top_level_name}"));
            }
        }
        Some(paths)
    }

    fn write_pulled_resources(
        &self,
        root: &Path,
        remote: ResourceMap,
        force: bool,
        format: bool,
    ) -> Result<Vec<String>, ServiceError> {
        let mut files_with_conflicts = Vec::new();
        let mut written_paths = Vec::new();
        let local_resources_before_force = force.then(|| self.discover_local_resources(root));
        let snapshot_hashes = if force {
            None
        } else {
            self.load_status_snapshot_file_hashes(root)?
        };
        for (path, resource) in &remote {
            let target = root.join(path);
            let content = resource
                .payload
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let file_content = resource_file_content(path, &content);
            if self.workspace.file_system().exists(&target) && !force {
                let existing = self
                    .workspace
                    .file_system()
                    .read_to_string(&target)
                    .unwrap_or_default();
                if existing.contains("<<<<<<<")
                    && existing.contains("=======")
                    && existing.contains(">>>>>>>")
                {
                    files_with_conflicts.push(target.to_string_lossy().to_string());
                    continue;
                }
                if let Some(hashes) = snapshot_hashes.as_ref()
                    && let Some(status_hashes) = Self::pull_status_hashes_for_file(path, hashes)
                {
                    let local_hashes =
                        Self::status_hashes_for_content(&status_hashes, &existing, hashes);
                    let incoming_hashes =
                        Self::status_hashes_for_content(&status_hashes, &file_content, hashes);
                    let snapshot_resource_paths = Self::status_resource_paths(&status_hashes);
                    let local_resource_paths_changed = !snapshot_resource_paths.is_empty()
                        && Self::yaml_status_resource_paths(path, &existing)
                            .is_some_and(|paths| paths != snapshot_resource_paths);
                    let incoming_resource_paths_changed = !snapshot_resource_paths.is_empty()
                        && Self::yaml_status_resource_paths(path, &file_content)
                            .is_some_and(|paths| paths != snapshot_resource_paths);
                    let local_changed = Self::status_hashes_changed(&status_hashes, &local_hashes)
                        || local_resource_paths_changed;
                    let incoming_changed =
                        Self::status_hashes_changed(&status_hashes, &incoming_hashes)
                            || incoming_resource_paths_changed;
                    if !incoming_changed
                        || (local_hashes == incoming_hashes && !incoming_resource_paths_changed)
                    {
                        continue;
                    }
                    if local_changed && incoming_changed && existing != file_content {
                        let merged = format!(
                            "<<<<<<< local\n{existing}\n=======\n{file_content}\n>>>>>>> remote\n"
                        );
                        self.workspace
                            .file_system()
                            .write_string(&target, &merged)?;
                        files_with_conflicts.push(target.to_string_lossy().to_string());
                        continue;
                    }
                }
            }
            if let Some(parent) = target.parent() {
                self.workspace.file_system().create_dir_all(parent)?;
            }
            self.workspace
                .file_system()
                .write_string(&target, &file_content)?;
            written_paths.push(path.clone());
        }
        if let Some(local_resources) = local_resources_before_force.as_ref() {
            delete_local_only_resource_files(root, &remote, local_resources)?;
            delete_empty_subdirectories(&root.join("flows"))?;
        }
        if files_with_conflicts.is_empty() {
            let mut snapshot_resources = remote.clone();
            if format && !written_paths.is_empty() {
                self.format_local_resources(root, &written_paths, false)?;
                for path in &written_paths {
                    if let Some(resource) = snapshot_resources.get_mut(path)
                        && let Some(payload) = resource.payload.as_object_mut()
                    {
                        let formatted_content = self
                            .workspace
                            .file_system()
                            .read_to_string(&root.join(path))?;
                        payload.insert(
                            "content".to_string(),
                            JsonValue::String(local_resource_content(path, &formatted_content)),
                        );
                    }
                }
            }
            self.save_replay_state_resources(root, &snapshot_resources)?;
            self.write_status_snapshot_from_resources(root, &snapshot_resources)?;
        }
        Ok(files_with_conflicts)
    }

    pub fn revert_changes(
        &self,
        root: &Path,
        files: &[String],
    ) -> Result<Vec<String>, ServiceError> {
        let remote = self.client.pull_resources()?;
        let all_files = files.is_empty();
        let selected: std::collections::HashSet<&str> = files.iter().map(String::as_str).collect();
        let mut reverted = Vec::new();
        for (path, resource) in remote {
            let target = root.join(&path);
            let target_abs = if target.is_absolute() {
                target.clone()
            } else {
                self.workspace
                    .file_system()
                    .current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(&target)
            };
            let target_abs_str = target_abs.to_string_lossy().to_string();
            if !all_files && !selected.contains(target_abs_str.as_str()) {
                continue;
            }
            if let Some(parent) = target.parent() {
                self.workspace.file_system().create_dir_all(parent)?;
            }
            let content = resource
                .payload
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            self.workspace
                .file_system()
                .write_string(&target, &resource_file_content(&path, &content))?;
            reverted.push(target_abs_str);
        }
        Ok(reverted)
    }

    pub fn list_deployments(&self, environment: &str) -> Result<DeploymentList, ServiceError> {
        Ok(self.client.list_deployments(environment)?)
    }

    pub fn promote_deployment(
        &self,
        deployment_id: &str,
        target_env: &str,
        message: &str,
    ) -> Result<JsonValue, ServiceError> {
        Ok(self
            .client
            .promote_deployment(deployment_id, target_env, message)?)
    }

    pub fn rollback_deployment(
        &self,
        deployment_id: &str,
        message: &str,
    ) -> Result<JsonValue, ServiceError> {
        Ok(self.client.rollback_deployment(deployment_id, message)?)
    }

    pub fn create_chat_session(&self, payload: JsonValue) -> Result<JsonValue, ServiceError> {
        Ok(self.client.create_chat_session(payload)?)
    }

    pub fn send_chat_message(&self, payload: JsonValue) -> Result<JsonValue, ServiceError> {
        Ok(self.client.send_chat_message(payload)?)
    }

    pub fn end_chat_session(&self, payload: JsonValue) -> Result<JsonValue, ServiceError> {
        Ok(self.client.end_chat_session(payload)?)
    }

    pub fn list_conversations(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<ConversationListResponse, ServiceError> {
        Ok(self.client.list_conversations(limit, offset)?)
    }

    pub fn get_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<ConversationDetail, ServiceError> {
        Ok(self.client.get_conversation(conversation_id)?)
    }

    pub fn get_conversation_audio(
        &self,
        conversation_id: &str,
        direction: &str,
        redacted: bool,
    ) -> Result<Vec<u8>, ServiceError> {
        Ok(self
            .client
            .get_conversation_audio(conversation_id, direction, redacted)?)
    }

    pub fn conversation_url(
        &self,
        root: &Path,
        conversation_id: &str,
    ) -> Result<String, ServiceError> {
        let cfg = self.load_project_config(root)?;
        let short_region = match cfg.region.as_str() {
            "uk-1" => "uk",
            "euw-1" => "eu",
            "us-1" => "us",
            other => other,
        };
        Ok(format!(
            "https://studio.{short_region}.poly.ai/{}/{}/conversations/{conversation_id}",
            cfg.account_id, cfg.project_id
        ))
    }

    pub fn current_branch(&self, root: &Path) -> Result<String, ServiceError> {
        Ok(self.load_project_config(root)?.branch_id)
    }

    pub fn current_branch_name(&self, root: &Path) -> Result<String, ServiceError> {
        let current_branch_id = self.current_branch(root)?;
        Ok(self
            .current_branch_name_optional(root)?
            .unwrap_or(current_branch_id))
    }

    pub fn current_branch_name_optional(
        &self,
        root: &Path,
    ) -> Result<Option<String>, ServiceError> {
        let current_branch_id = self.current_branch(root)?;
        Ok(self
            .client
            .list_branches()?
            .into_iter()
            .find(|branch| {
                branch.branch_id == current_branch_id || branch.name == current_branch_id
            })
            .map(|branch| branch.name))
    }

    pub fn list_known_branches(&self, root: &Path) -> Result<Vec<String>, ServiceError> {
        let _ = root;
        let names: Vec<String> = self
            .client
            .list_branches()?
            .into_iter()
            .map(|branch| branch.name)
            .collect();
        Ok(names)
    }

    pub fn list_branch_map(
        &self,
        root: &Path,
    ) -> Result<indexmap::IndexMap<String, String>, ServiceError> {
        let _ = self.load_project_config(root)?;
        let mut branches = indexmap::IndexMap::new();
        for branch in self.client.list_branches()? {
            branches.insert(branch.name, branch.branch_id);
        }
        Ok(branches)
    }

    pub fn create_branch(
        &self,
        root: &Path,
        branch_name: &str,
    ) -> Result<ProjectConfig, ServiceError> {
        let mut cfg = self.load_project_config(root)?;
        if cfg.branch_id != "main" {
            return Err(DomainError::InvalidData(
                "Branches can only be created from the main branch (sandbox).".to_string(),
            )
            .into());
        }
        let branch_id = self.client.create_branch(branch_name)?;
        cfg.branch_id = branch_id;
        self.write_project_config(root, &cfg)?;
        Ok(cfg)
    }

    pub fn set_branch(
        &self,
        root: &Path,
        branch_name: &str,
    ) -> Result<ProjectConfig, ServiceError> {
        let mut cfg = self.load_project_config(root)?;
        cfg.branch_id = self
            .client
            .list_branches()?
            .into_iter()
            .find(|branch| branch.name == branch_name || branch.branch_id == branch_name)
            .map(|branch| branch.branch_id)
            .unwrap_or_else(|| branch_name.to_string());
        self.write_project_config(root, &cfg)?;
        Ok(cfg)
    }

    pub fn delete_branch(
        &self,
        root: &Path,
        branch_name: &str,
    ) -> Result<(bool, Option<String>), ServiceError> {
        if branch_name == "main" {
            return Err(DomainError::InvalidData(format!(
                "Branch '{branch_name}' does not exist or cannot be deleted."
            ))
            .into());
        }
        let cfg = self.load_project_config(root)?;
        let branches = self.client.list_branches().map_err(|_| {
            DomainError::InvalidData(format!(
                "Branch '{branch_name}' does not exist or cannot be deleted."
            ))
        })?;
        let Some(branch_id) = branches
            .iter()
            .find(|branch| branch.name == branch_name)
            .map(|branch| branch.branch_id.clone())
        else {
            return Err(DomainError::InvalidData(format!(
                "Branch '{branch_name}' does not exist or cannot be deleted."
            ))
            .into());
        };
        self.client.delete_branch(&branch_id).map_err(|_| {
            DomainError::InvalidData(format!(
                "Branch '{branch_name}' does not exist or cannot be deleted."
            ))
        })?;
        let switched_to = if cfg.branch_id == branch_id || cfg.branch_id == branch_name {
            let mut updated_cfg = cfg;
            updated_cfg.branch_id = "main".to_string();
            self.write_project_config(root, &updated_cfg)?;
            Some("main".to_string())
        } else {
            None
        };
        Ok((true, switched_to))
    }

    pub fn merge_branch(
        &self,
        root: &Path,
        message: &str,
        conflict_resolutions: Option<Vec<JsonValue>>,
    ) -> Result<BranchMergeResult, ServiceError> {
        let result = self.client.merge_branch(message, conflict_resolutions)?;
        if result.success {
            let mut cfg = self.load_project_config(root)?;
            cfg.branch_id = "main".to_string();
            self.write_project_config(root, &cfg)?;
            let local = self.collect_local_resources(root)?;
            self.write_status_snapshot_from_resources(root, &local)?;
        }
        Ok(result)
    }

    pub fn validate_local_resources(&self, root: &Path) -> Result<Vec<String>, ServiceError> {
        let resources = self.collect_local_resources(root)?;
        Ok(validation::validate_local_resources(root, &resources)?)
    }

    pub fn format_local_resources(
        &self,
        root: &Path,
        files: &[String],
        check: bool,
    ) -> Result<Vec<String>, ServiceError> {
        let resources = self.collect_local_resources(root)?;
        let file_patterns = normalize_format_file_patterns(root, files);
        let matcher = if files.is_empty() {
            None
        } else {
            Some(build_file_matcher(&file_patterns)?)
        };
        let mut changed_files = Vec::new();
        for (path, resource) in resources {
            if let Some(m) = &matcher
                && !m.is_match(&path)
            {
                continue;
            }
            let resource_content = resource
                .payload
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let (content, formatted, write_pretty_resource) = if path.ends_with(".yaml")
                || path.ends_with(".yml")
            {
                let formatted = match from_str::<YamlValue>(resource_content) {
                    Ok(YamlValue::Null) | Err(_) => resource_content.to_string(),
                    Ok(parsed) => to_string(&parsed).map_err(|e| {
                        DomainError::InvalidData(format!("{path}: yaml error: {e}"))
                    })?,
                };
                (resource_content.to_string(), formatted, true)
            } else if path.ends_with(".json") && !files.is_empty() {
                let formatted = match serde_json::from_str::<JsonValue>(resource_content) {
                    Ok(mut parsed) => {
                        sort_json_value_keys(&mut parsed);
                        let mut formatted = serde_json::to_string_pretty(&parsed).map_err(|e| {
                            DomainError::InvalidData(format!("{path}: json error: {e}"))
                        })?;
                        formatted.push('\n');
                        formatted
                    }
                    Err(_) => resource_content.to_string(),
                };
                (resource_content.to_string(), formatted, true)
            } else if path.ends_with(".py") {
                let file_content = self
                    .workspace
                    .file_system()
                    .read_to_string(&root.join(&path))
                    .unwrap_or_else(|_| resource_file_content(&path, resource_content));
                let formatted = format_python_content(root.join(&path).as_path(), &file_content);
                (file_content, formatted, false)
            } else {
                continue;
            };
            if formatted.trim() != content.trim() {
                changed_files.push(path.clone());
                if !check {
                    let output = if write_pretty_resource {
                        resource_file_content(&path, &formatted)
                    } else {
                        formatted
                    };
                    self.workspace
                        .file_system()
                        .write_string(&root.join(&path), &output)?;
                }
            }
        }
        self.order_formatted_files(root, changed_files)
    }

    fn order_formatted_files(
        &self,
        root: &Path,
        changed_files: Vec<String>,
    ) -> Result<Vec<String>, ServiceError> {
        let mut remaining = changed_files.into_iter().collect::<BTreeSet<_>>();
        let Some(existing_typed) = self.load_status_snapshot_discovered_resources(root)? else {
            return Ok(remaining.into_iter().collect());
        };
        let discovered_typed = self.discover_local_resources(root);
        let typed_changes = self.find_new_kept_deleted(&discovered_typed, &existing_typed);
        let mut ordered = Vec::new();

        for paths_by_type in [&typed_changes.new_resources, &typed_changes.kept_resources] {
            for type_name in adk_types::ORDERED_TYPE_NAMES {
                let Some(paths) = paths_by_type.get(type_name) else {
                    continue;
                };
                for logical_path in paths {
                    let file_path = parse_multi_resource_path(logical_path).0;
                    if remaining.remove(&file_path) {
                        ordered.push(file_path);
                    }
                }
            }
        }

        ordered.extend(remaining);
        Ok(ordered)
    }

    fn load_status_snapshot_discovered_resources(
        &self,
        root: &Path,
    ) -> Result<Option<DiscoveredResourcePaths>, ServiceError> {
        Ok(self
            .workspace
            .load_status_snapshot_discovered_resources(root)?)
    }

    fn resolve_named_state(&self, root: &Path, name: &str) -> Result<ResourceMap, ServiceError> {
        if name == "local" {
            let mut resources = self.collect_local_resources(root)?;
            self.apply_status_resource_ids(root, &mut resources)?;
            return Ok(resources);
        }
        Ok(self.client.pull_resources_by_name(name)?)
    }

    fn apply_status_resource_ids(
        &self,
        root: &Path,
        resources: &mut ResourceMap,
    ) -> Result<(), ServiceError> {
        let existing_resource_ids = self.load_status_snapshot_resource_ids(root)?;
        for resource in resources.values_mut() {
            let file_path = resource.file_path.replace('\\', "/");
            if let Some(resource_id) = existing_resource_ids.get(&file_path) {
                resource.resource_id = resource_id.clone();
            }
        }
        Ok(())
    }

    fn apply_deleted_reference_names(
        &self,
        root: &Path,
        resources: &mut ResourceMap,
    ) -> Result<(), ServiceError> {
        let Some(existing_typed) = self.load_status_snapshot_discovered_resources(root)? else {
            return Ok(());
        };
        let discovered_typed = self.discover_local_resources(root);
        let typed_changes = self.find_new_kept_deleted(&discovered_typed, &existing_typed);
        let replacements =
            self.deleted_resource_reference_replacements(root, &typed_changes.deleted_resources)?;
        apply_reference_name_replacements(resources, &replacements);
        Ok(())
    }

    fn deleted_resource_reference_replacements(
        &self,
        root: &Path,
        deleted_resources: &DiscoveredResourcePaths,
    ) -> Result<Vec<ReferenceNameReplacement>, ServiceError> {
        Ok(self
            .workspace
            .deleted_resource_reference_replacements(root, deleted_resources)?)
    }

    fn load_replay_state_resources(
        &self,
        root: &Path,
    ) -> Result<Option<ResourceMap>, ServiceError> {
        let Some(path) = self.replay_state_path(root)? else {
            return Ok(None);
        };
        if !self.workspace.file_system().exists(&path) {
            return Ok(None);
        }
        let raw = self.workspace.file_system().read_to_string(&path)?;
        Ok(Some(serde_json::from_str(&raw)?))
    }

    fn save_replay_state_resources(
        &self,
        root: &Path,
        resources: &ResourceMap,
    ) -> Result<(), ServiceError> {
        let Some(path) = self.replay_state_path(root)? else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            self.workspace.file_system().create_dir_all(parent)?;
        }
        self.workspace
            .file_system()
            .write_string(&path, &serde_json::to_string(resources)?)?;
        Ok(())
    }

    fn replay_state_path(&self, root: &Path) -> Result<Option<PathBuf>, ServiceError> {
        // Replay tests persist snapshots between recorded Python workflow steps.
        let Ok(state_dir) = env::var("POLY_ADK_REPLAY_STATE_DIR") else {
            return Ok(None);
        };
        let cfg = self.load_project_config(root)?;
        let branch = cfg
            .branch_id
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '_'
                }
            })
            .collect::<String>();
        Ok(Some(
            PathBuf::from(state_dir).join(format!("{branch}.json")),
        ))
    }

    fn write_status_snapshot_from_resources(
        &self,
        root: &Path,
        baseline: &ResourceMap,
    ) -> Result<(), ServiceError> {
        let project_root = find_project_root(root).unwrap_or_else(|| root.to_path_buf());
        let baseline_file_paths: HashSet<String> = baseline
            .iter()
            .flat_map(|(path, resource)| [path.clone(), resource.file_path.clone()])
            .collect();
        let existing_resource_ids = self.load_status_snapshot_resource_ids(&project_root)?;
        let variant_name_to_id =
            self.status_variant_name_to_id(&project_root, &existing_resource_ids)?;
        let flow_step_name_to_id =
            self.status_flow_step_name_to_id(&project_root, &existing_resource_ids)?;
        let discovered = self.discover_local_resources(&project_root);
        let mut resources = indexmap::IndexMap::new();
        let mut file_structure_metadata = BTreeMap::new();

        for (type_name, paths) in discovered {
            let Some(resource_name) =
                adk_types::descriptor_by_type_name(&type_name).map(|d| d.status_resource_name)
            else {
                continue;
            };
            let mut entries = indexmap::IndexMap::new();
            for logical_path in paths {
                let (file_path, resource_suffix) = parse_multi_resource_path(&logical_path);
                if type_name != "Variable" && !baseline_file_paths.contains(&file_path) {
                    continue;
                }
                let fallback_resource_name = resource_suffix
                    .clone()
                    .or_else(|| {
                        baseline
                            .get(&file_path)
                            .map(|resource| resource.name.clone())
                    })
                    .unwrap_or_else(|| logical_path.clone());
                let resource_id = existing_resource_ids
                    .get(&logical_path)
                    .cloned()
                    .or_else(|| {
                        resource_suffix
                            .as_ref()
                            .map(|suffix| format!("{resource_name}:{}", suffix.replace('/', ":")))
                    })
                    .or_else(|| {
                        baseline
                            .get(&file_path)
                            .map(|resource| resource.resource_id.clone())
                    })
                    .unwrap_or_else(|| logical_path.clone());
                let content = if type_name == "Variable" {
                    String::new()
                } else {
                    self.workspace
                        .file_system()
                        .read_to_string(&project_root.join(&file_path))
                        .unwrap_or_default()
                };
                let payload = resource_status_payload(ResourceStatusPayloadInput {
                    type_name: &type_name,
                    logical_path: &logical_path,
                    content: &content,
                    resource_id: &resource_id,
                    fallback_name: &fallback_resource_name,
                    variant_name_to_id: &variant_name_to_id,
                    flow_step_name_to_id: &flow_step_name_to_id,
                });
                let status_resource_name = payload
                    .get("name")
                    .and_then(JsonValue::as_str)
                    .unwrap_or(&fallback_resource_name)
                    .to_string();
                let status_hash = resource_status_file_hash(
                    &type_name,
                    &logical_path,
                    &content,
                    &payload,
                    &variant_name_to_id,
                );
                let file_structure_path = if type_name == "Variable" || resource_suffix.is_some() {
                    logical_path.clone()
                } else {
                    file_path.clone()
                };
                file_structure_metadata.insert(
                    file_structure_path,
                    (
                        resource_name.to_string(),
                        resource_id.clone(),
                        status_resource_name,
                        status_hash,
                    ),
                );
                entries.insert(
                    resource_id.clone(),
                    StatusResourcePayload::from_value(payload),
                );
            }
            if !entries.is_empty() {
                resources.insert(resource_name.to_string(), entries);
            }
        }

        let mut file_structure_info = indexmap::IndexMap::new();
        for (file_path, (resource_type, resource_id, resource_name, hash)) in
            file_structure_metadata
        {
            file_structure_info.insert(
                file_path.clone(),
                FileStructureEntry {
                    resource_type,
                    resource_id,
                    resource_name,
                    hash,
                    extra: serde_json::Map::new(),
                },
            );
        }

        let config = self.load_project_config(&project_root).ok();
        let branch_id = config
            .as_ref()
            .map(|cfg| cfg.branch_id.clone())
            .unwrap_or_else(|| "main".to_string());
        let migration_flags = self.run_and_persist_project_migrations(&project_root)?;
        let status = StatusSnapshot {
            region: config
                .as_ref()
                .map(|cfg| cfg.region.clone())
                .unwrap_or_default(),
            account_id: config
                .as_ref()
                .map(|cfg| cfg.account_id.clone())
                .unwrap_or_default(),
            project_id: config
                .as_ref()
                .map(|cfg| cfg.project_id.clone())
                .unwrap_or_default(),
            project_name: config.as_ref().and_then(|cfg| cfg.project_name.clone()),
            resources,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
            file_structure_info,
            branch_id,
            migration_flags: migration_flags.into_iter().collect::<Vec<_>>(),
            extra: serde_json::Map::new(),
        };
        self.write_python_gen_package(&project_root)?;
        self.workspace
            .write_status_snapshot(&project_root, &status)?;
        Ok(())
    }

    fn status_variant_name_to_id(
        &self,
        root: &Path,
        existing_resource_ids: &indexmap::IndexMap<String, String>,
    ) -> Result<BTreeMap<String, String>, ServiceError> {
        let content = self
            .workspace
            .file_system()
            .read_to_string(&root.join("config/variant_attributes.yaml"))
            .unwrap_or_default();
        let yaml = from_str::<YamlValue>(&content).ok();
        let variants = yaml
            .as_ref()
            .and_then(|yaml| yaml.get("variants"))
            .and_then(YamlValue::as_sequence)
            .into_iter()
            .flatten();
        let mut map = BTreeMap::new();
        for variant in variants {
            let Some(name) = variant.get("name").and_then(YamlValue::as_str) else {
                continue;
            };
            let logical_path = format!(
                "config/variant_attributes.yaml/variants/{}",
                clean_name(name, false)
            );
            if let Some(id) = existing_resource_ids.get(&logical_path) {
                map.insert(name.to_string(), id.clone());
            } else if let (_, Some(suffix)) = parse_multi_resource_path(&logical_path) {
                map.insert(
                    name.to_string(),
                    format!("variants:{}", suffix.replace('/', ":")),
                );
            }
        }
        Ok(map)
    }

    fn status_flow_step_name_to_id(
        &self,
        root: &Path,
        existing_resource_ids: &indexmap::IndexMap<String, String>,
    ) -> Result<BTreeMap<(String, String), String>, ServiceError> {
        let discovered = self.discover_local_resources(root);
        let mut map = BTreeMap::new();
        for logical_path in discovered.get("FlowStep").into_iter().flatten() {
            let Some(folder) = flow_folder_name(logical_path) else {
                continue;
            };
            let (file_path, _) = parse_multi_resource_path(logical_path);
            let content = self
                .workspace
                .file_system()
                .read_to_string(&root.join(file_path))
                .unwrap_or_default();
            let yaml = from_str::<YamlValue>(&content).ok();
            let Some(name) = yaml
                .as_ref()
                .and_then(|yaml| yaml.get("name"))
                .and_then(YamlValue::as_str)
            else {
                continue;
            };
            if let Some(id) = existing_resource_ids.get(logical_path) {
                map.insert((folder.clone(), name.to_string()), id.clone());
                if let Some(stem) = Path::new(logical_path)
                    .file_stem()
                    .and_then(|value| value.to_str())
                {
                    map.insert((folder, stem.to_string()), id.clone());
                }
            }
        }
        Ok(map)
    }

    fn write_python_gen_package(&self, project_root: &Path) -> Result<(), ServiceError> {
        Ok(self.workspace.write_python_gen_package(project_root)?)
    }

    fn run_and_persist_project_migrations(
        &self,
        project_root: &Path,
    ) -> Result<BTreeSet<String>, ServiceError> {
        Ok(self
            .workspace
            .run_and_persist_project_migrations(project_root)?)
    }

    fn write_project_config(&self, root: &Path, cfg: &ProjectConfig) -> Result<(), ServiceError> {
        Ok(self.workspace.write_project_config(root, cfg)?)
    }

    fn detect_conflict_files(&self, root: &Path) -> Result<Vec<String>, ServiceError> {
        Ok(self.workspace.detect_conflict_files(root)?)
    }

    fn load_status_snapshot_file_hashes(
        &self,
        root: &Path,
    ) -> Result<Option<indexmap::IndexMap<String, String>>, ServiceError> {
        Ok(self.workspace.load_status_snapshot_file_hashes(root)?)
    }

    fn load_status_snapshot_resource_ids(
        &self,
        root: &Path,
    ) -> Result<indexmap::IndexMap<String, String>, ServiceError> {
        Ok(self.workspace.load_status_snapshot_resource_ids(root)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_api_client::InMemoryPlatformClient;

    #[test]
    fn format_check_preserves_python_shaped_yaml_key_order() {
        let fs = adk_io::MemoryFileSystem::new();
        fs.write_string(
            Path::new("workspace/project.yaml"),
            "region: dev\naccount_id: acct\nproject_id: proj\nbranch_id: main\n",
        )
        .expect("write config");
        fs.write_string(
            Path::new("workspace/topics/billing_general.yaml"),
            "name: Billing General\nenabled: true\nactions: Transfer the caller.\ncontent: |-\n  Line one.\n  Line two.\nexample_queries:\n- Question about my bill\n",
        )
        .expect("write topic");
        fs.write_string(
            Path::new("workspace/config/entities.yaml"),
            "entities:\n- name: Age\n  description: Customer age\n  entity_type: numeric\n  config:\n    min: 1\n    max: 120\n",
        )
        .expect("write entities");

        let service = AdkService::with_file_system(InMemoryPlatformClient::default(), fs.clone());
        let changed = service
            .format_local_resources(Path::new("workspace"), &[], true)
            .expect("format check");

        assert!(changed.is_empty());
        assert_eq!(
            fs.read_to_string(Path::new("workspace/topics/billing_general.yaml"))
                .expect("read topic"),
            "name: Billing General\nenabled: true\nactions: Transfer the caller.\ncontent: |-\n  Line one.\n  Line two.\nexample_queries:\n- Question about my bill\n"
        );
        assert_eq!(
            fs.read_to_string(Path::new("workspace/config/entities.yaml"))
                .expect("read entities"),
            "entities:\n- name: Age\n  description: Customer age\n  entity_type: numeric\n  config:\n    min: 1\n    max: 120\n"
        );
    }
}
