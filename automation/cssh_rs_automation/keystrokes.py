"""Deliver synthetic keystrokes for the cssh-rs Windows E2E suite.

This Robot Framework library types into whichever window is foreground; it
never selects a target itself. Suites focus a window first (via the window
focus library), then call these keywords. The keywords hold no sleeps;
readiness is polled at the Robot Framework layer with
``Wait Until Keyword Succeeds``.
"""

from __future__ import annotations

import logging
import sys
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Callable

LOGGER = logging.getLogger(__name__)

# INPUT type and KEYBDINPUT flags, from
# https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-keybdinput
_INPUT_KEYBOARD = 1
_KEYEVENTF_EXTENDEDKEY = 0x0001
_KEYEVENTF_KEYUP = 0x0002
_KEYEVENTF_SCANCODE = 0x0008


class _Default:
    """Type of the ``DEFAULT_LABEL`` sentinel."""


# Default ``label``: show the overlay for whatever was typed or pressed.
DEFAULT_LABEL = _Default()


class KeystrokesError(RuntimeError):
    """Raised when a keystroke keyword is called with invalid arguments."""


class KeysStuckError(KeystrokesError):
    """Raised when keys a keystroke keyword pressed cannot be released."""

    # Robot Framework stops the whole run on an exception with this attribute:
    # https://robotframework.org/robotframework/latest/RobotFrameworkUserGuide.html#stopping-test-execution
    ROBOT_EXIT_ON_FAILURE = True


