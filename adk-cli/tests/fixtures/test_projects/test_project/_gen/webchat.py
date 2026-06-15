# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

from typing import Any
__all__ = ["ChatCallAction", "WebchatInterface"]

class ChatCallAction:
    contact_number: Any
    title: Any
    def __init__(self, contact_number: str, title: str | None = None) -> None: ...
    def to_dict(self): ...

class WebchatInterface:
    def __init__(self) -> None: ...
    @property
    def chat_call_actions(self) -> list[ChatCallAction]: ...
    def set_chat_call_actions(self, actions: list[ChatCallAction]) -> None: ...
