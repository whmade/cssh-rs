"""Append the stdin byte stream to the marker file named on the command line.

Invoked by sshd through the per-key ``command="..."`` restriction in
``authorized_keys``: each connection from an alias's key runs this helper
with that alias's marker path, so whatever the client sends on the channel
lands in ``markers/<alias>.log``.
"""

from __future__ import annotations

import contextlib
import os
import sys
from pathlib import Path

_READ_CHUNK_BYTES = 4096
_CTRL_C = b"\x03"
_RAW_INPUT_FLAG = "--raw-input"
_ENABLE_VIRTUAL_TERMINAL_INPUT = 0x0200


def main() -> int:
    """Append stdin to the marker file: ``_marker_writer.py [--raw-input] <marker-path>``.

    Flushes each chunk so input surfaces immediately instead of only at EOF.
    Exits on a relayed Ctrl+C - delivered as ``KeyboardInterrupt`` under a PTY
    or as a raw ``0x03`` byte without one - so the session ends like any
    interrupted remote command.

    ``--raw-input`` reads a console stdin as raw virtual terminal input, so
    line editing cannot consume ESC and other control bytes before they reach
    the marker. That turns the console's echo off, so the writer echoes what
    it records itself.

    Returns:
        Exit code: 2 if the marker path argument is missing, 1 if writing
        fails, 0 on success.
    """
    args = sys.argv[1:]
    raw_input = args[:1] == [_RAW_INPUT_FLAG]
    if raw_input:
        args = args[1:]
    if len(args) != 1:
        sys.stderr.write(f"usage: _marker_writer.py [{_RAW_INPUT_FLAG}] <marker-path>\n")
        return 2
    marker_path = args[0]
    try:
        stdin_fd = sys.stdin.fileno()
        raw_console = raw_input and os.isatty(stdin_fd)
        if raw_console:
            # Virtual terminal input alone turns off line editing, processed
            # Ctrl+C and echo: https://learn.microsoft.com/en-us/windows/console/setconsolemode
            _set_console_mode(stdin_fd, _ENABLE_VIRTUAL_TERMINAL_INPUT)
        # os.read returns a console's input in its code page, mangling non-ASCII;
        # an unbuffered binary open() reads it as UTF-8: https://peps.python.org/pep-0528/
        with (
            Path(marker_path).open("ab") as marker,
            os.fdopen(stdin_fd, "rb", buffering=0, closefd=False) as stdin,
            (
                os.fdopen(sys.stdout.fileno(), "wb", buffering=0, closefd=False)
                if raw_console
                else contextlib.nullcontext()
            ) as stdout,
        ):
            while True:
                chunk = stdin.read(_READ_CHUNK_BYTES)
                if not chunk or _CTRL_C in chunk:
                    break
                marker.write(chunk)
                marker.flush()
                if stdout is not None:
                    stdout.write(_terminal_echo(chunk))
    except KeyboardInterrupt:
        return 0
    except OSError as exc:
        sys.stderr.write(f"marker write failed: {exc}\n")
        return 1
    return 0


def _terminal_echo(chunk: bytes) -> bytes:
    """Return ``chunk`` as a terminal echoes it, control bytes in caret notation."""
    echoed = bytearray()
    for byte in chunk.replace(b"\r\n", b"\r"):
        if byte in b"\r\n":
            echoed += b"\r\n"
        elif byte == 0x09 or (byte >= 0x20 and byte != 0x7F):
            echoed.append(byte)
        else:
            echoed += bytes([ord("^"), byte ^ 0x40])
    return bytes(echoed)


def _set_console_mode(fd: int, mode: int) -> None:
    """Set the input mode of the console behind ``fd``; raise ``OSError`` on failure."""
    import ctypes
    import msvcrt
    from ctypes import wintypes

    # These ctypes and msvcrt names exist only in the Windows typeshed stubs;
    # only a console stdin, which exists only on Windows, gets here.
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)  # pyrefly: ignore[missing-attribute]
    kernel32.SetConsoleMode.argtypes = [wintypes.HANDLE, wintypes.DWORD]
    kernel32.SetConsoleMode.restype = wintypes.BOOL
    handle = msvcrt.get_osfhandle(fd)  # pyrefly: ignore[missing-attribute]
    if not kernel32.SetConsoleMode(handle, mode):
        raise ctypes.WinError(ctypes.get_last_error())  # pyrefly: ignore[missing-attribute]


if __name__ == "__main__":
    raise SystemExit(main())
