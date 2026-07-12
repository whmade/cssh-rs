"""Integration test that drives a real sshd through the fixture.

Skipped automatically when an OpenSSH server or client is unavailable, so
the hermetic unit suite still runs everywhere. On a host with both (and in
the Windows E2E CI), it verifies end-to-end that input sent over ssh to
each alias lands in that alias's marker file.
"""

from __future__ import annotations

import shutil
import subprocess
import time

import pytest

from cssh_rs_automation.sshd_fixture import SshdFixture, SshdFixtureError, _resolve_sshd_path


def _ssh_stack_available() -> bool:
    if shutil.which("ssh") is None:
        return False
    try:
        _resolve_sshd_path()
    except SshdFixtureError:
        return False
    return True


pytestmark = pytest.mark.skipif(
    not _ssh_stack_available(), reason="sshd server or ssh client not available"
)


def _await_marker(fixture: SshdFixture, alias: str, needle: str) -> str:
    """Poll ``alias``'s marker until it contains ``needle`` or a deadline passes."""
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if needle in fixture.read_marker(alias):
            break
        time.sleep(0.1)
    return fixture.read_marker(alias)


def test_daemon_input_fans_out_to_every_marker() -> None:
    ssh = shutil.which("ssh")
    assert ssh is not None
    fixture = SshdFixture()
    info = fixture.start_sshd(["h1", "h2"])
    # The session gets a PTY, which stays interactive rather than exiting on
    # stdin EOF, so write into a live session and poll the marker instead of
    # running ssh to completion. Send a real Enter (CR): a PTY completes a
    # cooked line on CR, and its newline translation differs by OS, so assert
    # on the payload text rather than exact bytes.
    sessions = []
    try:
        for alias in info["aliases"]:
            session = subprocess.Popen(
                [ssh, "-F", info["ssh_config"], alias],
                stdin=subprocess.PIPE,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            sessions.append(session)
            assert session.stdin is not None
            session.stdin.write(f"payload-{alias}\r".encode())
            session.stdin.flush()
        assert "payload-h1" in _await_marker(fixture, "h1", "payload-h1")
        assert "payload-h2" in _await_marker(fixture, "h2", "payload-h2")
    finally:
        for session in sessions:
            session.kill()
            session.wait()
        fixture.stop_sshd()
