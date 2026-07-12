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

from cssh_rs_automation.clipboard import Clipboard
from cssh_rs_automation.config_gen import ConfigGen
from cssh_rs_automation.keycast import Keycast, KeycastOverlay
from cssh_rs_automation.keystrokes import Keystrokes
from cssh_rs_automation.screen_recorder import ScreenRecorder
from cssh_rs_automation.sshd_fixture import SshdFixture
from cssh_rs_automation.window_focus import WindowFocus

from cssh_rs_demo.gif_export import export_gif

if TYPE_CHECKING:
    from collections.abc import Callable

DAEMON_TITLE = "cssh-rs daemon"
DEFAULT_CLUSTER = "demo"
DEFAULT_FPS = 10

# Two prod and two dev hosts. The second dev host joins live via control mode,
# and only the first dev host ships the README the demo propagates to the rest.
HOSTS = ("hosta.prod", "hostb.prod", "hosta.dev", "hostb.dev")
LATE_HOST = "hostb.dev"
INITIAL_HOSTS = tuple(host for host in HOSTS if host != LATE_HOST)
README_HOST = "hosta.dev"
README_NAME = "README"
README_CONTENT = "This README is valid for dev and prod clusters"
SHELL_RC_LINES = "alias ll='ls -alF'"

# Shown in the client window titles (root@host); the real login is forced back
# to the fixture's account with `ssh -o User=`, so the title reads clean.
DISPLAY_USER = "root"

