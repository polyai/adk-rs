# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

from typing import TYPE_CHECKING

__all__ = ["Integration"]

_registry: dict[str, type[Integration]] = {}

class Integration:
    integration_id: str
    integration_name: str
    def __init_subclass__(cls, **kwargs) -> None: ...
    def __init__(self, log: ConversationLogger, proxy_request) -> None: ...
    def proxy_request(self, endpoint: str, http_method: str, headers: dict[str, str] | None = None, params: dict[str, str] | None = None, body: dict[str, any] | None = None) -> requests.Response: ...

if TYPE_CHECKING:
    import requests

if TYPE_CHECKING:
    from ..log_utils import ConversationLogger as ConversationLogger
