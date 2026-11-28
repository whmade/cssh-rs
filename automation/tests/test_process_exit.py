"""Unit tests for the process exit library's argument checks and handle bookkeeping."""

from __future__ import annotations

import contextlib
import ctypes
import math
from collections.abc import Callable
from types import SimpleNamespace

import pytest
import pywinctl

from cssh_rs_automation import process_exit
from cssh_rs_automation.process_exit import ProcessExit, ProcessExitError


class _FakeWindow:
    def __init__(self, process_id: int | None) -> None:
        self._process_id = process_id

    def getPID(self) -> int | None:  # noqa: N802 - mirrors pywinctl's method name
        return self._process_id


def _patch_windows(monkeypatch: pytest.MonkeyPatch, *process_ids: int | None) -> None:
    """Make each window lookup find one window, owned by the next of ``process_ids``."""
    windows = iter([_FakeWindow(process_id) for process_id in process_ids])
    monkeypatch.setattr(pywinctl, "getWindowsWithTitle", lambda *_, **__: [next(windows)])


def _patch_handles(monkeypatch: pytest.MonkeyPatch) -> list[int]:
    """Open handles 100, 101, ... in turn and return the list of handles closed."""
    handles = iter(range(100, 200))
    closed: list[int] = []
    monkeypatch.setattr(process_exit, "_open_process", lambda _: next(handles))
    monkeypatch.setattr(process_exit, "_close_handle", closed.append)
    return closed


def _raise_timeout(*_: object) -> int:
    raise ProcessExitError("process did not exit within 0.0s")


def test_watch_window_process_raises_its_own_error_when_no_window_matches(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(pywinctl, "getWindowsWithTitle", lambda *_, **__: [])

    with pytest.raises(ProcessExitError, match="expected exactly one window"):
        ProcessExit().watch_window_process("@h1", "substring")


@pytest.mark.parametrize("process_id", [None, 0])
def test_watch_window_process_rejects_a_window_without_a_process(
    monkeypatch: pytest.MonkeyPatch, process_id: int | None
) -> None:
    _patch_windows(monkeypatch, process_id)
    monkeypatch.setattr(process_exit, "_open_process", pytest.fail)

    with pytest.raises(ProcessExitError, match="cannot read the process"):
        ProcessExit().watch_window_process("@h1", "substring")


def test_watch_window_process_closes_the_handle_of_an_earlier_watch(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _patch_windows(monkeypatch, 4242, 4242)
    closed = _patch_handles(monkeypatch)
    library = ProcessExit()
    library.watch_window_process("@h1", "substring")

    library.watch_window_process("@h1", "substring")

    assert closed == [100]


@pytest.mark.parametrize("wait", [lambda *_: 0, _raise_timeout])
def test_wait_for_process_exit_closes_the_handle_whatever_the_outcome(
    monkeypatch: pytest.MonkeyPatch, wait: Callable[..., int]
) -> None:
    _patch_windows(monkeypatch, 4242)
    closed = _patch_handles(monkeypatch)
    monkeypatch.setattr(process_exit, "_wait_for_exit_code", wait)
    library = ProcessExit()
    process_id = library.watch_window_process("@h1", "substring")

    with contextlib.suppress(ProcessExitError):
        library.wait_for_process_exit(process_id)

    assert closed == [100]


def test_close_closes_the_handle_of_every_watched_process(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _patch_windows(monkeypatch, 4242, 4343)
    closed = _patch_handles(monkeypatch)
    library = ProcessExit()
    library.watch_window_process("@h1", "substring")
    library.watch_window_process("@h2", "substring")

    library._close()

    assert closed == [100, 101]


@pytest.mark.parametrize("timeout", [-0.001, 5e6, math.inf, math.nan])
def test_wait_for_process_exit_rejects_a_timeout_windows_cannot_wait_for(timeout: float) -> None:
    with pytest.raises(ProcessExitError, match="timeout must be"):
        ProcessExit().wait_for_process_exit(4242, timeout)


def test_wait_for_process_exit_rejects_an_unwatched_process() -> None:
    with pytest.raises(ProcessExitError, match="not watched"):
        ProcessExit().wait_for_process_exit(4242)


def test_close_handle_logs_a_warning_when_closehandle_fails(
    monkeypatch: pytest.MonkeyPatch, caplog: pytest.LogCaptureFixture
) -> None:
    kernel32 = SimpleNamespace(CloseHandle=lambda _: 0)
    monkeypatch.setattr(process_exit, "_kernel32", lambda: kernel32)
    monkeypatch.setattr(ctypes, "get_last_error", lambda: 6, raising=False)

    process_exit._close_handle(0x100)

    assert caplog.messages == ["CloseHandle(0x100) failed: error 6"]
