"""Unit tests for the demo keyword library.

Every cssh_rs_automation collaborator is injected as a mock, so the tests drive
the DemoRecorder keywords with zero process, window or filesystem side-effects.
``platform.system``, ``subprocess.Popen`` and ``subprocess.run`` are patched so
the Windows-only guard and the process launch/teardown never touch the host.
"""

from __future__ import annotations

import getpass
from typing import TYPE_CHECKING
from unittest.mock import MagicMock

import pytest
from cssh_rs_automation.sshd_fixture import ScriptedShellMode

from cssh_rs_demo.recorder import (
    DEFAULT_CLUSTER,
    DISPLAY_USER,
    HOSTS,
    README_CONTENT,
    README_NAME,
    SHELL_RC_LINES,
    USERNAME_HOST_PLACEHOLDER,
    DemoError,
    DemoRecorder,
)

if TYPE_CHECKING:
    from pathlib import Path


def _recorder(**overrides: object) -> tuple[DemoRecorder, dict[str, MagicMock]]:
    mocks = {
        "sshd": MagicMock(name="sshd"),
        "recorder": MagicMock(name="recorder"),
        "keystrokes": MagicMock(name="keystrokes"),
        "focus": MagicMock(name="focus"),
        "config_gen": MagicMock(name="config_gen"),
        "gif_exporter": MagicMock(name="gif_exporter", return_value="out.gif"),
    }
    mocks.update(overrides)  # type: ignore[arg-type]
    return DemoRecorder(**mocks), mocks  # type: ignore[arg-type]


@pytest.mark.parametrize(
    ("system", "match"),
    [("Linux", "Windows only"), ("Windows", "not found")],
)
def test_start_demo_bails_on_bad_precondition(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, system: str, match: str
) -> None:
    monkeypatch.setattr("cssh_rs_demo.recorder.platform.system", lambda: system)
    recorder, _ = _recorder()

    # On Linux the guard fires before the binary is checked; on Windows the
    # missing binary is what trips it.
    with pytest.raises(DemoError, match=match):
        recorder.start_demo(str(tmp_path / "missing.exe"), "out")


def test_start_demo_seeds_hosts_wires_overlay_and_launches(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.setattr("cssh_rs_demo.recorder.platform.system", lambda: "Windows")
    popen = MagicMock(name="Popen")
    monkeypatch.setattr("cssh_rs_demo.recorder.subprocess.Popen", popen)
    binary = tmp_path / "cssh-rs.exe"
    binary.write_text("stub")
    homes = {"hosta.dev": str(tmp_path / "dev"), "hostb.prod": str(tmp_path / "prod")}
    recorder, mocks = _recorder()
    mocks["sshd"].start_sshd.return_value = {"ssh_config": "cfg", "homes": homes}
    mocks["config_gen"].generate_config.return_value = "config.toml"
    order: list[str] = []
    mocks["recorder"].start_recording.side_effect = lambda *_a, **_k: order.append("record")
    popen.side_effect = lambda *_a, **_k: order.append("launch")

    recorder.start_demo(str(binary), "out", fps="10")

    mocks["sshd"].start_sshd.assert_called_once_with(
        HOSTS, mode=ScriptedShellMode(rc_lines=SHELL_RC_LINES)
    )
    generate_kwargs = mocks["config_gen"].generate_config.call_args.kwargs
    assert mocks["config_gen"].generate_config.call_args.args[3] == HOSTS
    assert generate_kwargs["cluster_name"] == DEFAULT_CLUSTER
    assert generate_kwargs["arguments"] == [
        "-o",
        f"User={getpass.getuser()}",
        USERNAME_HOST_PLACEHOLDER,
    ]
    mocks["recorder"].add_overlay.assert_called_once()
    mocks["keystrokes"].add_key_listener.assert_called_once()
    popen.assert_called_once_with([str(binary), "-u", DISPLAY_USER, DEFAULT_CLUSTER])
    assert (tmp_path / "dev" / "demo" / "data" / README_NAME).read_text() == f"{README_CONTENT}\n"
    assert not (tmp_path / "prod" / "demo" / "data" / README_NAME).exists()
    assert order == ["record", "launch"]


def test_wait_for_hosts_returns_once_sessions_are_ready() -> None:
    recorder, mocks = _recorder()
    mocks["sshd"].count_connected_markers.return_value = len(HOSTS)

    recorder.wait_for_hosts()

    mocks["sshd"].count_connected_markers.assert_called()


def test_broadcast_focuses_daemon_then_types_each_key(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("cssh_rs_demo.recorder.time.sleep", lambda _s: None)
    recorder, mocks = _recorder()

    recorder.broadcast("hi")

    mocks["focus"].focus_window.assert_called_once()
    typed = [call.args[0] for call in mocks["keystrokes"].type_text.call_args_list]
    assert typed == ["h", "i"]
    mocks["keystrokes"].press_key.assert_called_once_with("enter")


def test_export_demo_gif_stops_then_exports() -> None:
    recorder, mocks = _recorder()
    mocks["recorder"].stop_recording.return_value = "demo.mp4"

    result = recorder.export_demo_gif("demo.gif", fps="10")

    mocks["gif_exporter"].assert_called_once_with("demo.mp4", "demo.gif", fps=10)
    assert result == "out.gif"


def test_export_demo_gif_raises_without_recording() -> None:
    recorder, mocks = _recorder()
    mocks["recorder"].stop_recording.return_value = None

    with pytest.raises(DemoError, match="no recording"):
        recorder.export_demo_gif("demo.gif")


def test_tear_down_demo_is_best_effort(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    killed = MagicMock(name="run")
    monkeypatch.setattr("cssh_rs_demo.recorder.subprocess.run", killed)
    config = tmp_path / "config.toml"
    config.write_text("x")
    recorder, mocks = _recorder()
    recorder._launched = True
    recorder._config_path = str(config)

    recorder.tear_down_demo()

    mocks["recorder"].stop_recording.assert_called_once()
    killed.assert_called_once()
    mocks["sshd"].stop_sshd.assert_called_once()
    assert not config.exists()
