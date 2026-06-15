# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

from typing import Any, TYPE_CHECKING

__all__ = ["ApiIntegrations"]

class ApiIntegrations:
    environment: Any
    project_id: Any
    def __init__(self, api_configs: list[ApiIntegrationData], environment: str, project_id: str | None = None) -> None: ...
    def flush_logs(self) -> None: ...
    def __getattr__(self, api_name: str) -> ApiConnector: ...

if TYPE_CHECKING:
    from .conversation import ApiIntegrationData as ApiIntegrationData
