"""Convert a recorded MP4 into an optimized GIF using the bundled ffmpeg.

imageio-ffmpeg (a dependency of the automation harness) ships an ffmpeg binary,
so the conversion needs no system tool. A single ``palettegen``/``paletteuse``
filtergraph yields a small, high-quality GIF.
"""

from __future__ import annotations

import subprocess
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Callable

    # subprocess.run-compatible callable: (argv, **kwargs) -> CompletedProcess.
    Runner = Callable[..., subprocess.CompletedProcess[str]]


class GifExportError(RuntimeError):
    """Raised when the MP4 cannot be converted to a GIF."""


def export_gif(
    source_mp4: str,
    output_gif: str,
    *,
    fps: int = 10,
    ffmpeg_exe: str | None = None,
    runner: Runner | None = None,
) -> str:
    """Convert ``source_mp4`` to ``output_gif`` and return the GIF path.

    Args:
        source_mp4: Path to the source MP4; must exist.
        output_gif: Destination GIF path; parent directories are created.
        fps: Frames per second of the output GIF.
        ffmpeg_exe: ffmpeg executable to use; defaults to the imageio-ffmpeg
            bundled binary.
        runner: ``subprocess.run``-compatible callable; injectable for tests.

    Returns:
        Absolute path to the written GIF as a str.
    """
    source = Path(source_mp4)
    if not source.is_file():
        raise GifExportError(f"source MP4 does not exist: {source}")
    if fps <= 0:
        raise GifExportError(f"fps must be positive, got {fps}")

    ffmpeg = ffmpeg_exe or _default_ffmpeg()
    run = runner or subprocess.run
    output = Path(output_gif)
    output.parent.mkdir(parents=True, exist_ok=True)

    filtergraph = f"fps={fps},split[a][b];[a]palettegen[p];[b][p]paletteuse"
    argv = [
        ffmpeg,
        "-y",
        "-i",
        str(source),
        "-filter_complex",
        filtergraph,
        str(output),
    ]

    try:
        result = run(argv, capture_output=True, text=True, check=False)
    except OSError as exc:
        raise GifExportError(f"failed to run ffmpeg: {exc}") from exc
    if result.returncode != 0:
        raise GifExportError(
            f"ffmpeg exited with code {result.returncode}: {result.stderr.strip()}"
        )
    if not output.is_file():
        raise GifExportError(f"ffmpeg reported success but did not write {output}")
    return str(output.resolve())


def _default_ffmpeg() -> str:
    """Return the path to the ffmpeg binary bundled with imageio-ffmpeg."""
    try:
        import imageio_ffmpeg
    except ImportError as exc:
        raise GifExportError(
            "imageio-ffmpeg is required for GIF export but is not installed"
        ) from exc
    return imageio_ffmpeg.get_ffmpeg_exe()
