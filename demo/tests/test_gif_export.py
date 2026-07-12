"""Unit tests for the demo GIF export.

A fake ``subprocess.run`` is injected so the tests never launch ffmpeg; the
success cases emulate ffmpeg by writing the output file the code then checks.
"""

from __future__ import annotations

import subprocess
from pathlib import Path

import pytest

from cssh_rs_demo.gif_export import GifExportError, export_gif


class _FakeRunner:
    def __init__(self, *, returncode: int = 0, write_output: bool = True, stderr: str = "") -> None:
        self.returncode = returncode
        self.write_output = write_output
        self.stderr = stderr
        self.argv: list[str] = []

    def __call__(self, argv: list[str], **_kwargs: object) -> subprocess.CompletedProcess[str]:
        self.argv = list(argv)
        if self.write_output and self.returncode == 0:
            Path(argv[-1]).write_bytes(b"GIF89a")
        return subprocess.CompletedProcess(argv, self.returncode, stdout="", stderr=self.stderr)


def _source(tmp_path: Path) -> Path:
    source = tmp_path / "in.mp4"
    source.write_bytes(b"fake mp4")
    return source


def test_export_gif_builds_palette_filtergraph(tmp_path: Path) -> None:
    out = tmp_path / "out.gif"
    runner = _FakeRunner()

    result = export_gif(
        str(_source(tmp_path)), str(out), fps=12, ffmpeg_exe="ffmpeg", runner=runner
    )

    assert result == str(out.resolve())
    assert out.is_file()
    assert runner.argv[0] == "ffmpeg"
    graph = runner.argv[runner.argv.index("-filter_complex") + 1]
    assert "fps=12" in graph
    assert "palettegen" in graph
    assert "paletteuse" in graph


@pytest.mark.parametrize(
    ("source_exists", "runner", "kwargs", "match"),
    [
        (False, _FakeRunner(), {}, "does not exist"),
        (True, _FakeRunner(), {"fps": 0}, "fps must be positive"),
        (True, _FakeRunner(returncode=1, write_output=False, stderr="boom"), {}, "code 1"),
        (True, _FakeRunner(write_output=False), {}, "did not write"),
    ],
)
def test_export_gif_raises(
    tmp_path: Path,
    source_exists: bool,
    runner: _FakeRunner,
    kwargs: dict[str, object],
    match: str,
) -> None:
    source = _source(tmp_path) if source_exists else tmp_path / "nope.mp4"
    with pytest.raises(GifExportError, match=match):
        export_gif(
            str(source), str(tmp_path / "out.gif"), ffmpeg_exe="ffmpeg", runner=runner, **kwargs
        )
