# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
from __future__ import annotations

from .integration import Integration as Integration, _registry as registry

__all__ = ["Integration", "registry"]
