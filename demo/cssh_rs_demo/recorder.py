"""Robot Framework keyword library that drives and records the cssh-rs demo.

Composes the shared cssh_rs_automation primitives into the keywords the demo
task suite calls. Windows only: it synthesises keystrokes into and captures
real console windows.
"""

from __future__ import annotations

import platform
import subprocess
import time
from pathlib import Path
from typing import TYPE_CHECKING

from cssh_rs_automation.config_gen import ConfigGen
from cssh_rs_automation.keycast import Keycast, KeycastOverlay
from cssh_rs_automation.keystrokes import Keystrokes
from cssh_rs_automation.screen_recorder import ScreenRecorder
from cssh_rs_automation.sshd_fixture import SshdFixture
from cssh_rs_automation.window_focus import WindowFocus

from cssh_rs_demo.gif_export import export_gif

if TYPE_CHECKING:
    from collections.abc import Callable, Sequence

DAEMON_TITLE = "cssh-rs daemon"
# "cssh-rs -" matches every client ("cssh-rs - user@host") but not the daemon.
CLIENT_TITLE_SUBSTRING = "cssh-rs -"
DEFAULT_HOSTS = ("web01", "web02", "db01")
DEFAULT_CLUSTER = "demo"
DEFAULT_FPS = 10

_CONNECT_TIMEOUT_SECONDS = 30.0
_WINDOW_TIMEOUT_SECONDS = 20.0
_POLL_INTERVAL_SECONDS = 0.5
_TYPING_INTERVAL_SECONDS = 0.008


class DemoError(RuntimeError):
    """Raised when the demo cannot run or record."""


class DemoRecorder:
    """Robot Framework library that drives and records the cssh-rs demo."""

    ROBOT_LIBRARY_SCOPE = "SUITE"
    ROBOT_LIBRARY_VERSION = "0.1.0"

    def __init__(
        self,
        sshd: SshdFixture | None = None,
        recorder: ScreenRecorder | None = None,
        keystrokes: Keystrokes | None = None,
        focus: WindowFocus | None = None,
        config_gen: ConfigGen | None = None,
        gif_exporter: Callable[..., str] | None = None,
    ) -> None:
        self._sshd = sshd or SshdFixture()
        self._recorder = recorder or ScreenRecorder()
        self._keystrokes = keystrokes or Keystrokes()
        self._focus = focus or WindowFocus()
        self._config_gen = config_gen or ConfigGen()
        self._export_gif = gif_exporter or export_gif
        self._hosts: tuple[str, ...] = ()
        self._config_path: str | None = None
        self._launched = False

    def start_demo(
        self,
        binary: str,
        output_dir: str,
        hosts: Sequence[str] = DEFAULT_HOSTS,
        fps: int = DEFAULT_FPS,
    ) -> None:
        """Bring up the cluster in shell mode and start recording with the keycast overlay.

        Args:
            binary: Path to the cssh-rs executable to drive.
            output_dir: Directory the intermediate MP4 is written into.
            hosts: Host aliases the demo cluster launches.
            fps: Frames per second for the recording.
        """
        if platform.system() != "Windows":
            raise DemoError("the demo recorder runs on Windows only")
        binary_path = Path(binary)
        if not binary_path.is_file():
            raise DemoError(f"cssh-rs binary not found: {binary_path}")
        self._hosts = tuple(hosts)

        info = self._sshd.start_sshd(self._hosts, shell=True)
        self._config_path = self._config_gen.generate_config(
            str(binary_path),
            str(binary_path.resolve().parent),
            str(info["ssh_config"]),
            self._hosts,
            cluster_name=DEFAULT_CLUSTER,
        )

        keycast = Keycast()
        self._recorder.add_overlay(KeycastOverlay(keycast))
        self._keystrokes.add_key_listener(keycast.record)

        # Record before launching so the clip captures the windows arranging.
        self._recorder.start_recording("cssh-rs", output_dir, fps=int(fps))
        subprocess.Popen([str(binary_path), DEFAULT_CLUSTER])
        self._launched = True

    def wait_for_hosts(self) -> None:
        """Wait until every demo host has an open client window.

        Shell-mode sessions write no markers, so readiness is the client windows
        coming up; raises ``DemoError`` on timeout.
        """
        expected = len(self._hosts)
        deadline = time.monotonic() + _CONNECT_TIMEOUT_SECONDS
        while time.monotonic() < deadline:
            if self._focus.count_windows(CLIENT_TITLE_SUBSTRING) >= expected:
                return
            time.sleep(_POLL_INTERVAL_SECONDS)
        raise DemoError(
            f"timed out after {_CONNECT_TIMEOUT_SECONDS}s waiting for "
            f"{expected} client windows to open"
        )

    def broadcast(self, command: str) -> None:
        """Focus the daemon and broadcast ``command`` to every enabled client.

        Types one character at a time so the keycast overlay reveals each key as
        it is pressed.

        Args:
            command: Command line typed into the daemon and run everywhere.
        """
        self._focus.focus_window(DAEMON_TITLE, timeout=_WINDOW_TIMEOUT_SECONDS)
        for char in command:
            self._keystrokes.type_text(char)
            time.sleep(_TYPING_INTERVAL_SECONDS)
        self._keystrokes.press_key("enter")

    def export_demo_gif(self, gif: str, fps: int = DEFAULT_FPS) -> str:
        """Stop recording and export the captured MP4 to ``gif``.

        Args:
            gif: Destination path for the exported GIF.
            fps: Frames per second for the GIF.

        Returns:
            Absolute path to the written GIF as a str.
        """
        mp4_path = self._recorder.stop_recording()
        if mp4_path is None:
            raise DemoError("no recording to export; start_demo was not called")
        return self._export_gif(mp4_path, gif, fps=int(fps))

    def tear_down_demo(self) -> None:
        """Stop recording and tear the cluster down; every step is best-effort."""
        self._recorder.stop_recording()
        if self._launched:
            # /T ends cssh-rs together with the client processes it spawned.
            subprocess.run(
                ["taskkill", "/F", "/T", "/IM", "cssh-rs.exe"],
                capture_output=True,
                check=False,
            )
            self._launched = False
        self._sshd.stop_sshd()
        if self._config_path is not None:
            Path(self._config_path).unlink(missing_ok=True)
            self._config_path = None
