from collections.abc import Mapping, Sequence
from typing import Any

__version__: str


class AdkError(Exception):
    code: str
    message: str


class ProjectConfig:
    region: str
    account_id: str
    project_id: str
    project_name: str | None
    branch_id: str


class StatusSummary:
    conflict_detection_available: bool
    files_with_conflicts: list[str]
    modified_files: list[str]
    new_files: list[str]
    deleted_files: list[str]
    has_changes: bool


class PullResult:
    success: bool
    files_with_conflicts: list[str]
    new_branch_name: str | None
    new_branch_id: str | None


class PushResult:
    success: bool
    message: str
    dry_run: bool
    commands: list[Mapping[str, Any]]
    new_branch_id: str | None
    switched_to: str | None


class DiffEntry:
    path: str
    diff: str


class DiffResult:
    diffs: list[DiffEntry]
    has_changes: bool
    def __len__(self) -> int: ...


class FormatResult:
    success: bool
    check_only: bool
    affected: list[str]
    format_errors: list[str]


class ValidationResult:
    valid: bool
    errors: list[str]


class Branch:
    name: str
    branch_id: str


class BranchListResult:
    current_branch: str | None
    branches: list[Branch]


class BranchSwitchResult:
    success: bool
    branch_name: str
    files_with_conflicts: list[str]


class BranchDeleteResult:
    success: bool
    switched_to: str | None


class BranchMergeResult:
    success: bool
    conflicts: list[Mapping[str, Any]]
    errors: list[Mapping[str, Any]]
    sequence: str | None


class Deployment:
    raw: Mapping[str, Any]
    id: str | None
    hash: str | None
    message: str | None
    created_at: str | None
    created_by: str | None
    deployment_type: str | None


class DeploymentListResult:
    versions: list[Deployment]
    active_deployment_hashes: dict[str, str]


class DeploymentShowResult:
    success: bool
    deployment: Deployment
    active_deployment_hashes: dict[str, str]
    included_deployments: list[Deployment]
    is_rollback: bool


class DeploymentPromoteResult:
    success: bool
    to_env: str
    from_hash: str
    message: str
    included_deployments: list[Deployment]
    is_rollback: bool
    dry_run: bool


class DeploymentRollbackResult:
    success: bool
    target_hash: str
    message: str
    reverted_deployments: list[Deployment]
    dry_run: bool


class BranchManager:
    def list(self) -> BranchListResult: ...
    def current(self) -> str | None: ...
    def create(self, branch_name: str) -> Branch: ...
    def switch(
        self,
        branch_name: str,
        force: bool = False,
        format: bool = False,
    ) -> BranchSwitchResult: ...
    def delete(self, branch_name: str) -> BranchDeleteResult: ...
    def merge(
        self,
        message: str,
        resolutions: Sequence[Mapping[str, Any]] | None = None,
    ) -> BranchMergeResult: ...


class DeploymentManager:
    def list(
        self,
        env: str = "sandbox",
        limit: int = 10,
        offset: int = 0,
        version_hash: str | None = None,
    ) -> DeploymentListResult: ...
    def show(self, version_hash: str, env: str = "sandbox") -> DeploymentShowResult: ...
    def promote(
        self,
        from_deployment: str,
        to_env: str,
        message: str | None = None,
        dry_run: bool = False,
    ) -> DeploymentPromoteResult: ...
    def rollback(
        self,
        to_deployment: str,
        message: str | None = None,
        dry_run: bool = False,
    ) -> DeploymentRollbackResult: ...


class Project:
    root: str
    config: ProjectConfig
    branches: BranchManager
    deployments: DeploymentManager

    @staticmethod
    def open(path: str = ".", api_key: str | None = None) -> Project: ...
    def status(self) -> StatusSummary: ...
    def diff(
        self,
        files: Sequence[str] | None = None,
        before: str | None = None,
        after: str | None = None,
    ) -> DiffResult: ...
    def pull(self, force: bool = False, format: bool = False) -> PullResult: ...
    def push(
        self,
        force: bool = False,
        skip_validation: bool = False,
        dry_run: bool = False,
        format: bool = False,
    ) -> PushResult: ...
    def format(
        self,
        files: Sequence[str] | None = None,
        check: bool = False,
    ) -> FormatResult: ...
    def validate(self) -> ValidationResult: ...


__all__: list[str]
