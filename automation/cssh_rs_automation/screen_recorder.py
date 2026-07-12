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
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Callable

    from PIL.ImageDraw import ImageDraw
    from PIL.ImageFont import FreeTypeFont, ImageFont

    # Per-frame overlay: (RGB frame, monotonic capture time) -> frame to write.
    Overlay = Callable[[object, float], object]

LOGGER = logging.getLogger(__name__)

DEFAULT_FPS = 8
# mss monitor 0 is the virtual bounding box spanning every physical monitor.
DESKTOP_MONITOR = 0
_STOP_JOIN_TIMEOUT_SECONDS = 10.0
_START_WAIT_TIMEOUT_SECONDS = 10.0


class ScreenRecorderError(RuntimeError):
    """Raised when a screen recording keyword is called with invalid arguments."""


class ScreenRecorder:
    """Robot Framework library that records the desktop to an MP4 per suite."""

    ROBOT_LIBRARY_SCOPE = "SUITE"
    ROBOT_LIBRARY_VERSION = "0.1.0"

    def __init__(self) -> None:
        self._thread: threading.Thread | None = None
        self._stop_event: threading.Event | None = None
        self._started_event: threading.Event | None = None
        self._output_path: Path | None = None
        self._banner_lock = threading.Lock()
        self._banner_text: str | None = None
        self._banner_until = 0.0
        # Applied in order per frame; banner first so overlays composite over it.
        self._overlays: list[Overlay] = [self._banner_overlay]

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
        started_event = threading.Event()
        thread = threading.Thread(
            target=self._record,
            args=(output_path, fps, stop_event, started_event),
            name="cssh-rs-automation-screen-recorder",
            daemon=True,
        )
        self._output_path = output_path
        self._stop_event = stop_event
        self._started_event = started_event
        self._thread = thread
        thread.start()
        return str(output_path)

    def wait_until_recording(self, timeout: float = _START_WAIT_TIMEOUT_SECONDS) -> bool:
        """Block until the capture thread is live; return whether it started in time.

        The backend (mss, ffmpeg writer) is created on the worker thread, so
        start_recording returns before capture begins; callers wait on this to
        avoid launching what they want recorded during that warm-up.

        Args:
            timeout: Seconds to wait for capture to start.

        Returns:
            ``True`` once capture is live, ``False`` on timeout or if not started.
        """
        if self._started_event is None:
            return False
        return self._started_event.wait(timeout)

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
        self._started_event = None
        self._output_path = None
        return str(output_path)

    def show_banner(self, text: str, seconds: float) -> None:
        """Display ``text`` centered over the next ``seconds`` of captured frames.

        Called from the Robot execution thread; the capture thread reads the
        banner under a lock and draws it until the deadline passes.

        Args:
            text: Banner caption to render (typically the test name).
            seconds: How long the banner stays visible, in seconds.
        """
        with self._banner_lock:
            self._banner_text = text
            self._banner_until = time.monotonic() + seconds

    def add_overlay(self, overlay: Overlay) -> None:
        """Register a per-frame overlay, run after the built-in banner.

        Args:
            overlay: Callable ``(frame, now) -> frame`` given the RGB uint8 frame
                and its monotonic capture time; returns the frame to write.
        """
        self._overlays.append(overlay)

    def _current_banner(self) -> str | None:
        """Return the banner text while its deadline is live, else ``None``."""
        with self._banner_lock:
            if self._banner_text is not None and time.monotonic() < self._banner_until:
                return self._banner_text
            return None

    def _banner_overlay(self, frame: object, _now: float) -> object:
        """Built-in overlay: draw the active banner over ``frame``, if any."""
        banner = self._current_banner()
        if banner is not None:
            return _draw_banner(frame, banner)
        return frame

    def _record(
        self,
        output_path: Path,
        fps: int,
        stop_event: threading.Event,
        started_event: threading.Event,
    ) -> None:
        """Capture frames until ``stop_event`` is set; best-effort.

        mss and the imageio writer are created here, in the worker thread: mss
        binds its device context to the creating thread, and owning the ffmpeg
        writer on the same thread avoids a cross-thread teardown race. Imports
        are local so the headless marker writer that imports the package does
        not pull in mss/imageio/numpy. ``started_event`` fires once capture is
        live (or on any exit) so waiters never hang.
        """
        try:
            try:
                import imageio.v2 as imageio
                import mss
                import numpy as np
            except ImportError as exc:
                LOGGER.warning("screen recording disabled, backend import failed: %s", exc)
                return

            with (
                mss.mss() as sct,
                imageio.get_writer(str(output_path), fps=fps, codec="libx264") as writer,
            ):
                region = sct.monitors[DESKTOP_MONITOR]
                frame_interval = 1.0 / fps
                next_capture = time.monotonic()
                started_event.set()
                while not stop_event.is_set():
                    # mss grabs BGRA; reorder to the RGB imageio wants, dropping alpha.
                    frame = np.asarray(sct.grab(region))[..., [2, 1, 0]]
                    now = time.monotonic()
                    for overlay in self._overlays:
                        try:
                            frame = overlay(frame, now)
                        except Exception as exc:
                            # Broad by design: one overlay must not abort recording.
                            LOGGER.warning("overlay failed, skipped this frame: %s", exc)
                    # get_writer's declared return type omits append_data.
                    writer.append_data(frame)  # pyrefly: ignore[missing-attribute]
                    next_capture += frame_interval
                    stop_event.wait(max(0.0, next_capture - time.monotonic()))
        except Exception as exc:
            # Broad by design: recording is best-effort and must never fail a suite.
            LOGGER.warning("screen recording failed for %s: %s", output_path, exc)
        finally:
            # Never leave a start waiter hanging if capture never reached its loop.
            started_event.set()


