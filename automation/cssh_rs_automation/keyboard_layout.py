"""Switch a console window's keyboard layout for the cssh-rs Windows E2E suite."""

from __future__ import annotations

import logging
import subprocess
import sys
import time
import uuid
from typing import Any

from cssh_rs_automation.window_focus import WindowFocus

LOGGER = logging.getLogger(__name__)

DEFAULT_TIMEOUT_SECONDS = 5.0
_POLL_INTERVAL_SECONDS = 0.1

_WM_INPUTLANGCHANGEREQUEST = 0x0050
_TH32CS_SNAPTHREAD = 0x00000004
# Process creation flag that gives the child its own console window:
# https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags
_CREATE_NEW_CONSOLE = 0x00000010


class KeyboardLayoutError(RuntimeError):
    """Raised when a window's keyboard layout cannot be switched or read."""


class KeyboardLayout:
    """Robot Framework library that switches a window's keyboard layout and restores it."""

    ROBOT_LIBRARY_SCOPE = "SUITE"
    ROBOT_LIBRARY_VERSION = "0.1.0"

    def __init__(self) -> None:
        self._original: int | None = None
        self._loaded: set[int] = set()

    def switch_keyboard_layout(
        self,
        title: str,
        klid: str,
        timeout: float = DEFAULT_TIMEOUT_SECONDS,
    ) -> None:
        """Switch the window titled ``title`` to layout ``klid`` and wait until it has it.

        Focus the window first. Since Windows 8 the layout follows the user, not
        the thread: a switch made in the foreground window carries over to every
        window focused after it, while one made in a background window is
        overridden as soon as that window gets focus:
        https://learn.microsoft.com/en-us/previous-versions/hh994466(v=vs.85)

        Args:
            title: Exact title of the window to switch.
            klid: Keyboard layout ID, e.g. ``00000809`` for United Kingdom.
            timeout: Seconds to wait for the window to report the layout.
        """
        hwnd = _find_window(title)
        loaded_before = _loaded_keyboard_layouts()
        target = _load_keyboard_layout(klid)
        # LoadKeyboardLayoutW returns the system default layout instead of
        # failing when it has no match, and a layout's low word is its language:
        # https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-loadkeyboardlayoutw
        # https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input#languages-locales-and-keyboard-layouts
        if target & 0xFFFF != int(klid, 16) & 0xFFFF:
            raise KeyboardLayoutError(f"layout {klid} loaded as {target:#010x}")
        if target not in loaded_before:
            self._loaded.add(target)
        if self._original is None:
            self._original = _window_keyboard_layout(hwnd)
        _request_keyboard_layout(hwnd, target)
        _await_layout(hwnd, target, timeout)

    def restore_keyboard_layout(self, timeout: float = DEFAULT_TIMEOUT_SECONDS) -> None:
        """Switch back to the layout the window had before the first switch.

        The switch is made in a spare console window, which works even when the
        switched window is gone, because a switch in the foreground window
        reaches every window. Layouts the switches had to load are unloaded
        afterwards; a failed unload is only logged. Does nothing when no switch
        was made.

        Args:
            timeout: Seconds to wait for the spare console to report the layout.
        """
        original = self._original
        if original is None:
            return
        title = f"cssh-rs-automation-layout-restore-{uuid.uuid4().hex}"
        console = _open_console(title)
        try:
            hwnd = _find_window(_focus_window(title))
            _request_keyboard_layout(hwnd, original)
            _await_layout(hwnd, original, timeout)
        finally:
            console.kill()
            console.wait()
        self._original = None
        loaded, self._loaded = self._loaded, set()
        for hkl in loaded:
            try:
                _unload_keyboard_layout(hkl)
            except KeyboardLayoutError as exc:
                LOGGER.warning("could not unload layout %#010x: %s", hkl, exc)


