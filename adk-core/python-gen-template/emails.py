# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

from dataclasses import dataclass

__all__ = ["OutgoingEmail"]

@dataclass
class OutgoingEmail:
    to: str
    body: str
    subject: str
    def asdict(self) -> dict: ...
