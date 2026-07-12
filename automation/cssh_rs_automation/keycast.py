"""On-screen keypress overlay for screen recordings.

A :class:`Keycast` buffers the labels the keystroke driver emits; the paired
:class:`KeycastOverlay` draws the still-visible ones in a corner as a
screen-recorder per-frame overlay. Both use ``time.monotonic``, the clock the
recorder stamps frames with, so labels fade in step with the video.
"""

from __future__ import annotations

import threading
import time
from collections import deque
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from PIL.ImageFont import FreeTypeFont, ImageFont

DEFAULT_FADE_SECONDS = 2.0
DEFAULT_MAX_LABELS = 4


class Keycast:
    """Thread-safe buffer of recent key labels with a fade window."""

    def __init__(
        self, fade_seconds: float = DEFAULT_FADE_SECONDS, max_labels: int = DEFAULT_MAX_LABELS
    ) -> None:
        self._fade_seconds = fade_seconds
        self._lock = threading.Lock()
        self._events: deque[tuple[str, float]] = deque(maxlen=max_labels)

    def record(self, label: str) -> None:
        """Key-listener callback: stamp ``label`` with the current monotonic time.

        Args:
            label: Display label of a delivered keystroke (e.g. ``"Ctrl+A"``).
        """
        with self._lock:
            self._events.append((label, time.monotonic()))

    def active(self, now: float) -> list[str]:
        """Return the labels still within the fade window at ``now``, oldest first.

        Args:
            now: Monotonic timestamp of the frame being rendered.

        Returns:
            The unexpired labels.
        """
        with self._lock:
            return [label for label, stamped in self._events if now - stamped <= self._fade_seconds]


class KeycastOverlay:
    """Screen-recorder overlay that draws a :class:`Keycast`'s active labels."""

    def __init__(self, keycast: Keycast) -> None:
        self._keycast = keycast

    def __call__(self, frame: object, now: float) -> object:
        """Return ``frame`` with the active key labels drawn, or unchanged if none."""
        labels = self._keycast.active(now)
        if not labels:
            return frame
        return _draw_keycast(frame, labels)


def _draw_keycast(frame: object, labels: list[str]) -> object:
    """Return ``frame`` with ``labels`` drawn as a pill in the bottom-right corner.

    Args:
        frame: RGB uint8 numpy array of shape ``(height, width, 3)``.
        labels: Key labels to render, oldest first, joined on one line.

    Returns:
        A new RGB uint8 numpy array of the same shape.
    """
    import numpy as np
    from PIL import Image, ImageDraw

    image = Image.fromarray(np.asarray(frame))
    width, height = image.size
    font = _keycast_font(max(12, int(height / 28)))
    text = "   ".join(labels)

    draw = ImageDraw.Draw(image, "RGBA")
    left, top, right, bottom = draw.textbbox((0, 0), text, font=font)
    text_width = right - left
    text_height = bottom - top
    pad = max(8, text_height // 2)
    box_width = text_width + 2 * pad
    box_height = text_height + 2 * pad
    x0 = width - box_width - pad
    y0 = height - box_height - pad

    draw.rounded_rectangle(
        (x0, y0, x0 + box_width, y0 + box_height), radius=pad, fill=(0, 0, 0, 165)
    )
    # Offset by the bbox origin so glyphs with negative bearings sit inside the pill.
    draw.text((x0 + pad - left, y0 + pad - top), text, fill=(255, 255, 255, 255), font=font)
    return np.asarray(image)


def _keycast_font(size: int) -> FreeTypeFont | ImageFont:
    """Return a monospace overlay font of ``size``, falling back to the bundled default."""
    from PIL import ImageFont

    for candidate in ("consola.ttf", "cour.ttf", "DejaVuSansMono.ttf"):
        try:
            return ImageFont.truetype(candidate, size)
        except OSError:
            continue
    return ImageFont.load_default(size)
