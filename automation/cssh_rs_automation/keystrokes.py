"""Deliver synthetic keystrokes for the cssh-rs Windows E2E suite.

This Robot Framework library types into whichever window is foreground; it
never selects a target itself. Suites focus a window first (via the window
focus library), then call these keywords. The keywords hold no sleeps;
readiness is polled at the Robot Framework layer with
``Wait Until Keyword Succeeds``.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Callable

LOGGER = logging.getLogger(__name__)


class KeystrokesError(RuntimeError):
    """Raised when a keystroke keyword is called with invalid arguments."""


class Keystrokes:
    """Robot Framework library that delivers keystrokes via pyautogui."""

    ROBOT_LIBRARY_SCOPE = "SUITE"
    ROBOT_LIBRARY_VERSION = "0.1.0"

    def __init__(self) -> None:
        self._listeners: list[Callable[[str, str], None]] = []

    def add_key_listener(self, listener: Callable[[str, str], None]) -> None:
        """Register a callback invoked with each delivered action's label and kind.

        Labels are human-readable (``"echo hello"``, ``"Enter"``, ``"Ctrl+A"``),
        intended for an on-screen keypress overlay. The kind is ``"text"`` for
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

    def type_text(self, text: str, interval: float = 0.0, label: str | None = None) -> None:
        """Type ``text`` as literal printable characters into the foreground window.

        Use ``press_key`` for named keys such as Enter or Tab.

        Args:
            text: Characters to type.
            interval: Seconds between characters; raise it if a terminal drops
                fast input.
            label: Overlay token to show instead of ``text``, as a discrete
                action rather than typed characters (e.g. ``"PASTE"``).
        """
        if interval < 0:
            raise KeystrokesError(f"interval must be non-negative, got {interval}")
        if label is not None:
            self._notify(label, "key")
        elif text:
            self._notify(text, "text")
        self._pyautogui().write(text, interval=interval)

    def type_line(self, text: str, interval: float = 0.0) -> None:
        """Type ``text`` then press Enter.

        Args:
            text: Characters to type before Enter.
            interval: Seconds to wait between characters of ``text``.
        """
        self.type_text(text, interval=interval)
        self.press_key("enter")

    def press_key(self, key: str, notify: bool = True) -> None:
        """Press a named key such as ``enter``, ``tab`` or ``esc``.

        Args:
            key: pyautogui key name to press.
            notify: When ``False``, skip the overlay listener - for a key that
                should not appear on screen, such as an input-absorbing no-op.
        """
        if not key:
            raise KeystrokesError("key must be a non-empty string")
        if notify:
            self._notify(_key_label(key), "key")
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
    "shift": "Shift",
    "shiftleft": "Shift",
    "shiftright": "Shift",
    "win": "Win",
    "winleft": "Win",
    "winright": "Win",
}


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
