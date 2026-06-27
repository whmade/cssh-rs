"""Focus a terminal window by its title for the cssh-rs Windows E2E suite.

This Robot Framework library activates a window so that synthetic input
(delivered later by the keystroke library) lands in the intended target:
the cssh-rs daemon window for fan-out tests, or one client window for
control-mode tests. cssh-rs sets deterministic titles - ``cssh-rs daemon``
for the daemon and ``cssh-rs - <user>@<host>[:port]`` for each client - so
suites pass those titles verbatim.

Matching is exact by default; substring matching is available for callers
that only know a stable fragment. An ambiguous match (more than one window)
is always an error rather than a silent first-pick.
"""

from __future__ import annotations

import time
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import pywinctl

DEFAULT_TIMEOUT_SECONDS = 5.0
DEFAULT_POLL_INTERVAL_SECONDS = 0.1

_VALID_MATCH_MODES = ("exact", "substring")


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
    ) -> dict[str, object]:
        """Activate the single window whose title matches and return its details.

        Polls until exactly one window matches, then brings it to the
        foreground. A match count other than one is an error: zero matches
        after the timeout, or more than one match at any poll (so a substring
        like ``@h1`` cannot silently pick the wrong window when ``@h10`` also
        exists).

        Args:
            title: Window title to match.
            match_mode: ``"exact"`` for a full-title match or ``"substring"``
                for a contains match.
            timeout: Seconds to wait for a unique match before giving up.
            poll_interval: Seconds between match attempts.

        Returns:
            A dict with keys ``title`` (str), ``handle`` (int) and ``pid``
            (int) describing the focused window.
        """
        # pywinctl pulls in display/GUI machinery, so import it lazily: the
        # package __init__ and the SSH-invoked marker writer that imports it
        # must stay free of any display dependency.
        import pywinctl

        if match_mode not in _VALID_MATCH_MODES:
            raise WindowFocusError(f"match_mode must be 'exact' or 'substring', got {match_mode!r}")
        condition = pywinctl.Re.IS if match_mode == "exact" else pywinctl.Re.CONTAINS

        deadline = time.monotonic() + timeout
        while True:
            matches = pywinctl.getWindowsWithTitle(title, condition=condition)
            if len(matches) > 1:
                titles = [window.title for window in matches]
                raise WindowFocusError(f"multiple windows match {title!r}: {titles}")
            if len(matches) == 1:
                return _activate(matches[0], title)
            if time.monotonic() >= deadline:
                raise WindowFocusError(f"no window matching {title!r} within {timeout}s")
            time.sleep(poll_interval)

    def get_active_window_title(self) -> str:
        """Return the foreground window's title, or ``""`` when none is active.

        Returns:
            The active window title, or the empty string if no window holds
            focus.
        """
        import pywinctl

        return pywinctl.getActiveWindowTitle() or ""


def _activate(window: pywinctl.Window, title: str) -> dict[str, object]:
    """Bring ``window`` to the foreground and return its title, handle and pid."""
    if not window.activate(wait=True, user=True):
        raise WindowFocusError(f"failed to focus window {title!r}")
    handle = window.getHandle()
    pid = window.getPID()
    return {
        "title": window.title,
        "handle": int(handle) if handle is not None else None,
        "pid": int(pid) if pid is not None else None,
    }
