use adk_api_client::{ApiError, HttpPlatformClient, InMemoryPlatformClient};
use adk_core::{CoreError, PROJECT_CONFIG_FILE, STATUS_FILE};
use adk_io::{FileSystem, StdFileSystem};
use adk_service::{AdkService, PullOutcome, ServiceError};
use adk_types::{
    BranchMergeResult as RustBranchMergeResult, DeploymentList as RustDeploymentList, DomainError,
    ProjectConfig as RustProjectConfig, PushResult as RustPushResult,
    StatusSummary as RustStatusSummary,
};
use indexmap::IndexMap;
use pyo3::IntoPyObject;
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyList, PyModule, PyTuple};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

create_exception!(_native, AdkError, PyException);

type HttpService = AdkService<HttpPlatformClient>;
type LocalService = AdkService<InMemoryPlatformClient>;

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct Project {
    root: PathBuf,
    api_key: Option<String>,
}

#[pymethods]
impl Project {
    #[staticmethod]
    #[pyo3(signature = (path = ".", api_key = None))]
    fn open(path: &str, api_key: Option<String>) -> PyResult<Self> {
        let root =
            resolve_project_root(path).map_err(|message| adk_error("INVALID_PROJECT", message))?;
        local_service()
            .load_project_config(&root)
            .map_err(service_error)?;
        let api_key = match api_key {
            Some(value) if !value.trim().is_empty() => Some(value),
            Some(_) => {
                return Err(adk_error(
                    "AUTH_ERROR",
                    "api_key override must not be empty",
                ));
            }
            None => None,
        };
        Ok(Self { root, api_key })
    }

    #[getter]
    fn root(&self) -> String {
        self.root.to_string_lossy().to_string()
    }

    #[getter]
    fn config(&self) -> PyResult<ProjectConfig> {
        Ok(ProjectConfig::from(self.load_config()?))
    }

    #[getter]
    fn branches(&self) -> BranchManager {
        BranchManager::new(self.root.clone(), self.api_key.clone())
    }

    #[getter]
    fn deployments(&self) -> DeploymentManager {
        DeploymentManager::new(self.root.clone(), self.api_key.clone())
    }

    fn status(&self) -> PyResult<StatusSummary> {
        local_service()
            .status(&self.root)
            .map(StatusSummary::from)
            .map_err(service_error)
    }

    #[pyo3(signature = (files = None, before = None, after = None))]
    fn diff(
        &self,
        files: Option<Vec<String>>,
        before: Option<String>,
        after: Option<String>,
    ) -> PyResult<DiffResult> {
        let files = normalize_file_args(&self.root, &files.unwrap_or_default());
        let diffs = self
            .service()?
            .diff(&self.root, &files, before, after)
            .map_err(service_error)?;
        Ok(DiffResult {
            diffs: diffs
                .into_iter()
                .map(|(path, diff)| DiffEntry { path, diff })
                .collect(),
        })
    }

    #[pyo3(signature = (force = false, format = false))]
    fn pull(&self, force: bool, format: bool) -> PyResult<PullResult> {
        self.service()?
            .pull_detailed_with_format(&self.root, force, format)
            .map(PullResult::from)
            .map_err(service_error)
    }

    #[pyo3(signature = (force = false, skip_validation = false, dry_run = false, format = false))]
    fn push(
        &self,
        force: bool,
        skip_validation: bool,
        dry_run: bool,
        format: bool,
    ) -> PyResult<PushResult> {
        let service = self.service()?;
        if format {
            service
                .format_local_resources(&self.root, &[], false)
                .map_err(service_error)?;
        }

        let current_branch = service.current_branch(&self.root).map_err(service_error)?;
        if current_branch == "main" && !dry_run {
            let branch_name = generated_adk_branch_name();
            let (cfg, result) = service
                .push_main_to_new_branch(&self.root, &branch_name, force, skip_validation)
                .map_err(service_error)?;
            let mut out = PushResult::from_rust(result, false);
            if out.success {
                out.new_branch_id = Some(cfg.branch_id);
                out.switched_to = Some(branch_name);
            }
            return Ok(out);
        }

        service
            .push_with_options(&self.root, force, skip_validation, dry_run, None)
            .map(|result| PushResult::from_rust(result, dry_run))
            .map_err(service_error)
    }

    #[pyo3(signature = (files = None, check = false))]
    fn format(&self, files: Option<Vec<String>>, check: bool) -> PyResult<FormatResult> {
        let affected = local_service()
            .format_local_resources(&self.root, &files.unwrap_or_default(), check)
            .map_err(service_error)?;
        Ok(FormatResult {
            success: !check || affected.is_empty(),
            check_only: check,
            affected,
            format_errors: Vec::new(),
        })
    }

    fn validate(&self) -> PyResult<ValidationResult> {
        let errors = local_service()
            .validate_local_resources(&self.root)
            .map_err(service_error)?;
        Ok(ValidationResult {
            valid: errors.is_empty(),
            errors,
        })
    }

    fn __repr__(&self) -> String {
        format!("Project(root={:?})", self.root.to_string_lossy())
    }
}

impl Project {
    fn load_config(&self) -> PyResult<RustProjectConfig> {
        local_service()
            .load_project_config(&self.root)
            .map_err(service_error)
    }

