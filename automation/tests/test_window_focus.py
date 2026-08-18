"""Unit tests for the window focus library's matching and activation logic."""

from __future__ import annotations

import sys
import types

import pytest
import pywinctl

from cssh_rs_automation import window_focus
from cssh_rs_automation.window_focus import WindowFocus, WindowFocusError


class _FakeWindow:
    def __init__(self, title: str, handle: int = 0) -> None:
        self.title = title
        self._handle = handle

    def getHandle(self) -> int:  # noqa: N802 - mirrors pywinctl's method name
        return self._handle


class _FakeClosableWindow(_FakeWindow):
    def __init__(self, title: str) -> None:
        super().__init__(title)
        self.closed = False

    def close(self) -> None:
        self.closed = True


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


def _patch_activation(
    monkeypatch: pytest.MonkeyPatch, results: list[bool] | None = None
) -> list[_FakeWindow]:
    """Patch ``_activate_window`` to record windows and return queued results.

    ``results`` is consumed one entry per call; when exhausted (or ``None``)
    activation succeeds.
    """
    windows: list[_FakeWindow] = []
    queue = list(results or [])

    def fake(window: _FakeWindow) -> bool:
        windows.append(window)
        return queue.pop(0) if queue else True

    monkeypatch.setattr(window_focus, "_activate_window", fake)
    return windows


def test_focus_window_exact_uses_is_condition(monkeypatch: pytest.MonkeyPatch) -> None:
    calls = _patch_matches(monkeypatch, [_FakeWindow("cssh-rs daemon")])
    _patch_activation(monkeypatch)

    WindowFocus().focus_window("cssh-rs daemon")

    assert calls[0]["condition"] == pywinctl.Re.IS


