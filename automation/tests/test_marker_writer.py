"""Unit tests for the forced-command marker writer."""

from __future__ import annotations

import os
import types
from pathlib import Path

import pytest

from cssh_rs_automation import _marker_writer


def _run_with_stdin(monkeypatch: pytest.MonkeyPatch, marker: Path, data: bytes, *flags: str) -> int:
    """Run ``main`` with ``flags`` and ``data`` fed via a pipe on the stdin fd."""
    read_fd, write_fd = os.pipe()
    os.write(write_fd, data)
    os.close(write_fd)
    monkeypatch.setattr("sys.argv", ["_marker_writer.py", *flags, str(marker)])
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


def test_raw_input_flag_without_marker_returns_2(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("sys.argv", ["_marker_writer.py", "--raw-input"])

    assert _marker_writer.main() == 2


def _patch_console(monkeypatch: pytest.MonkeyPatch) -> list[int]:
    """Make stdin look like a console read as UTF-16 and record every console mode set on it."""
    modes: list[int] = []
    monkeypatch.setattr(_marker_writer.os, "isatty", lambda _fd: True)
    monkeypatch.setattr(_marker_writer, "_set_console_mode", lambda _fd, mode: modes.append(mode))
    monkeypatch.setattr(
        _marker_writer,
        "_read_console",
        lambda fd, max_units: os.read(fd, max_units).decode().encode("utf-16-le"),
    )
    return modes


def _run_echoing(monkeypatch: pytest.MonkeyPatch, marker: Path, data: bytes, *flags: str) -> bytes:
    """Run ``main`` with stdin fed ``data`` and return what it echoed to stdout."""
    read_fd, write_fd = os.pipe()
    monkeypatch.setattr("sys.stdout", types.SimpleNamespace(fileno=lambda: write_fd))
    try:
        assert _run_with_stdin(monkeypatch, marker, data, *flags) == 0
    finally:
        os.close(write_fd)
    with os.fdopen(read_fd, "rb") as echoed:
        return echoed.read()


def test_raw_input_reads_a_console_as_raw_vt_input(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    modes = _patch_console(monkeypatch)
    marker = tmp_path / "h1.log"

    _run_echoing(monkeypatch, marker, b"\x1b\xe2\x82\xac\x00\x1c\r", "--raw-input")

    # ENABLE_VIRTUAL_TERMINAL_INPUT alone.
    assert modes == [0x0200]
    assert marker.read_bytes() == b"\x1b\xe2\x82\xac\x00\x1c\r"


def test_raw_input_echoes_what_it_records_like_a_terminal(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    _patch_console(monkeypatch)
    marker = tmp_path / "h1.log"
    typed = b"ab\x1b\xe2\x82\xac\x00\x1c\x1d\x02\t\x7f\r"

    echoed = _run_echoing(monkeypatch, marker, typed, "--raw-input")

    assert echoed == b"ab^[\xe2\x82\xac^@^\\^]^B\t^?\r\n"
    assert marker.read_bytes() == typed


def test_raw_input_records_a_character_split_across_console_reads(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    _patch_console(monkeypatch)
    reads = iter(
        [
            "a\ud83d".encode("utf-16-le", "surrogatepass"),
            "\ude00b".encode("utf-16-le", "surrogatepass"),
        ]
    )
    monkeypatch.setattr(_marker_writer, "_read_console", lambda _fd, _max_units: next(reads, b""))
    marker = tmp_path / "h1.log"

    _run_echoing(monkeypatch, marker, b"", "--raw-input")

    assert marker.read_bytes() == "a\U0001f600b".encode()


def test_raw_input_echoes_a_pasted_line_break_once(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    _patch_console(monkeypatch)

    echoed = _run_echoing(monkeypatch, tmp_path / "h1.log", b"one\r\ntwo\n", "--raw-input")

    assert echoed == b"one\r\ntwo\r\n"


def test_default_mode_leaves_the_console_alone(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    modes = _patch_console(monkeypatch)

    echoed = _run_echoing(monkeypatch, tmp_path / "h1.log", b"hello\r")

    assert echoed == b""
    assert modes == []


def test_raw_input_leaves_a_pipe_alone(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    modes: list[int] = []
    monkeypatch.setattr(_marker_writer, "_set_console_mode", lambda _fd, mode: modes.append(mode))

    rc = _run_with_stdin(monkeypatch, tmp_path / "h1.log", b"hello\n", "--raw-input")

    assert rc == 0
    assert modes == []
