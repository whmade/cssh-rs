"""Unit tests for the demo GIF export.

A fake ``subprocess.run`` is injected so the tests never launch ffmpeg or
gifsicle; the success cases emulate them by writing the output file the code
then checks.
"""

from __future__ import annotations

import subprocess
from pathlib import Path

import pytest

from cssh_rs_demo import gif_export
from cssh_rs_demo.gif_export import GifExportError, export_gif


class _FakeRunner:
    def __init__(
        self,
        *,
        returncode: int = 0,
        write_output: bool = True,
        stderr: str = "",
        fail_cmd: str | None = None,
    ) -> None:
        self.returncode = returncode
        self.write_output = write_output
        self.stderr = stderr
        # argv[0] substring forced to exit non-zero, so one fake lets ffmpeg pass
        # while gifsicle fails.
        self.fail_cmd = fail_cmd
        self.calls: list[list[str]] = []

    def __call__(self, argv: list[str], **_kwargs: object) -> subprocess.CompletedProcess[str]:
        self.calls.append(list(argv))
        returncode = self.returncode
        stderr = self.stderr
        if self.fail_cmd is not None and self.fail_cmd in argv[0]:
            returncode = 1
            stderr = stderr or "boom"
        if self.write_output and returncode == 0:
            Path(argv[-1]).write_bytes(b"GIF89a")
        return subprocess.CompletedProcess(argv, returncode, stdout="", stderr=stderr)

    @property
    def argv(self) -> list[str]:
        return self.calls[-1] if self.calls else []


def _source(tmp_path: Path) -> Path:
    source = tmp_path / "in.mp4"
    source.write_bytes(b"fake mp4")
    return source


def test_export_gif_builds_palette_filtergraph(tmp_path: Path) -> None:
    out = tmp_path / "out.gif"
    runner = _FakeRunner()

    result = export_gif(
        str(_source(tmp_path)),
        str(out),
        fps=12,
        ffmpeg_exe="ffmpeg",
        gifsicle_exe="gifsicle",
        runner=runner,
    )

    assert result == str(out.resolve())
    assert out.is_file()
    ffmpeg_argv = runner.calls[0]
    assert ffmpeg_argv[0] == "ffmpeg"
    graph = ffmpeg_argv[ffmpeg_argv.index("-filter_complex") + 1]
    assert "fps=12" in graph
    assert "palettegen" in graph
    assert "paletteuse" in graph
    assert "stats_mode=diff" in graph
    assert "diff_mode=rectangle" in graph
    assert "max_colors=128" in graph


def test_export_gif_recompresses_with_gifsicle(tmp_path: Path) -> None:
    out = tmp_path / "out.gif"
    runner = _FakeRunner()

    export_gif(
        str(_source(tmp_path)),
        str(out),
        ffmpeg_exe="ffmpeg",
        gifsicle_exe="gifsicle",
        runner=runner,
    )

    gifsicle_argv = runner.calls[1]
    assert gifsicle_argv[0] == "gifsicle"
    assert "-O3" in gifsicle_argv
    assert "--lossy=80" in gifsicle_argv
    assert gifsicle_argv[gifsicle_argv.index("--colors") + 1] == "128"
    assert "--batch" in gifsicle_argv
    assert gifsicle_argv[-1] == str(out)


def test_export_gif_skips_gifsicle_when_missing(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.setattr(gif_export.shutil, "which", lambda _cmd: None)
    out = tmp_path / "out.gif"
    runner = _FakeRunner()

    result = export_gif(str(_source(tmp_path)), str(out), ffmpeg_exe="ffmpeg", runner=runner)

    assert result == str(out.resolve())
    assert len(runner.calls) == 1
    assert runner.calls[0][0] == "ffmpeg"


@pytest.mark.parametrize(
    ("source_exists", "runner", "kwargs", "match"),
    [
        (False, _FakeRunner(), {}, "does not exist"),
        (True, _FakeRunner(), {"fps": 0}, "fps must be positive"),
        (True, _FakeRunner(returncode=1, write_output=False, stderr="boom"), {}, "code 1"),
        (True, _FakeRunner(write_output=False), {}, "did not write"),
        (
            True,
            _FakeRunner(fail_cmd="gifsicle"),
            {"gifsicle_exe": "gifsicle"},
            "gifsicle exited with code 1",
        ),
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
