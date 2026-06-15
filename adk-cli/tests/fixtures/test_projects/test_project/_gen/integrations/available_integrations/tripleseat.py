# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

from typing import TYPE_CHECKING
from ..integration import Integration

__all__ = ["Tripleseat"]

DEFAULT_PUBLIC_KEY = 'a6dd8bc1e4018836b8e9643c978ca04b7ac58081'

class Tripleseat(Integration):
    integration_id = 'custom.tripleseat'
    integration_name = 'tripleseat'
    def get_bookings(self) -> requests.Response: ...
    def create_lead(self, public_key: str = ..., first_name: str | None = None, last_name: str | None = None, email_address: str | None = None, phone_number: str | None = None, location_id: str | None = None, additional_fields: dict | None = None) -> requests.Response: ...

if TYPE_CHECKING:
    import requests
