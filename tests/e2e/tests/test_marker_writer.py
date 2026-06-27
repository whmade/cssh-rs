"""Unit tests for the forced-command marker writer."""

from __future__ import annotations

import io
import types
from pathlib import Path

import pytest

from libraries import _marker_writer


def _fake_stdin(data: bytes) -> types.SimpleNamespace:
    return types.SimpleNamespace(buffer=io.BytesIO(data))


def test_appends_stdin_to_marker(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    marker = tmp_path / "h1.log"
    monkeypatch.setattr("sys.argv", ["_marker_writer.py", str(marker)])
    monkeypatch.setattr("sys.stdin", _fake_stdin(b"hello-h1\n"))

    rc = _marker_writer.main()

    assert rc == 0
    assert marker.read_bytes() == b"hello-h1\n"


def test_appends_without_truncating(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    marker = tmp_path / "h1.log"
    marker.write_bytes(b"first\n")
    monkeypatch.setattr("sys.argv", ["_marker_writer.py", str(marker)])
    monkeypatch.setattr("sys.stdin", _fake_stdin(b"second\n"))

    rc = _marker_writer.main()

    assert rc == 0
    assert marker.read_bytes() == b"first\nsecond\n"


def test_missing_argument_returns_2(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("sys.argv", ["_marker_writer.py"])

    assert _marker_writer.main() == 2
