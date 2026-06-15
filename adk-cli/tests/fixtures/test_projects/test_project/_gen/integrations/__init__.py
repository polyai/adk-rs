# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

from .integration import Integration as Integration, _registry as registry
__all__ = ["Integration", "registry"]