def _await_layout(hwnd: int, target: int, timeout: float) -> None:
    """Poll ``hwnd``'s layout until it is ``target``; raise after ``timeout``."""
    deadline = time.monotonic() + timeout
    while True:
        current = _window_keyboard_layout(hwnd)
        if current == target:
            return
        if time.monotonic() >= deadline:
            raise KeyboardLayoutError(
                f"window still has layout {current:#010x} {timeout}s after"
                f" requesting {target:#010x}"
            )
        time.sleep(_POLL_INTERVAL_SECONDS)


def _find_window(title: str) -> int:
    """Return the handle of the single window titled exactly ``title``."""
    # Imported lazily, as in window_focus: the SSH-invoked marker writer imports
    # this package headless and must stay free of pywinctl's GUI dependencies.
    import pywinctl

    matches = pywinctl.getWindowsWithTitle(title, condition=pywinctl.Re.IS)
    if len(matches) != 1:
        raise KeyboardLayoutError(
            f"expected exactly one window titled {title!r}, got {len(matches)}"
        )
    return matches[0].getHandle()


def _open_console(title: str) -> subprocess.Popen[bytes]:
    """Start a cmd console in its own window titled ``title``."""
    return subprocess.Popen(["cmd.exe", "/k", f"title {title}"], creationflags=_CREATE_NEW_CONSOLE)


def _focus_window(title: str) -> str:
    """Focus the single window whose title contains ``title``; return its full title.

    cmd prefixes an elevated console's title with ``Administrator: ``.
    """
    return WindowFocus().focus_window(title, match_mode="substring")


def _loaded_keyboard_layouts() -> set[int]:
    """Return the input locale identifiers loaded in the session."""
    import ctypes
    from ctypes import wintypes

    user32 = _user32()
    user32.GetKeyboardLayoutList.argtypes = [ctypes.c_int, ctypes.POINTER(wintypes.HKL)]
    user32.GetKeyboardLayoutList.restype = ctypes.c_int
    count = user32.GetKeyboardLayoutList(0, None)
    layouts = (wintypes.HKL * count)()
    copied = user32.GetKeyboardLayoutList(count, layouts)
    if not copied:
        raise KeyboardLayoutError(f"GetKeyboardLayoutList failed: error {ctypes.get_last_error()}")
    return {int(layout or 0) for layout in layouts[:copied]}


def _unload_keyboard_layout(hkl: int) -> None:
    """Unload the input locale identifier ``hkl`` from the session."""
    import ctypes
    from ctypes import wintypes

    user32 = _user32()
    user32.UnloadKeyboardLayout.argtypes = [wintypes.HKL]
    user32.UnloadKeyboardLayout.restype = wintypes.BOOL
    if not user32.UnloadKeyboardLayout(hkl):
        raise KeyboardLayoutError(f"UnloadKeyboardLayout failed: error {ctypes.get_last_error()}")


def _load_keyboard_layout(klid: str) -> int:
    """Load layout ``klid`` into the session and return its input locale identifier."""
    import ctypes
    from ctypes import wintypes

    user32 = _user32()
    user32.LoadKeyboardLayoutW.argtypes = [wintypes.LPCWSTR, wintypes.UINT]
    user32.LoadKeyboardLayoutW.restype = wintypes.HKL
    hkl = user32.LoadKeyboardLayoutW(klid, 0)
    if not hkl:
        raise KeyboardLayoutError(
            f"LoadKeyboardLayoutW({klid!r}) failed: error {ctypes.get_last_error()}"
        )
    return int(hkl)


def _request_keyboard_layout(hwnd: int, hkl: int) -> None:
    """Post ``hwnd`` the request the language bar sends to switch to ``hkl``.

    The window's default procedure activates the layout on the window's own
    thread:
    https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-inputlangchangerequest
    conhost passes the message on to it unchanged:
    https://github.com/microsoft/terminal/blob/main/src/interactivity/win32/windowproc.cpp
    """
    import ctypes
    from ctypes import wintypes

    user32 = _user32()
    user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
    user32.PostMessageW.restype = wintypes.BOOL
    if not user32.PostMessageW(hwnd, _WM_INPUTLANGCHANGEREQUEST, 0, hkl):
        raise KeyboardLayoutError(
            f"PostMessageW(WM_INPUTLANGCHANGEREQUEST) failed: error {ctypes.get_last_error()}"
        )


