"""On-screen keypress overlay for screen recordings.

A ``Keycast`` buffers the labels the keystroke driver emits; the paired
``KeycastOverlay`` draws the still-visible ones in a corner as a
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
        self._events: deque[tuple[str, str, float]] = deque(maxlen=max_labels)

    def record(self, label: str, kind: str = "text") -> None:
        """Key-listener callback: buffer ``label`` stamped with the monotonic time.

        Consecutive ``"text"`` events merge into one growing token so typed
        characters read as continuous text; a ``"key"`` event (a named key or
        chord) always starts a new token so it stays distinct.

        Args:
            label: Display label of a delivered action (e.g. ``"e"`` or ``"Ctrl+A"``).
            kind: ``"text"`` for a literal typed character, ``"key"`` for a named
                key or chord.
        """
        with self._lock:
            now = time.monotonic()
            if (
                kind == "text"
                and self._events
                and self._events[-1][1] == "text"
                and now - self._events[-1][2] <= self._fade_seconds
            ):
                self._events[-1] = (self._events[-1][0] + label, "text", now)
            else:
                self._events.append((label, kind, now))

    def clear(self) -> None:
        """Drop all buffered labels so a new recording starts clean."""
        with self._lock:
            self._events.clear()

    def active(self, now: float) -> list[tuple[str, str]]:
        """Return the ``(label, kind)`` tokens still within the fade window, oldest first.

        Args:
            now: Monotonic timestamp of the frame being rendered.

        Returns:
            The unexpired tokens as ``(label, kind)`` pairs.
        """
        with self._lock:
            return [
                (label, kind)
                for label, kind, stamped in self._events
                if now - stamped <= self._fade_seconds
            ]


class KeycastOverlay:
    """Screen-recorder overlay that draws a ``Keycast``'s active labels."""

    def __init__(self, keycast: Keycast) -> None:
        self._keycast = keycast

    def __call__(self, frame: object, now: float) -> object:
        """Return ``frame`` with the active key tokens drawn, or unchanged if none."""
        events = self._keycast.active(now)
        if not events:
            return frame
        return _draw_keycast(frame, events)


def _keycast_text(events: list[tuple[str, str]]) -> str:
    """Return the overlay line for ``events``.

    Text tokens render verbatim; key tokens are upper-cased so named keys stand
    out. Tokens join with a single space, so a literal space inside a text token
    is the only thing that ever separates typed characters.

    Args:
        events: ``(label, kind)`` tokens, oldest first.

    Returns:
        The single-line overlay string.
    """
    return " ".join(label.upper() if kind == "key" else label for label, kind in events)


def _draw_keycast(frame: object, events: list[tuple[str, str]]) -> object:
    """Return ``frame`` with ``events`` drawn as a pill in the bottom-right corner.

    Args:
        frame: RGB uint8 numpy array of shape ``(height, width, 3)``.
        events: ``(label, kind)`` tokens to render, oldest first, on one line.

    Returns:
        A new RGB uint8 numpy array of the same shape.
    """
    import numpy as np
    from PIL import Image, ImageDraw

    image = Image.fromarray(np.asarray(frame))
    width, height = image.size
    font = _keycast_font(max(12, int(height / 28)))
    text = _keycast_text(events)

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
