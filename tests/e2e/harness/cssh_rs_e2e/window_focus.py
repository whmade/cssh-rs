"""Focus a terminal window by its title for the cssh-rs Windows E2E suite.

This Robot Framework library activates a window so synthetic input from the
keystroke library lands in the intended target: the daemon window for broadcast
tests, or one client window for control-mode tests. cssh-rs sets deterministic
titles (``cssh-rs daemon`` and ``cssh-rs - <user>@<host>[:port]``), so suites
pass those verbatim.

Foregrounding a window from this background process takes more than a plain
``SetForegroundWindow``; see ``_activate_window`` for the mechanism and the
Microsoft references behind it.

Matching and activation assume cssh-rs consoles are real conhost windows titled
by the console title; the suite forces conhost as the default terminal so this
holds even where Windows Terminal (ConPTY-hosted) would otherwise be used.
"""

from __future__ import annotations

import sys
import time

DEFAULT_TIMEOUT_SECONDS = 5.0
DEFAULT_POLL_INTERVAL_SECONDS = 0.1

_VALID_MATCH_MODES = ("exact", "substring")

_SW_RESTORE = 9
# SystemParametersInfoW action codes; the foreground-lock timeout is in milliseconds.
_SPI_GETFOREGROUNDLOCKTIMEOUT = 0x2000
_SPI_SETFOREGROUNDLOCKTIMEOUT = 0x2001
_SPIF_SENDCHANGE = 0x0002
_ACTIVATION_SETTLE_SECONDS = 2.0
_ACTIVATION_POLL_SECONDS = 0.02


class WindowFocusError(RuntimeError):
    """Raised when a window cannot be located uniquely or cannot be focused."""


class WindowFocus:
    """Robot Framework library that focuses windows by title via pywinctl."""

    ROBOT_LIBRARY_SCOPE = "SUITE"
    ROBOT_LIBRARY_VERSION = "0.1.0"

    def focus_window(
        self,
        title: str,
        match_mode: str = "exact",
        timeout: float = DEFAULT_TIMEOUT_SECONDS,
        poll_interval: float = DEFAULT_POLL_INTERVAL_SECONDS,
    ) -> str:
        """Activate the single window whose title matches; return its title.

        Polls until exactly one window matches. Zero matches after ``timeout``,
        or more than one match at any poll, is an error - so substring ``@h1``
        cannot silently grab ``@h10``.

        Args:
            title: Window title to match.
            match_mode: ``"exact"`` (full title) or ``"substring"`` (contains).
            timeout: Seconds to wait for a unique match before giving up.
            poll_interval: Seconds between match attempts.

        Returns:
            The matched window's title.
        """
        if match_mode not in _VALID_MATCH_MODES:
            raise WindowFocusError(
                f"match_mode must be one of {list(_VALID_MATCH_MODES)}, got {match_mode!r}"
            )
        if timeout < 0:
            raise WindowFocusError(f"timeout must be non-negative, got {timeout}")
        if poll_interval < 0:
            raise WindowFocusError(f"poll_interval must be non-negative, got {poll_interval}")

        # Imported lazily, not at module top: cssh_rs_e2e/__init__ re-exports
        # this module and is itself imported by the SSH-invoked marker writer,
        # which must stay free of pywinctl's display/GUI dependencies.
        import pywinctl

        condition = pywinctl.Re.IS if match_mode == "exact" else pywinctl.Re.CONTAINS

        deadline = time.monotonic() + timeout
        activation_failed = False
        while True:
            matches = pywinctl.getWindowsWithTitle(title, condition=condition)
            if len(matches) > 1:
                matched_titles = [window.title for window in matches]
                raise WindowFocusError(f"multiple windows match {title!r}: {matched_titles}")
            if len(matches) == 1:
                window = matches[0]
                if _activate_window(window):
                    return window.title
                # Activation can lose the foreground race right after a spawn; retry.
                activation_failed = True
            if time.monotonic() >= deadline:
                if activation_failed:
                    raise WindowFocusError(f"failed to focus window {title!r}")
                raise WindowFocusError(f"no window matching {title!r} within {timeout}s")
            time.sleep(poll_interval)

    def get_active_window_title(self) -> str:
        """Return the foreground window's title, or ``""`` when none is active."""
        import pywinctl

        return pywinctl.getActiveWindowTitle() or ""


