"""mah-py client: subprocess wrapper for `mah` CLI.

设计参考 dsh Python SDK (`deepseek_harness.DeepSeekHarness`), 但更轻量:
- 不依赖 jsonrpc stdio server (mah 没有)
- 直接 subprocess 调 `mah run` (本地, 一次性) / `mah run-stream` (gRPC streaming)
- v1: 用 `mah run` (同步, 等结果)
- v2: 用 `mah run-stream` (流式, 跟 dsh SDK 看齐)
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional, Union


class MahError(RuntimeError):
    """mah CLI 调用失败"""


@dataclass
class RunResult:
    """`mah run` 的解析结果.

    字段跟 `mah run` 输出对齐 (crates/ma-harness-cli/src/main.rs:407):
    - session_id
    - run_id
    - content (model response)
    - prompt_tokens
    - completion_tokens
    """
    session_id: str
    run_id: str
    content: str
    prompt_tokens: int
    completion_tokens: int

    def __str__(self) -> str:
        return self.content


# 解析 `mah run` 输出 (k=v, content 可能跨多行)
_KV_PATTERN = re.compile(r"^([A-Za-z_]+):\s*(.*)$")


def _parse_mah_run_output(stdout: str) -> RunResult:
    """Parse `mah run` stdout into RunResult.

    格式:
        Session: <session_id>
        Run: <run_id>
        Content: <content (可能跨多行)>
        Tokens: prompt=<n> completion=<n>

    Content 是特殊字段, 后面的行 (直到下个 "Field:" 开头) 都算 content.
    """
    fields: dict[str, str] = {}
    current_key: Optional[str] = None
    for line in stdout.splitlines():
        stripped = line.strip()
        m = _KV_PATTERN.match(stripped)
        if m:
            current_key = m.group(1).lower()
            val = m.group(2).strip()
            fields[current_key] = val
        elif current_key is not None and stripped:
            # continuation of previous field (e.g. multi-line Content)
            fields[current_key] = fields[current_key] + "\n" + stripped

    try:
        return RunResult(
            session_id=fields["session"],
            run_id=fields["run"],
            content=fields["content"],
            prompt_tokens=_parse_int(fields.get("tokens", "prompt=0"), "prompt"),
            completion_tokens=_parse_int(fields.get("tokens", "completion=0"), "completion"),
        )
    except KeyError as e:
        raise MahError(f"failed to parse `mah run` output: missing {e}, raw:\n{stdout}")


def _parse_int(s: str, key: str) -> int:
    """Parse 'prompt=42' / 'completion=10' style into int."""
    for part in s.split():
        if part.startswith(f"{key}="):
            return int(part.split("=", 1)[1])
    return 0


def find_mah_executable(explicit: Optional[str] = None) -> str:
    """Locate `mah` binary.

    顺序:
    1. 业务方显式 `mah_path` 传
    2. 环境变量 `MAH_PATH`
    3. PATH 里 `mah` / `mah.exe`
    """
    if explicit:
        if not Path(explicit).exists():
            raise MahError(f"mah binary not found at explicit path: {explicit}")
        return explicit

    env_path = os.environ.get("MAH_PATH")
    if env_path and Path(env_path).exists():
        return env_path

    binary = "mah.exe" if sys.platform == "win32" else "mah"
    found = shutil.which(binary)
    if found:
        return found

    raise MahError(
        "mah binary not found. Install ma-harness CLI or set MAH_PATH env var. "
        "Searched: $MAH_PATH, PATH (looking for `mah`/`mah.exe`)"
    )


class Mah:
    """mah-py client.

    业务方:
        from mah_py import Mah
        with Mah() as m:
            r = m.run("fix bug in repo")
            print(r.content)
    """

    def __init__(
        self,
        mah_path: Optional[str] = None,
        model: str = "stub",
        timeout: float = 60.0,
    ) -> None:
        """
        Args:
            mah_path: `mah` binary 路径 (None = auto-detect via PATH / MAH_PATH)
            model: 默认 model (default: "stub", 真 LLM 走 "openai:gpt-4o-mini" 等)
            timeout: subprocess timeout (秒, default: 60)
        """
        self.mah_path = find_mah_executable(mah_path)
        self.model = model
        self.timeout = timeout
        self._closed = False

    def __enter__(self) -> "Mah":
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        self.close()

    def close(self) -> None:
        """Mark client as closed. (No persistent subprocess in v1, kept for API symmetry.)"""
        self._closed = True

    def run(
        self,
        message: str,
        session: Optional[str] = None,
        model: Optional[str] = None,
    ) -> RunResult:
        """跑一次 agent (同步, 等结果).

        Args:
            message: 用户消息
            session: 可选 session ID (None = 新建)
            model: 覆盖默认 model (e.g. "openai:gpt-4o-mini")

        Returns:
            RunResult (session_id, run_id, content, tokens)

        Raises:
            MahError: mah binary 找不到 / 跑失败 / 输出解析失败
        """
        if self._closed:
            raise MahError("Mah client is closed")

        args = [self.mah_path, "run"]
        if session:
            args.extend(["--session", session])
        if model is not None:
            args.extend(["--model", model])
        else:
            args.extend(["--model", self.model])
        args.append(message)

        try:
            proc = subprocess.run(
                args,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=self.timeout,
                check=False,
            )
        except subprocess.TimeoutExpired as e:
            raise MahError(f"`mah run` timeout after {self.timeout}s: {e}") from e
        except FileNotFoundError as e:
            raise MahError(f"mah binary not executable: {self.mah_path}: {e}") from e

        if proc.returncode != 0:
            raise MahError(
                f"`mah run` failed (exit={proc.returncode}):\n"
                f"  stderr: {(proc.stderr or '').strip()}\n"
                f"  stdout: {(proc.stdout or '').strip()}"
            )

        return _parse_mah_run_output(proc.stdout or "")

    def version(self) -> str:
        """Run `mah version` and return version string."""
        try:
            proc = subprocess.run(
                [self.mah_path, "version"],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=10.0,
                check=True,
            )
            # Output format: "mah 0.1.0"
            return (proc.stdout or "").strip()
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired, FileNotFoundError) as e:
            raise MahError(f"failed to get mah version: {e}") from e

    def conformance(
        self,
        fixtures: Union[str, Path],
        dsh: bool = False,
        output: Optional[Union[str, Path]] = None,
        verbose: bool = False,
    ) -> str:
        """跑 conformance fixture (跟 `mah conformance` 一致).

        Args:
            fixtures: fixture 路径 (文件 .jsonl 或目录)
            dsh: 走 dsh 风格 fixture
            output: 报告输出目录 (None = default `target/`)
            verbose: 打印每条 fixture 跑的过程

        Returns:
            `mah conformance` 的 stdout 摘要

        Raises:
            MahError: 跑失败
        """
        args = [self.mah_path, "conformance", "--fixtures", str(fixtures)]
        if dsh:
            args.append("--dsh")
        if output:
            args.extend(["--output", str(output)])
        if verbose:
            args.append("--verbose")

        try:
            proc = subprocess.run(
                args,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=self.timeout,
                check=False,
            )
        except subprocess.TimeoutExpired as e:
            raise MahError(f"`mah conformance` timeout: {e}") from e

        if proc.returncode != 0:
            err = (proc.stderr or "").strip()
            raise MahError(
                f"`mah conformance` failed (exit={proc.returncode}): {err}"
            )
        # Combine stdout + stderr (mah 把 summary 写 stderr, debug log 也 stderr)
        return ((proc.stdout or "") + (proc.stderr or "")).strip()
