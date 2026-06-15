# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

from ..integration import Integration
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import requests

__all__ = ["DEFAULT_PUBLIC_KEY", "Tripleseat"]

DEFAULT_PUBLIC_KEY = 'a6dd8bc1e4018836b8e9643c978ca04b7ac58081'

class Tripleseat(Integration):
    """Tripleseat integration class for proxying requests to the Tripleseat API with"""

    def get_bookings(self) -> requests.Response:
        """Example method for getting bookings from Tripleseat"""
        ...
    def create_lead(self, public_key: str = ..., first_name: str | None = ..., last_name: str | None = ..., email_address: str | None = ..., phone_number: str | None = ..., location_id: str | None = ..., additional_fields: dict | None = ...) -> requests.Response:
        """Create a lead in Tripleseat using the proxy request method"""
        ...
