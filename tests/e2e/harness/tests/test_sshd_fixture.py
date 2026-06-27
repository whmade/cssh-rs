"""Unit tests for the sshd fixture library's pure logic and validation."""

from __future__ import annotations

from pathlib import Path

import pytest

from cssh_rs_e2e import sshd_fixture
from cssh_rs_e2e.sshd_fixture import SshdFixture, SshdFixtureError


def test_quote_escapes_backslashes_then_quotes() -> None:
    quoted = sshd_fixture._quote_authorized_keys_arg(r'C:\Program Files\py"thon')

    assert quoted == r"\"C:\\Program Files\\py\"thon\""


def test_build_forced_command_invokes_marker_module() -> None:
    forced = sshd_fixture._build_forced_command("/usr/bin/python3", "/tmp/markers/h1.log")

    assert forced == r"\"/usr/bin/python3\" -m cssh_rs_e2e._marker_writer \"/tmp/markers/h1.log\""


def test_as_forward_slash_normalizes_separators() -> None:
    assert sshd_fixture._as_forward_slash(Path("a/b/c")) == "a/b/c"
    assert sshd_fixture._as_forward_slash(Path("plain")) == "plain"


def test_render_sshd_config_emits_expected_directives(tmp_path: Path) -> None:
    config = sshd_fixture._render_sshd_config(
        port=2222,
        host_key=tmp_path / "host_ed25519",
        authorized_keys=tmp_path / "authorized_keys",
        pid_file=tmp_path / "sshd.pid",
    )

    assert "Port 2222\n" in config
    assert "ListenAddress 127.0.0.1\n" in config
    assert "PasswordAuthentication no\n" in config
    assert "PubkeyAuthentication yes\n" in config
    assert f"HostKey {sshd_fixture._as_forward_slash(tmp_path / 'host_ed25519')}\n" in config
    assert "\\" not in config


def test_render_ssh_config_one_block_per_alias(tmp_path: Path) -> None:
    config = sshd_fixture._render_ssh_config(
        aliases=["h1", "h2"],
        port=2222,
        keys_dir=tmp_path / "keys",
        known_hosts=tmp_path / "known_hosts",
        user="tester",
    )

    assert config.count("Host h1\n") == 1
    assert config.count("Host h2\n") == 1
    assert config.count("Port 2222\n") == 2
    assert config.count("User tester\n") == 2
    assert "IdentitiesOnly yes\n" in config


def test_pick_free_port_in_range() -> None:
    port = sshd_fixture._pick_free_port()

    assert 1024 <= port <= 65535


def test_write_openssh_keypair_creates_private_and_public(tmp_path: Path) -> None:
    private = tmp_path / "id_ed25519"

    sshd_fixture._write_openssh_keypair(private)

    assert private.is_file()
    public = private.with_suffix(".pub")
    assert public.read_text(encoding="utf-8").startswith("ssh-ed25519 ")


def test_resolve_sshd_path_honors_existing_override(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    fake_sshd = tmp_path / "sshd"
    fake_sshd.write_text("", encoding="utf-8")
    monkeypatch.setenv("CSSH_E2E_SSHD", str(fake_sshd))

    assert sshd_fixture._resolve_sshd_path() == str(fake_sshd)


def test_resolve_sshd_path_rejects_missing_override(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("CSSH_E2E_SSHD", "/nonexistent/sshd")

    with pytest.raises(SshdFixtureError, match="non-existent path"):
        sshd_fixture._resolve_sshd_path()


def test_resolve_sshd_path_falls_back_to_which(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("CSSH_E2E_SSHD", raising=False)
    monkeypatch.setattr(sshd_fixture.shutil, "which", lambda _: "/usr/sbin/sshd")

    assert sshd_fixture._resolve_sshd_path() == "/usr/sbin/sshd"


def test_resolve_sshd_path_raises_when_absent(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("CSSH_E2E_SSHD", raising=False)
    monkeypatch.setattr(sshd_fixture.shutil, "which", lambda _: None)
    monkeypatch.setattr(sshd_fixture, "DEFAULT_SSHD_LOCATIONS", ())

    with pytest.raises(SshdFixtureError, match="could not locate sshd"):
        sshd_fixture._resolve_sshd_path()


def test_start_sshd_rejects_empty_aliases() -> None:
    with pytest.raises(SshdFixtureError, match="non-empty"):
        SshdFixture().start_sshd([])


def test_start_sshd_rejects_duplicate_aliases() -> None:
    with pytest.raises(SshdFixtureError, match="unique"):
        SshdFixture().start_sshd(["h1", "h1"])


def test_start_sshd_rejects_when_already_running() -> None:
    fixture = SshdFixture()
    fixture._process = object()  # type: ignore[assignment]

    with pytest.raises(SshdFixtureError, match="already running"):
        fixture.start_sshd(["h1"])


def test_accessors_raise_before_start() -> None:
    fixture = SshdFixture()

    assert fixture.host_aliases() == []
    for accessor in (fixture.markers_dir, fixture.ssh_config_path, fixture.port):
        with pytest.raises(SshdFixtureError, match="not running"):
            accessor()
    with pytest.raises(SshdFixtureError, match="not running"):
        fixture.read_marker("h1")
