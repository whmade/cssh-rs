"""Unit tests for the sshd fixture library's pure logic and validation."""

from __future__ import annotations

import subprocess
from pathlib import Path

import pytest

from cssh_rs_automation import sshd_fixture
from cssh_rs_automation.sshd_fixture import SshdFixture, SshdFixtureError


def test_quote_escapes_backslashes_then_quotes() -> None:
    quoted = sshd_fixture._quote_authorized_keys_arg(r'C:\Program Files\py"thon')

    assert quoted == r"\"C:\\Program Files\\py\"thon\""


def test_build_forced_command_invokes_marker_module() -> None:
    forced = sshd_fixture._build_forced_command("/usr/bin/python3", "/tmp/markers/h1.log")

    assert (
        forced
        == r"\"/usr/bin/python3\" -m cssh_rs_automation._marker_writer \"/tmp/markers/h1.log\""
    )


def test_as_forward_slash_normalizes_separators() -> None:
    assert sshd_fixture._as_forward_slash(Path("a/b/c")) == "a/b/c"
    assert sshd_fixture._as_forward_slash(Path("plain")) == "plain"


def test_render_sshd_config_emits_expected_directives(tmp_path: Path) -> None:
    config = sshd_fixture._render_sshd_config(
        port=2222,
        host_key=tmp_path / "host_ed25519",
        authorized_keys=tmp_path / "authorized_keys",
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
    # Force a PTY (as in real use) so a relayed Ctrl+C forwards to the remote.
    assert config.count("RequestTTY force\n") == 2


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


def test_resolve_sshd_path_rejects_directory_override(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.setenv("CSSH_E2E_SSHD", str(tmp_path))

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


@pytest.mark.parametrize(
    "alias",
    ["../escape", "a/b", "a\\b", "has space", "with\nnewline", "tab\there", ""],
)
def test_start_sshd_rejects_unsafe_aliases(alias: str) -> None:
    with pytest.raises(SshdFixtureError):
        SshdFixture().start_sshd([alias])


@pytest.mark.parametrize("port", [0, -1, 1023, 65536, 100000])
def test_start_sshd_rejects_out_of_range_port(port: int) -> None:
    with pytest.raises(SshdFixtureError, match="port must be between"):
        SshdFixture().start_sshd(["h1"], port=port)


def test_start_sshd_rejects_when_already_running() -> None:
    fixture = SshdFixture()
    fixture._process = object()  # type: ignore[assignment]

    with pytest.raises(SshdFixtureError, match="already running"):
        fixture.start_sshd(["h1"])


def test_read_marker_raises_before_start() -> None:
    with pytest.raises(SshdFixtureError, match="not running"):
        SshdFixture().read_marker("h1")


def test_read_log_tail_returns_only_last_bytes(tmp_path: Path) -> None:
    log = tmp_path / "sshd.log"
    log.write_bytes(b"x" * (sshd_fixture.MAX_LOG_TAIL_BYTES + 100))

    assert len(sshd_fixture._read_log_tail(log)) == sshd_fixture.MAX_LOG_TAIL_BYTES


def test_read_log_tail_missing_file_returns_empty(tmp_path: Path) -> None:
    assert sshd_fixture._read_log_tail(tmp_path / "absent.log") == ""


def test_terminate_survives_process_that_never_reaps() -> None:
    class _StuckProcess:
        def __init__(self) -> None:
            self.killed = False

        def poll(self) -> None:
            return None

        def terminate(self) -> None:
            pass

        def kill(self) -> None:
            self.killed = True

        def wait(self, timeout: float | None = None) -> int:
            raise subprocess.TimeoutExpired(cmd="sshd", timeout=timeout or 0.0)

    process = _StuckProcess()

    sshd_fixture._terminate(process)  # type: ignore[arg-type]

    assert process.killed


class _FakeProcess:
    def __init__(self, pid: int = 1234) -> None:
        self.pid = pid
        self._alive = True
        self.terminated = False

    def poll(self) -> int | None:
        return None if self._alive else 0

    def terminate(self) -> None:
        self.terminated = True
        self._alive = False

    def kill(self) -> None:
        self._alive = False

    def wait(self, **_kwargs: object) -> int:
        self._alive = False
        return 0


def test_terminate_tree_kills_whole_tree_via_taskkill_on_windows(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(sshd_fixture.os, "name", "nt")
    calls: list[list[str]] = []
    monkeypatch.setattr(
        sshd_fixture.subprocess,
        "run",
        lambda args, **_: calls.append(args) or subprocess.CompletedProcess(args, 0),
    )
    process = _FakeProcess(pid=4321)

    sshd_fixture._terminate_tree(process)  # type: ignore[arg-type]

    assert calls == [["taskkill", "/F", "/T", "/PID", "4321"]]
    assert not process.terminated


def test_terminate_tree_falls_back_to_terminate_off_windows(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(sshd_fixture.os, "name", "posix")
    calls: list[list[str]] = []
    monkeypatch.setattr(
        sshd_fixture.subprocess,
        "run",
        lambda args, **_: calls.append(args) or subprocess.CompletedProcess(args, 0),
    )
    process = _FakeProcess()

    sshd_fixture._terminate_tree(process)  # type: ignore[arg-type]

    assert process.terminated
    assert calls == []


def test_remove_tempdir_retries_until_directory_gone(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    target = tmp_path / "run"
    target.mkdir()
    real_rmtree = sshd_fixture.shutil.rmtree
    calls = {"n": 0}

    def fake_rmtree(path: object, ignore_errors: bool = False) -> None:
        calls["n"] += 1
        if calls["n"] >= 3:
            real_rmtree(path, ignore_errors=ignore_errors)

    monkeypatch.setattr(sshd_fixture.shutil, "rmtree", fake_rmtree)
    monkeypatch.setattr(sshd_fixture.time, "sleep", lambda _seconds: None)

    sshd_fixture._remove_tempdir(target)

    assert not target.exists()
    assert calls["n"] == 3


def test_remove_tempdir_keeps_tree_when_env_set(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    target = tmp_path / "run"
    target.mkdir()
    monkeypatch.setenv("CSSH_E2E_KEEP_TEMP", "1")

    sshd_fixture._remove_tempdir(target)

    assert target.exists()


def test_count_connected_markers_counts_marker_files(tmp_path: Path) -> None:
    fixture = SshdFixture()
    fixture._tempdir = tmp_path
    markers = tmp_path / "markers"
    markers.mkdir()
    (markers / "alpha.log").write_bytes(b"")
    (markers / "bravo.log").write_bytes(b"")

    assert fixture.count_connected_markers() == 2


def test_count_connected_markers_zero_before_connections(tmp_path: Path) -> None:
    fixture = SshdFixture()
    fixture._tempdir = tmp_path
    (tmp_path / "markers").mkdir()

    assert fixture.count_connected_markers() == 0


def test_count_connected_markers_raises_before_start() -> None:
    with pytest.raises(SshdFixtureError):
        SshdFixture().count_connected_markers()