    fn service(&self) -> PyResult<HttpService> {
        service_for_root(&self.root, self.api_key.as_deref())
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct BranchManager {
    root: PathBuf,
    api_key: Option<String>,
}

#[pymethods]
impl BranchManager {
    fn list(&self) -> PyResult<BranchListResult> {
        let service = self.service()?;
        let current = service
            .current_branch_name_optional(&self.root)
            .map_err(service_error)?;
        let branches = service
            .list_branch_map(&self.root)
            .map_err(service_error)?
            .into_iter()
            .map(|(name, branch_id)| Branch { name, branch_id })
            .collect();
        Ok(BranchListResult {
            current_branch: current,
            branches,
        })
    }

    fn current(&self) -> PyResult<Option<String>> {
        self.service()?
            .current_branch_name_optional(&self.root)
            .map_err(service_error)
    }

    fn create(&self, branch_name: &str) -> PyResult<Branch> {
        if branch_name.trim().is_empty() {
            return Err(adk_error("INVALID_INPUT", "branch_name must not be empty"));
        }
        let cfg = self
            .service()?
            .create_branch(&self.root, branch_name)
            .map_err(service_error)?;
        Ok(Branch {
            name: branch_name.to_string(),
            branch_id: cfg.branch_id,
        })
    }

    #[pyo3(signature = (branch_name, force = false, format = false))]
    fn switch(&self, branch_name: &str, force: bool, format: bool) -> PyResult<BranchSwitchResult> {
        if branch_name.trim().is_empty() {
            return Err(adk_error("INVALID_INPUT", "branch_name must not be empty"));
        }
        let service = self.service()?;
        if !force {
            let diffs = service
                .diff(&self.root, &[], None, None)
                .map_err(service_error)?;
            if !diffs.is_empty() {
                return Err(adk_error(
                    "CONFLICT",
                    "Cannot switch branches with uncommitted changes. Pass force=True to switch and discard changes.",
                ));
            }
        }
        service
            .set_branch(&self.root, branch_name)
            .map_err(service_error)?;
        let files_with_conflicts = service
            .pull_named_with_format(&self.root, branch_name, force, format)
            .map_err(service_error)?;
        Ok(BranchSwitchResult {
            success: files_with_conflicts.is_empty(),
            branch_name: branch_name.to_string(),
            files_with_conflicts,
        })
    }

    fn delete(&self, branch_name: &str) -> PyResult<BranchDeleteResult> {
        if branch_name.trim().is_empty() {
            return Err(adk_error("INVALID_INPUT", "branch_name must not be empty"));
        }
        let (success, switched_to) = self
            .service()?
            .delete_branch(&self.root, branch_name)
            .map_err(service_error)?;
        Ok(BranchDeleteResult {
            success,
            switched_to,
        })
    }

    #[pyo3(signature = (message, resolutions = None))]
    fn merge(
        &self,
        py: Python<'_>,
        message: &str,
        resolutions: Option<Py<PyAny>>,
    ) -> PyResult<BranchMergeResult> {
        if message.trim().is_empty() {
            return Err(adk_error("INVALID_INPUT", "message must not be empty"));
        }
        let resolutions = match resolutions {
            Some(value) => match py_to_json(value.bind(py))? {
                JsonValue::Array(items) => Some(items),
                _ => {
                    return Err(adk_error(
                        "INVALID_INPUT",
                        "resolutions must be a list of JSON-compatible objects",
                    ));
                }
            },
            None => None,
        };
        self.service()?
            .merge_branch(&self.root, message, resolutions)
            .map(BranchMergeResult::from)
            .map_err(service_error)
    }

    fn __repr__(&self) -> String {
        format!("BranchManager(root={:?})", self.root.to_string_lossy())
    }
}

impl BranchManager {
    fn new(root: PathBuf, api_key: Option<String>) -> Self {
        Self { root, api_key }
    }

    fn service(&self) -> PyResult<HttpService> {
        service_for_root(&self.root, self.api_key.as_deref())
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct DeploymentManager {
    root: PathBuf,
    api_key: Option<String>,
}

#[pymethods]
impl DeploymentManager {
    #[pyo3(signature = (env = "sandbox", limit = 10, offset = 0, version_hash = None))]
    fn list(
        &self,
        env: &str,
        limit: usize,
        offset: usize,
        version_hash: Option<String>,
    ) -> PyResult<DeploymentListResult> {
        validate_deployment_env(env)?;
        let deployments = self
            .service()?
            .list_deployments(env)
            .map_err(service_error)?;
        deployment_list_result(deployments, limit, offset, version_hash.as_deref())
    }

    #[pyo3(signature = (version_hash, env = "sandbox"))]
    fn show(&self, version_hash: &str, env: &str) -> PyResult<DeploymentShowResult> {
        validate_deployment_env(env)?;
        let service = self.service()?;
        let deployments = service.list_deployments(env).map_err(service_error)?;
        let prefix = deployment_hash_prefix(version_hash);
        let Some((version_idx, deployment)) =
            find_deployment_by_prefix(&deployments.versions, &prefix)
        else {
            return Err(adk_error(
                "INVALID_INPUT",
                format!("Version hash '{prefix}' not found."),
            ));
        };
        let deployment = deployment.clone();
        let target_hash = deployment_hash(&deployment).unwrap_or_default().to_string();
        let predecessor_hash = deployments
            .versions
            .get(version_idx + 1)
            .and_then(deployment_hash)
            .map(ToString::to_string);
        let sandbox_versions = if env == "sandbox" {
            deployments.versions.clone()
        } else {
            service
                .list_deployments("sandbox")
                .map_err(service_error)?
                .versions
        };
        let (included, is_rollback) = resolve_included_deployments(
            &sandbox_versions,
            &target_hash,
            predecessor_hash.as_deref(),
        );
        Ok(DeploymentShowResult {
            success: true,
            deployment: Deployment::from_value(deployment),
            active_deployment_hashes: indexmap_to_btree(deployments.active_deployment_hashes),
            included_deployments: deployments_from_values(included),
            is_rollback,
        })
    }

    #[pyo3(signature = (from_deployment, to_env, message = None, dry_run = false))]
    fn promote(
        &self,
        from_deployment: &str,
        to_env: &str,
        message: Option<String>,
        dry_run: bool,
    ) -> PyResult<DeploymentPromoteResult> {
        validate_promote_env(to_env)?;
        let service = self.service()?;
        let search_env = deployment_promote_search_env(to_env);
        let deployments = service
            .list_deployments(search_env)
            .map_err(service_error)?;
        let selection = select_deployment_for_promotion(
            &deployments.versions,
            &deployments.active_deployment_hashes,
            from_deployment,
            to_env,
            message.as_deref(),
            search_env,
        )?;
        let sandbox_versions = if search_env == "sandbox" {
            deployments.versions
        } else {
            service
                .list_deployments("sandbox")
                .map_err(service_error)?
                .versions
        };
        let (included, is_rollback) = resolve_included_deployments(
            &sandbox_versions,
            &selection.from_hash,
            selection.predecessor_hash.as_deref(),
        );
        let mut result = DeploymentPromoteResult {
            success: false,
            to_env: to_env.to_string(),
            from_hash: selection.from_hash.clone(),
            message: selection.message.clone(),
            included_deployments: deployments_from_values(included),
            is_rollback,
            dry_run,
        };
        if dry_run {
            return Ok(result);
        }
        service
            .promote_deployment(&selection.deployment_id, to_env, &selection.message)
            .map_err(service_error)?;
        result.success = true;
        Ok(result)
    }

    #[pyo3(signature = (to_deployment, message = None, dry_run = false))]
    fn rollback(
        &self,
        to_deployment: &str,
        message: Option<String>,
        dry_run: bool,
    ) -> PyResult<DeploymentRollbackResult> {
        let service = self.service()?;
        let deployments = service.list_deployments("sandbox").map_err(service_error)?;
        let deployment_hash_or_alias = deployments
            .active_deployment_hashes
            .get(to_deployment)
            .map(String::as_str)
            .unwrap_or(to_deployment);
        let prefix = deployment_hash_prefix(deployment_hash_or_alias);
        let Some((_, deployment)) = find_deployment_by_prefix(&deployments.versions, &prefix)
        else {
            return Err(adk_error(
                "INVALID_INPUT",
                format!("Deployment '{to_deployment}' not found in sandbox."),
            ));
        };
        let deployment = deployment.clone();
        let Some(deployment_id) = deployment_id(&deployment).map(ToString::to_string) else {
            return Err(adk_error(
                "INVALID_DATA",
                "Selected deployment does not include an id.",
            ));
        };
        let target_hash = deployment_hash(&deployment).unwrap_or_default().to_string();
        let deployment_message = deployment_message(&deployment).unwrap_or("");
        let message = message.unwrap_or_else(|| deployment_message.to_string());
        let current_sandbox_hash = deployments
            .active_deployment_hashes
            .get("sandbox")
            .map(String::as_str);
        let (reverted, _) = resolve_included_deployments(
            &deployments.versions,
            current_sandbox_hash.unwrap_or(""),
            Some(&target_hash),
        );
        let mut result = DeploymentRollbackResult {
            success: false,
            target_hash,
            message,
            reverted_deployments: deployments_from_values(reverted),
            dry_run,
        };
        if dry_run {
            return Ok(result);
        }
        service
            .rollback_deployment(&deployment_id, &result.message)
            .map_err(service_error)?;
        result.success = true;
        Ok(result)
    }

    fn __repr__(&self) -> String {
        format!("DeploymentManager(root={:?})", self.root.to_string_lossy())
    }
}

impl DeploymentManager {
    fn new(root: PathBuf, api_key: Option<String>) -> Self {
        Self { root, api_key }
    }

    fn service(&self) -> PyResult<HttpService> {
        service_for_root(&self.root, self.api_key.as_deref())
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct ProjectConfig {
    #[pyo3(get)]
    region: String,
    #[pyo3(get)]
    account_id: String,
    #[pyo3(get)]
    project_id: String,
    #[pyo3(get)]
    project_name: Option<String>,
    #[pyo3(get)]
    branch_id: String,
}

#[pymethods]
impl ProjectConfig {
    fn __repr__(&self) -> String {
        format!(
            "ProjectConfig(region={:?}, account_id={:?}, project_id={:?}, branch_id={:?})",
            self.region, self.account_id, self.project_id, self.branch_id
        )
    }
}

impl From<RustProjectConfig> for ProjectConfig {
    fn from(value: RustProjectConfig) -> Self {
        Self {
            region: value.region,
            account_id: value.account_id,
            project_id: value.project_id,
            project_name: value.project_name,
            branch_id: value.branch_id,
        }
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct StatusSummary {
    #[pyo3(get)]
    conflict_detection_available: bool,
    #[pyo3(get)]
    files_with_conflicts: Vec<String>,
    #[pyo3(get)]
    modified_files: Vec<String>,
    #[pyo3(get)]
    new_files: Vec<String>,
    #[pyo3(get)]
    deleted_files: Vec<String>,
}

#[pymethods]
impl StatusSummary {
    #[getter]
    fn has_changes(&self) -> bool {
        !self.files_with_conflicts.is_empty()
            || !self.modified_files.is_empty()
            || !self.new_files.is_empty()
            || !self.deleted_files.is_empty()
    }

    fn __repr__(&self) -> String {
        format!(
            "StatusSummary(modified_files={}, new_files={}, deleted_files={}, files_with_conflicts={})",
            self.modified_files.len(),
            self.new_files.len(),
            self.deleted_files.len(),
            self.files_with_conflicts.len()
        )
    }
}

impl From<RustStatusSummary> for StatusSummary {
    fn from(value: RustStatusSummary) -> Self {
        Self {
            conflict_detection_available: value.conflict_detection_available,
            files_with_conflicts: value.files_with_conflicts,
            modified_files: value.modified_files,
            new_files: value.new_files,
            deleted_files: value.deleted_files,
        }
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct PullResult {
    #[pyo3(get)]
    success: bool,
    #[pyo3(get)]
    files_with_conflicts: Vec<String>,
    #[pyo3(get)]
    new_branch_name: Option<String>,
    #[pyo3(get)]
    new_branch_id: Option<String>,
}

#[pymethods]
impl PullResult {
    fn __repr__(&self) -> String {
        format!(
            "PullResult(success={}, files_with_conflicts={})",
            self.success,
            self.files_with_conflicts.len()
        )
    }
}

impl From<PullOutcome> for PullResult {
    fn from(value: PullOutcome) -> Self {
        let success = value.files_with_conflicts.is_empty();
        Self {
            success,
            files_with_conflicts: value.files_with_conflicts,
            new_branch_name: value.new_branch_name,
            new_branch_id: value.new_branch_id,
        }
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct PushResult {
    #[pyo3(get)]
    success: bool,
    #[pyo3(get)]
    message: String,
    #[pyo3(get)]
    dry_run: bool,
    commands: Vec<JsonValue>,
    #[pyo3(get)]
    new_branch_id: Option<String>,
    #[pyo3(get)]
    switched_to: Option<String>,
}

#[pymethods]
impl PushResult {
    #[getter]
    fn commands(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_py(py, &JsonValue::Array(self.commands.clone()))
    }

    fn __repr__(&self) -> String {
        format!(
            "PushResult(success={}, dry_run={}, message={:?})",
            self.success, self.dry_run, self.message
        )
    }
}

impl PushResult {
    fn from_rust(value: RustPushResult, dry_run: bool) -> Self {
        Self {
            success: value.success,
            message: value.message,
            dry_run,
            commands: value.commands,
            new_branch_id: None,
            switched_to: None,
        }
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct DiffEntry {
    #[pyo3(get)]
    path: String,
    #[pyo3(get)]
    diff: String,
}

#[pymethods]
impl DiffEntry {
    fn __repr__(&self) -> String {
        format!("DiffEntry(path={:?})", self.path)
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct DiffResult {
    #[pyo3(get)]
    diffs: Vec<DiffEntry>,
}

#[pymethods]
impl DiffResult {
    #[getter]
    fn has_changes(&self) -> bool {
        !self.diffs.is_empty()
    }

    fn __len__(&self) -> usize {
        self.diffs.len()
    }

    fn __repr__(&self) -> String {
        format!("DiffResult(diffs={})", self.diffs.len())
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct FormatResult {
    #[pyo3(get)]
    success: bool,
    #[pyo3(get)]
    check_only: bool,
    #[pyo3(get)]
    affected: Vec<String>,
    #[pyo3(get)]
    format_errors: Vec<String>,
}

#[pymethods]
impl FormatResult {
    fn __repr__(&self) -> String {
        format!(
            "FormatResult(success={}, check_only={}, affected={})",
            self.success,
            self.check_only,
            self.affected.len()
        )
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct ValidationResult {
    #[pyo3(get)]
    valid: bool,
    #[pyo3(get)]
    errors: Vec<String>,
}

#[pymethods]
impl ValidationResult {
    fn __repr__(&self) -> String {
        format!(
            "ValidationResult(valid={}, errors={})",
            self.valid,
            self.errors.len()
        )
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct Branch {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    branch_id: String,
}

#[pymethods]
impl Branch {
    fn __repr__(&self) -> String {
        format!(
            "Branch(name={:?}, branch_id={:?})",
            self.name, self.branch_id
        )
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct BranchListResult {
    #[pyo3(get)]
    current_branch: Option<String>,
    #[pyo3(get)]
    branches: Vec<Branch>,
}

#[pymethods]
impl BranchListResult {
    fn __repr__(&self) -> String {
        format!(
            "BranchListResult(current_branch={:?}, branches={})",
            self.current_branch,
            self.branches.len()
        )
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct BranchSwitchResult {
    #[pyo3(get)]
    success: bool,
    #[pyo3(get)]
    branch_name: String,
    #[pyo3(get)]
    files_with_conflicts: Vec<String>,
}

#[pymethods]
impl BranchSwitchResult {
    fn __repr__(&self) -> String {
        format!(
            "BranchSwitchResult(success={}, branch_name={:?}, files_with_conflicts={})",
            self.success,
            self.branch_name,
            self.files_with_conflicts.len()
        )
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct BranchDeleteResult {
    #[pyo3(get)]
    success: bool,
    #[pyo3(get)]
    switched_to: Option<String>,
}

#[pymethods]
impl BranchDeleteResult {
    fn __repr__(&self) -> String {
        format!(
            "BranchDeleteResult(success={}, switched_to={:?})",
            self.success, self.switched_to
        )
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct BranchMergeResult {
    #[pyo3(get)]
    success: bool,
    conflicts: Vec<JsonValue>,
    errors: Vec<JsonValue>,
    #[pyo3(get)]
    sequence: Option<String>,
}

#[pymethods]
impl BranchMergeResult {
    #[getter]
    fn conflicts(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_py(py, &JsonValue::Array(self.conflicts.clone()))
    }

    #[getter]
    fn errors(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_py(py, &JsonValue::Array(self.errors.clone()))
    }

    fn __repr__(&self) -> String {
        format!(
            "BranchMergeResult(success={}, conflicts={}, errors={})",
            self.success,
            self.conflicts.len(),
            self.errors.len()
        )
    }
}

impl From<RustBranchMergeResult> for BranchMergeResult {
    fn from(value: RustBranchMergeResult) -> Self {
        Self {
            success: value.success,
            conflicts: value.conflicts,
            errors: value.errors,
            sequence: value.sequence,
        }
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct Deployment {
    raw: JsonValue,
}

#[pymethods]
impl Deployment {
    #[getter]
    fn raw(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_py(py, &self.raw)
    }

    #[getter]
    fn id(&self) -> Option<String> {
        deployment_id(&self.raw).map(ToString::to_string)
    }

    #[getter]
    fn hash(&self) -> Option<String> {
        deployment_hash(&self.raw).map(ToString::to_string)
    }

    #[getter]
    fn message(&self) -> Option<String> {
        deployment_message(&self.raw).map(ToString::to_string)
    }

    #[getter]
    fn created_at(&self) -> Option<String> {
        string_field(&self.raw, &["created_at", "createdAt", "artifact_version"])
            .map(ToString::to_string)
    }

    #[getter]
    fn created_by(&self) -> Option<String> {
        string_field(&self.raw, &["created_by", "createdBy"]).map(ToString::to_string)
    }

    #[getter]
    fn deployment_type(&self) -> Option<String> {
        self.raw
            .pointer("/deployment_metadata/deployment_type")
            .and_then(JsonValue::as_str)
            .map(ToString::to_string)
    }

    fn __repr__(&self) -> String {
        format!(
            "Deployment(hash={:?}, id={:?})",
            deployment_hash(&self.raw),
            deployment_id(&self.raw)
        )
    }
}

impl Deployment {
    fn from_value(raw: JsonValue) -> Self {
        Self { raw }
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct DeploymentListResult {
    #[pyo3(get)]
    versions: Vec<Deployment>,
    active_deployment_hashes: BTreeMap<String, String>,
}

#[pymethods]
impl DeploymentListResult {
    #[getter]
    fn active_deployment_hashes(&self) -> BTreeMap<String, String> {
        self.active_deployment_hashes.clone()
    }

    fn __repr__(&self) -> String {
        format!("DeploymentListResult(versions={})", self.versions.len())
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct DeploymentShowResult {
    #[pyo3(get)]
    success: bool,
    #[pyo3(get)]
    deployment: Deployment,
    active_deployment_hashes: BTreeMap<String, String>,
    #[pyo3(get)]
    included_deployments: Vec<Deployment>,
    #[pyo3(get)]
    is_rollback: bool,
}

#[pymethods]
impl DeploymentShowResult {
    #[getter]
    fn active_deployment_hashes(&self) -> BTreeMap<String, String> {
        self.active_deployment_hashes.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "DeploymentShowResult(success={}, included_deployments={}, is_rollback={})",
            self.success,
            self.included_deployments.len(),
            self.is_rollback
        )
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct DeploymentPromoteResult {
    #[pyo3(get)]
    success: bool,
    #[pyo3(get)]
    to_env: String,
    #[pyo3(get)]
    from_hash: String,
    #[pyo3(get)]
    message: String,
    #[pyo3(get)]
    included_deployments: Vec<Deployment>,
    #[pyo3(get)]
    is_rollback: bool,
    #[pyo3(get)]
    dry_run: bool,
}

#[pymethods]
impl DeploymentPromoteResult {
    fn __repr__(&self) -> String {
        format!(
            "DeploymentPromoteResult(success={}, to_env={:?}, from_hash={:?}, dry_run={})",
            self.success, self.to_env, self.from_hash, self.dry_run
        )
    }
}

#[pyclass(module = "poly_adk", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct DeploymentRollbackResult {
    #[pyo3(get)]
    success: bool,
    #[pyo3(get)]
    target_hash: String,
    #[pyo3(get)]
    message: String,
    #[pyo3(get)]
    reverted_deployments: Vec<Deployment>,
    #[pyo3(get)]
    dry_run: bool,
}

#[pymethods]
impl DeploymentRollbackResult {
    fn __repr__(&self) -> String {
        format!(
            "DeploymentRollbackResult(success={}, target_hash={:?}, dry_run={})",
            self.success, self.target_hash, self.dry_run
        )
    }
}

#[pymodule]
fn _native(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("AdkError", py.get_type::<AdkError>())?;
    m.add_class::<Project>()?;
    m.add_class::<ProjectConfig>()?;
    m.add_class::<StatusSummary>()?;
    m.add_class::<PullResult>()?;
    m.add_class::<PushResult>()?;
    m.add_class::<DiffEntry>()?;
    m.add_class::<DiffResult>()?;
    m.add_class::<FormatResult>()?;
    m.add_class::<ValidationResult>()?;
    m.add_class::<BranchManager>()?;
    m.add_class::<Branch>()?;
    m.add_class::<BranchListResult>()?;
    m.add_class::<BranchSwitchResult>()?;
    m.add_class::<BranchDeleteResult>()?;
    m.add_class::<BranchMergeResult>()?;
    m.add_class::<DeploymentManager>()?;
    m.add_class::<Deployment>()?;
    m.add_class::<DeploymentListResult>()?;
    m.add_class::<DeploymentShowResult>()?;
    m.add_class::<DeploymentPromoteResult>()?;
    m.add_class::<DeploymentRollbackResult>()?;
    Ok(())
}

fn service_for_root(root: &Path, api_key: Option<&str>) -> PyResult<HttpService> {
    let cfg = local_service()
        .load_project_config(root)
        .map_err(service_error)?;
    let api_key = match api_key {
        Some(value) => value.to_string(),
        None => {
            api_key_for_region(&cfg.region).map_err(|message| adk_error("AUTH_ERROR", message))?
        }
    };
    HttpPlatformClient::new_with_api_key(
        &cfg.region,
        &cfg.account_id,
        &cfg.project_id,
        Some(&cfg.branch_id),
        api_key,
    )
    .map(AdkService::new)
    .map_err(api_error)
}

fn local_service() -> LocalService {
    AdkService::new(InMemoryPlatformClient::default())
}

fn resolve_project_root(path: &str) -> Result<PathBuf, String> {
    let fs = StdFileSystem;
    let start = if path.trim().is_empty() { "." } else { path };
    let mut current = PathBuf::from(start);
    if !current.is_absolute() {
        current = fs
            .current_dir()
            .map_err(|error| format!("Failed to resolve current directory: {error}"))?
            .join(current);
    }
    if fs.is_file(&current) {
        current.pop();
    }
    if let Ok(canonical) = fs.canonicalize(&current) {
        current = canonical;
    }
    loop {
        if fs.exists(&current.join(PROJECT_CONFIG_FILE)) || fs.exists(&current.join(STATUS_FILE)) {
            return Ok(current);
        }
        if !current.pop() {
            return Err(
                "No project configuration found. Run poly init to initialize a project."
                    .to_string(),
            );
        }
    }
}

fn normalize_file_args(root: &Path, files: &[String]) -> Vec<String> {
    let fs = StdFileSystem;
    let root_abs = if root.is_absolute() {
        root.to_path_buf()
    } else {
        fs.current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(root)
    };
    files
        .iter()
        .map(|file| {
            let path = PathBuf::from(file);
            if path.is_absolute() {
                path.strip_prefix(&root_abs)
                    .unwrap_or(path.as_path())
                    .to_string_lossy()
                    .replace('\\', "/")
            } else {
                file.replace('\\', "/")
            }
        })
        .collect()
}

fn generated_adk_branch_name() -> String {
    if let Ok(name) = env::var("POLY_ADK_GENERATED_BRANCH_NAME")
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

#[derive(Debug, Clone, Default, Deserialize)]
struct CredentialsFile {
    #[serde(flatten)]
    api_keys: IndexMap<String, String>,
}

impl CredentialsFile {
    fn api_key(&self, region: &str) -> Option<String> {
        self.api_keys
            .get(region)
            .filter(|key| !key.trim().is_empty())
            .cloned()
    }
}

fn api_key_for_region(region: &str) -> Result<String, String> {
    if let Some(path) = credentials_file_path()
        && let Some(value) = read_api_key_from_credential_file_at(&path, region)
    {
        return Ok(value);
    }

    for name in api_key_env_names(region) {
        if let Ok(value) = env::var(name)
            && !value.trim().is_empty()
        {
            return Ok(value);
        }
    }

    Err(format!(
        "No API key found for region {region}. Run poly start or poly login, or set POLY_ADK_KEY."
    ))
}

fn read_api_key_from_credential_file_at(path: &Path, region: &str) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    let credentials: CredentialsFile = serde_json::from_str(&contents).ok()?;
    credentials.api_key(region)
}

fn credentials_file_path() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".poly").join("credentials.json"))
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .or_else(|| env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
        .map(PathBuf::from)
}

fn api_key_env_names(region: &str) -> Vec<&'static str> {
    let mut names = Vec::new();
    match region {
        "us-1" => names.push("POLY_ADK_KEY_US"),
        "euw-1" => names.push("POLY_ADK_KEY_EUW"),
        "uk-1" => names.push("POLY_ADK_KEY_UK"),
        "studio" => names.push("POLY_ADK_KEY_STUDIO"),
        "staging" => names.push("POLY_ADK_KEY_STAGING"),
        "dev" => names.push("POLY_ADK_KEY_DEV"),
        _ => {}
    }
    names.push("POLY_ADK_KEY");
    names
}

fn deployment_list_result(
    deployments: RustDeploymentList,
    limit: usize,
    mut offset: usize,
    version_hash: Option<&str>,
) -> PyResult<DeploymentListResult> {
    if let Some(version_hash) = version_hash {
        let prefix = deployment_hash_prefix(version_hash);
        if let Some((idx, _)) = find_deployment_by_prefix(&deployments.versions, &prefix) {
            offset = idx;
        } else {
            return Err(adk_error(
                "INVALID_INPUT",
                format!("Version hash '{prefix}' not found."),
            ));
        }
    }
    Ok(DeploymentListResult {
        versions: deployments_from_values(
            deployments
                .versions
                .into_iter()
                .skip(offset)
                .take(limit)
                .collect(),
        ),
        active_deployment_hashes: indexmap_to_btree(deployments.active_deployment_hashes),
    })
}

fn validate_deployment_env(env: &str) -> PyResult<()> {
    match env {
        "sandbox" | "pre-release" | "live" => Ok(()),
        _ => Err(adk_error(
            "INVALID_INPUT",
            "env must be one of: sandbox, pre-release, live",
        )),
    }
}

fn validate_promote_env(env: &str) -> PyResult<()> {
    match env {
        "pre-release" | "live" => Ok(()),
        _ => Err(adk_error(
            "INVALID_INPUT",
            "to_env must be one of: pre-release, live",
        )),
    }
}

fn deployments_from_values(values: Vec<JsonValue>) -> Vec<Deployment> {
    values.into_iter().map(Deployment::from_value).collect()
}

fn indexmap_to_btree(map: IndexMap<String, String>) -> BTreeMap<String, String> {
    map.into_iter().collect()
}

fn deployment_promote_search_env(to_env: &str) -> &'static str {
    if to_env == "live" {
        "pre-release"
    } else {
        "sandbox"
    }
}

fn deployment_hash_or_active_alias<'a>(
    active_deployment_hashes: &'a IndexMap<String, String>,
    requested: &'a str,
) -> &'a str {
    active_deployment_hashes
        .get(requested)
        .map(String::as_str)
        .unwrap_or(requested)
}

#[derive(Debug, PartialEq)]
struct DeploymentPromotionSelection {
    deployment_id: String,
    from_hash: String,
    message: String,
    predecessor_hash: Option<String>,
}

fn select_deployment_for_promotion(
    deployments: &[JsonValue],
    active_deployment_hashes: &IndexMap<String, String>,
    from_deployment: &str,
    to_env: &str,
    message: Option<&str>,
    search_env: &'static str,
) -> PyResult<DeploymentPromotionSelection> {
    let deployment_hash_or_alias =
        deployment_hash_or_active_alias(active_deployment_hashes, from_deployment);
    let prefix = deployment_hash_prefix(deployment_hash_or_alias);
    let Some((_, deployment)) = find_deployment_by_prefix(deployments, &prefix) else {
        return Err(adk_error(
            "INVALID_INPUT",
            format!("Deployment '{from_deployment}' not found in {search_env}."),
        ));
    };
    let Some(deployment_id) = deployment_id(deployment).map(ToString::to_string) else {
        return Err(adk_error(
            "INVALID_DATA",
            "Selected deployment does not include an id.",
        ));
    };
    let deployment_message = deployment_message(deployment).unwrap_or("");
    Ok(DeploymentPromotionSelection {
        deployment_id,
        from_hash: deployment_hash(deployment).unwrap_or_default().to_string(),
        message: message.unwrap_or(deployment_message).to_string(),
        predecessor_hash: active_deployment_hashes.get(to_env).cloned(),
    })
}

fn find_deployment_by_prefix<'a>(
    deployments: &'a [JsonValue],
    prefix: &str,
) -> Option<(usize, &'a JsonValue)> {
    deployments.iter().enumerate().find(|(_, deployment)| {
        deployment_hash(deployment)
            .map(|hash| hash.chars().take(9).collect::<String>() == prefix)
            .unwrap_or(false)
    })
}

fn deployment_hash_prefix(hash: &str) -> String {
    hash.chars().take(9).collect()
}

fn deployment_hash(deployment: &JsonValue) -> Option<&str> {
    string_field(deployment, &["version_hash", "versionHash", "hash"])
}

fn deployment_id(deployment: &JsonValue) -> Option<&str> {
    string_field(deployment, &["id", "deployment_id", "deploymentId"])
}

fn deployment_message(deployment: &JsonValue) -> Option<&str> {
    deployment
        .pointer("/deployment_metadata/deployment_message")
        .and_then(JsonValue::as_str)
        .filter(|message| !message.is_empty())
}

fn resolve_included_deployments(
    sandbox_versions: &[JsonValue],
    target_hash: &str,
    predecessor_hash: Option<&str>,
) -> (Vec<JsonValue>, bool) {
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

fn string_field<'a>(value: &'a JsonValue, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(JsonValue::as_str))
}

fn py_to_json(value: &Bound<'_, PyAny>) -> PyResult<JsonValue> {
    if value.is_none() {
        return Ok(JsonValue::Null);
    }
    if let Ok(value) = value.extract::<bool>() {
        return Ok(JsonValue::Bool(value));
    }
    if let Ok(value) = value.extract::<i64>() {
        return Ok(JsonValue::Number(value.into()));
    }
    if let Ok(value) = value.extract::<u64>() {
        return Ok(JsonValue::Number(value.into()));
    }
    if let Ok(value) = value.extract::<f64>() {
        return serde_json::Number::from_f64(value)
            .map(JsonValue::Number)
            .ok_or_else(|| PyTypeError::new_err("float values must be finite"));
    }
    if let Ok(value) = value.extract::<String>() {
        return Ok(JsonValue::String(value));
    }
    if let Ok(dict) = value.cast::<PyDict>() {
        let mut out = serde_json::Map::new();
        for (key, item) in dict.iter() {
            let key = key.extract::<String>()?;
            out.insert(key, py_to_json(&item)?);
        }
        return Ok(JsonValue::Object(out));
    }
    if let Ok(list) = value.cast::<PyList>() {
        let mut out = Vec::new();
        for item in list.iter() {
            out.push(py_to_json(&item)?);
        }
        return Ok(JsonValue::Array(out));
    }
    if let Ok(tuple) = value.cast::<PyTuple>() {
        let mut out = Vec::new();
        for item in tuple.iter() {
            out.push(py_to_json(&item)?);
        }
        return Ok(JsonValue::Array(out));
    }
    Err(PyTypeError::new_err(
        "value must be JSON-compatible: None, bool, int, float, str, list, tuple, or dict",
    ))
}

fn json_to_py(py: Python<'_>, value: &JsonValue) -> PyResult<Py<PyAny>> {
    match value {
        JsonValue::Null => Ok(py.None()),
        JsonValue::Bool(value) => Ok(PyBool::new(py, *value).to_owned().into_any().unbind()),
        JsonValue::Number(number) => {
            if let Some(value) = number.as_i64() {
                Ok(value.into_pyobject(py)?.into_any().unbind())
            } else if let Some(value) = number.as_u64() {
                Ok(value.into_pyobject(py)?.into_any().unbind())
            } else if let Some(value) = number.as_f64() {
                Ok(value.into_pyobject(py)?.into_any().unbind())
            } else {
                Ok(py.None())
            }
        }
        JsonValue::String(value) => Ok(value.into_pyobject(py)?.into_any().unbind()),
        JsonValue::Array(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(json_to_py(py, item)?)?;
            }
            Ok(list.into_any().unbind())
        }
        JsonValue::Object(items) => {
            let dict = PyDict::new(py);
            for (key, value) in items {
                dict.set_item(key, json_to_py(py, value)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

fn service_error(error: ServiceError) -> PyErr {
    match &error {
        ServiceError::Api(api) => api_error_ref(api, error.to_string()),
        ServiceError::Core(core) => core_error_ref(core, error.to_string()),
    }
}

fn api_error(error: ApiError) -> PyErr {
    api_error_ref(&error, error.to_string())
}

fn api_error_ref(error: &ApiError, message: String) -> PyErr {
    match error {
        ApiError::MissingConfig(_) => adk_error("AUTH_ERROR", message),
        ApiError::HttpStatus { .. } | ApiError::Http(_) => adk_error("API_ERROR", message),
    }
}

fn core_error_ref(error: &CoreError, message: String) -> PyErr {
    match error {
        CoreError::Domain(DomainError::ConfigNotFound(_)) => adk_error("INVALID_PROJECT", message),
        CoreError::Domain(DomainError::InvalidData(_)) => adk_error("INVALID_DATA", message),
        CoreError::CommandGeneration(_) => adk_error("COMMAND_GENERATION_FAILED", message),
        CoreError::Json(_) => adk_error("INVALID_DATA", message),
        CoreError::Io(_) => adk_error("IO_ERROR", message),
    }
}

fn adk_error(code: &'static str, message: impl Into<String>) -> PyErr {
    let message = message.into();
    Python::attach(|py| {
        let err = PyErr::new::<AdkError, _>(message.clone());
        let value = err.value(py);
        let _ = value.setattr("code", code);
        let _ = value.setattr("message", message);
        err
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_prefix_finds_hash_aliases() {
        let deployments = vec![serde_json::json!({"version_hash": "abcdef123456"})];
        assert!(find_deployment_by_prefix(&deployments, "abcdef123").is_some());
    }

    #[test]
    fn normalize_file_args_strips_absolute_project_root() {
        let root = PathBuf::from("/tmp/adk-python-project");
        let files = vec![
            "/tmp/adk-python-project/topics/topic.yaml".to_string(),
            "functions/test.py".to_string(),
        ];
        assert_eq!(
            normalize_file_args(&root, &files),
            vec![
                "topics/topic.yaml".to_string(),
                "functions/test.py".to_string()
            ]
        );
    }
}
