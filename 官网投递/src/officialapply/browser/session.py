"""Persist which tab the tool is currently driving."""

from __future__ import annotations

import json
import os
import time
from pathlib import Path
from typing import Any


class SessionStore:
    def __init__(self, path: str | Path) -> None:
        self.path = Path(path)

    def load(self) -> dict[str, Any]:
        try:
            data = json.loads(self.path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return {}
        return data if isinstance(data, dict) else {}

    def save(self, target_id: str, url: str) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        payload = {"target_id": target_id, "url": url, "updated_at": time.time()}
        temp = self.path.with_suffix(self.path.suffix + ".tmp")
        temp.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
        os.replace(temp, self.path)

    def clear(self) -> None:
        try:
            self.path.unlink()
        except OSError:
            pass
