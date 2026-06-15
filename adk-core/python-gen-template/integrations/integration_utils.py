# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

from typing import TYPE_CHECKING

VALID_HTTP_METHODS = {'GET', 'POST', 'DELETE', 'PUT', 'PATCH'}
US_PROXY_BASE_URL = 'https://proxy.useparagon.com'
EU_PROXY_BASE_URL = 'https://worker-proxy.eu.paragon.so'
DEFAULT_REQUEST_TIMEOUT_SECONDS = 10

def proxy_integration_request_to_paragon(paragon_proxy_url: str, paragon_connection_id: str, paragon_project_id: str, integration_token: str, integration_id: str, endpoint: str, http_method: str, headers: dict[str, str] | None = None, params: dict[str, str] | None = None, body: dict[str, any] | None = None, request_timeout_seconds: int = ...) -> requests.Response: ...

if TYPE_CHECKING:
    import requests