def _window_keyboard_layout(hwnd: int) -> int:
    """Return the active layout of the thread that really owns ``hwnd``.

    conhost reports a console window as owned by the console's client process
    and thread, so ``GetWindowThreadProcessId`` names a thread that never
    handles the window's keys:
    https://github.com/microsoft/terminal/blob/main/src/interactivity/win32/windowio.cpp
    ``EnumThreadWindows`` still lists the window under its real conhost thread,
    so every thread in the system is asked for its windows instead.
    """
    import ctypes
    from ctypes import wintypes

    class ThreadEntry32(ctypes.Structure):
        _fields_ = (
            ("dwSize", wintypes.DWORD),
            ("cntUsage", wintypes.DWORD),
            ("th32ThreadID", wintypes.DWORD),
            ("th32OwnerProcessID", wintypes.DWORD),
            ("tpBasePri", wintypes.LONG),
            ("tpDeltaPri", wintypes.LONG),
            ("dwFlags", wintypes.DWORD),
        )

    user32 = _user32()
    # ctypes.WinDLL exists only in the Windows typeshed stub; _user32 guards it.
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)  # pyrefly: ignore[missing-attribute]
    kernel32.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
    kernel32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    kernel32.Thread32First.argtypes = [wintypes.HANDLE, ctypes.POINTER(ThreadEntry32)]
    kernel32.Thread32First.restype = wintypes.BOOL
    kernel32.Thread32Next.argtypes = [wintypes.HANDLE, ctypes.POINTER(ThreadEntry32)]
    kernel32.Thread32Next.restype = wintypes.BOOL
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel32.CloseHandle.restype = wintypes.BOOL
    enum_proc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    user32.EnumThreadWindows.argtypes = [wintypes.DWORD, enum_proc, wintypes.LPARAM]
    user32.EnumThreadWindows.restype = wintypes.BOOL
    user32.GetKeyboardLayout.argtypes = [wintypes.DWORD]
    user32.GetKeyboardLayout.restype = wintypes.HKL

    found = False

    def is_target(window: int | None, _: int) -> bool:
        nonlocal found
        found = window == hwnd
        return not found

    callback = enum_proc(is_target)
    snapshot = kernel32.CreateToolhelp32Snapshot(_TH32CS_SNAPTHREAD, 0)
    if snapshot in (None, wintypes.HANDLE(-1).value):
        raise KeyboardLayoutError(
            f"CreateToolhelp32Snapshot failed: error {ctypes.get_last_error()}"
        )
    try:
        entry = ThreadEntry32()
        entry.dwSize = ctypes.sizeof(ThreadEntry32)
        more = kernel32.Thread32First(snapshot, ctypes.byref(entry))
        while more:
            thread_id = entry.th32ThreadID
            # The idle process lists its threads with ID 0, which EnumThreadWindows
            # takes as every thread and GetKeyboardLayout as the calling thread:
            # https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getkeyboardlayout
            if thread_id:
                user32.EnumThreadWindows(thread_id, callback, 0)
                if found:
                    return int(user32.GetKeyboardLayout(thread_id) or 0)
            more = kernel32.Thread32Next(snapshot, ctypes.byref(entry))
    finally:
        kernel32.CloseHandle(snapshot)
    raise KeyboardLayoutError(f"no thread owns window {hwnd:#x}")


def _user32() -> Any:
    """Return user32 loaded with ``GetLastError`` capture; Windows only."""
    if sys.platform != "win32":
        raise KeyboardLayoutError("keyboard layouts are only supported on Windows")

    import ctypes

    # ctypes.WinDLL exists only in the Windows typeshed stub; guarded above.
    return ctypes.WinDLL("user32", use_last_error=True)  # pyrefly: ignore[missing-attribute]
