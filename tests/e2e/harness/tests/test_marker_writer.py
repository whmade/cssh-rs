"""Unit tests for the forced-command marker writer."""

from __future__ import annotations

import os
import types
from pathlib import Path

import pytest

from cssh_rs_e2e import _marker_writer


def _run_with_stdin(monkeypatch: pytest.MonkeyPatch, marker: Path, data: bytes) -> int:
    """Run ``main`` with ``data`` fed through a real pipe as stdin.

    The writer reads via ``os.read`` on the stdin fd, so the test supplies a
    pipe whose write end is closed to signal EOF.
    """
    read_fd, write_fd = os.pipe()
    os.write(write_fd, data)
    os.close(write_fd)
    monkeypatch.setattr("sys.argv", ["_marker_writer.py", str(marker)])
    monkeypatch.setattr("sys.stdin", types.SimpleNamespace(fileno=lambda: read_fd))
    try:
        return _marker_writer.main()
    finally:
        os.close(read_fd)


def test_appends_stdin_to_marker(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    marker = tmp_path / "h1.log"

    rc = _run_with_stdin(monkeypatch, marker, b"hello-h1\n")

    assert rc == 0
    assert marker.read_bytes() == b"hello-h1\n"


def test_appends_without_truncating(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    marker = tmp_path / "h1.log"
    marker.write_bytes(b"first\n")

    rc = _run_with_stdin(monkeypatch, marker, b"second\n")

    assert rc == 0
    assert marker.read_bytes() == b"first\nsecond\n"


def test_missing_argument_returns_2(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("sys.argv", ["_marker_writer.py"])

    assert _marker_writer.main() == 2
