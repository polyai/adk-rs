# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations

from typing import Protocol

__all__ = ["SMSClientFailure", "SMSCredentials", "SMSTemplate", "OutgoingSMSTemplate", "OutgoingSMS", "SMSObj", "parse_sms_dict", "SMSSentEvent", "fibonacci_backoff", "SMSClient", "TwilioSMSClient", "TelnyxSMSClient"]

class SMSClientFailure(Exception):
    """SMS Client Failure"""

    def __init__(self, integration: str, reason: str): ...

class SMSCredentials:
    """SMS credentials"""

    account_sid: str
    auth_token: str
    def __init__(self, account_sid: str, auth_token: str) -> None: ...

class SMSTemplate:
    """SMS template"""

    name: str
    content: str
    phone_number: str
    def __init__(self, name: str, content: str, phone_number: str) -> None: ...

class OutgoingSMSTemplate:
    """SMS template to send"""

    to_number: str
    template: str
    def __init__(self, to_number: str, template: str) -> None: ...

class OutgoingSMS:
    """SMS to send"""

    to_number: str
    from_number: str
    content: str
    content_id: str | None
    def __init__(self, to_number: str, from_number: str, content: str, content_id: str | None = ...) -> None: ...

class SMSSentEvent:
    """Represents an attempt to send an SMS"""

    success: bool
    sms: SMSObj
    def __init__(self, success: bool, sms: SMSObj) -> None: ...
    @classmethod
    def from_dict(cls, d) -> SMSSentEvent:
        """Create an SMSSentEvent from a JSON dict"""
        ...
    def to_dict(self) -> dict:
        """Convert SMSSentEvent to a JSON dict"""
        ...

class SMSClient(Protocol):
    """SMS Client Protocol"""

    def send_sms(self, sms: OutgoingSMS) -> dict:
        """Send SMS protocol"""
        ...
    def retry_send_sms(self, sms: OutgoingSMS, retry_count: int) -> dict:
        """Send SMS protocol with retry"""
        ...

class TwilioSMSClient(SMSClient):
    """Twilio SMS Client"""

    def __init__(self, sms_credentials: SMSCredentials): ...
    def send_content_template(self, sms: OutgoingSMS, **kwargs) -> dict:
        """Sends SMS using twilio API"""
        ...
    def send_sms(self, sms: OutgoingSMS) -> dict:
        """Sends SMS using twilio API"""
        ...
    def retry_send_sms(self, sms: OutgoingSMS, retry_count: int):
        """Sends SMS using Twilio API with retry."""
        ...
    def retry_send_content_template(self, sms: OutgoingSMS, retry_count: int, **kwargs):
        """Sends Content Template using Twilio API with retry."""
        ...

class TelnyxSMSClient(SMSClient):
    """Telnyx SMS Client"""

    def __init__(self, sms_credentials: SMSCredentials): ...
    def send_sms(self, sms: OutgoingSMS) -> dict:
        """Sends SMS using Telnyx API"""
        ...
    def retry_send_sms(self, sms: OutgoingSMS, retry_count: int) -> dict:
        """Sends SMS using Telnyx API with retry"""
        ...

def parse_sms_dict(d: dict) -> SMSObj:
    """Parse a dictionary to an SMS object"""
    ...

def fibonacci_backoff(n: int):
    """Dynamic programming fibonacci backoff"""
    ...

SMSObj = OutgoingSMS | OutgoingSMSTemplate
