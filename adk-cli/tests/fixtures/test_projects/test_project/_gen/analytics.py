# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

__all__ = ["APIRequestMetadata", "AnalyticsEvent", "response_to_analytics_events"]

class APIRequestMetadata:
    """API request metadata"""

    url: str
    method: str
    response_time: float
    status_code: int
    error: dict
    def __init__(self, url: str, method: str, response_time: float, status_code: int, error: dict = ...) -> None: ...
    def to_json_str(self) -> str:
        """Convert to JSON string"""
        ...

class AnalyticsEvent:
    """Call analytics events"""

    name: str
    value: str
    timestamp_str: str
    def __init__(self, name: str, value: str, timestamp_str: str = ...) -> None: ...
    def __post_init__(self):
        """set default timestamp string"""
        ...
    @classmethod
    def from_dict(cls, data: dict) -> AnalyticsEvent:
        """from dict object"""
        ...

def response_to_analytics_events(response: list[dict]) -> list[AnalyticsEvent]:
    """Response json to analytics event"""
    ...
