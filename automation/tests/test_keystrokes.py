"""Unit tests for the keystroke delivery library.

A fake pyautogui module is injected into ``sys.modules`` so the tests never
import the real library, which fails to load on a headless Linux dev box.
A fake user32 likewise records the ``INPUT`` records
``press_keys_with_scan_codes`` would inject.
"""

from __future__ import annotations

import ctypes
import sys
import types
from typing import TYPE_CHECKING

import pytest

from cssh_rs_automation import keystrokes
from cssh_rs_automation.keyboard_layout import KeyboardLayoutError
from cssh_rs_automation.keystrokes import Keystrokes, KeystrokesError

if TYPE_CHECKING:
    from collections.abc import Callable

# SendInput constants, from the INPUT and KEYBDINPUT reference on MS Learn.
_INPUT_KEYBOARD = 1
_KEYEVENTF_EXTENDEDKEY = 0x0001
_KEYEVENTF_KEYUP = 0x0002
_KEYEVENTF_SCANCODE = 0x0008
# sizeof(INPUT) on 64-bit Windows; SendInput rejects any other cbSize.
_INPUT_SIZE_64_BIT = 40


class _FakeSendInput:
    """Record each call's events; call ``i`` inserts ``inserted[i]``, or all past the list."""

    def __init__(self, inserted: list[int]) -> None:
        self.calls: list[list[tuple[int, int, int, int]]] = []
        self.sizes: list[int] = []
        self._inserted = list(inserted)

    @property
    def events(self) -> list[tuple[int, int, int, int]]:
        return [event for call in self.calls for event in call]

    def __call__(self, count: int, inputs: ctypes.Array, size: int) -> int:
        self.calls.append(
            [
                (inputs[i].type, inputs[i].ki.wVk, inputs[i].ki.wScan, inputs[i].ki.dwFlags)
                for i in range(count)
            ]
        )
        self.sizes.append(size)
        return self._inserted.pop(0) if self._inserted else count


def _patch_user32(monkeypatch: pytest.MonkeyPatch, *inserted: int) -> _FakeSendInput:
    send_input = _FakeSendInput(list(inserted))
    monkeypatch.setattr(keystrokes, "_user32", lambda: types.SimpleNamespace(SendInput=send_input))
    return send_input


@pytest.fixture
def fake_send_input(monkeypatch: pytest.MonkeyPatch) -> _FakeSendInput:
    return _patch_user32(monkeypatch)


def _no_foreground_window() -> int:
    raise KeyboardLayoutError("no window is in the foreground")


@pytest.fixture(autouse=True)
def _unreadable_layout(monkeypatch: pytest.MonkeyPatch) -> None:
    """Keep overlay labels independent of this machine's keyboard layout."""
    monkeypatch.setattr(keystrokes, "foreground_keyboard_layout", _no_foreground_window)


def _down(scan: int, extended: bool = False) -> tuple[int, int, int, int]:
    flags = _KEYEVENTF_SCANCODE | (_KEYEVENTF_EXTENDEDKEY if extended else 0)
    return (_INPUT_KEYBOARD, 0, scan, flags)


def _up(scan: int, extended: bool = False) -> tuple[int, int, int, int]:
    kind, vk, _, flags = _down(scan, extended)
    return (kind, vk, scan, flags | _KEYEVENTF_KEYUP)


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


