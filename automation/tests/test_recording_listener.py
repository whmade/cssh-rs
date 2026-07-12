"""Unit tests for the recording listener.

The ScreenRecorder is replaced with a fake that records its calls so the tests
assert the listener's leaf-suite gating and best-effort behaviour without
launching real screen capture.
"""

from __future__ import annotations

import time
import types

import pytest

from cssh_rs_automation import recording_listener
from cssh_rs_automation.keystrokes import Keystrokes
from cssh_rs_automation.recording_listener import (
    DEFAULT_BANNER_SECONDS,
    DEFAULT_OUTPUT_DIR,
    RecordingListener,
)


class _FakeRecorder:
    def __init__(self) -> None:
        self.started: list[tuple[str, str]] = []
        self.stopped = 0
        self.banners: list[tuple[str, float]] = []
        self.overlays: list[object] = []
        self.start_error: Exception | None = None
        self.stop_error: Exception | None = None
        self.banner_error: Exception | None = None

    def add_overlay(self, overlay: object) -> None:
        self.overlays.append(overlay)

    def start_recording(self, name: str, output_dir: str) -> str:
        self.started.append((name, output_dir))
        if self.start_error is not None:
            raise self.start_error
        return f"{output_dir}/{name}.mp4"

    def stop_recording(self) -> str | None:
        self.stopped += 1
        if self.stop_error is not None:
            raise self.stop_error
        return None

    def show_banner(self, text: str, seconds: float) -> None:
        self.banners.append((text, seconds))
        if self.banner_error is not None:
            raise self.banner_error


def _suite(name: str, *, has_tests: bool) -> types.SimpleNamespace:
    return types.SimpleNamespace(name=name, tests=[object()] if has_tests else [])


def _test(name: str) -> types.SimpleNamespace:
    return types.SimpleNamespace(name=name)


def _keyword(instance: object) -> types.SimpleNamespace:
    return types.SimpleNamespace(owner=types.SimpleNamespace(instance=instance))


def _muted_keystrokes() -> Keystrokes:
    keystrokes = Keystrokes()
    keystrokes._pyautogui = lambda: types.SimpleNamespace(write=lambda *_args, **_kwargs: None)
    return keystrokes


@pytest.fixture
def recorder(monkeypatch: pytest.MonkeyPatch) -> _FakeRecorder:
    fake = _FakeRecorder()
    monkeypatch.setattr(recording_listener, "ScreenRecorder", lambda: fake)
    return fake


@pytest.fixture(autouse=True)
def _no_sleep(monkeypatch: pytest.MonkeyPatch) -> list[float]:
    slept: list[float] = []
    monkeypatch.setattr(recording_listener.time, "sleep", slept.append)
    return slept


def test_leaf_suite_is_recorded(recorder: _FakeRecorder) -> None:
    listener = RecordingListener()
    suite = _suite("Cluster E2E", has_tests=True)

    listener.start_suite(suite, object())
    listener.end_suite(suite, object())

    assert recorder.started == [("Cluster E2E", DEFAULT_OUTPUT_DIR)]
    assert recorder.stopped == 1


def test_parent_suite_without_tests_is_skipped(recorder: _FakeRecorder) -> None:
    listener = RecordingListener()
    parent = _suite("Suites", has_tests=False)

    listener.start_suite(parent, object())
    listener.end_suite(parent, object())

    assert recorder.started == []
    assert recorder.stopped == 0


def test_output_dir_override_is_passed_through(recorder: _FakeRecorder) -> None:
    listener = RecordingListener("custom-dir")
    listener.start_suite(_suite("suite", has_tests=True), object())

    assert recorder.started == [("suite", "custom-dir")]


def test_start_failure_is_swallowed(recorder: _FakeRecorder) -> None:
    recorder.start_error = RuntimeError("no display")
    listener = RecordingListener()

    listener.start_suite(_suite("suite", has_tests=True), object())

    assert recorder.started == [("suite", DEFAULT_OUTPUT_DIR)]


def test_stop_failure_is_swallowed(recorder: _FakeRecorder) -> None:
    recorder.stop_error = RuntimeError("thread stuck")
    listener = RecordingListener()

    listener.end_suite(_suite("suite", has_tests=True), object())

    assert recorder.stopped == 1


def test_start_test_shows_banner_and_pauses(
    recorder: _FakeRecorder, _no_sleep: list[float]
) -> None:
    listener = RecordingListener()

    listener.start_test(_test("Cluster Launch"), object())

    assert recorder.banners == [("Cluster Launch", DEFAULT_BANNER_SECONDS)]
    assert _no_sleep == [DEFAULT_BANNER_SECONDS]


def test_banner_seconds_override_is_passed_through(
    recorder: _FakeRecorder, _no_sleep: list[float]
) -> None:
    listener = RecordingListener(banner_seconds=2.5)

    listener.start_test(_test("Broadcast"), object())

    assert recorder.banners == [("Broadcast", 2.5)]
    assert _no_sleep == [2.5]


def test_banner_failure_is_swallowed(recorder: _FakeRecorder) -> None:
    recorder.banner_error = RuntimeError("no font")
    listener = RecordingListener()

    listener.start_test(_test("suite"), object())

    assert recorder.banners == [("suite", DEFAULT_BANNER_SECONDS)]


def test_keycast_overlay_registered_on_recorder(recorder: _FakeRecorder) -> None:
    RecordingListener()

    assert len(recorder.overlays) == 1


@pytest.mark.usefixtures("recorder")
def test_running_keyword_feeds_keystrokes_into_keycast() -> None:
    listener = RecordingListener()
    keystrokes = _muted_keystrokes()

    listener.start_library_keyword(object(), _keyword(keystrokes), object())
    keystrokes.type_text("echo hi")

    assert listener._keycast.active(time.monotonic()) == ["echo hi"]


@pytest.mark.usefixtures("recorder")
def test_running_keyword_wires_each_keystrokes_instance_once() -> None:
    listener = RecordingListener()
    keystrokes = _muted_keystrokes()

    listener.start_library_keyword(object(), _keyword(keystrokes), object())
    listener.start_library_keyword(object(), _keyword(keystrokes), object())
    keystrokes.type_text("hi")

    # Wired once, so the keystroke is recorded once rather than duplicated.
    assert listener._keycast.active(time.monotonic()) == ["hi"]


@pytest.mark.usefixtures("recorder")
def test_running_keyword_ignores_non_keystroke_libraries() -> None:
    listener = RecordingListener()

    listener.start_library_keyword(object(), _keyword(object()), object())

    assert listener._keycast.active(time.monotonic()) == []


@pytest.mark.usefixtures("recorder")
def test_start_suite_resets_keycast_and_wiring() -> None:
    listener = RecordingListener()
    keystrokes = _muted_keystrokes()
    listener.start_library_keyword(object(), _keyword(keystrokes), object())
    keystrokes.type_text("stale")

    listener.start_suite(_suite("next", has_tests=True), object())

    # The new suite's recording starts with no leftover labels or wiring state.
    assert listener._keycast.active(time.monotonic()) == []
    assert listener._wired_keystrokes == set()