def _activate_window(window: object) -> bool:
    """Bring ``window`` to the foreground and return whether it settled there.

    A background process may foreground a window only once the foreground-lock
    timeout has expired [1]; it is dropped to zero (and restored afterwards) and
    the calling thread is attached to the foreground thread to share its focus
    state [2]. The foreground is then polled until it lands on the target, since
    focusing the daemon makes cssh-rs raise the clients and refocus the daemon -
    it dips first.

    [1] https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow
    [2] https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-attachthreadinput
    """
    if sys.platform != "win32":
        raise WindowFocusError("window focus is only supported on Windows")

    import ctypes
    from ctypes import wintypes

    # ctypes.windll exists only in the Windows typeshed stub; guarded above.
    user32 = ctypes.windll.user32  # pyrefly: ignore[missing-attribute]
    kernel32 = ctypes.windll.kernel32  # pyrefly: ignore[missing-attribute]
    user32.GetForegroundWindow.restype = wintypes.HWND
    user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user32.GetWindowThreadProcessId.restype = wintypes.DWORD
    user32.AttachThreadInput.argtypes = [wintypes.DWORD, wintypes.DWORD, wintypes.BOOL]
    user32.AttachThreadInput.restype = wintypes.BOOL
    user32.SetForegroundWindow.argtypes = [wintypes.HWND]
    user32.SetForegroundWindow.restype = wintypes.BOOL
    user32.BringWindowToTop.argtypes = [wintypes.HWND]
    user32.BringWindowToTop.restype = wintypes.BOOL
    user32.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
    user32.ShowWindow.restype = wintypes.BOOL
    user32.IsIconic.argtypes = [wintypes.HWND]
    user32.IsIconic.restype = wintypes.BOOL
    user32.SystemParametersInfoW.argtypes = [
        wintypes.UINT,
        wintypes.UINT,
        ctypes.c_void_p,
        wintypes.UINT,
    ]
    user32.SystemParametersInfoW.restype = wintypes.BOOL
    kernel32.GetCurrentThreadId.restype = wintypes.DWORD

    hwnd = int(window.getHandle())  # pyrefly: ignore[missing-attribute]
    if user32.IsIconic(hwnd):
        user32.ShowWindow(hwnd, _SW_RESTORE)

    previous_lock_timeout = wintypes.DWORD()
    user32.SystemParametersInfoW(
        _SPI_GETFOREGROUNDLOCKTIMEOUT, 0, ctypes.byref(previous_lock_timeout), 0
    )
    user32.SystemParametersInfoW(_SPI_SETFOREGROUNDLOCKTIMEOUT, 0, 0, _SPIF_SENDCHANGE)
    try:
        foreground = user32.GetForegroundWindow()
        foreground_thread = user32.GetWindowThreadProcessId(foreground, None) if foreground else 0
        current_thread = kernel32.GetCurrentThreadId()
        attached = False
        if foreground_thread and foreground_thread != current_thread:
            attached = bool(user32.AttachThreadInput(current_thread, foreground_thread, True))
        try:
            user32.BringWindowToTop(hwnd)
            user32.SetForegroundWindow(hwnd)
        finally:
            if attached:
                user32.AttachThreadInput(current_thread, foreground_thread, False)

        deadline = time.monotonic() + _ACTIVATION_SETTLE_SECONDS
        while True:
            if user32.GetForegroundWindow() == hwnd:
                return True
            if time.monotonic() >= deadline:
                return False
            time.sleep(_ACTIVATION_POLL_SECONDS)
    finally:
        user32.SystemParametersInfoW(
            _SPI_SETFOREGROUNDLOCKTIMEOUT, 0, previous_lock_timeout.value, _SPIF_SENDCHANGE
        )