@pytest.mark.parametrize(
    ("deliver", "expected_events", "expected_call"),
    [
        pytest.param(
            lambda keys: keys.type_text("secret", label="PASTE"),
            [("PASTE", "key")],
            ("write", ("secret",), {"interval": 0.0}),
            id="type_text-string-overrides",
        ),
        pytest.param(
            lambda keys: keys.type_text("secret", label=None),
            [],
            ("write", ("secret",), {"interval": 0.0}),
            id="type_text-none-suppresses",
        ),
        pytest.param(
            lambda keys: keys.press_key("f5", label="REFRESH"),
            [("REFRESH", "key")],
            ("press", ("f5",), {}),
            id="press_key-string-overrides",
        ),
        pytest.param(
            lambda keys: keys.press_key("backspace", label=None),
            [],
            ("press", ("backspace",), {}),
            id="press_key-none-suppresses",
        ),
    ],
)
def test_label_overrides_or_suppresses_the_overlay(
    deliver: Callable[[Keystrokes], None],
    expected_events: list[tuple[str, str]],
    expected_call: tuple[str, tuple[object, ...], dict[str, object]],
    fake_pyautogui: _FakePyAutoGui,
) -> None:
    events: list[tuple[str, str]] = []
    keys = Keystrokes()
    keys.add_key_listener(lambda label, kind: events.append((label, kind)))

    deliver(keys)

    assert events == expected_events
    assert fake_pyautogui.calls == [expected_call]


def test_key_listener_reports_line_then_enter(fake_pyautogui: _FakePyAutoGui) -> None:  # noqa: ARG001
    events: list[tuple[str, str]] = []
    keys = Keystrokes()
    keys.add_key_listener(lambda label, kind: events.append((label, kind)))

    keys.type_line("cmd")

    assert events == [("cmd", "text"), ("Enter", "key")]


def test_type_text_rejects_empty_text(fake_pyautogui: _FakePyAutoGui) -> None:
    with pytest.raises(KeystrokesError, match="non-empty"):
        Keystrokes().type_text("")

    assert fake_pyautogui.calls == []


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


@pytest.mark.skipif(ctypes.sizeof(ctypes.c_void_p) != 8, reason="INPUT size is for 64-bit")
def test_scan_code_keys_pass_the_64_bit_input_size(fake_send_input: _FakeSendInput) -> None:
    Keystrokes().press_keys_with_scan_codes("altgr", "4")

    assert fake_send_input.sizes == [_INPUT_SIZE_64_BIT]


def test_scan_code_keys_go_down_in_order_and_up_in_reverse(
    fake_send_input: _FakeSendInput,
) -> None:
    Keystrokes().press_keys_with_scan_codes("shiftleft", "altgr", "4")

    # Scan 1 make codes from MS Learn: left Shift is 0x2A, right Alt is the
    # extended 0xE0 0x38, and 4 is 0x05.
    assert fake_send_input.events == [
        _down(0x2A),
        _down(0x38, True),
        _down(0x05),
        _up(0x05),
        _up(0x38, True),
        _up(0x2A),
    ]


@pytest.mark.parametrize(
    ("key", "scan", "extended"),
    [
        # Scan 1 make codes from the MS Learn scan code table.
        pytest.param("-", 0x0C, False, id="dash"),
        pytest.param("]", 0x1B, False, id="right-brace"),
        pytest.param("nonusbackslash", 0x56, False, id="non-us-backslash"),
        pytest.param("left", 0x4B, True, id="left"),
        pytest.param("f1", 0x3B, False, id="f1"),
        pytest.param("f5", 0x3F, False, id="f5"),
        pytest.param("backspace", 0x0E, False, id="backspace"),
        pytest.param("tab", 0x0F, False, id="tab"),
        pytest.param("enter", 0x1C, False, id="enter"),
    ],
)
def test_scan_code_keys_send_each_key_by_its_scan_code(
    fake_send_input: _FakeSendInput, key: str, scan: int, extended: bool
) -> None:
    Keystrokes().press_keys_with_scan_codes(key)

    assert fake_send_input.events == [_down(scan, extended), _up(scan, extended)]


def test_scan_code_keys_send_ctrl_break_as_a_keyboard_does(
    fake_send_input: _FakeSendInput,
) -> None:
    Keystrokes().press_keys_with_scan_codes("ctrlleft", "break")

    # MS Learn scan code table: left Ctrl is 0x1D, and Ctrl+Pause emits the
    # Break key's extended 0xE0 0x46.
    assert fake_send_input.events == [
        _down(0x1D),
        _down(0x46, True),
        _up(0x46, True),
        _up(0x1D),
    ]


