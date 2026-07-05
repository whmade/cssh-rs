"""Integration smoke test that focuses a real window through the library.

Skipped off Windows. On ``windows-latest`` it spawns a uniquely-titled Tk
window and asserts the library activates it and reports it as foreground.
"""

from __future__ import annotations

import subprocess
import sys
import uuid

import pytest

from cssh_rs_e2e.window_focus import WindowFocus

pytestmark = pytest.mark.skipif(sys.platform != "win32", reason="requires a Windows desktop")

# Tk script that opens one top-level window titled argv[1], forces it to be
# mapped with update() and prints "ready" so the test does not race a
# not-yet-enumerable window under CI load, then blocks in the event loop.
_TK_WINDOW_SCRIPT = (
    "import sys, tkinter; "
    "root = tkinter.Tk(); "
    "root.title(sys.argv[1]); "
    "root.geometry('320x120'); "
    "root.update(); "
    "sys.stdout.write('ready\\n'); "
    "sys.stdout.flush(); "
    "root.mainloop()"
)

_GRACE_SECONDS = 3.0


def test_focus_window_activates_a_real_window() -> None:
    title = f"cssh-rs-e2e-focus-{uuid.uuid4().hex}"
    helper = subprocess.Popen(
        [sys.executable, "-c", _TK_WINDOW_SCRIPT, title],
        stdout=subprocess.PIPE,
        text=True,
    )
    try:
        _wait_until_window_ready(helper)
        result = WindowFocus().focus_window(title, timeout=10.0)

        assert result == title
        assert WindowFocus().get_active_window_title() == title
    finally:
        _terminate(helper)


def _wait_until_window_ready(process: subprocess.Popen[str]) -> None:
    """Block until the Tk helper reports its window is mapped.

    An empty read means the helper exited before printing, so fail loudly
    rather than let the focus attempt race a window that never appears.
    """
    assert process.stdout is not None
    line = process.stdout.readline()
    assert line.strip() == "ready", f"Tk helper did not report readiness (got {line!r})"


def _terminate(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=_GRACE_SECONDS)
    except subprocess.TimeoutExpired:
        process.kill()
