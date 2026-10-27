"""Structured errors returned by CLI and MCP tools."""

from __future__ import annotations

from typing import Any


class ToolError(RuntimeError):
    def __init__(self, code: str, message: str, **details: Any) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.details = details

    def as_dict(self) -> dict[str, Any]:
        return {"code": self.code, "message": self.message, **self.details}


def error_payload(exc: Exception) -> dict[str, Any]:
    code = getattr(exc, "code", None)
    message = getattr(exc, "message", None)
    details = getattr(exc, "details", None)
    if isinstance(code, str) and isinstance(message, str):
        payload = {"code": code, "message": message}
        if isinstance(details, dict):
            payload.update(details)
        return payload
    return {"code": "internal_error", "message": str(exc) or exc.__class__.__name__}