def test_scan_code_keys_repeat_the_last_key_down_like_a_held_key(
    fake_send_input: _FakeSendInput,
) -> None:
    Keystrokes().press_keys_with_scan_codes("altgr", "e", repeat=3)

    assert fake_send_input.events == [
        _down(0x38, True),
        _down(0x12),
        _down(0x12),
        _down(0x12),
        _up(0x12),
        _up(0x38, True),
    ]


def test_scan_code_keys_reject_a_repeat_below_one(fake_send_input: _FakeSendInput) -> None:
    with pytest.raises(KeystrokesError, match="repeat must be at least 1"):
        Keystrokes().press_keys_with_scan_codes("e", repeat=0)

    assert fake_send_input.events == []


def test_scan_code_keys_reject_an_unknown_key(fake_send_input: _FakeSendInput) -> None:
    with pytest.raises(KeystrokesError, match="no scan code for 'f13'"):
        Keystrokes().press_keys_with_scan_codes("altgr", "f13")

    assert fake_send_input.events == []


def test_scan_code_keys_reject_no_keys(fake_send_input: _FakeSendInput) -> None:
    with pytest.raises(KeystrokesError, match="at least one key"):
        Keystrokes().press_keys_with_scan_codes()

    assert fake_send_input.events == []


@pytest.mark.skipif(sys.platform != "win32", reason="GetLastError capture is Windows-only")
def test_scan_code_keys_raise_when_input_is_blocked(monkeypatch: pytest.MonkeyPatch) -> None:
    send_input = _patch_user32(monkeypatch, 0)

    with pytest.raises(KeystrokesError, match="inserted 0 of 4") as raised:
        Keystrokes().press_keys_with_scan_codes("altgr", "a")

    # Nothing went down, so there is nothing to release.
    assert len(send_input.calls) == 1
    assert not getattr(raised.value, "ROBOT_EXIT_ON_FAILURE", False)


@pytest.mark.skipif(sys.platform != "win32", reason="GetLastError capture is Windows-only")
@pytest.mark.parametrize(
    ("inserted", "released"),
    [
        pytest.param(1, [_up(0x38, True)], id="right-alt-down"),
        pytest.param(2, [_up(0x1E), _up(0x38, True)], id="both-down"),
        pytest.param(3, [_up(0x38, True)], id="a-released"),
    ],
)
def test_scan_code_keys_release_what_a_partial_insert_left_down(
    monkeypatch: pytest.MonkeyPatch,
    inserted: int,
    released: list[tuple[int, int, int, int]],
) -> None:
    send_input = _patch_user32(monkeypatch, inserted)

    with pytest.raises(KeystrokesError, match=f"inserted {inserted} of 4") as raised:
        Keystrokes().press_keys_with_scan_codes("altgr", "a")

    assert send_input.calls[1] == released
    assert not getattr(raised.value, "ROBOT_EXIT_ON_FAILURE", False)


