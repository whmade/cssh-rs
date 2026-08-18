"""Focus a terminal window by its title for the cssh-rs Windows E2E suite.

This Robot Framework library activates a window so synthetic input from the
keystroke library lands in the intended target: the daemon window for broadcast
tests, or one client window for gating tests. cssh-rs sets deterministic
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
# GetWindow relationship: the next window below in the z-order.
_GW_HWNDNEXT = 2
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
        _validate_match_args(match_mode, timeout, poll_interval)

        # Imported lazily, not at module top: cssh_rs_automation/__init__ re-exports
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

    def close_window(
        self,
        title: str,
        match_mode: str = "exact",
        timeout: float = DEFAULT_TIMEOUT_SECONDS,
        poll_interval: float = DEFAULT_POLL_INTERVAL_SECONDS,
    ) -> str:
        """Close the single window whose title matches; return its title.

        Polls for exactly one match, closes it, then polls until no window
        matches so callers can treat the close as complete. More than one match
        at any poll - so substring ``@h1`` cannot grab ``@h10`` - or a window
        still present after ``timeout`` is an error.

        Args:
            title: Window title to match.
            match_mode: ``"exact"`` (full title) or ``"substring"`` (contains).
            timeout: Seconds to wait for the unique match and for it to vanish.
            poll_interval: Seconds between match attempts.

        Returns:
            The closed window's title.
        """
        _validate_match_args(match_mode, timeout, poll_interval)

        # Imported lazily for the same reason as focus_window; see its comment.
        import pywinctl

        condition = pywinctl.Re.IS if match_mode == "exact" else pywinctl.Re.CONTAINS

        deadline = time.monotonic() + timeout
        closed_title: str | None = None
        while True:
            matches = pywinctl.getWindowsWithTitle(title, condition=condition)
            if len(matches) > 1:
                matched_titles = [window.title for window in matches]
                raise WindowFocusError(f"multiple windows match {title!r}: {matched_titles}")
            if closed_title is None:
                if len(matches) == 1:
                    closed_title = matches[0].title
                    matches[0].close()
            elif not matches:
                return closed_title
            if time.monotonic() >= deadline:
                if closed_title is None:
                    raise WindowFocusError(f"no window matching {title!r} within {timeout}s")
                raise WindowFocusError(f"window {title!r} still present {timeout}s after close")
            time.sleep(poll_interval)

    def count_windows(self, title: str, match_mode: str = "substring") -> int:
        """Return how many top-level windows match ``title``.

        Unlike ``focus_window``, multiple matches are expected: callers poll this
        until all N client windows of a cluster have come up.

        Args:
            title: Window title to match.
            match_mode: ``"exact"`` (full title) or ``"substring"`` (contains).

        Returns:
            The number of matching top-level windows.
        """
        if match_mode not in _VALID_MATCH_MODES:
            raise WindowFocusError(
                f"match_mode must be one of {list(_VALID_MATCH_MODES)}, got {match_mode!r}"
            )

        # Imported lazily for the same reason as focus_window; see its comment.
        import pywinctl

        condition = pywinctl.Re.IS if match_mode == "exact" else pywinctl.Re.CONTAINS
        return len(pywinctl.getWindowsWithTitle(title, condition=condition))

    def window_boxes(
        self, title: str, match_mode: str = "substring"
    ) -> list[tuple[str, int, int, int, int]]:
        """Return the on-screen box of every top-level window matching ``title``.

        Like ``count_windows``, multiple matches are expected: one entry per
        client window of a cluster.

        Args:
            title: Window title to match.
            match_mode: ``"exact"`` (full title) or ``"substring"`` (contains).

        Returns:
            One ``(title, left, top, width, height)`` tuple per matching window.
        """
        if match_mode not in _VALID_MATCH_MODES:
            raise WindowFocusError(
                f"match_mode must be one of {list(_VALID_MATCH_MODES)}, got {match_mode!r}"
            )

        # Imported lazily for the same reason as focus_window; see its comment.
        import pywinctl

        condition = pywinctl.Re.IS if match_mode == "exact" else pywinctl.Re.CONTAINS
        matches = pywinctl.getWindowsWithTitle(title, condition=condition)
        return [
            (window.title, window.box.left, window.box.top, window.box.width, window.box.height)
            for window in matches
        ]

    def get_active_window_title(self) -> str:
        """Return the foreground window's title, or ``""`` when none is active."""
        import pywinctl

        return pywinctl.getActiveWindowTitle() or ""

    def window_z_order_index(self, title: str, match_mode: str = "exact") -> int:
        """Return how many top-level windows sit above the one matching ``title``.

        0 is topmost, so comparing two windows' indices reveals which is stacked
        above the other - unlike a minimized-state check, this catches a window
        left hidden beneath another rather than raised on top.

        Args:
            title: Window title to match.
            match_mode: ``"exact"`` (full title) or ``"substring"`` (contains).

        Returns:
            The window's index in the top-level z-order, 0 for the topmost.
        """
        if match_mode not in _VALID_MATCH_MODES:
            raise WindowFocusError(
                f"match_mode must be one of {list(_VALID_MATCH_MODES)}, got {match_mode!r}"
            )

        import pywinctl

        condition = pywinctl.Re.IS if match_mode == "exact" else pywinctl.Re.CONTAINS
        matches = pywinctl.getWindowsWithTitle(title, condition=condition)
        if len(matches) != 1:
            titles = [window.title for window in matches]
            raise WindowFocusError(f"expected exactly one window matching {title!r}, got {titles}")
        return _z_order_index(int(matches[0].getHandle()))

    def right_click_window(self, title: str, match_mode: str = "exact") -> str:
        """Right-click the center of the single window matching ``title``; return its title.

        conhost pastes the clipboard on a right-click in QuickEdit mode but has no
        Ctrl+V paste, so a paste test drives the daemon this way. The caller must
        focus the window first so it is on top and the click lands on it.

        Args:
            title: Window title to match.
            match_mode: ``"exact"`` (full title) or ``"substring"`` (contains).

        Returns:
            The clicked window's title.
        """
        if match_mode not in _VALID_MATCH_MODES:
            raise WindowFocusError(
                f"match_mode must be one of {list(_VALID_MATCH_MODES)}, got {match_mode!r}"
            )

        import pywinctl

        condition = pywinctl.Re.IS if match_mode == "exact" else pywinctl.Re.CONTAINS
        matches = pywinctl.getWindowsWithTitle(title, condition=condition)
        if len(matches) != 1:
            titles = [window.title for window in matches]
            raise WindowFocusError(f"expected exactly one window matching {title!r}, got {titles}")
        box = matches[0].box
        center_x = box.left + box.width // 2
        center_y = box.top + box.height // 2

        # pyautogui imported lazily and its corner failsafe disabled for the same
        # reasons as the keystroke library; a center click never nears a corner.
        import pyautogui

        pyautogui.FAILSAFE = False
        pyautogui.rightClick(center_x, center_y)
        return matches[0].title


