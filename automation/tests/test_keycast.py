"""Unit tests for the keycast overlay.

The fade clock (``time.monotonic``) used when recording labels is monkeypatched
so the fade window is exercised deterministically; ``active`` takes the frame
time as an argument and needs no patching.
"""

from __future__ import annotations

import numpy as np
import pytest

from cssh_rs_automation.keycast import Keycast, KeycastOverlay, _draw_keycast


def test_active_empty_when_nothing_recorded() -> None:
    assert Keycast().active(0.0) == []


def test_active_filters_by_fade_window(monkeypatch: pytest.MonkeyPatch) -> None:
    now = {"t": 100.0}
    monkeypatch.setattr("cssh_rs_automation.keycast.time.monotonic", lambda: now["t"])
    keycast = Keycast(fade_seconds=2.0, max_labels=4)

    keycast.record("A")  # stamped at 100.0
    now["t"] = 101.0
    keycast.record("B")  # stamped at 101.0

    assert keycast.active(101.5) == ["A", "B"]
    # A is now 2.5s old (past the 2.0s fade); B at 1.5s survives.
    assert keycast.active(102.5) == ["B"]


def test_active_keeps_only_the_last_max_labels(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("cssh_rs_automation.keycast.time.monotonic", lambda: 0.0)
    keycast = Keycast(fade_seconds=100.0, max_labels=2)

    keycast.record("A")
    keycast.record("B")
    keycast.record("C")

    assert keycast.active(0.0) == ["B", "C"]


def test_overlay_passes_frame_through_when_idle() -> None:
    frame = np.full((10, 10, 3), 5, dtype=np.uint8)

    assert KeycastOverlay(Keycast())(frame, 0.0) is frame


def test_overlay_draws_active_labels(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("cssh_rs_automation.keycast.time.monotonic", lambda: 5.0)
    keycast = Keycast(fade_seconds=2.0)
    keycast.record("Enter")
    frame = np.full((60, 120, 3), 20, dtype=np.uint8)

    drawn = np.asarray(KeycastOverlay(keycast)(frame, 5.0))

    assert drawn.shape == frame.shape
    assert not np.array_equal(drawn, frame)


def test_draw_keycast_preserves_shape_and_changes_pixels() -> None:
    frame = np.full((120, 240, 3), 30, dtype=np.uint8)

    drawn = np.asarray(_draw_keycast(frame, ["Ctrl+A", "Enter"]))

    assert drawn.shape == frame.shape
    assert drawn.dtype == frame.dtype
    assert not np.array_equal(drawn, frame)
