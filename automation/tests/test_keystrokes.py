"""Unit tests for the keystroke delivery library.

A fake pyautogui module is injected into ``sys.modules`` so the tests never
import the real library, which fails to load on a headless Linux dev box.
"""

from __future__ import annotations

import sys
import types

import pytest

from cssh_rs_automation.keystrokes import Keystrokes, KeystrokesError


class _FakePyAutoGui(types.SimpleNamespace):
    def __init__(self) -> None:
        super().__init__(FAILSAFE=True)
        self.calls: list[tuple[str, tuple[object, ...], dict[str, object]]] = []

    def write(self, *args: object, **kwargs: object) -> None:
        self.calls.append(("write", args, kwargs))

    def press(self, *args: object, **kwargs: object) -> None:
        self.calls.append(("press", args, kwargs))

    def hotkey(self, *args: object, **kwargs: object) -> None:
        self.calls.append(("hotkey", args, kwargs))


@pytest.fixture
def fake_pyautogui(monkeypatch: pytest.MonkeyPatch) -> _FakePyAutoGui:
    fake = _FakePyAutoGui()
    monkeypatch.setitem(sys.modules, "pyautogui", fake)
    return fake


def test_type_text_forwards_text_and_interval(fake_pyautogui: _FakePyAutoGui) -> None:
    Keystrokes().type_text("hello", interval=0.05)

    assert fake_pyautogui.calls == [("write", ("hello",), {"interval": 0.05})]


def test_type_text_defaults_interval_to_zero(fake_pyautogui: _FakePyAutoGui) -> None:
    Keystrokes().type_text("hello")

    assert fake_pyautogui.calls == [("write", ("hello",), {"interval": 0.0})]


def test_type_text_rejects_negative_interval(fake_pyautogui: _FakePyAutoGui) -> None:
    with pytest.raises(KeystrokesError, match="non-negative"):
        Keystrokes().type_text("hello", interval=-0.1)

    assert fake_pyautogui.calls == []


def test_type_line_types_text_then_presses_enter(fake_pyautogui: _FakePyAutoGui) -> None:
    Keystrokes().type_line("payload", interval=0.01)

    assert fake_pyautogui.calls == [
        ("write", ("payload",), {"interval": 0.01}),
        ("press", ("enter",), {}),
    ]


def test_press_key_forwards_key(fake_pyautogui: _FakePyAutoGui) -> None:
    Keystrokes().press_key("tab")

    assert fake_pyautogui.calls == [("press", ("tab",), {})]


def test_press_key_rejects_empty_key(fake_pyautogui: _FakePyAutoGui) -> None:
    with pytest.raises(KeystrokesError, match="non-empty"):
        Keystrokes().press_key("")

    assert fake_pyautogui.calls == []


def test_send_hotkey_forwards_all_keys(fake_pyautogui: _FakePyAutoGui) -> None:
    Keystrokes().send_hotkey("ctrl", "c")

    assert fake_pyautogui.calls == [("hotkey", ("ctrl", "c"), {})]


def test_send_hotkey_rejects_no_keys(fake_pyautogui: _FakePyAutoGui) -> None:
    with pytest.raises(KeystrokesError, match="at least one key"):
        Keystrokes().send_hotkey()

    assert fake_pyautogui.calls == []


def test_send_hotkey_rejects_empty_key(fake_pyautogui: _FakePyAutoGui) -> None:
    with pytest.raises(KeystrokesError, match="non-empty"):
        Keystrokes().send_hotkey("ctrl", "")

    assert fake_pyautogui.calls == []


def test_keywords_disable_failsafe(fake_pyautogui: _FakePyAutoGui) -> None:
    Keystrokes().type_text("x")

    assert fake_pyautogui.FAILSAFE is False


def test_key_listener_receives_typed_text(fake_pyautogui: _FakePyAutoGui) -> None:  # noqa: ARG001
    events: list[tuple[str, str]] = []
    keys = Keystrokes()
    keys.add_key_listener(lambda label, kind: events.append((label, kind)))

    keys.type_text("echo hi")

    assert events == [("echo hi", "text")]


def test_key_listener_reports_line_then_enter(fake_pyautogui: _FakePyAutoGui) -> None:  # noqa: ARG001
    events: list[tuple[str, str]] = []
    keys = Keystrokes()
    keys.add_key_listener(lambda label, kind: events.append((label, kind)))

    keys.type_line("cmd")

    assert events == [("cmd", "text"), ("Enter", "key")]


def test_key_listener_skips_empty_text(fake_pyautogui: _FakePyAutoGui) -> None:
    events: list[tuple[str, str]] = []
    keys = Keystrokes()
    keys.add_key_listener(lambda label, kind: events.append((label, kind)))

    keys.type_text("")

    assert events == []
    # The empty write is still forwarded; only the notification is skipped.
    assert fake_pyautogui.calls == [("write", ("",), {"interval": 0.0})]


def test_key_listener_formats_named_keys_and_hotkeys(
    fake_pyautogui: _FakePyAutoGui,  # noqa: ARG001
) -> None:
    events: list[tuple[str, str]] = []
    keys = Keystrokes()
    keys.add_key_listener(lambda label, kind: events.append((label, kind)))

    keys.press_key("esc")
    keys.press_key("f4")
    keys.send_hotkey("ctrl", "a")
    keys.send_hotkey("alt", "f4")

    assert events == [("Esc", "key"), ("F4", "key"), ("Ctrl+A", "key"), ("Alt+F4", "key")]


def test_key_listener_error_does_not_break_delivery(fake_pyautogui: _FakePyAutoGui) -> None:
    def boom(_label: str, _kind: str) -> None:
        raise RuntimeError("listener down")

    keys = Keystrokes()
    keys.add_key_listener(boom)

    keys.type_text("hello")

    # A failing listener is swallowed; the keystroke is still delivered.
    assert fake_pyautogui.calls == [("write", ("hello",), {"interval": 0.0})]
