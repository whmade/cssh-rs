"""Unit tests for the keyboard layout library.

The Windows calls behind it are replaced by a fake that keeps one layout per
window, so the tests never change a real layout or open a real window.
"""

from __future__ import annotations

import logging

import pytest
import pywinctl

from cssh_rs_automation import keyboard_layout
from cssh_rs_automation.keyboard_layout import KeyboardLayout, KeyboardLayoutError

_DAEMON = "cssh-rs daemon"
_DAEMON_HWND = 0x1234
# Input locale identifiers as LoadKeyboardLayout returns them: the low word is
# the language (0x0409 US English, 0x0809 UK English, 0x0407 German).
_US = 0x04090409
_UK = 0x08090809
_DE = 0x04070407


class _FakeWindow:
    def __init__(self, handle: int) -> None:
        self._handle = handle

    def getHandle(self) -> int:  # noqa: N802 - mirrors pywinctl's method name
        return self._handle


class _FakeConsole:
    def __init__(self) -> None:
        self.killed = False

    def kill(self) -> None:
        self.killed = True

    def wait(self) -> int:
        return 1


class _FakeKeyboard:
    def __init__(self) -> None:
        self.windows = {_DAEMON: _DAEMON_HWND}
        self.layouts = {_DAEMON_HWND: _US}
        self.loadable = {"00000409": _US, "00000809": _UK, "00000407": _DE}
        self.loaded = {_US}
        self.requests: list[tuple[int, int]] = []
        self.unloaded: list[int] = []
        self.consoles: list[_FakeConsole] = []
        self.applies_requests = True
        self.unload_fails = False

    def load(self, klid: str) -> int:
        hkl = self.loadable[klid]
        self.loaded.add(hkl)
        return hkl

    def loaded_layouts(self) -> set[int]:
        return set(self.loaded)

    def unload(self, hkl: int) -> None:
        if self.unload_fails:
            raise KeyboardLayoutError("in use")
        self.unloaded.append(hkl)
        self.loaded.discard(hkl)

    def layout_of(self, hwnd: int) -> int:
        return self.layouts[hwnd]

    def request(self, hwnd: int, hkl: int) -> None:
        self.requests.append((hwnd, hkl))
        if self.applies_requests:
            self.layouts[hwnd] = hkl

    def open_console(self, title: str) -> _FakeConsole:
        hwnd = 0x5000 + len(self.consoles)
        self.windows[title] = hwnd
        # A newly focused window gets the user's current layout.
        self.layouts[hwnd] = self.layouts[_DAEMON_HWND]
        console = _FakeConsole()
        self.consoles.append(console)
        return console

    def focus(self, title: str) -> str:
        return next(name for name in self.windows if title in name)

    def windows_titled(self, title: str, **_: object) -> list[_FakeWindow]:
        return [_FakeWindow(self.windows[title])] if title in self.windows else []

    def console_hwnd(self) -> int:
        return 0x5000 + len(self.consoles) - 1


@pytest.fixture
def keyboard(monkeypatch: pytest.MonkeyPatch) -> _FakeKeyboard:
    fake = _FakeKeyboard()
    monkeypatch.setattr(keyboard_layout, "_load_keyboard_layout", fake.load)
    monkeypatch.setattr(keyboard_layout, "_loaded_keyboard_layouts", fake.loaded_layouts)
    monkeypatch.setattr(keyboard_layout, "_unload_keyboard_layout", fake.unload)
    monkeypatch.setattr(keyboard_layout, "_window_keyboard_layout", fake.layout_of)
    monkeypatch.setattr(keyboard_layout, "_request_keyboard_layout", fake.request)
    monkeypatch.setattr(keyboard_layout, "_open_console", fake.open_console)
    monkeypatch.setattr(keyboard_layout, "_focus_window", fake.focus)
    monkeypatch.setattr(keyboard_layout.time, "sleep", lambda _: None)
    monkeypatch.setattr(pywinctl, "getWindowsWithTitle", fake.windows_titled)
    return fake


def test_switch_asks_the_window_for_the_layout_and_waits_until_it_has_it(
    keyboard: _FakeKeyboard,
) -> None:
    KeyboardLayout().switch_keyboard_layout(_DAEMON, "00000809")

    assert keyboard.requests == [(_DAEMON_HWND, _UK)]
    assert keyboard.layouts[_DAEMON_HWND] == _UK


