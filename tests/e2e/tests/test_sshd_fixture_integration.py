"""Integration test that drives a real sshd through the fixture.

Skipped automatically when an OpenSSH server or client is unavailable, so
the hermetic unit suite still runs everywhere. On a host with both (and in
the Windows E2E CI), it verifies end-to-end that input sent over ssh to
each alias lands in that alias's marker file.
"""

from __future__ import annotations

import shutil
import subprocess

import pytest

from libraries.sshd_fixture import SshdFixture, SshdFixtureError, _resolve_sshd_path


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


def test_daemon_input_fans_out_to_every_marker() -> None:
    ssh = shutil.which("ssh")
    assert ssh is not None
    fixture = SshdFixture()
    info = fixture.start_sshd(["h1", "h2"])
    try:
        for alias in info["aliases"]:
            result = subprocess.run(
                [ssh, "-F", info["ssh_config"], alias],
                input=f"payload-{alias}\n".encode(),
                capture_output=True,
                timeout=15,
                check=False,
            )
            assert result.returncode == 0, result.stderr.decode("utf-8", "replace")
        assert fixture.read_marker("h1") == "payload-h1\n"
        assert fixture.read_marker("h2") == "payload-h2\n"
    finally:
        fixture.stop_sshd()
