# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

__all__ = ["ApiIntegrations"]

class ApiIntegrations:
    """Container for all API integrations"""

    def __init__(self, api_configs: list[ApiIntegrationData], environment: str, project_id: str | None = ...):
        """Initialize API integrations container"""
        ...
    def flush_logs(self):
        """Flush all deferred logs to plog."""
        ...
    def __getattr__(self, api_name: str) -> ApiConnector:
        """Dynamic resolution for conv.api.{api_name}"""
        ...
