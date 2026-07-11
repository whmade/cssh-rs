"""Record the screen to an MP4 per suite for the cssh-rs E2E suite.

This Robot Framework library captures the whole desktop while a suite runs so a
CI failure ships a video, not just logs. Recording is best-effort: a capture
backend that is missing or fails is logged and swallowed so it never aborts a
suite.
"""

from __future__ import annotations

import logging
import threading
import time
from pathlib import Path

LOGGER = logging.getLogger(__name__)

DEFAULT_FPS = 8
# mss monitor 0 is the virtual bounding box spanning every physical monitor.
DESKTOP_MONITOR = 0
_STOP_JOIN_TIMEOUT_SECONDS = 10.0


class ScreenRecorderError(RuntimeError):
    """Raised when a screen recording keyword is called with invalid arguments."""


class ScreenRecorder:
    """Robot Framework library that records the desktop to an MP4 per suite."""

    ROBOT_LIBRARY_SCOPE = "SUITE"
    ROBOT_LIBRARY_VERSION = "0.1.0"

    def __init__(self) -> None:
        self._thread: threading.Thread | None = None
        self._stop_event: threading.Event | None = None
        self._output_path: Path | None = None

    def start_recording(self, name: str, output_dir: str, fps: int = DEFAULT_FPS) -> str:
        """Start recording the desktop in the background; return the MP4 path.

        Args:
            name: File stem for the MP4, sanitized to filesystem-safe characters
                (pass the suite name for one video per suite).
            output_dir: Directory the MP4 is written into; created if absent.
            fps: Frames captured per second.

        Returns:
            Absolute path to the MP4 being written, as a str.
        """
        if self._thread is not None:
            raise ScreenRecorderError("a recording is already in progress")
        if not name:
            raise ScreenRecorderError("name must be a non-empty string")
        fps = int(fps)
        if fps <= 0:
            raise ScreenRecorderError(f"fps must be positive, got {fps}")

        directory = Path(output_dir)
        directory.mkdir(parents=True, exist_ok=True)
        output_path = (directory / f"{_safe_filename(name)}.mp4").resolve()

        stop_event = threading.Event()
        thread = threading.Thread(
            target=self._record,
            args=(output_path, fps, stop_event),
            name="cssh-rs-e2e-screen-recorder",
            daemon=True,
        )
        self._output_path = output_path
        self._stop_event = stop_event
        self._thread = thread
        thread.start()
        return str(output_path)

    def stop_recording(self) -> str | None:
        """Stop the in-progress recording and return its MP4 path.

        Signals the capture thread to stop and waits for it to finalize the
        file. Returns ``None`` when nothing is recording. If the thread does not
        stop within the join timeout, its state is kept so a later
        start_recording refuses to run a second recorder over a still-live one.

        Returns:
            Absolute path to the finalized MP4, or ``None`` if not recording or
            the capture thread did not stop in time.
        """
        thread = self._thread
        stop_event = self._stop_event
        output_path = self._output_path
        if thread is None or stop_event is None:
            return None
        stop_event.set()
        thread.join(_STOP_JOIN_TIMEOUT_SECONDS)
        if thread.is_alive():
            LOGGER.warning(
                "screen recorder thread did not stop within %ss", _STOP_JOIN_TIMEOUT_SECONDS
            )
            return None
        self._thread = None
        self._stop_event = None
        self._output_path = None
        return str(output_path)

    def _record(self, output_path: Path, fps: int, stop_event: threading.Event) -> None:
        """Capture frames until ``stop_event`` is set; best-effort.

        mss and the imageio writer are created here, in the worker thread: mss
        binds its device context to the creating thread, and owning the ffmpeg
        writer on the same thread avoids a cross-thread teardown race. Imports
        are local so the headless marker writer that imports the package does
        not pull in mss/imageio/numpy.
        """
        try:
            import imageio.v2 as imageio
            import mss
            import numpy as np
        except ImportError as exc:
            LOGGER.warning("screen recording disabled, backend import failed: %s", exc)
            return

        try:
            with (
                mss.mss() as sct,
                imageio.get_writer(str(output_path), fps=fps, codec="libx264") as writer,
            ):
                region = sct.monitors[DESKTOP_MONITOR]
                frame_interval = 1.0 / fps
                next_capture = time.monotonic()
                while not stop_event.is_set():
                    # mss grabs BGRA; reorder to the RGB imageio wants, dropping alpha.
                    frame = np.asarray(sct.grab(region))[..., [2, 1, 0]]
                    # get_writer's declared return type omits append_data.
                    writer.append_data(frame)  # pyrefly: ignore[missing-attribute]
                    next_capture += frame_interval
                    stop_event.wait(max(0.0, next_capture - time.monotonic()))
        except Exception as exc:
            # Broad by design: recording is best-effort and must never fail a suite.
            LOGGER.warning("screen recording failed for %s: %s", output_path, exc)


def _safe_filename(name: str) -> str:
    """Return ``name`` reduced to filesystem-safe characters for a file stem."""
    safe = "".join(ch if ch.isalnum() or ch in "-_." else "_" for ch in name.strip())
    return safe or "recording"
