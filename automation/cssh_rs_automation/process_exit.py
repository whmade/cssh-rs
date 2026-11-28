"""Report how a window's process exits for the cssh-rs Windows E2E suite."""

from __future__ import annotations

import logging
import sys
from typing import Any

from cssh_rs_automation.window_focus import WindowFocusError, find_window

LOGGER = logging.getLogger(__name__)

DEFAULT_TIMEOUT_SECONDS = 10.0

# OpenProcess access rights to wait on a process and read its exit code:
# https://learn.microsoft.com/en-us/windows/win32/procthread/process-security-and-access-rights
_SYNCHRONIZE = 0x00100000
_PROCESS_QUERY_LIMITED_INFORMATION = 0x00001000

# WaitForSingleObject return values and its INFINITE timeout:
# https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject
_WAIT_OBJECT_0 = 0x00000000
_WAIT_TIMEOUT = 0x00000102
_INFINITE = 0xFFFFFFFF


class ProcessExitError(RuntimeError):
    """Raised when a window's process cannot be watched or does not exit in time."""


class ProcessExit:
    """Robot Framework library that reports the exit code of a window's process."""

    ROBOT_LIBRARY_SCOPE = "SUITE"
    ROBOT_LIBRARY_VERSION = "0.1.0"

    def __init__(self) -> None:
        self.ROBOT_LIBRARY_LISTENER = self
        self._handles: dict[int, int] = {}

    def watch_window_process(self, title: str, match_mode: str = "exact") -> int:
        """Open the process that owns the single window matching ``title``.

        Args:
            title: Window title to match.
            match_mode: ``"exact"`` (full title) or ``"substring"`` (contains).

        Returns:
            The process id to pass to ``wait_for_process_exit``.
        """
        try:
            window = find_window(title, match_mode)
        except WindowFocusError as error:
            raise ProcessExitError(str(error)) from error
        # conhost names the console's client process as its window's owner:
        # https://github.com/microsoft/terminal/blob/main/src/interactivity/win32/windowio.cpp
        process_id = window.getPID()
        if not process_id:
            raise ProcessExitError(f"cannot read the process of window {title!r}")
        # Open it now: the exit code lasts only until the last handle to the process closes:
        # https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process
        handle = _open_process(process_id)
        self._stop_watching(process_id)
        self._handles[process_id] = handle
        return process_id

    def wait_for_process_exit(
        self, process_id: int, timeout: float = DEFAULT_TIMEOUT_SECONDS
    ) -> int:
        """Wait for a watched process to exit, stop watching it, and return its exit code.

        Args:
            process_id: A process id that ``watch_window_process`` returned.
            timeout: Seconds to wait for the process to exit.

        Returns:
            The process's exit code.
        """
        timeout_milliseconds = timeout * 1000
        if not 0 <= timeout_milliseconds < _INFINITE:
            raise ProcessExitError(
                f"timeout must be at least 0 and below {_INFINITE / 1000}s, got {timeout}"
            )
        handle = self._handles.get(process_id)
        if handle is None:
            raise ProcessExitError(f"process {process_id} is not watched")
        try:
            return _wait_for_exit_code(handle, int(timeout_milliseconds))
        finally:
            self._stop_watching(process_id)

    def _close(self) -> None:
        """Close the handles still open; Robot calls this library listener when the suite ends."""
        for process_id in list(self._handles):
            self._stop_watching(process_id)

    def _stop_watching(self, process_id: int) -> None:
        handle = self._handles.pop(process_id, None)
        if handle is not None:
            _close_handle(handle)


def _kernel32() -> Any:
    """Return kernel32 with ``GetLastError`` capture and prototypes set; Windows only."""
    if sys.platform != "win32":
        raise ProcessExitError("watching a process is only supported on Windows")

    import ctypes
    from ctypes import wintypes

    # ctypes.WinDLL exists only in the Windows typeshed stub; guarded above.
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)  # pyrefly: ignore[missing-attribute]
    kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
    kernel32.WaitForSingleObject.restype = wintypes.DWORD
    kernel32.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
    kernel32.GetExitCodeProcess.restype = wintypes.BOOL
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel32.CloseHandle.restype = wintypes.BOOL
    return kernel32


def _open_process(process_id: int) -> int:
    import ctypes

    handle = _kernel32().OpenProcess(
        _SYNCHRONIZE | _PROCESS_QUERY_LIMITED_INFORMATION, False, process_id
    )
    if not handle:
        raise ProcessExitError(f"OpenProcess({process_id}) failed: error {ctypes.get_last_error()}")
    return handle


def _wait_for_exit_code(handle: int, timeout_milliseconds: int) -> int:
    import ctypes
    from ctypes import wintypes

    kernel32 = _kernel32()
    result = kernel32.WaitForSingleObject(handle, timeout_milliseconds)
    if result == _WAIT_TIMEOUT:
        raise ProcessExitError(f"process did not exit within {timeout_milliseconds / 1000}s")
    if result != _WAIT_OBJECT_0:
        raise ProcessExitError(f"WaitForSingleObject failed: error {ctypes.get_last_error()}")
    exit_code = wintypes.DWORD()
    if not kernel32.GetExitCodeProcess(handle, ctypes.byref(exit_code)):
        raise ProcessExitError(f"GetExitCodeProcess failed: error {ctypes.get_last_error()}")
    return exit_code.value


def _close_handle(handle: int) -> None:
    import ctypes

    if not _kernel32().CloseHandle(handle):
        LOGGER.warning("CloseHandle(%#x) failed: error %d", handle, ctypes.get_last_error())
