"""Robot Framework listener that records every E2E suite to an MP4.

Registered once with ``robot --listener``, it records every suite - present and
future - with no per-suite Suite Setup/Teardown opt-in. Recording is best-effort:
a capture failure is logged and swallowed so it can never abort a suite.
"""

from __future__ import annotations

import logging
import time

from robot.result import TestCase as ResultTest
from robot.result import TestSuite as ResultSuite
from robot.running import TestCase as RunningTest
from robot.running import TestSuite as RunningSuite

from cssh_rs_e2e.screen_recorder import ScreenRecorder

LOGGER = logging.getLogger(__name__)

DEFAULT_OUTPUT_DIR = "e2e-recordings"
DEFAULT_BANNER_SECONDS = 1.0


class RecordingListener:
    """Record each leaf suite to an MP4 named after the suite.

    Args:
        output_dir: Directory the per-suite MP4s are written into; defaults to
            ``e2e-recordings`` beside the Robot output dir so it stays out of
            the report artifact.
        banner_seconds: Seconds each test-name title card is shown before the
            test body runs, marking the boundary between tests in the video.
    """

    ROBOT_LISTENER_API_VERSION = 3

    def __init__(
        self,
        output_dir: str = DEFAULT_OUTPUT_DIR,
        banner_seconds: float = DEFAULT_BANNER_SECONDS,
    ) -> None:
        self._output_dir = output_dir
        self._banner_seconds = float(banner_seconds)
        self._recorder = ScreenRecorder()

    def start_suite(self, data: RunningSuite, _result: ResultSuite) -> None:
        """Start recording when the suite holds tests.

        Only leaf suites do; skipping the parent directory suite avoids one
        recording spanning every child suite.
        """
        if not data.tests:
            return
        try:
            self._recorder.start_recording(data.name, self._output_dir)
        except Exception as exc:
            # Broad by design: recording is best-effort and must never fail a suite.
            LOGGER.warning("could not start recording for %s: %s", data.name, exc)

    def start_test(self, data: RunningTest, _result: ResultTest) -> None:
        """Flash the test name as a title card before the test body runs.

        The pause happens on the Robot execution thread, so the capture thread
        records the dimmed name card over the idle desktop; the banner then
        expires on its own once the test proceeds.
        """
        try:
            self._recorder.show_banner(data.name, self._banner_seconds)
            time.sleep(self._banner_seconds)
        except Exception as exc:
            # Broad by design: a banner error must never abort a test.
            LOGGER.warning("could not show test banner for %s: %s", data.name, exc)

    def end_suite(self, data: RunningSuite, _result: ResultSuite) -> None:
        """Stop the leaf suite's recording."""
        if not data.tests:
            return
        try:
            self._recorder.stop_recording()
        except Exception as exc:
            # Broad by design: a recorder error must not mask a real suite result.
            LOGGER.warning("could not stop recording for %s: %s", data.name, exc)
