"""Focus a terminal window by its title for the cssh-rs Windows E2E suite.

This Robot Framework library activates a window so synthetic input from the
keystroke library lands in the intended target: the daemon window for fan-out
tests, or one client window for control-mode tests. cssh-rs sets deterministic
titles (``cssh-rs daemon`` and ``cssh-rs - <user>@<host>[:port]``), so suites
pass those verbatim.

Matching is exact by default; ``substring`` is opt-in. More than one match is
always an error, never a silent first-pick.
"""

from __future__ import annotations

import time

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
        # Validate before importing pywinctl so a bad argument fails fast
        # without spinning up the display/GUI machinery the import pulls in.
        if match_mode not in _VALID_MATCH_MODES:
            raise WindowFocusError(
                f"match_mode must be one of {list(_VALID_MATCH_MODES)}, got {match_mode!r}"
            )
        if timeout < 0:
            raise WindowFocusError(f"timeout must be non-negative, got {timeout}")
        if poll_interval < 0:
            raise WindowFocusError(f"poll_interval must be non-negative, got {poll_interval}")

        # Imported lazily for the same reason: both the package __init__ (which
        # re-exports this module) and the SSH-invoked marker writer must stay
        # display-free.
        import pywinctl

        condition = pywinctl.Re.IS if match_mode == "exact" else pywinctl.Re.CONTAINS

        deadline = time.monotonic() + timeout
        while True:
            matches = pywinctl.getWindowsWithTitle(title, condition=condition)
            if len(matches) > 1:
                matched_titles = [window.title for window in matches]
                raise WindowFocusError(f"multiple windows match {title!r}: {matched_titles}")
            if len(matches) == 1:
                window = matches[0]
                if not window.activate(wait=True, user=True):
                    raise WindowFocusError(f"failed to focus window {title!r}")
                return window.title
            if time.monotonic() >= deadline:
                raise WindowFocusError(f"no window matching {title!r} within {timeout}s")
            time.sleep(poll_interval)

    def get_active_window_title(self) -> str:
        """Return the foreground window's title, or ``""`` when none is active."""
        import pywinctl

        return pywinctl.getActiveWindowTitle() or ""