def _safe_filename(name: str) -> str:
    """Return ``name`` reduced to filesystem-safe characters for a file stem."""
    safe = "".join(ch if ch.isalnum() or ch in "-_." else "_" for ch in name.strip())
    return safe or "recording"


def _draw_banner(frame: object, text: str) -> object:
    """Return ``frame`` dimmed with ``text`` drawn centered, as a title card.

    Args:
        frame: RGB uint8 numpy array of shape ``(height, width, 3)``.
        text: Caption to center on the dimmed frame.

    Returns:
        A new RGB uint8 numpy array of the same shape.
    """
    import numpy as np
    from PIL import Image, ImageDraw

    image = Image.fromarray(np.asarray(frame))
    dimmed = Image.blend(image, Image.new("RGB", image.size, (0, 0, 0)), 0.6)
    draw = ImageDraw.Draw(dimmed)
    width, height = dimmed.size
    font = _banner_font(max(10, int(height / 15 * 0.8)))
    wrapped = _wrap_text(draw, text, font, int(width * 0.9))
    draw.multiline_text(
        (width // 2, height // 2),
        wrapped,
        fill=(255, 255, 255),
        font=font,
        align="center",
        anchor="mm",
    )
    return np.asarray(dimmed)


def _wrap_text(draw: ImageDraw, text: str, font: FreeTypeFont | ImageFont, max_width: int) -> str:
    """Return ``text`` with newlines inserted so each line fits within ``max_width``.

    Args:
        draw: Drawing context used to measure rendered line widths.
        text: Caption to wrap on whitespace boundaries.
        font: Font the caption is rendered with.
        max_width: Maximum rendered line width in pixels.

    Returns:
        The caption with ``\\n`` between wrapped lines. A single word wider than
        ``max_width`` is left on its own line rather than broken mid-word.
    """
    words = text.split()
    if not words:
        return text
    lines = [words[0]]
    for word in words[1:]:
        candidate = f"{lines[-1]} {word}"
        if draw.textlength(candidate, font=font) <= max_width:
            lines[-1] = candidate
        else:
            lines.append(word)
    return "\n".join(lines)


def _banner_font(size: int) -> FreeTypeFont | ImageFont:
    """Return a TrueType banner font of ``size``, falling back to the bundled default."""
    from PIL import ImageFont

    try:
        return ImageFont.truetype("arial.ttf", size)
    except OSError:
        return ImageFont.load_default(size)
