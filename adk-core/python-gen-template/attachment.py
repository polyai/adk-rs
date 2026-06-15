# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

from typing import Any
import typing

__all__ = ["Attachment"]

class Attachment:
    content_url: Any
    content_type: Any
    title: Any
    preview_image_url: Any
    call_to_action: Any
    def __init__(self, content_url: str, content_type: typing.Literal['image', 'weblink', 'unspecified'], title: str | None = None, preview_image_url: str | None = None, call_to_action: str | None = None) -> None: ...
    def to_dict(self): ...