class Keystrokes:
    """Robot Framework library that delivers keystrokes via pyautogui or ``SendInput``."""

    ROBOT_LIBRARY_SCOPE = "SUITE"
    ROBOT_LIBRARY_VERSION = "0.1.0"

    def __init__(self) -> None:
        self._listeners: list[Callable[[str, str], None]] = []

    def add_key_listener(self, listener: Callable[[str, str], None]) -> None:
        """Register a callback invoked with each delivered action's label and kind.

        Labels are human-readable (``"echo hello"``, ``"Enter"``, ``"Ctrl+A"``),
        intended for an on-screen keystroke overlay. The kind is ``"text"`` for
        literal typed characters or ``"key"`` for named keys and chords.

        Args:
            listener: Callable invoked with the label and kind of each delivered
                action.
        """
        self._listeners.append(listener)

    def _notify(self, label: str, kind: str) -> None:
        """Send ``(label, kind)`` to every listener; a listener error is logged, not raised."""
        for listener in self._listeners:
            try:
                listener(label, kind)
            except Exception as exc:
                # Broad by design: a listener must never break input delivery.
                LOGGER.warning("key listener failed for %r: %s", label, exc)

    def _notify_label(
        self, label: str | _Default | None, *, default: str, default_kind: str
    ) -> None:
        """Route ``label`` to the overlay: ``DEFAULT_LABEL`` shows ``default`` as
        ``default_kind``, ``None`` suppresses it, any string shows that token as a
        discrete key.
        """
        if label is None:
            return
        if isinstance(label, str):
            self._notify(label, "key")
            return
        self._notify(default, default_kind)

    def type_text(
        self,
        text: str,
        interval: float = 0.0,
        label: str | _Default | None = DEFAULT_LABEL,
    ) -> None:
        """Type ``text`` as literal printable characters into the foreground window.

        Use ``press_key`` for named keys such as Enter or Tab.

        Args:
            text: Characters to type.
            interval: Seconds between characters; raise it if a terminal drops
                fast input.
            label: Overlay control. ``DEFAULT_LABEL`` (the default) shows ``text``
                as typed characters; ``None`` suppresses the overlay; any string
                shows that token as a discrete action instead (e.g. ``"PASTE"``).
        """
        if not text:
            raise KeystrokesError("text must be a non-empty string")
        if interval < 0:
            raise KeystrokesError(f"interval must be non-negative, got {interval}")
        self._notify_label(label, default=text, default_kind="text")
        self._pyautogui().write(text, interval=interval)

    def type_line(self, text: str, interval: float = 0.0) -> None:
        """Type ``text`` then press Enter.

        Args:
            text: Characters to type before Enter.
            interval: Seconds to wait between characters of ``text``.
        """
        self.type_text(text, interval=interval)
        self.press_key("enter")

    def press_key(self, key: str, label: str | _Default | None = DEFAULT_LABEL) -> None:
        """Press a named key such as ``enter``, ``tab`` or ``esc``.

        Args:
            key: pyautogui key name to press.
            label: Overlay control. ``DEFAULT_LABEL`` (the default) shows the
                key's own label; ``None`` suppresses the overlay; any string
                shows that token instead.
        """
        if not key:
            raise KeystrokesError("key must be a non-empty string")
        self._notify_label(label, default=_key_label(key), default_kind="key")
        self._pyautogui().press(key)

    def send_hotkey(self, *keys: str) -> None:
        """Press ``keys`` together as a chord, e.g. ``ctrl`` ``c``.

        Args:
            keys: pyautogui key names to hold down in order and release.
        """
        if not keys:
            raise KeystrokesError("send_hotkey requires at least one key")
        if not all(keys):
            raise KeystrokesError("hotkey keys must be non-empty strings")
        self._notify("+".join(_key_label(key) for key in keys), "key")
        self._pyautogui().hotkey(*keys)

    def press_keys_with_scan_codes(self, *keys: str, repeat: int = 1) -> None:
        """Press ``keys`` together as a chord by their scan codes, through ``SendInput``.

        Unlike ``send_hotkey``, whose virtual keys type what US prints, these
        keys reach the foreground window as a keyboard reports them, so its
        keyboard layout decides what they type.

        Args:
            keys: Keys to hold down in order and release in reverse: ``altgr``
                (right Alt), ``altleft``, ``ctrlleft``, ``ctrlright``,
                ``shiftleft``, a letter, a digit, ``-`` or ``]`` (named by their
                US characters), or ``nonusbackslash``, the extra key left of Z
                on 102-key keyboards.
            repeat: Key-down events the last key sends before its release, as
                when a held key auto-repeats.
        """
        if not keys:
            raise KeystrokesError("press_keys_with_scan_codes requires at least one key")
        if repeat < 1:
            raise KeystrokesError(f"repeat must be at least 1, got {repeat}")
        unknown = [key for key in keys if key not in _SCAN_CODES]
        if unknown:
            raise KeystrokesError(f"no scan code for {unknown[0]!r}")
        self._notify("+".join(_key_label(key) for key in keys), "key")
        presses = [(key, False) for key in keys]
        presses += presses[-1:] * (repeat - 1)
        releases = [(key, True) for key in reversed(keys)]
        _send_input(presses + releases)

    @staticmethod
    def _pyautogui() -> Any:
        """Import pyautogui lazily, disable its failsafe, and return the module.

        Imported here, not at module top: cssh_rs_automation/__init__ re-exports this
        module, which the SSH-invoked marker writer imports headless and must
        stay free of pyautogui's GUI dependencies. The failsafe aborts input
        on a mouse move into a screen corner, which would kill these
        keyboard-only keywords in CI.
        """
        import pyautogui

        pyautogui.FAILSAFE = False
        return pyautogui


_KEY_LABELS = {
    "enter": "Enter",
    "esc": "Esc",
    "escape": "Esc",
    "tab": "Tab",
    "space": "Space",
    "backspace": "Backspace",
    "delete": "Del",
    "up": "Up",
    "down": "Down",
    "left": "Left",
    "right": "Right",
    "ctrl": "Ctrl",
    "ctrlleft": "Ctrl",
    "ctrlright": "Ctrl",
    "alt": "Alt",
    "altleft": "Alt",
    "altright": "Alt",
    "altgr": "AltGr",
    "shift": "Shift",
    "shiftleft": "Shift",
    "shiftright": "Shift",
    "win": "Win",
    "winleft": "Win",
    "winright": "Win",
}


# Scan 1 make codes and whether the key is extended (``0xE0`` before the code), from
# https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input#scan-codes
_SCAN_CODES: dict[str, tuple[int, bool]] = {
    "altleft": (0x38, False),
    "altgr": (0x38, True),
    "ctrlleft": (0x1D, False),
    "ctrlright": (0x1D, True),
    "shiftleft": (0x2A, False),
    "-": (0x0C, False),
    "]": (0x1B, False),
    "nonusbackslash": (0x56, False),
    **{key: (code, False) for code, key in enumerate("1234567890", 0x02)},
    **{key: (code, False) for code, key in enumerate("qwertyuiop", 0x10)},
    **{key: (code, False) for code, key in enumerate("asdfghjkl", 0x1E)},
    **{key: (code, False) for code, key in enumerate("zxcvbnm", 0x2C)},
}


