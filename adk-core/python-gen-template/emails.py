# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

from dataclasses import asdict

__all__ = ["OutgoingEmail"]

class OutgoingEmail:
    """Sent email information"""

    to: str
    body: str
    subject: str
    def __init__(self, to: str, body: str, subject: str) -> None: ...
    def asdict(self) -> dict:
        """Returns the OutgoingEmail as a dictionary"""
        ...
