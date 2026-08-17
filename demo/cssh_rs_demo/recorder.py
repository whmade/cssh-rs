"""Robot Framework keyword library that drives and records the cssh-rs demo.

Composes the shared cssh_rs_automation primitives into the keywords the demo
task suite calls. Windows only: it synthesises keystrokes into and captures
real console windows.
"""

from __future__ import annotations

import getpass
import platform
import subprocess
import time
from pathlib import Path
from typing import TYPE_CHECKING, cast

from cssh_rs_automation.config_gen import ConfigGen
from cssh_rs_automation.keycast import Keycast, KeycastOverlay
from cssh_rs_automation.keystrokes import Keystrokes
from cssh_rs_automation.screen_recorder import ScreenRecorder
from cssh_rs_automation.sshd_fixture import ScriptedShellMode, SshdFixture
from cssh_rs_automation.window_focus import WindowFocus

from cssh_rs_demo.gif_export import export_gif

if TYPE_CHECKING:
    from collections.abc import Callable

DAEMON_TITLE = "cssh-rs daemon"
DEFAULT_CLUSTER = "demo"
DEFAULT_FPS = 10

HOSTS = ("hosta.prod", "hostb.prod", "hosta.dev")
README_HOST = "hosta.dev"
README_NAME = "README"
README_CONTENT = "This README is valid for dev and prod clusters"
SHELL_RC_LINES = "alias ll='ls -alF'"

# The scripted shell prompts as root@host; -u root makes the window titles match,
# while -o User= in start_demo still authenticates as the real user sshd accepts.
DISPLAY_USER = "root"
USERNAME_HOST_PLACEHOLDER = "{{USERNAME_AT_HOST}}"

_CONNECT_TIMEOUT_SECONDS = 30.0
_WINDOW_TIMEOUT_SECONDS = 20.0
_POLL_INTERVAL_SECONDS = 0.5
# Paced for a readable keycast overlay, not for speed.
_TYPING_INTERVAL_SECONDS = 0.05
_KEY_INTERVAL_SECONDS = 0.2


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
        self._config_path: str | None = None
        self._launched = False

    def start_demo(self, binary: str, output_dir: str, fps: int = DEFAULT_FPS) -> None:
        """Bring up the cluster in scripted-shell mode and start recording.

        Args:
            binary: Path to the cssh-rs executable to drive.
            output_dir: Directory the intermediate MP4 is written into.
            fps: Frames per second for the recording.
        """
        if platform.system() != "Windows":
            raise DemoError("the demo recorder runs on Windows only")
        binary_path = Path(binary)
        if not binary_path.is_file():
            raise DemoError(f"cssh-rs binary not found: {binary_path}")

        info = self._sshd.start_sshd(HOSTS, mode=ScriptedShellMode(rc_lines=SHELL_RC_LINES))
        self._seed_host_files(cast("dict[str, str]", info["homes"]))
        self._config_path = self._config_gen.generate_config(
            str(binary_path),
            str(binary_path.resolve().parent),
            str(info["ssh_config"]),
            HOSTS,
            cluster_name=DEFAULT_CLUSTER,
            arguments=["-o", f"User={getpass.getuser()}", USERNAME_HOST_PLACEHOLDER],
        )

        keycast = Keycast()
        self._recorder.add_overlay(KeycastOverlay(keycast))
        self._keystrokes.add_key_listener(keycast.record)

        # Launch only once capture is live so the clip catches the windows arranging.
        self._recorder.start_recording("cssh-rs", output_dir, fps=int(fps))
        self._recorder.wait_until_recording()
        subprocess.Popen([str(binary_path), "-u", DISPLAY_USER, DEFAULT_CLUSTER])
        self._launched = True

    def wait_for_hosts(self) -> None:
        """Poll the sshd markers until every session is ready, else raise ``DemoError``."""
        deadline = time.monotonic() + _CONNECT_TIMEOUT_SECONDS
        while time.monotonic() < deadline:
            if self._sshd.count_connected_markers() >= len(HOSTS):
                return
            time.sleep(_POLL_INTERVAL_SECONDS)
        raise DemoError(
            f"timed out after {_CONNECT_TIMEOUT_SECONDS}s waiting for {len(HOSTS)} ssh sessions"
        )

    def broadcast(self, command: str) -> None:
        """Focus the daemon and broadcast ``command`` to every enabled client.

        Types one character at a time so the keycast overlay reveals each key.

        Args:
            command: Command line typed into the daemon and run everywhere.
        """
        self._focus.focus_window(DAEMON_TITLE, timeout=_WINDOW_TIMEOUT_SECONDS)
        for char in command:
            self._keystrokes.type_text(char)
            time.sleep(_TYPING_INTERVAL_SECONDS)
        self._keystrokes.press_key("enter")
        time.sleep(_KEY_INTERVAL_SECONDS)

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

    def _seed_host_files(self, homes: dict[str, str]) -> None:
        """Create ``demo/data`` in every host home, with the README only on ``README_HOST``."""
        for alias, home in homes.items():
            data_dir = Path(home) / "demo" / "data"
            data_dir.mkdir(parents=True)
            if alias == README_HOST:
                readme = data_dir / README_NAME
                readme.write_text(f"{README_CONTENT}\n", encoding="utf-8", newline="\n")
