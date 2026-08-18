"""Unit tests for the demo keyword library.

Every cssh_rs_automation collaborator is injected as a mock, so the tests drive
the DemoRecorder keywords with zero process, window or filesystem side-effects.
``platform.system``, ``subprocess.Popen`` and ``subprocess.run`` are patched so
the Windows-only guard and the process launch/teardown never touch the host.
"""

from __future__ import annotations

import getpass
from typing import TYPE_CHECKING
from unittest.mock import ANY, MagicMock, call

import pytest
from cssh_rs_automation.sshd_fixture import ScriptedShellMode

from cssh_rs_demo.recorder import (
    CLIENT_TITLE_MATCH,
    DAEMON_TITLE,
    DEFAULT_CLUSTER,
    DISPLAY_USER,
    HOSTS,
    INITIAL_HOSTS,
    README_CONTENT,
    README_NAME,
    SHELL_RC_LINES,
    USERNAME_HOST_PLACEHOLDER,
    DemoError,
    DemoRecorder,
    _disable_moves,
    _require_vim,
)

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path


def _recorder(**overrides: object) -> tuple[DemoRecorder, dict[str, MagicMock]]:
    mocks = {
        "sshd": MagicMock(name="sshd"),
        "recorder": MagicMock(name="recorder"),
        "keystrokes": MagicMock(name="keystrokes"),
        "focus": MagicMock(name="focus"),
        "config_gen": MagicMock(name="config_gen"),
        "clipboard": MagicMock(name="clipboard"),
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
    monkeypatch.setattr("cssh_rs_demo.recorder._require_vim", lambda: None)
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
    assert mocks["config_gen"].generate_config.call_args.args[3] == INITIAL_HOSTS
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
    mocks["sshd"].count_connected_markers.return_value = len(INITIAL_HOSTS)

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


def test_type_command_types_into_focused_window(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("cssh_rs_demo.recorder.time.sleep", lambda _s: None)
    recorder, mocks = _recorder()

    recorder.type_command("ls")

    mocks["focus"].focus_window.assert_not_called()
    mocks["keystrokes"].press_key.assert_called_once_with("enter")


def test_focus_client_targets_alias_window() -> None:
    recorder, mocks = _recorder()

    recorder.focus_client("hosta.dev")

    mocks["focus"].focus_window.assert_called_once_with(
        "@hosta.dev", match_mode="substring", timeout=ANY
    )


def test_copy_readme_puts_the_readme_on_the_clipboard() -> None:
    recorder, mocks = _recorder()

    recorder.copy_readme()

    mocks["clipboard"].set_clipboard.assert_called_once_with(README_CONTENT)


def test_edit_readme_opens_vim_then_pastes(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("cssh_rs_demo.recorder.time.sleep", lambda _s: None)
    recorder, mocks = _recorder()
    mocks["clipboard"].get_clipboard.return_value = "PASTED"

    recorder.edit_readme()

    mocks["focus"].focus_window.assert_called_with(DAEMON_TITLE, timeout=ANY)
    expected = (
        [call.type_text(char) for char in "vim README"]
        + [call.press_key("enter"), call.press_key("i")]
        + [call.type_text("PASTED", label="PASTE")]  # the paste, typed at once
        + [call.press_key("esc")]
        + [call.type_text(char) for char in ":wq"]
        + [call.press_key("enter")]
    )
    assert mocks["keystrokes"].method_calls == expected


@pytest.mark.parametrize(
    ("drive", "expected_keys"),
    [
        (lambda r: r.enable_all(), [call.send_hotkey("ctrl", "a"), call.press_key("n")]),
        (lambda r: r.interrupt(), [call.send_hotkey("ctrl", "c")]),
    ],
)
def test_control_mode_key_sequences(
    monkeypatch: pytest.MonkeyPatch,
    drive: Callable[[DemoRecorder], None],
    expected_keys: list[object],
) -> None:
    monkeypatch.setattr("cssh_rs_demo.recorder.time.sleep", lambda _s: None)
    recorder, mocks = _recorder()

    drive(recorder)

    mocks["focus"].focus_window.assert_called_with(DAEMON_TITLE, timeout=ANY)
    assert mocks["keystrokes"].method_calls == expected_keys


def _client_box(alias: str, left: int, top: int) -> tuple[str, int, int, int, int]:
    """Build a client window box tuple for ``alias`` at ``(left, top)``."""
    return (f"cssh-rs - root@{alias}", left, top, 100, 100)


def test_disable_client_finds_the_alias_cell_and_navigates_to_it(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr("cssh_rs_demo.recorder.time.sleep", lambda _s: None)
    recorder, mocks = _recorder()
    # 2x2 grid with hosta.dev bottom-left: one Down reaches it from the top-left.
    mocks["focus"].window_boxes.return_value = [
        _client_box("hosta.prod", 0, 0),
        _client_box("hostb.prod", 100, 0),
        _client_box("hosta.dev", 0, 100),
        _client_box("hostb.dev", 100, 100),
    ]

    recorder.disable_client("hosta.dev")

    mocks["focus"].window_boxes.assert_called_once_with(CLIENT_TITLE_MATCH, match_mode="substring")
    assert mocks["keystrokes"].method_calls == [
        call.send_hotkey("ctrl", "a"),
        call.press_key("e"),
        call.press_key("down"),
        call.press_key("d"),
        call.press_key("esc"),
    ]


@pytest.mark.parametrize(
    ("boxes", "expected"),
    [
        (
            # 2x2 grid: hosta.dev bottom-left.
            [
                _client_box("hosta.prod", 0, 0),
                _client_box("hostb.prod", 100, 0),
                _client_box("hosta.dev", 0, 100),
                _client_box("hostb.dev", 100, 100),
            ],
            ["down"],
        ),
        (
            # Single column: hosta.dev is the third row down.
            [
                _client_box("hosta.prod", 0, 0),
                _client_box("hostb.prod", 0, 100),
                _client_box("hosta.dev", 0, 200),
                _client_box("hostb.dev", 0, 300),
            ],
            ["down", "down"],
        ),
        (
            # Single row: hosta.dev is the third column across.
            [
                _client_box("hosta.prod", 0, 0),
                _client_box("hostb.prod", 100, 0),
                _client_box("hosta.dev", 200, 0),
                _client_box("hostb.dev", 300, 0),
            ],
            ["right", "right"],
        ),
        (
            # 3 columns: hosta.dev top-right, hostb.dev the stretched last row.
            [
                _client_box("hosta.prod", 0, 0),
                _client_box("hostb.prod", 100, 0),
                _client_box("hosta.dev", 200, 0),
                _client_box("hostb.dev", 0, 100),
            ],
            ["right", "right"],
        ),
    ],
)
def test_disable_moves_derives_the_path_from_the_layout(
    boxes: list[tuple[str, int, int, int, int]], expected: list[str]
) -> None:
    assert _disable_moves(boxes, "hosta.dev") == expected


def test_disable_moves_tolerates_a_pixel_nudge_in_the_row() -> None:
    boxes = [
        _client_box("hosta.prod", 0, 0),
        _client_box("hostb.prod", 100, 0),
        # The tiler's 1px repaint nudge must not split hosta.dev into its own row.
        _client_box("hosta.dev", 1, 101),
        _client_box("hostb.dev", 100, 100),
    ]

    assert _disable_moves(boxes, "hosta.dev") == ["down"]


def test_disable_moves_raises_for_an_unknown_alias() -> None:
    boxes = [_client_box("hosta.prod", 0, 0)]

    with pytest.raises(DemoError, match="no client window found"):
        _disable_moves(boxes, "hostc.dev")


def test_add_host_types_the_alias_then_waits_for_its_session(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr("cssh_rs_demo.recorder.time.sleep", lambda _s: None)
    recorder, mocks = _recorder()
    # One more session appears after the host is added.
    mocks["sshd"].count_connected_markers.side_effect = [3, 4]

    recorder.add_host("hostb.dev")

    expected = (
        [call.send_hotkey("ctrl", "a"), call.press_key("c")]
        + [call.type_text(char) for char in "hostb.dev"]
        + [call.press_key("enter")]
        # A one-byte Backspace, kept off the overlay, absorbs conhost's post-resize swallow.
        + [call.press_key("backspace", label=None)]
    )
    assert mocks["keystrokes"].method_calls == expected
    assert mocks["sshd"].count_connected_markers.call_count == 2


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


@pytest.mark.parametrize(
    ("run_result", "match"),
    [(OSError("no bash"), "bash on PATH"), (MagicMock(returncode=1), "vim")],
)
def test_require_vim_fails_loudly(
    monkeypatch: pytest.MonkeyPatch, run_result: object, match: str
) -> None:
    def fake_run(*_a: object, **_k: object) -> object:
        if isinstance(run_result, BaseException):
            raise run_result
        return run_result

    monkeypatch.setattr("cssh_rs_demo.recorder.resolve_bash_path", lambda: "bash")
    monkeypatch.setattr("cssh_rs_demo.recorder.subprocess.run", fake_run)

    with pytest.raises(DemoError, match=match):
        _require_vim()


def test_require_vim_probes_the_resolved_bash(monkeypatch: pytest.MonkeyPatch) -> None:
    git_bash = r"C:\Program Files\Git\bin\bash.exe"
    calls: list[list[str]] = []

    def fake_run(cmd: list[str], **_k: object) -> object:
        calls.append(cmd)
        return MagicMock(returncode=0)

    monkeypatch.setattr("cssh_rs_demo.recorder.resolve_bash_path", lambda: git_bash)
    monkeypatch.setattr("cssh_rs_demo.recorder.subprocess.run", fake_run)

    _require_vim()

    assert calls == [[git_bash, "-lc", "command -v vim"]]
