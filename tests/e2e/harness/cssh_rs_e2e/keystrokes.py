"""Deliver synthetic keystrokes for the cssh-rs Windows E2E suite.

This Robot Framework library types into whichever window is foreground; it
never selects a target itself. Suites focus a window first (via the window
focus library), then call these keywords. The keywords hold no sleeps;
readiness is polled at the Robot Framework layer with
``Wait Until Keyword Succeeds``.
"""

from __future__ import annotations

from typing import Any


class KeystrokesError(RuntimeError):
    """Raised when a keystroke keyword is called with invalid arguments."""


class Keystrokes:
    """Robot Framework library that delivers keystrokes via pyautogui."""

    ROBOT_LIBRARY_SCOPE = "SUITE"
    ROBOT_LIBRARY_VERSION = "0.1.0"

    def type_text(self, text: str, interval: float = 0.0) -> None:
        """Type ``text`` as literal printable characters into the foreground window.

        Use ``press_key`` for named keys such as Enter or Tab.

        Args:
            text: Characters to type.
            interval: Seconds between characters; raise it if a terminal drops
                fast input.
        """
        if interval < 0:
            raise KeystrokesError(f"interval must be non-negative, got {interval}")
        self._pyautogui().write(text, interval=interval)

    def type_line(self, text: str, interval: float = 0.0) -> None:
        """Type ``text`` then press Enter.

        Args:
            text: Characters to type before Enter.
            interval: Seconds to wait between characters of ``text``.
        """
        self.type_text(text, interval=interval)
        self.press_key("enter")

    def press_key(self, key: str) -> None:
        """Press a named key such as ``enter``, ``tab`` or ``esc``.

        Args:
            key: pyautogui key name to press.
        """
        if not key:
            raise KeystrokesError("key must be a non-empty string")
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
        self._pyautogui().hotkey(*keys)

    @staticmethod
    def _pyautogui() -> Any:
        """Import pyautogui lazily, disable its failsafe, and return the module.

        Imported here, not at module top: cssh_rs_e2e/__init__ re-exports this
        module, which the SSH-invoked marker writer imports headless and must
        stay free of pyautogui's GUI dependencies. The failsafe aborts input
        on a mouse move into a screen corner, which would kill these
        keyboard-only keywords in CI.
        """
        import pyautogui

        pyautogui.FAILSAFE = False
        return pyautogui