def test_focus_window_substring_uses_contains_condition(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls = _patch_matches(monkeypatch, [_FakeWindow("cssh-rs - tester@h1")])
    _patch_activation(monkeypatch)

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
    activated = _patch_activation(monkeypatch)

    result = WindowFocus().focus_window("cssh-rs daemon")

    assert result == "cssh-rs daemon"
    assert activated == [window]


def test_focus_window_raises_when_activation_fails(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _patch_matches(monkeypatch, [_FakeWindow("cssh-rs daemon")])
    _patch_activation(monkeypatch, [False])

    with pytest.raises(WindowFocusError, match="failed to focus"):
        WindowFocus().focus_window("cssh-rs daemon", timeout=0.0)


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
    _patch_activation(monkeypatch)

    result = WindowFocus().focus_window("cssh-rs daemon", poll_interval=0.0)

    assert result == "cssh-rs daemon"
    assert results == []


def test_focus_window_retries_until_activation_succeeds(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _patch_matches(monkeypatch, [_FakeWindow("cssh-rs daemon")])
    activated = _patch_activation(monkeypatch, [False, True])
    monkeypatch.setattr(window_focus.time, "sleep", lambda _: None)

    result = WindowFocus().focus_window("cssh-rs daemon", poll_interval=0.0)

    assert result == "cssh-rs daemon"
    assert len(activated) == 2


def test_focus_window_times_out_without_a_match(monkeypatch: pytest.MonkeyPatch) -> None:
    _patch_matches(monkeypatch, [])
    monkeypatch.setattr(window_focus.time, "sleep", lambda _: None)

    with pytest.raises(WindowFocusError, match="no window matching"):
        WindowFocus().focus_window("cssh-rs daemon", timeout=0.0)


def test_count_windows_returns_match_count_via_default_contains(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls = _patch_matches(
        monkeypatch,
        [_FakeWindow("cssh-rs - tester@h1"), _FakeWindow("cssh-rs - tester@h2")],
    )

    assert WindowFocus().count_windows("cssh-rs -") == 2
    assert calls[0]["condition"] == pywinctl.Re.CONTAINS


def test_count_windows_rejects_unknown_match_mode() -> None:
    with pytest.raises(WindowFocusError, match="match_mode must be"):
        WindowFocus().count_windows("cssh-rs -", match_mode="fuzzy")


def test_window_boxes_returns_a_tuple_per_match(monkeypatch: pytest.MonkeyPatch) -> None:
    first = _FakeWindow("cssh-rs - tester@h1")
    first.box = types.SimpleNamespace(left=0, top=0, width=100, height=80)
    second = _FakeWindow("cssh-rs - tester@h2")
    second.box = types.SimpleNamespace(left=100, top=0, width=100, height=80)
    calls = _patch_matches(monkeypatch, [first, second])

    boxes = WindowFocus().window_boxes("cssh-rs -")

    assert boxes == [
        ("cssh-rs - tester@h1", 0, 0, 100, 80),
        ("cssh-rs - tester@h2", 100, 0, 100, 80),
    ]
    assert calls[0]["condition"] == pywinctl.Re.CONTAINS


def test_window_boxes_rejects_unknown_match_mode() -> None:
    with pytest.raises(WindowFocusError, match="match_mode must be"):
        WindowFocus().window_boxes("cssh-rs -", match_mode="fuzzy")


def test_get_active_window_title_returns_value(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(pywinctl, "getActiveWindowTitle", lambda: "cssh-rs daemon")

    assert WindowFocus().get_active_window_title() == "cssh-rs daemon"


def test_get_active_window_title_returns_empty_when_none(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(pywinctl, "getActiveWindowTitle", lambda: None)

    assert WindowFocus().get_active_window_title() == ""


def _patch_match_sequence(
    monkeypatch: pytest.MonkeyPatch, results: list[list[object]]
) -> list[int]:
    """Patch getWindowsWithTitle to return one entry of ``results`` per call."""
    conditions: list[int] = []

    def fake(_title: str, condition: int = pywinctl.Re.IS, **_: object) -> list[object]:
        conditions.append(condition)
        return results.pop(0)

    monkeypatch.setattr(pywinctl, "getWindowsWithTitle", fake)
    monkeypatch.setattr(window_focus.time, "sleep", lambda _: None)
    return conditions


def test_close_window_rejects_unknown_match_mode() -> None:
    with pytest.raises(WindowFocusError, match="match_mode must be"):
        WindowFocus().close_window("cssh-rs daemon", match_mode="fuzzy")


def test_close_window_closes_match_and_returns_title_once_gone(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    window = _FakeClosableWindow("cssh-rs - tester@bravo")
    _patch_match_sequence(monkeypatch, [[window], []])

    result = WindowFocus().close_window("@bravo", match_mode="substring", poll_interval=0.0)

    assert result == "cssh-rs - tester@bravo"
    assert window.closed is True


def test_close_window_substring_uses_contains_condition(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    window = _FakeClosableWindow("cssh-rs - tester@bravo")
    conditions = _patch_match_sequence(monkeypatch, [[window], []])

    WindowFocus().close_window("@bravo", match_mode="substring", poll_interval=0.0)

    assert conditions[0] == pywinctl.Re.CONTAINS


def test_close_window_rejects_multiple_matches(monkeypatch: pytest.MonkeyPatch) -> None:
    _patch_matches(
        monkeypatch,
        [_FakeClosableWindow("cssh-rs - tester@h1"), _FakeClosableWindow("cssh-rs - tester@h10")],
    )

    with pytest.raises(WindowFocusError, match=r"multiple windows match.*h10"):
        WindowFocus().close_window("@h1", match_mode="substring")


def test_close_window_times_out_without_a_match(monkeypatch: pytest.MonkeyPatch) -> None:
    _patch_matches(monkeypatch, [])
    monkeypatch.setattr(window_focus.time, "sleep", lambda _: None)

    with pytest.raises(WindowFocusError, match="no window matching"):
        WindowFocus().close_window("@bravo", match_mode="substring", timeout=0.0)


def test_close_window_times_out_when_window_persists(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    window = _FakeClosableWindow("cssh-rs - tester@bravo")
    _patch_matches(monkeypatch, [window])
    monkeypatch.setattr(window_focus.time, "sleep", lambda _: None)

    with pytest.raises(WindowFocusError, match="still present"):
        WindowFocus().close_window("@bravo", match_mode="substring", timeout=0.0)

    assert window.closed is True


def test_window_z_order_index_returns_walk_result(monkeypatch: pytest.MonkeyPatch) -> None:
    _patch_matches(monkeypatch, [_FakeWindow("cssh-rs - tester@h1", handle=42)])
    walked: list[int] = []
    monkeypatch.setattr(window_focus, "_z_order_index", lambda hwnd: walked.append(hwnd) or 3)

    assert WindowFocus().window_z_order_index("@h1", match_mode="substring") == 3
    assert walked == [42]


def test_window_z_order_index_rejects_unknown_match_mode() -> None:
    with pytest.raises(WindowFocusError, match="match_mode must be"):
        WindowFocus().window_z_order_index("cssh-rs daemon", match_mode="fuzzy")


def test_window_z_order_index_raises_when_not_exactly_one(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _patch_matches(monkeypatch, [])

    with pytest.raises(WindowFocusError, match="expected exactly one window"):
        WindowFocus().window_z_order_index("cssh-rs daemon")


def _patch_pyautogui(monkeypatch: pytest.MonkeyPatch) -> types.SimpleNamespace:
    """Inject a fake pyautogui recording right-clicks and its failsafe state."""
    fake = types.SimpleNamespace(
        FAILSAFE=True, clicks=[], rightClick=lambda x, y: fake.clicks.append((x, y))
    )
    monkeypatch.setitem(sys.modules, "pyautogui", fake)
    return fake


def test_right_click_window_clicks_center(monkeypatch: pytest.MonkeyPatch) -> None:
    window = _FakeWindow("cssh-rs daemon")
    window.box = types.SimpleNamespace(left=10, top=20, width=100, height=80)
    _patch_matches(monkeypatch, [window])
    fake = _patch_pyautogui(monkeypatch)

    assert WindowFocus().right_click_window("cssh-rs daemon") == "cssh-rs daemon"
    assert fake.clicks == [(60, 60)]
    assert fake.FAILSAFE is False


def test_right_click_window_rejects_unknown_match_mode() -> None:
    with pytest.raises(WindowFocusError, match="match_mode must be"):
        WindowFocus().right_click_window("cssh-rs daemon", match_mode="fuzzy")


def test_right_click_window_raises_when_not_exactly_one(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _patch_matches(monkeypatch, [])

    with pytest.raises(WindowFocusError, match="expected exactly one window"):
        WindowFocus().right_click_window("cssh-rs daemon")
