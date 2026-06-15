# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

import requests
from ..integration import Integration

__all__ = ["BASE_OPENTABLE_API_URL", "V1_BASE_OPENTABLE_API_URL_SUFFIX", "V2_BASE_OPENTABLE_API_URL_SUFFIX", "OPENTABLE_AUTH_URL", "OPENTABLE_SECRET_NAME", "OpenTable"]

BASE_OPENTABLE_API_URL = 'https://api.opentable.com'
V1_BASE_OPENTABLE_API_URL_SUFFIX = '/inhouse/v1'
V2_BASE_OPENTABLE_API_URL_SUFFIX = '/v2'
OPENTABLE_AUTH_URL = 'https://oauth.opentable.com/api/v2/oauth/token?grant_type=client_credentials'
OPENTABLE_SECRET_NAME = 'opentable_api'

class OpenTable(Integration):
    """OpenTable integration class for proxying requests to the OpenTable API with"""

    def proxy_request(self, endpoint: str, http_method: str, base_url: str | None = ..., headers: dict[str, str] | None = ..., params: dict[str, str] | None = ..., body: dict[str, any] | None = ..., timeout: int = ...) -> requests.Response:
        """Proxy a request to the OpenTable API using the integration's authentication"""
        ...
    def check_availability(self, restaurant_id: str, party_size: int, start_date_time: str, forward_minutes: int | None = ..., backward_minutes: int | None = ..., include_experiences: bool = ...) -> requests.Response:
        """Search for availability at a specific restaurant using the proxy request method"""
        ...
    def lock_slot(self, restaurant_id: int, party_size: int, date_time: str, table_type: str = ..., dining_area_id: int | None = ..., experience_id: int | None = ...) -> requests.Response:
        """Lock a reservation slot at a specific restaurant. AKA Booking Lock"""
        ...
    def make_reservation(self, restaurant_id: str, reservation_token: str, first_name: str, last_name: str, phone_number: str, phone_country_code: int = ..., special_request: str | None = ..., sms_notifications_opt_in: bool = ..., table_type: str = ..., dining_area_id: int | None = ..., payments: dict | None = ...) -> requests.Response:
        """Create a reservation at a specific restaurant"""
        ...
    def lookup_bookings(self, restaurant_id: str, phone_number: str, phone_country_code: int = ...) -> requests.Response:
        """Search for reservations at a specific restaurant by phone number"""
        ...
    def update_reservation(self, restaurant_id: str, reservation_id: str, party_size: int | None = ..., date_time: str | None = ..., special_request: str | None = ...) -> requests.Response:
        """Update an existing reservation at a specific restaurant"""
        ...
    def cancel_booking(self, restaurant_id: str, reservation_id: str) -> requests.Response:
        """Cancel an existing reservation at a specific restaurant"""
        ...
    def get_experiences(self, restaurant_id: str) -> requests.Response:
        """Get available experiences for a specific restaurant"""
        ...
    def check_availability_v2(self, restaurant_id: str, party_size: int, start_date_time: str, forward_minutes: int | None = ..., backward_minutes: int | None = ..., require_attributes: str | None = ..., include_credit_card_results: bool = ..., include_experiences: bool = ...) -> requests.Response:
        """Search for availability at a specific restaurant using the V2 API"""
        ...
