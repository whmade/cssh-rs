"""Convert a recorded MP4 into an optimized GIF via ffmpeg, then gifsicle.

gifsicle recompresses lossily, which ffmpeg cannot; the pass is skipped when
gifsicle is not on PATH.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Callable

    # subprocess.run-compatible callable: (argv, **kwargs) -> CompletedProcess.
    Runner = Callable[..., subprocess.CompletedProcess[str]]


# A terminal screencast needs few colours, so a capped palette shrinks the GIF.
_MAX_COLORS = 128
# gifsicle lossy level; higher is smaller, 80 keeps the console text legible.
_GIFSICLE_LOSSY = 80


class GifExportError(RuntimeError):
    """Raised when the MP4 cannot be converted to a GIF."""


def export_gif(
    source_mp4: str,
    output_gif: str,
    *,
    fps: int = 10,
    ffmpeg_exe: str | None = None,
    gifsicle_exe: str | None = None,
    runner: Runner | None = None,
) -> str:
    """Convert ``source_mp4`` to ``output_gif`` and return the GIF path.

    Args:
        source_mp4: Path to the source MP4; must exist.
        output_gif: Destination GIF path; parent directories are created.
        fps: Frames per second of the output GIF.
        ffmpeg_exe: ffmpeg executable to use; defaults to the imageio-ffmpeg
            bundled binary.
        gifsicle_exe: gifsicle executable for the lossy recompression pass;
            defaults to ``gifsicle`` on PATH, and the pass is skipped when it is
            not found.
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

    # A screencast mostly holds still, so the diff-based palette and encoding win
    # big; bayer dithering compresses flat console text better than diffusion.
    filtergraph = (
        f"fps={fps},split[a][b];"
        f"[a]palettegen=max_colors={_MAX_COLORS}:stats_mode=diff[p];"
        f"[b][p]paletteuse=dither=bayer:bayer_scale=3:diff_mode=rectangle"
    )
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

    _optimize_with_gifsicle(output, gifsicle_exe, run)
    return str(output.resolve())


def _optimize_with_gifsicle(output: Path, gifsicle_exe: str | None, run: Runner) -> None:
    """Recompress ``output`` in place with gifsicle; skip if gifsicle is absent."""
    gifsicle = gifsicle_exe or shutil.which("gifsicle")
    if gifsicle is None:
        print("gifsicle not found on PATH; skipping GIF recompression", file=sys.stderr)
        return

    argv = [
        gifsicle,
        "-O3",
        f"--lossy={_GIFSICLE_LOSSY}",
        "--colors",
        str(_MAX_COLORS),
        "--batch",
        str(output),
    ]
    try:
        result = run(argv, capture_output=True, text=True, check=False)
    except OSError as exc:
        raise GifExportError(f"failed to run gifsicle: {exc}") from exc
    if result.returncode != 0:
        raise GifExportError(
            f"gifsicle exited with code {result.returncode}: {result.stderr.strip()}"
        )


def _default_ffmpeg() -> str:
    """Return the path to the ffmpeg binary bundled with imageio-ffmpeg."""
    try:
        import imageio_ffmpeg
    except ImportError as exc:
        raise GifExportError(
            "imageio-ffmpeg is required for GIF export but is not installed"
        ) from exc
    return imageio_ffmpeg.get_ffmpeg_exe()