def _z_order_index(hwnd: int) -> int:
    """Return how many top-level windows sit above ``hwnd`` (0 = topmost).

    Walks the top-level z-order from the front with ``GetWindow``/``GW_HWNDNEXT``.
    """
    if sys.platform != "win32":
        raise WindowFocusError("window focus is only supported on Windows")

    import ctypes
    from ctypes import wintypes

    # ctypes.windll exists only in the Windows typeshed stub; guarded above.
    user32 = ctypes.windll.user32  # pyrefly: ignore[missing-attribute]
    user32.GetTopWindow.argtypes = [wintypes.HWND]
    user32.GetTopWindow.restype = wintypes.HWND
    user32.GetWindow.argtypes = [wintypes.HWND, wintypes.UINT]
    user32.GetWindow.restype = wintypes.HWND

    index = 0
    current = user32.GetTopWindow(None)
    while current:
        if current == hwnd:
            return index
        index += 1
        current = user32.GetWindow(current, _GW_HWNDNEXT)
    raise WindowFocusError(f"window {hwnd} is not in the top-level z-order")


def _validate_match_args(match_mode: str, timeout: float, poll_interval: float) -> None:
    """Reject an invalid match mode or a negative timeout or poll interval."""
    if match_mode not in _VALID_MATCH_MODES:
        raise WindowFocusError(
            f"match_mode must be one of {list(_VALID_MATCH_MODES)}, got {match_mode!r}"
        )
    if timeout < 0:
        raise WindowFocusError(f"timeout must be non-negative, got {timeout}")
    if poll_interval < 0:
        raise WindowFocusError(f"poll_interval must be non-negative, got {poll_interval}")


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
