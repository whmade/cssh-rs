"""Shared desktop-automation library for cssh-rs, used by the E2E suites and the demo recorder."""

from cssh_rs_automation.clipboard import Clipboard, ClipboardError
from cssh_rs_automation.config_gen import ConfigGen, ConfigGenError
from cssh_rs_automation.keystrokes import Keystrokes, KeystrokesError
from cssh_rs_automation.screen_recorder import ScreenRecorder, ScreenRecorderError
from cssh_rs_automation.sshd_fixture import SshdFixture, SshdFixtureError
from cssh_rs_automation.window_focus import WindowFocus, WindowFocusError

__all__ = [
    "Clipboard",
    "ClipboardError",
    "ConfigGen",
    "ConfigGenError",
    "Keystrokes",
    "KeystrokesError",
    "ScreenRecorder",
    "ScreenRecorderError",
    "SshdFixture",
    "SshdFixtureError",
    "WindowFocus",
    "WindowFocusError",
]
