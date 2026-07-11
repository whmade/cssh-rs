"""Robot Framework keyword library that drives and records the cssh-rs demo.

Composes the shared cssh_rs_automation primitives (sshd fixture, config
generator, window focus, keystroke driver, screen recorder with the keycast
overlay) into the keywords the demo task suite calls. The storyline lives in
the suite; this library only holds the glue that Robot syntax cannot express
directly (compositing the keycast overlay, GIF export, connection polling).

Windows only: the demo synthesises keystrokes into and captures real console
windows, so it cannot run on the Linux/macOS CI hosts.
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
DEFAULT_HOSTS = ("web01", "web02", "db01")
DEFAULT_CLUSTER = "demo"
DEFAULT_FPS = 10
DEFAULT_CHAPTER_SECONDS = 2.5

_CONNECT_TIMEOUT_SECONDS = 30.0
_WINDOW_TIMEOUT_SECONDS = 20.0
_POLL_INTERVAL_SECONDS = 0.5
_TYPING_INTERVAL_SECONDS = 0.07


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
        """Bring up the cluster and start recording with the keycast overlay.

        Starts the sshd fixture, generates the config into the binary's own
        directory, wires the keystroke driver to the keycast overlay, launches
        cssh-rs, and begins recording the desktop.

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

        info = self._sshd.start_sshd(self._hosts)
        self._config_path = self._config_gen.generate_config(
            str(binary_path),
            str(binary_path.resolve().parent),
            info["ssh_config"],
            self._hosts,
            cluster_name=DEFAULT_CLUSTER,
        )

        keycast = Keycast()
        self._recorder.add_overlay(KeycastOverlay(keycast))
        self._keystrokes.add_key_listener(keycast.record)

        subprocess.Popen([str(binary_path), DEFAULT_CLUSTER])
        self._launched = True
        self._recorder.start_recording("cssh-rs", output_dir, fps=int(fps))

    def wait_for_hosts(self) -> None:
        """Wait until every demo host has an established ssh session, else raise ``DemoError``."""
        expected = len(self._hosts)
        deadline = time.monotonic() + _CONNECT_TIMEOUT_SECONDS
        while time.monotonic() < deadline:
            if self._sshd.count_connected_markers() >= expected:
                return
            time.sleep(_POLL_INTERVAL_SECONDS)
        raise DemoError(
            f"timed out after {_CONNECT_TIMEOUT_SECONDS}s waiting for "
            f"{expected} ssh sessions to connect"
        )

    def show_chapter(self, text: str, seconds: float = DEFAULT_CHAPTER_SECONDS) -> None:
        """Show a chapter title card and hold on it for its duration.

        Args:
            text: Caption drawn centered over the recording.
            seconds: How long the card stays on screen, in seconds.
        """
        seconds = float(seconds)
        self._recorder.show_banner(text, seconds)
        time.sleep(seconds)

    def focus_daemon(self) -> None:
        """Bring the cssh-rs daemon window to the foreground."""
        self._focus.focus_window(DAEMON_TITLE, timeout=_WINDOW_TIMEOUT_SECONDS)

    def broadcast(self, command: str) -> None:
        """Focus the daemon and broadcast ``command`` to every enabled client.

        Args:
            command: Command line typed into the daemon and run everywhere.
        """
        self.focus_daemon()
        self._keystrokes.type_line(command, interval=_TYPING_INTERVAL_SECONDS)

    def enter_control_mode(self) -> None:
        """Focus the daemon and enter control mode with Ctrl+A."""
        self.focus_daemon()
        self._keystrokes.send_hotkey("ctrl", "a")

    def press_key(self, key: str) -> None:
        """Press a named key such as ``t`` or ``n`` in the foreground window.

        Args:
            key: pyautogui key name to press.
        """
        self._keystrokes.press_key(key)

    def send_hotkey(self, *keys: str) -> None:
        """Press ``keys`` together as a chord, e.g. ``alt`` ``f4``.

        Args:
            keys: pyautogui key names to hold down in order and release.
        """
        self._keystrokes.send_hotkey(*keys)

    def hold(self, seconds: float) -> None:
        """Hold the current frame on screen for ``seconds``.

        Args:
            seconds: How long to pause, in seconds.
        """
        time.sleep(float(seconds))

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
            _kill_process_tree("cssh-rs.exe")
            self._launched = False
        self._sshd.stop_sshd()
        if self._config_path is not None:
            Path(self._config_path).unlink(missing_ok=True)
            self._config_path = None


def _kill_process_tree(image_name: str) -> None:
    """Terminate the cssh-rs process tree by image name; best-effort."""
    subprocess.run(
        ["taskkill", "/F", "/T", "/IM", image_name],
        capture_output=True,
        check=False,
    )
