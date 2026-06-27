"""Append the stdin byte stream to the file path passed in argv[1].

Invoked by sshd through the per-key ``command="..."`` restriction in
``authorized_keys``. Each successful connection from an alias-specific
key runs this helper with that alias's marker path, so whatever the
client writes on the channel lands in ``markers/<alias>.log``.

The helper opens the marker file in binary append mode and copies
``sys.stdin.buffer`` into it byte-for-byte. It must exit promptly so
sshd can close the channel cleanly.
"""

from __future__ import annotations

import shutil
import sys


def main() -> int:
    """Copy stdin to the file named in ``sys.argv[1]`` and return 0.

    Returns:
        Process exit code. 2 if the marker path argument is missing,
        1 if writing fails, 0 otherwise.
    """
    if len(sys.argv) != 2:
        sys.stderr.write("usage: _marker_writer.py <marker-path>\n")
        return 2
    marker_path = sys.argv[1]
    try:
        with open(marker_path, "ab") as marker:
            shutil.copyfileobj(sys.stdin.buffer, marker)
    except OSError as exc:
        sys.stderr.write(f"marker write failed: {exc}\n")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
