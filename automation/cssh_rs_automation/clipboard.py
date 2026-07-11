"""Read and write the system clipboard for the cssh-rs Windows E2E suite.

pyperclip is imported lazily, for the same reason as keystrokes.Keystrokes:
cssh_rs_automation/__init__ re-exports this module and the SSH-invoked marker writer
must import the package headless.
"""

from __future__ import annotations

from typing import Any


class ClipboardError(RuntimeError):
    """Raised when a clipboard keyword is called with invalid arguments."""


class Clipboard:
    """Robot Framework library that reads and writes the system clipboard."""

    ROBOT_LIBRARY_SCOPE = "SUITE"
    ROBOT_LIBRARY_VERSION = "0.1.0"

    def set_clipboard(self, text: str) -> None:
        """Replace the system clipboard contents with ``text``.

        Args:
            text: Text to place on the clipboard.
        """
        if not isinstance(text, str):
            raise ClipboardError(f"text must be a string, got {type(text).__name__}")
        self._pyperclip().copy(text)

    def get_clipboard(self) -> str:
        """Return the current system clipboard contents.

        Returns:
            The clipboard's text, or the empty string when it holds no text.
        """
        return self._pyperclip().paste()

    @staticmethod
    def _pyperclip() -> Any:
        """Import pyperclip lazily and return the module."""
        import pyperclip

        return pyperclip
