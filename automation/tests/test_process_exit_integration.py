"""Integration tests that wait on a real window process through the library."""

from __future__ import annotations

import subprocess
import sys
import uuid
from collections.abc import Iterator

import pytest

from cssh_rs_automation.process_exit import ProcessExit, ProcessExitError

pytestmark = pytest.mark.skipif(sys.platform != "win32", reason="requires a Windows desktop")

_EXIT_CODE = 7

# Prints "ready" only once its window is mapped, so the test never looks it up too early.
_TK_WINDOW_SCRIPT = (
    "import sys, tkinter; "
    "root = tkinter.Tk(); "
    "root.title(sys.argv[1]); "
    "root.update(); "
    "sys.stdout.write('ready\\n'); "
    "sys.stdout.flush(); "
    "sys.stdin.read(); "
    f"sys.exit({_EXIT_CODE})"
)


@pytest.fixture
def tk_window() -> Iterator[tuple[str, subprocess.Popen[str]]]:
    title = f"cssh-rs-automation-exit-{uuid.uuid4().hex}"
    helper = subprocess.Popen(
        [sys.executable, "-c", _TK_WINDOW_SCRIPT, title],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
    )
    try:
        assert helper.stdout is not None
        assert helper.stdout.readline().strip() == "ready"
        yield title, helper
    finally:
        helper.kill()
        helper.wait()


def test_wait_for_process_exit_returns_the_exit_code_of_a_window_process(
    tk_window: tuple[str, subprocess.Popen[str]],
) -> None:
    title, helper = tk_window
    library = ProcessExit()
    process_id = library.watch_window_process(title)
    assert helper.stdin is not None
    helper.stdin.close()

    exit_code = library.wait_for_process_exit(process_id)

    assert exit_code == _EXIT_CODE


def test_wait_for_process_exit_raises_when_the_process_outlives_the_timeout(
    tk_window: tuple[str, subprocess.Popen[str]],
) -> None:
    title, _ = tk_window
    library = ProcessExit()
    process_id = library.watch_window_process(title)

    with pytest.raises(ProcessExitError, match="did not exit"):
        library.wait_for_process_exit(process_id, timeout=0)