def test_switch_rejects_a_layout_loaded_for_another_language(keyboard: _FakeKeyboard) -> None:
    # LoadKeyboardLayout returns the system default layout when it has no match.
    keyboard.loadable["00000809"] = _US

    with pytest.raises(KeyboardLayoutError, match=r"00000809 loaded as 0x04090409"):
        KeyboardLayout().switch_keyboard_layout(_DAEMON, "00000809")

    assert keyboard.requests == []


def test_switch_raises_when_the_window_never_takes_the_layout(keyboard: _FakeKeyboard) -> None:
    keyboard.applies_requests = False

    with pytest.raises(KeyboardLayoutError, match="still has layout 0x04090409"):
        KeyboardLayout().switch_keyboard_layout(_DAEMON, "00000809", timeout=0.0)


def test_switch_raises_when_no_single_window_has_the_title(keyboard: _FakeKeyboard) -> None:
    with pytest.raises(KeyboardLayoutError, match="exactly one window titled 'gone'"):
        KeyboardLayout().switch_keyboard_layout("gone", "00000809")

    assert keyboard.requests == []


def test_restore_switches_a_spare_console_back_to_the_original_layout(
    keyboard: _FakeKeyboard,
) -> None:
    layout = KeyboardLayout()
    layout.switch_keyboard_layout(_DAEMON, "00000809")
    layout.switch_keyboard_layout(_DAEMON, "00000809")

    layout.restore_keyboard_layout()

    assert keyboard.requests[-1] == (keyboard.console_hwnd(), _US)
    assert keyboard.layouts[keyboard.console_hwnd()] == _US
    assert [console.killed for console in keyboard.consoles] == [True]


def test_restore_does_not_need_the_switched_window(keyboard: _FakeKeyboard) -> None:
    layout = KeyboardLayout()
    layout.switch_keyboard_layout(_DAEMON, "00000809")
    del keyboard.windows[_DAEMON]

    layout.restore_keyboard_layout()

    assert keyboard.layouts[keyboard.console_hwnd()] == _US


def test_restore_unloads_every_layout_the_switches_loaded(keyboard: _FakeKeyboard) -> None:
    layout = KeyboardLayout()
    layout.switch_keyboard_layout(_DAEMON, "00000809")
    layout.switch_keyboard_layout(_DAEMON, "00000407")

    layout.restore_keyboard_layout()

    assert sorted(keyboard.unloaded) == sorted([_UK, _DE])


def test_restore_keeps_a_layout_that_was_loaded_before_the_switch(
    keyboard: _FakeKeyboard,
) -> None:
    keyboard.loaded.add(_UK)
    layout = KeyboardLayout()
    layout.switch_keyboard_layout(_DAEMON, "00000809")

    layout.restore_keyboard_layout()

    assert keyboard.unloaded == []


def test_restore_logs_a_failed_unload_instead_of_raising(
    keyboard: _FakeKeyboard, caplog: pytest.LogCaptureFixture
) -> None:
    layout = KeyboardLayout()
    layout.switch_keyboard_layout(_DAEMON, "00000809")
    keyboard.unload_fails = True

    with caplog.at_level(logging.WARNING):
        layout.restore_keyboard_layout()

    assert "could not unload layout 0x08090809" in caplog.text


def test_restore_after_a_switch_that_never_took_effect_still_restores(
    keyboard: _FakeKeyboard,
) -> None:
    layout = KeyboardLayout()
    keyboard.applies_requests = False
    with pytest.raises(KeyboardLayoutError):
        layout.switch_keyboard_layout(_DAEMON, "00000809", timeout=0.0)
    # The request may still land after the switch gave up waiting.
    keyboard.layouts[_DAEMON_HWND] = _UK
    keyboard.applies_requests = True

    layout.restore_keyboard_layout()

    assert keyboard.layouts[keyboard.console_hwnd()] == _US


def test_restore_without_a_switch_does_nothing(keyboard: _FakeKeyboard) -> None:
    KeyboardLayout().restore_keyboard_layout()

    assert keyboard.consoles == []
    assert keyboard.requests == []


def test_restore_raises_and_closes_the_console_when_the_layout_never_comes_back(
    keyboard: _FakeKeyboard,
) -> None:
    layout = KeyboardLayout()
    layout.switch_keyboard_layout(_DAEMON, "00000809")
    keyboard.applies_requests = False

    with pytest.raises(KeyboardLayoutError, match="still has layout 0x08090809"):
        layout.restore_keyboard_layout(timeout=0.0)

    assert [console.killed for console in keyboard.consoles] == [True]
    assert keyboard.unloaded == []
