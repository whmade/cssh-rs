"""Integration test that focuses a real window through the library.

Skipped off Windows, so the hermetic unit suite still runs everywhere. On
``windows-latest`` it spawns a short-lived Tk window with a unique title and
verifies the library activates it and reports it as foreground.
"""

from __future__ import annotations

import contextlib
import subprocess
import sys
import uuid

import pytest

from cssh_rs_e2e.window_focus import WindowFocus

pytestmark = pytest.mark.skipif(sys.platform != "win32", reason="requires a Windows desktop")

# Tk script that opens one top-level window with the title passed as argv[1].
_TK_WINDOW_SCRIPT = (
    "import sys, tkinter; "
    "root = tkinter.Tk(); "
    "root.title(sys.argv[1]); "
    "root.geometry('320x120'); "
    "root.mainloop()"
)

_GRACE_SECONDS = 3.0


def test_focus_window_activates_a_real_window() -> None:
    title = f"cssh-rs-e2e-focus-{uuid.uuid4().hex}"
    helper = subprocess.Popen([sys.executable, "-c", _TK_WINDOW_SCRIPT, title])
    try:
        result = WindowFocus().focus_window(title, timeout=10.0)

        assert result["title"] == title
        assert WindowFocus().get_active_window_title() == title
    finally:
        _terminate(helper)


def _terminate(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=_GRACE_SECONDS)
    except subprocess.TimeoutExpired:
        process.kill()
        with contextlib.suppress(subprocess.TimeoutExpired):
            process.wait(timeout=_GRACE_SECONDS)
