"""Append the stdin byte stream to the marker file named in ``argv[1]``.

Invoked by sshd through the per-key ``command="..."`` restriction in
``authorized_keys``: each connection from an alias's key runs this helper
with that alias's marker path, so whatever the client sends on the channel
lands in ``markers/<alias>.log``.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

# Read whatever bytes are already available rather than waiting for a full
# buffer. os.read returns as soon as any data arrives, so each keystroke
# surfaces in the marker promptly.
_READ_CHUNK_BYTES = 4096


def main() -> int:
    """Append stdin to the file named in ``sys.argv[1]``.

    Each chunk is flushed as it arrives so suites can observe input over a
    still-open SSH channel; a buffered copy would only surface bytes once the
    channel closed (EOF), which never happens while cssh-rs holds the session.

    Returns:
        Exit code: 2 if the marker path argument is missing, 1 if writing
        fails, 0 on success.
    """
    if len(sys.argv) != 2:
        sys.stderr.write("usage: _marker_writer.py <marker-path>\n")
        return 2
    marker_path = sys.argv[1]
    stdin_fd = sys.stdin.fileno()
    try:
        with Path(marker_path).open("ab") as marker:
            while True:
                chunk = os.read(stdin_fd, _READ_CHUNK_BYTES)
                if not chunk:
                    break
                marker.write(chunk)
                marker.flush()
    except OSError as exc:
        sys.stderr.write(f"marker write failed: {exc}\n")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
