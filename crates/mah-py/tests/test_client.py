"""Tests for mah_py.client.

跑法: pytest tests/test_client.py -v
需要 `mah` binary 在 PATH 或 MAH_PATH (跟 examples 一样).
"""
import os
import shutil
import subprocess
from pathlib import Path

import pytest

from mah_py.client import Mah, MahError, RunResult, _parse_mah_run_output, find_mah_executable


# ---- Fixtures ----

@pytest.fixture
def mah_path() -> str:
    """Skip all tests if mah not found."""
    try:
        return find_mah_executable()
    except MahError as e:
        pytest.skip(f"mah binary not found: {e}")


# ---- find_mah_executable ----

def test_find_mah_executable_returns_path(mah_path):
    """find_mah_executable returns existing path."""
    assert Path(mah_path).exists()


def test_find_mah_explicit_bad_path():
    """Explicit bad path raises MahError."""
    with pytest.raises(MahError, match="not found"):
        find_mah_executable(explicit="/nonexistent/mah")


def test_find_mah_explicit_good_path(mah_path):
    """Explicit good path returns."""
    assert find_mah_executable(explicit=mah_path) == mah_path


def test_find_mah_env_path(mah_path, monkeypatch):
    """MAH_PATH env var is honored."""
    monkeypatch.setenv("MAH_PATH", mah_path)
    assert find_mah_executable() == mah_path


def test_find_mah_not_in_path(monkeypatch):
    """No PATH, no MAH_PATH, no explicit → MahError."""
    monkeypatch.delenv("MAH_PATH", raising=False)
    monkeypatch.setenv("PATH", "")  # empty PATH
    monkeypatch.setenv("MAH_BIN", "")  # ensure shutil.which returns None
    with pytest.raises(MahError, match="not found"):
        find_mah_executable()


# ---- _parse_mah_run_output ----

def test_parse_mah_run_output_basic():
    """Parse well-formed `mah run` output."""
    raw = """\
Session: local-abc-123
Run: run-001
Content: Hello, world!
Tokens: prompt=10 completion=5
"""
    r = _parse_mah_run_output(raw)
    assert r.session_id == "local-abc-123"
    assert r.run_id == "run-001"
    assert r.content == "Hello, world!"
    assert r.prompt_tokens == 10
    assert r.completion_tokens == 5


def test_parse_mah_run_output_multiline_content():
    """Content with newlines preserved."""
    raw = """\
Session: s
Run: r
Content: line 1
line 2
line 3
Tokens: prompt=1 completion=2
"""
    r = _parse_mah_run_output(raw)
    assert r.content == "line 1\nline 2\nline 3"


def test_parse_mah_run_output_missing_session():
    """Missing Session field → MahError."""
    raw = """\
Run: r
Content: hi
"""
    with pytest.raises(MahError, match="missing"):
        _parse_mah_run_output(raw)


# ---- Mah.version ----

def test_mah_version(mah_path):
    """Mah.version() returns version string."""
    with Mah(mah_path=mah_path) as m:
        v = m.version()
    assert v.startswith("mah "), f"unexpected version format: {v!r}"
    # e.g. "mah 0.1.0"
    assert len(v.split()) == 2


# ---- Mah.run ----

def test_mah_run_basic(mah_path):
    """Mah.run with stub model returns RunResult."""
    with Mah(mah_path=mah_path, model="stub", timeout=30.0) as m:
        r = m.run("say hi")
    assert isinstance(r, RunResult)
    assert r.session_id  # non-empty
    assert r.run_id  # non-empty
    # stub model may or may not return content; just check it parses
    assert isinstance(r.content, str)


def test_mah_run_with_explicit_model(mah_path):
    """Mah.run with explicit model override."""
    with Mah(mah_path=mah_path) as m:
        r = m.run("say hi", model="stub")
    assert r.session_id


def test_mah_run_with_session(mah_path):
    """Mah.run with explicit session id."""
    session_id = "test-session-xyz"
    with Mah(mah_path=mah_path) as m:
        r1 = m.run("turn 1", session=session_id)
        r2 = m.run("turn 2", session=session_id)
    assert r1.session_id == session_id
    assert r2.session_id == session_id


def test_mah_run_after_close_raises(mah_path):
    """Run after close → MahError."""
    m = Mah(mah_path=mah_path)
    m.close()
    with pytest.raises(MahError, match="closed"):
        m.run("test")


def test_mah_run_bad_binary_raises():
    """Run with non-existent binary → MahError."""
    with pytest.raises(MahError, match="not found"):
        Mah(mah_path="/totally/nonexistent/mah").run("test")


# ---- Mah.conformance ----

def test_mah_conformance_runs(mah_path):
    """Mah.conformance with dsh_synthetic fixture."""
    fixture_root = Path(__file__).parent.parent.parent / "ma-harness-conformance" / "fixtures"
    dsh_path = fixture_root / "dsh_synthetic.jsonl"
    if not dsh_path.exists():
        pytest.skip(f"dsh_synthetic.jsonl not found at {dsh_path}")

    with Mah(mah_path=mah_path) as m:
        summary = m.conformance(fixtures=dsh_path, dsh=True)
    # Should mention 7/7 pass (P11-1.5 收官)
    assert "7 / 7" in summary or "7/7" in summary
    assert "100" in summary  # 100% pass rate


def test_mah_conformance_missing_fixture(mah_path):
    """Mah.conformance with missing fixture → MahError."""
    with Mah(mah_path=mah_path) as m:
        with pytest.raises(MahError, match="failed|Error|not found"):
            m.conformance(fixtures="/nonexistent/fixture.jsonl")
