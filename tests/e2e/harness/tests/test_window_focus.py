"""Unit tests for the window focus library's matching and activation logic."""

from __future__ import annotations

import pytest
import pywinctl

from cssh_rs_e2e import window_focus
from cssh_rs_e2e.window_focus import WindowFocus, WindowFocusError


class _FakeWindow:
    def __init__(self, title: str, *, activates: bool = True) -> None:
        self.title = title
        self._activates = activates
        self.activate_calls: list[dict[str, object]] = []

    def activate(self, wait: bool = False, user: bool = True) -> bool:
        self.activate_calls.append({"wait": wait, "user": user})
        return self._activates


def _patch_matches(
    monkeypatch: pytest.MonkeyPatch, matches: list[_FakeWindow]
) -> list[dict[str, object]]:
    """Patch getWindowsWithTitle to return ``matches`` and record its calls."""
    calls: list[dict[str, object]] = []

    def fake(title: str, condition: int = pywinctl.Re.IS, **_: object) -> list[_FakeWindow]:
        calls.append({"title": title, "condition": condition})
        return matches

    monkeypatch.setattr(pywinctl, "getWindowsWithTitle", fake)
    return calls


def test_focus_window_exact_uses_is_condition(monkeypatch: pytest.MonkeyPatch) -> None:
    calls = _patch_matches(monkeypatch, [_FakeWindow("cssh-rs daemon")])

    WindowFocus().focus_window("cssh-rs daemon")

    assert calls[0]["condition"] == pywinctl.Re.IS


def test_focus_window_substring_uses_contains_condition(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls = _patch_matches(monkeypatch, [_FakeWindow("cssh-rs - tester@h1")])

    WindowFocus().focus_window("@h1", match_mode="substring")

    assert calls[0]["condition"] == pywinctl.Re.CONTAINS


def test_focus_window_rejects_unknown_match_mode() -> None:
    with pytest.raises(WindowFocusError, match="match_mode must be"):
        WindowFocus().focus_window("cssh-rs daemon", match_mode="fuzzy")


def test_focus_window_rejects_negative_timeout() -> None:
    with pytest.raises(WindowFocusError, match="timeout must be non-negative"):
        WindowFocus().focus_window("cssh-rs daemon", timeout=-1.0)


def test_focus_window_rejects_negative_poll_interval() -> None:
    with pytest.raises(WindowFocusError, match="poll_interval must be non-negative"):
        WindowFocus().focus_window("cssh-rs daemon", poll_interval=-1.0)


def test_focus_window_returns_matched_title(monkeypatch: pytest.MonkeyPatch) -> None:
    window = _FakeWindow("cssh-rs daemon")
    _patch_matches(monkeypatch, [window])

    result = WindowFocus().focus_window("cssh-rs daemon")

    assert result == "cssh-rs daemon"
    assert window.activate_calls == [{"wait": True, "user": True}]


def test_focus_window_raises_when_activation_fails(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _patch_matches(monkeypatch, [_FakeWindow("cssh-rs daemon", activates=False)])

    with pytest.raises(WindowFocusError, match="failed to focus"):
        WindowFocus().focus_window("cssh-rs daemon")


def test_focus_window_rejects_multiple_matches(monkeypatch: pytest.MonkeyPatch) -> None:
    _patch_matches(
        monkeypatch,
        [_FakeWindow("cssh-rs - tester@h1"), _FakeWindow("cssh-rs - tester@h10")],
    )

    with pytest.raises(WindowFocusError, match=r"multiple windows match.*h10"):
        WindowFocus().focus_window("@h1", match_mode="substring")


def test_focus_window_retries_until_a_match_appears(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    window = _FakeWindow("cssh-rs daemon")
    results: list[list[_FakeWindow]] = [[], [window]]

    def fake(*_: object, **__: object) -> list[_FakeWindow]:
        return results.pop(0)

    monkeypatch.setattr(pywinctl, "getWindowsWithTitle", fake)
    monkeypatch.setattr(window_focus.time, "sleep", lambda _: None)

    result = WindowFocus().focus_window("cssh-rs daemon", poll_interval=0.0)

    assert result == "cssh-rs daemon"
    assert results == []


def test_focus_window_times_out_without_a_match(monkeypatch: pytest.MonkeyPatch) -> None:
    _patch_matches(monkeypatch, [])
    monkeypatch.setattr(window_focus.time, "sleep", lambda _: None)

    with pytest.raises(WindowFocusError, match="no window matching"):
        WindowFocus().focus_window("cssh-rs daemon", timeout=0.0)


def test_get_active_window_title_returns_value(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(pywinctl, "getActiveWindowTitle", lambda: "cssh-rs daemon")

    assert WindowFocus().get_active_window_title() == "cssh-rs daemon"


def test_get_active_window_title_returns_empty_when_none(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(pywinctl, "getActiveWindowTitle", lambda: None)

    assert WindowFocus().get_active_window_title() == ""
