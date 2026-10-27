"""BossHunter - BOSS直聘岗位搜索与投递工具。"""

import os
import sys

# Windows: force UTF-8 for console output
if sys.platform == "win32":
    os.environ.setdefault("PYTHONUTF8", "1")
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")

__version__ = "3.0.0"