def _send_input(events: list[tuple[str, bool]]) -> None:
    """Inject ``(key, key_up)`` events in one ``SendInput`` call.

    After a partial insert, release the keys left down so a held modifier cannot
    leak into later input, then raise ``KeystrokesError``, or ``KeysStuckError``
    if they stay down.
    """
    import ctypes

    class KeybdInput(ctypes.Structure):
        _fields_ = (
            ("wVk", ctypes.c_uint16),
            ("wScan", ctypes.c_uint16),
            ("dwFlags", ctypes.c_uint32),
            ("time", ctypes.c_uint32),
            ("dwExtraInfo", ctypes.c_size_t),
        )

    class MouseInput(ctypes.Structure):
        _fields_ = (
            ("dx", ctypes.c_int32),
            ("dy", ctypes.c_int32),
            ("mouseData", ctypes.c_uint32),
            ("dwFlags", ctypes.c_uint32),
            ("time", ctypes.c_uint32),
            ("dwExtraInfo", ctypes.c_size_t),
        )

    class InputUnion(ctypes.Union):
        # MOUSEINPUT, the union's largest member, sets sizeof(INPUT), and SendInput
        # fails on any other cbSize:
        # https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
        _fields_ = (("ki", KeybdInput), ("mi", MouseInput))

    class Input(ctypes.Structure):
        _anonymous_ = ("u",)
        _fields_ = (("type", ctypes.c_uint32), ("u", InputUnion))

    user32 = _user32()
    user32.SendInput.argtypes = [ctypes.c_uint, ctypes.POINTER(Input), ctypes.c_int]
    user32.SendInput.restype = ctypes.c_uint

    def inject(batch: list[tuple[str, bool]]) -> int:
        records = (Input * len(batch))()
        for record, (key, key_up) in zip(records, batch, strict=True):
            scan, extended = _SCAN_CODES[key]
            record.type = _INPUT_KEYBOARD
            record.ki.wScan = scan
            record.ki.dwFlags = (
                _KEYEVENTF_SCANCODE
                | (_KEYEVENTF_EXTENDEDKEY if extended else 0)
                | (_KEYEVENTF_KEYUP if key_up else 0)
            )
        return user32.SendInput(len(batch), records, ctypes.sizeof(Input))

    inserted = inject(events)
    if inserted == len(events):
        return
    # Neither the count nor GetLastError tells a UIPI block from other failures:
    # https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
    error = ctypes.get_last_error()
    held = _held_keys(events[:inserted])
    releases = [(key, True) for key in reversed(held)]
    if releases and inject(releases) != len(releases):
        raise KeysStuckError(
            f"SendInput inserted {inserted} of {len(events)} events (error {error}) and"
            f" could not release the keys still held down: {', '.join(held)}"
        )
    raise KeystrokesError(f"SendInput inserted {inserted} of {len(events)} events: error {error}")


def _held_keys(events: list[tuple[str, bool]]) -> list[str]:
    """Return the keys ``events`` leave held down, in the order they went down."""
    held: dict[str, None] = {}
    for key, key_up in events:
        if key_up:
            held.pop(key, None)
        else:
            held[key] = None
    return list(held)


def _user32() -> Any:
    """Return user32 loaded with ``GetLastError`` capture; Windows only."""
    if sys.platform != "win32":
        raise KeystrokesError("SendInput is only available on Windows")

    import ctypes

    # ctypes.WinDLL exists only in the Windows typeshed stub; guarded above.
    return ctypes.WinDLL("user32", use_last_error=True)  # pyrefly: ignore[missing-attribute]


def _key_label(key: str) -> str:
    """Return a human-readable overlay label for a pyautogui key name.

    Args:
        key: pyautogui key name, e.g. ``enter``, ``ctrl`` or ``f4``.

    Returns:
        A display label such as ``Enter``, ``F4`` or ``A``. Single characters
        render uppercase; typed text is emitted verbatim and never routed here.
    """
    lowered = key.lower()
    if lowered in _KEY_LABELS:
        return _KEY_LABELS[lowered]
    if lowered.startswith("f") and lowered[1:].isdigit():
        return lowered.upper()
    if len(key) == 1:
        return key.upper()
    return key.capitalize()
