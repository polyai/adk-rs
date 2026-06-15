# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

__all__ = ["KnowledgeBase"]

class KnowledgeBase:
    """Interface for managing knowledge base topics at runtime."""

    def __init__(self) -> None: ...
    @property
    def disabled_topics(self) -> list[str]:
        """List of currently disabled topic names."""
        ...
    def disable_topics(self, topic_names: list[str]) -> None:
        """Disable specific topics so they are not retrieved in conversations.."""
        ...
    def enable_topics(self, topic_names: list[str]) -> None:
        """Re-enable specific (previously disabled) topics."""
        ...
    def enable_all_topics(self) -> None:
        """Re-enable all previously disabled topics."""
        ...
