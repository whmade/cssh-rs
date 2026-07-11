"""Unit tests for the clipboard library.

A fake pyperclip module is injected into ``sys.modules`` so the tests never
import the real library, which needs a clipboard backend the headless Linux
dev box lacks.
"""

from __future__ import annotations

import sys
import types

import pytest

from cssh_rs_automation.clipboard import Clipboard, ClipboardError


class _FakePyperclip(types.SimpleNamespace):
    def __init__(self) -> None:
        super().__init__(contents="")
        self.calls: list[tuple[str, tuple[object, ...]]] = []

    def copy(self, text: object) -> None:
        self.calls.append(("copy", (text,)))
        self.contents = text

    def paste(self) -> object:
        self.calls.append(("paste", ()))
        return self.contents


@pytest.fixture
def fake_pyperclip(monkeypatch: pytest.MonkeyPatch) -> _FakePyperclip:
    fake = _FakePyperclip()
    monkeypatch.setitem(sys.modules, "pyperclip", fake)
    return fake


def test_set_clipboard_forwards_text(fake_pyperclip: _FakePyperclip) -> None:
    Clipboard().set_clipboard("alpha bravo")

    assert fake_pyperclip.calls == [("copy", ("alpha bravo",))]


def test_set_clipboard_rejects_non_string(fake_pyperclip: _FakePyperclip) -> None:
    with pytest.raises(ClipboardError, match="must be a string"):
        Clipboard().set_clipboard(None)  # type: ignore[arg-type]

    assert fake_pyperclip.calls == []


def test_get_clipboard_returns_contents(fake_pyperclip: _FakePyperclip) -> None:
    fake_pyperclip.contents = "charlie"

    assert Clipboard().get_clipboard() == "charlie"
    assert fake_pyperclip.calls == [("paste", ())]


def test_get_clipboard_round_trips_set(fake_pyperclip: _FakePyperclip) -> None:
    clipboard = Clipboard()
    clipboard.set_clipboard("round trip")

    assert clipboard.get_clipboard() == "round trip"
    assert fake_pyperclip.calls == [("copy", ("round trip",)), ("paste", ())]