@pytest.mark.skipif(sys.platform != "win32", reason="GetLastError capture is Windows-only")
def test_scan_code_keys_stop_the_run_when_stuck_keys_cannot_be_released(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _patch_user32(monkeypatch, 1, 0)

    with pytest.raises(KeystrokesError, match="still held down: altgr") as raised:
        Keystrokes().press_keys_with_scan_codes("altgr", "a")

    assert getattr(raised.value, "ROBOT_EXIT_ON_FAILURE", False) is True


def test_key_listener_reports_scan_code_keys(fake_send_input: _FakeSendInput) -> None:  # noqa: ARG001
    events: list[tuple[str, str]] = []
    keys = Keystrokes()
    keys.add_key_listener(lambda label, kind: events.append((label, kind)))

    keys.press_keys_with_scan_codes("altleft", "altgr", "e")
    keys.press_keys_with_scan_codes("ctrlleft", "left")
    keys.press_keys_with_scan_codes("shiftleft", "f5")
    keys.press_keys_with_scan_codes("ctrlleft", "break")

    assert events == [
        ("Alt+AltGr+E", "key"),
        ("Ctrl+Left", "key"),
        ("Shift+F5", "key"),
        ("Ctrl+Break", "key"),
    ]


def test_key_listener_names_keys_the_layout_types_as_control_characters(
    fake_send_input: _FakeSendInput,  # noqa: ARG001
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    # ToUnicodeEx types BS for Backspace, HT for Tab and CR for Enter, as on the UK layout.
    control_characters = {0x0E: "\b", 0x0F: "\t", 0x1C: "\r"}
    monkeypatch.setattr(keystrokes, "_layout_character", control_characters.get)
    events: list[tuple[str, str]] = []
    keys = Keystrokes()
    keys.add_key_listener(lambda label, kind: events.append((label, kind)))

    keys.press_keys_with_scan_codes("ctrlleft", "backspace")
    keys.press_keys_with_scan_codes("shiftleft", "tab")
    keys.press_keys_with_scan_codes("ctrlleft", "shiftleft", "enter")

    assert events == [
        ("Ctrl+Backspace", "key"),
        ("Shift+Tab", "key"),
        ("Ctrl+Shift+Enter", "key"),
    ]


def test_key_listener_shows_scan_code_keys_as_the_layout_types_them(
    fake_send_input: _FakeSendInput,  # noqa: ARG001
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    german = {0x0C: "ß", 0x56: "<", 0x10: "q"}
    monkeypatch.setattr(keystrokes, "_layout_character", german.get)
    events: list[tuple[str, str]] = []
    keys = Keystrokes()
    keys.add_key_listener(lambda label, kind: events.append((label, kind)))

    keys.press_keys_with_scan_codes("altgr", "-")
    keys.press_keys_with_scan_codes("altgr", "nonusbackslash")
    keys.press_keys_with_scan_codes("ctrlleft", "altleft", "q")

    assert events == [("AltGr+ß", "key"), ("AltGr+<", "key"), ("Ctrl+Alt+Q", "key")]


class _FakeLayoutUser32:
    """Stands in for user32's ``MapVirtualKeyExW`` and ``ToUnicodeEx`` on one layout."""

    def __init__(self, typed: str, count: int) -> None:
        self.calls: list[tuple[object, ...]] = []

        def map_virtual_key(scan: int, map_type: int, layout: int) -> int:
            self.calls.append(("MapVirtualKeyExW", scan, map_type, layout))
            return 0xDB

        def to_unicode(
            virtual_key: int,
            scan: int,
            _state: object,
            buffer: ctypes.Array[ctypes.c_wchar],
            _size: int,
            flags: int,
            layout: int,
        ) -> int:
            self.calls.append(("ToUnicodeEx", virtual_key, scan, flags, layout))
            buffer.value = typed
            return count

        self.MapVirtualKeyExW = map_virtual_key
        self.ToUnicodeEx = to_unicode


def test_layout_character_reads_what_the_foreground_layout_types(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    fake = _FakeLayoutUser32("ß", 1)
    monkeypatch.setattr(keystrokes, "_user32", lambda: fake)
    monkeypatch.setattr(keystrokes, "foreground_keyboard_layout", lambda: 0x04070407)

    assert keystrokes._layout_character(0x0C) == "ß"
    # MAPVK_VSC_TO_VK_EX is 3; ToUnicodeEx flag 0x4 leaves the keyboard state alone.
    assert fake.calls == [
        ("MapVirtualKeyExW", 0x0C, 3, 0x04070407),
        ("ToUnicodeEx", 0xDB, 0x0C, 0x4, 0x04070407),
    ]


def test_layout_character_is_none_for_a_dead_key(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(keystrokes, "_user32", lambda: _FakeLayoutUser32(chr(0xB4), -1))
    monkeypatch.setattr(keystrokes, "foreground_keyboard_layout", lambda: 0x04070407)

    assert keystrokes._layout_character(0x0D) is None


def test_layout_character_is_none_without_a_foreground_layout() -> None:
    assert keystrokes._layout_character(0x0C) is None
