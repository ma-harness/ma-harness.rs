"""ma-harness Python SDK.

业务方 `from mah_py import Mah; m = Mah(); m.run("fix bug")` — 走 subprocess 调 `mah` CLI.

设计: 简化版 v1, 复用 `mah run` (本地, 不连 server). v2 走 `mah run-stream` (gRPC streaming).
"""

from .client import Mah, RunResult, MahError

__version__ = "0.1.0"
__all__ = ["Mah", "RunResult", "MahError"]
