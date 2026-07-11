"""Unit tests for the recording listener.

The ScreenRecorder is replaced with a fake that records its calls so the tests
assert the listener's leaf-suite gating and best-effort behaviour without
launching real screen capture.
"""

from __future__ import annotations

import types

import pytest

from cssh_rs_e2e import recording_listener
from cssh_rs_e2e.recording_listener import DEFAULT_OUTPUT_DIR, RecordingListener


class _FakeRecorder:
    def __init__(self) -> None:
        self.started: list[tuple[str, str]] = []
        self.stopped = 0
        self.start_error: Exception | None = None
        self.stop_error: Exception | None = None

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


def _suite(name: str, *, has_tests: bool) -> types.SimpleNamespace:
    return types.SimpleNamespace(name=name, tests=[object()] if has_tests else [])


@pytest.fixture
def recorder(monkeypatch: pytest.MonkeyPatch) -> _FakeRecorder:
    fake = _FakeRecorder()
    monkeypatch.setattr(recording_listener, "ScreenRecorder", lambda: fake)
    return fake


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
