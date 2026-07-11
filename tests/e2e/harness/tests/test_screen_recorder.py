"""Unit tests for the screen recorder library.

Fake ``mss`` and ``imageio`` modules are injected into ``sys.modules`` so the
tests never launch real screen capture or ffmpeg; numpy is real, so the
BGRA->RGB frame conversion is exercised for real.
"""

from __future__ import annotations

import sys
import threading
import time
import types
from collections.abc import Callable
from pathlib import Path

import numpy as np
import pytest

from cssh_rs_e2e.screen_recorder import ScreenRecorder, ScreenRecorderError, _safe_filename


class _FakeSct:
    def __init__(self, frame: np.ndarray) -> None:
        self._frame = frame
        self.monitors = [{"left": 0, "top": 0, "width": 2, "height": 2}]

    def __enter__(self) -> _FakeSct:
        return self

    def __exit__(self, *_exc: object) -> bool:
        return False

    def grab(self, _region: object) -> np.ndarray:
        return self._frame


class _FakeWriter:
    def __init__(self, path: str, **kwargs: object) -> None:
        self.path = path
        self.kwargs = kwargs
        self.frames: list[np.ndarray] = []
        self.closed = False
        self.first_frame = threading.Event()

    def __enter__(self) -> _FakeWriter:
        return self

    def __exit__(self, *_exc: object) -> bool:
        self.closed = True
        return False

    def append_data(self, frame: np.ndarray) -> None:
        self.frames.append(frame)
        self.first_frame.set()


class _Backends(types.SimpleNamespace):
    sct: _FakeSct
    writers: list[_FakeWriter]
    frame: np.ndarray


@pytest.fixture
def fake_backends(monkeypatch: pytest.MonkeyPatch) -> _Backends:
    frame = np.zeros((2, 2, 4), dtype=np.uint8)
    frame[0, 0] = [1, 2, 3, 255]  # BGRA -> expected RGB [3, 2, 1]
    sct = _FakeSct(frame)
    writers: list[_FakeWriter] = []

    def get_writer(path: str, **kwargs: object) -> _FakeWriter:
        writer = _FakeWriter(path, **kwargs)
        writers.append(writer)
        return writer

    fake_imageio_v2 = types.SimpleNamespace(get_writer=get_writer)
    fake_imageio = types.ModuleType("imageio")
    fake_imageio.v2 = fake_imageio_v2

    monkeypatch.setitem(sys.modules, "mss", types.SimpleNamespace(mss=lambda: sct))
    monkeypatch.setitem(sys.modules, "imageio", fake_imageio)
    monkeypatch.setitem(sys.modules, "imageio.v2", fake_imageio_v2)
    return _Backends(sct=sct, writers=writers, frame=frame)


def _await(predicate: Callable[[], object], timeout: float = 3.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.01)
    return False


def test_start_recording_writes_converted_frames(fake_backends: _Backends, tmp_path: Path) -> None:
    recorder = ScreenRecorder()
    recordings = tmp_path / "recordings"

    path = recorder.start_recording("Cluster E2E", str(recordings), fps=100)
    try:
        assert path == str((recordings / "Cluster_E2E.mp4").resolve())
        assert recordings.is_dir()
        assert _await(lambda: bool(fake_backends.writers))
        assert fake_backends.writers[0].first_frame.wait(3.0)
    finally:
        returned = recorder.stop_recording()

    writer = fake_backends.writers[0]
    assert returned == path
    assert writer.closed is True
    assert writer.kwargs == {"fps": 100, "codec": "libx264"}
    assert len(writer.frames) >= 1
    assert list(writer.frames[0][0, 0]) == [3, 2, 1]


def test_stop_recording_without_start_returns_none() -> None:
    assert ScreenRecorder().stop_recording() is None


@pytest.mark.usefixtures("fake_backends")
def test_start_recording_twice_raises(tmp_path: Path) -> None:
    recorder = ScreenRecorder()
    recorder.start_recording("suite", str(tmp_path), fps=100)
    try:
        with pytest.raises(ScreenRecorderError, match="already in progress"):
            recorder.start_recording("suite", str(tmp_path), fps=100)
    finally:
        recorder.stop_recording()


def test_start_recording_rejects_empty_name(tmp_path: Path) -> None:
    with pytest.raises(ScreenRecorderError, match="non-empty"):
        ScreenRecorder().start_recording("", str(tmp_path))


def test_start_recording_rejects_non_positive_fps(tmp_path: Path) -> None:
    with pytest.raises(ScreenRecorderError, match="fps must be positive"):
        ScreenRecorder().start_recording("suite", str(tmp_path), fps=0)


def test_recording_survives_capture_backend_error(
    fake_backends: _Backends, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    def boom() -> object:
        raise RuntimeError("no display")

    monkeypatch.setitem(sys.modules, "mss", types.SimpleNamespace(mss=boom))
    recorder = ScreenRecorder()

    path = recorder.start_recording("suite", str(tmp_path), fps=100)
    returned = recorder.stop_recording()

    assert returned == path
    assert fake_backends.writers == []


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("Cluster E2E", "Cluster_E2E"),
        ("control_mode_add_host", "control_mode_add_host"),
        ("a/b:c*d", "a_b_c_d"),
        ("  spaced  ", "spaced"),
        ("v1.2-final", "v1.2-final"),
        ("", "recording"),
        ("   ", "recording"),
    ],
)
def test_safe_filename(raw: str, expected: str) -> None:
    assert _safe_filename(raw) == expected