_CONNECT_TIMEOUT_SECONDS = 30.0
_WINDOW_TIMEOUT_SECONDS = 20.0
_POLL_INTERVAL_SECONDS = 0.5
# Per-character typing; discrete keys pause longer so the overlay is readable.
_TYPING_INTERVAL_SECONDS = 0.05
_KEY_INTERVAL_SECONDS = 0.2
# Linger on the control-mode menu before the action key, as a human would.
_CONTROL_MODE_PAUSE_SECONDS = 0.7


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
        clipboard: Clipboard | None = None,
        gif_exporter: Callable[..., str] | None = None,
    ) -> None:
        self._sshd = sshd or SshdFixture()
        self._recorder = recorder or ScreenRecorder()
        self._keystrokes = keystrokes or Keystrokes()
        self._focus = focus or WindowFocus()
        self._config_gen = config_gen or ConfigGen()
        self._clipboard = clipboard or Clipboard()
        self._export_gif = gif_exporter or export_gif
        self._config_path: str | None = None
        self._launched = False

    def start_demo(self, binary: str, output_dir: str, fps: int = DEFAULT_FPS) -> None:
        """Bring up the cluster in interactive-shell mode and start recording.

        Serves every host but ``LATE_HOST`` as a client, seeds each host's
        ``demo/data`` (only ``README_HOST`` gets the README), wires the keycast
        overlay, launches cssh-rs and begins recording.

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
        _require_vim()

        info = self._sshd.start_sshd(HOSTS, interactive=True, rc_lines=SHELL_RC_LINES)
        self._seed_host_files(cast("dict[str, str]", info["homes"]))
        self._config_path = self._config_gen.generate_config(
            str(binary_path),
            str(binary_path.resolve().parent),
            str(info["ssh_config"]),
            INITIAL_HOSTS,
            cluster_name=DEFAULT_CLUSTER,
            # Titles show root (from -u root); force the real login back here.
            extra_ssh_args=["-o", f"User={getpass.getuser()}"],
        )

        keycast = Keycast()
        self._recorder.add_overlay(KeycastOverlay(keycast))
        self._keystrokes.add_key_listener(keycast.record)

        # Wait for capture to be live before launching so the clip catches the
        # windows arranging rather than opening on a cluster already in place.
        self._recorder.start_recording("cssh-rs", output_dir, fps=int(fps))
        self._recorder.wait_until_recording()
        subprocess.Popen([str(binary_path), "-u", DISPLAY_USER, DEFAULT_CLUSTER])
        self._launched = True

    def wait_for_hosts(self) -> None:
        """Wait until the initial ssh sessions are ready, else raise ``DemoError``."""
        self._wait_for_sessions(len(INITIAL_HOSTS))

    def add_host(self, alias: str) -> None:
        """Add ``alias`` as a client, wait for its session, then clear the resize swallow.

        Adding a host resizes the other client consoles, and conhost drops the
        first byte each then receives (a discrete swallow, not a settling delay).
        A Backspace - one keystroke and one byte, so nothing leaks however much
        is dropped, and a no-op at the prompt - is broadcast off the overlay to
        absorb it so the next real command lands in full.

        Args:
            alias: Host alias to launch, resolved via the generated ssh_config.
        """
        connected = self._sshd.count_connected_markers()
        self.enter_control_mode()
        self._press("c")
        self._type_command(alias)
        self._wait_for_sessions(connected + 1)
        self.focus_daemon()
        self._keystrokes.press_key("backspace", notify=False)

    def broadcast(self, command: str) -> None:
        """Focus the daemon and broadcast ``command`` to every enabled client.

        Args:
            command: Command line typed into the daemon and run everywhere.
        """
        self.focus_daemon()
        self._type_command(command)

    def focus_client(self, alias: str) -> None:
        """Focus the client window for ``alias`` so input reaches only that host.

        Args:
            alias: Host alias whose client window is brought to the foreground.
        """
        self._focus.focus_window(
            f"@{alias}", match_mode="substring", timeout=_WINDOW_TIMEOUT_SECONDS
        )

    def type_command(self, command: str) -> None:
        """Type ``command`` into the currently focused window only.

        Args:
            command: Command line typed into whichever window is focused.
        """
        self._type_command(command)

    def copy_readme(self) -> None:
        """Copy the README text to the clipboard, as if from the console output."""
        self._clipboard.set_clipboard(README_CONTENT)

    def edit_readme(self) -> None:
        """Open README in vim, paste the clipboard, save and quit."""
        self.focus_daemon()
        self._type_command(f"vim {README_NAME}")
        self._press("i")
        self._paste()
        self._press("esc")
        self._type_command(":wq")

    def disable_client(self, *moves: str) -> None:
        """Disable one client via the submenu: open it, navigate ``moves``, press [d].

        Args:
            moves: Arrow keys stepping the submenu selection from the top-left
                cell to the target client before it is disabled.
        """
        self.enter_control_mode()
        self._press("e")
        for move in moves:
            self._press(move)
        self._press("d")
        self._press("esc")

    def enable_all(self) -> None:
        """Enable every client with control mode's top-level [n]."""
        self.enter_control_mode()
        self._press("n")

    def enter_control_mode(self) -> None:
        """Focus the daemon, press Ctrl+A, then pause on the control-mode menu."""
        self.focus_daemon()
        self._hotkey("ctrl", "a")
        time.sleep(_CONTROL_MODE_PAUSE_SECONDS)

    def focus_daemon(self) -> None:
        """Bring the cssh-rs daemon window to the foreground."""
        self._focus.focus_window(DAEMON_TITLE, timeout=_WINDOW_TIMEOUT_SECONDS)

    def interrupt(self) -> None:
        """Broadcast Ctrl+C from the daemon to abort the running remote command."""
        self.focus_daemon()
        self._hotkey("ctrl", "c")

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

    def _wait_for_sessions(self, expected: int) -> None:
        """Wait until at least ``expected`` interactive sessions signal readiness.

        Each session's bash rc writes its marker once it can take input, so this
        blocks until the shells are ready rather than racing the first keystroke.
        """
        deadline = time.monotonic() + _CONNECT_TIMEOUT_SECONDS
        while time.monotonic() < deadline:
            if self._sshd.count_connected_markers() >= expected:
                return
            time.sleep(_POLL_INTERVAL_SECONDS)
        raise DemoError(
            f"timed out after {_CONNECT_TIMEOUT_SECONDS}s waiting for {expected} ssh sessions"
        )

    def _type_command(self, command: str) -> None:
        """Type ``command`` char by char so the overlay reveals each key, then Enter."""
        self._type(command)
        self._press("enter")

    def _type(self, text: str) -> None:
        """Type ``text`` one character at a time with the typing delay; no Enter."""
        for char in text:
            self._keystrokes.type_text(char)
            time.sleep(_TYPING_INTERVAL_SECONDS)

    def _press(self, key: str) -> None:
        """Press a named key, then pause so the overlay stays readable."""
        self._keystrokes.press_key(key)
        time.sleep(_KEY_INTERVAL_SECONDS)

    def _hotkey(self, *keys: str) -> None:
        """Press a chord, then pause so the overlay stays readable."""
        self._keystrokes.send_hotkey(*keys)
        time.sleep(_KEY_INTERVAL_SECONDS)

    def _paste(self) -> None:
        """Type the clipboard contents at once; the overlay shows PASTE, not the text."""
        self._keystrokes.type_text(self._clipboard.get_clipboard(), label="PASTE")

    def _seed_host_files(self, homes: dict[str, str]) -> None:
        """Create ``demo/data`` in every host home, with the README only on ``README_HOST``."""
        for alias, home in homes.items():
            data_dir = Path(home) / "demo" / "data"
            data_dir.mkdir(parents=True)
            if alias == README_HOST:
                readme = data_dir / README_NAME
                readme.write_text(f"{README_CONTENT}\n", encoding="utf-8", newline="\n")


def _require_vim() -> None:
    """Fail loudly if the interactive bash has no vim, which the demo's edit step needs."""
    try:
        found = subprocess.run(["bash", "-lc", "command -v vim"], capture_output=True, check=False)
    except OSError as exc:
        raise DemoError("the demo needs bash on PATH; install Git for Windows") from exc
    if found.returncode != 0:
        raise DemoError("the demo needs vim in the interactive bash; Git for Windows ships it")
