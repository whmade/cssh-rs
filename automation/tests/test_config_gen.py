"""Unit tests for the cssh-rs config generation library.

``subprocess.run`` is replaced with a fake so the tests never invoke a real
cssh-rs binary; they assert the argv the library builds and how it maps exit
codes and validation failures onto ``ConfigGenError``.
"""

from __future__ import annotations

import types
from pathlib import Path

import pytest

from cssh_rs_automation import config_gen
from cssh_rs_automation.config_gen import CONFIG_FILENAME, ConfigGen, ConfigGenError


class _FakeRun:
    def __init__(
        self,
        *,
        returncode: int = 0,
        stderr: str = "",
        write_file: bool = True,
        raises: OSError | None = None,
    ) -> None:
        self.returncode = returncode
        self.stderr = stderr
        self.write_file = write_file
        self.raises = raises
        self.calls: list[tuple[list[str], dict[str, object]]] = []

    def __call__(self, argv: list[str], **kwargs: object) -> types.SimpleNamespace:
        self.calls.append((argv, kwargs))
        if self.raises is not None:
            raise self.raises
        if self.write_file:
            output = argv[argv.index("--output") + 1]
            Path(output).write_text("generated", encoding="utf-8")
        return types.SimpleNamespace(returncode=self.returncode, stdout="", stderr=self.stderr)


@pytest.fixture
def fake_run(monkeypatch: pytest.MonkeyPatch) -> _FakeRun:
    fake = _FakeRun()
    monkeypatch.setattr(config_gen.subprocess, "run", fake)
    return fake


@pytest.fixture
def ssh_config(tmp_path: Path) -> str:
    path = tmp_path / "ssh_config"
    path.write_text("Host h1\n", encoding="utf-8")
    return str(path)


def test_generate_config_builds_expected_argv(
    fake_run: _FakeRun, tmp_path: Path, ssh_config: str
) -> None:
    ConfigGen().generate_config(
        "cssh-rs.exe",
        str(tmp_path),
        ssh_config,
        ["h1", "h2"],
    )

    assert fake_run.calls[0][0] == [
        "cssh-rs.exe",
        "generate-config",
        "--ssh-config",
        ssh_config,
        "--program",
        "ssh",
        "--cluster",
        "e2e",
        "--output",
        str(tmp_path.resolve() / CONFIG_FILENAME),
        "h1",
        "h2",
    ]


@pytest.mark.usefixtures("fake_run")
def test_generate_config_returns_written_config_path(tmp_path: Path, ssh_config: str) -> None:
    returned = ConfigGen().generate_config(
        "cssh-rs.exe",
        str(tmp_path),
        ssh_config,
        ["h1"],
    )

    assert returned == str(tmp_path.resolve() / CONFIG_FILENAME)
    assert Path(returned).is_file()


def test_generate_config_honors_program_and_cluster(
    fake_run: _FakeRun, tmp_path: Path, ssh_config: str
) -> None:
    ConfigGen().generate_config(
        "cssh-rs.exe",
        str(tmp_path),
        ssh_config,
        ["h1"],
        cluster_name="prod",
        program="ssh.exe",
    )

    argv = fake_run.calls[0][0]
    assert argv[argv.index("--program") + 1] == "ssh.exe"
    assert argv[argv.index("--cluster") + 1] == "prod"


def test_generate_config_captures_output(
    fake_run: _FakeRun, tmp_path: Path, ssh_config: str
) -> None:
    ConfigGen().generate_config(
        "cssh-rs.exe",
        str(tmp_path),
        ssh_config,
        ["h1"],
    )

    assert fake_run.calls[0][1].get("capture_output") is True


@pytest.mark.parametrize(
    ("kwargs", "message"),
    [
        ({"cssh_rs_binary": ""}, "cssh_rs_binary"),
        ({"ssh_config": ""}, "ssh_config"),
        ({"ssh_config": "does-not-exist"}, "existing file"),
        ({"cluster_name": ""}, "cluster_name"),
        ({"program": ""}, "program"),
        ({"aliases": []}, "non-empty"),
        ({"aliases": ["h1", ""]}, "each alias"),
    ],
)
def test_generate_config_rejects_invalid_arguments(
    fake_run: _FakeRun,
    tmp_path: Path,
    ssh_config: str,
    kwargs: dict[str, object],
    message: str,
) -> None:
    call = {
        "cssh_rs_binary": "cssh-rs.exe",
        "output_dir": str(tmp_path),
        "ssh_config": ssh_config,
        "aliases": ["h1"],
        **kwargs,
    }

    with pytest.raises(ConfigGenError, match=message):
        ConfigGen().generate_config(**call)

    assert fake_run.calls == []


def test_generate_config_rejects_missing_output_dir(
    fake_run: _FakeRun, tmp_path: Path, ssh_config: str
) -> None:
    with pytest.raises(ConfigGenError, match="existing directory"):
        ConfigGen().generate_config(
            "cssh-rs.exe",
            str(tmp_path / "does-not-exist"),
            ssh_config,
            ["h1"],
        )

    assert fake_run.calls == []


def test_generate_config_raises_on_non_zero_exit(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, ssh_config: str
) -> None:
    fake = _FakeRun(returncode=2, stderr="no hosts supplied", write_file=False)
    monkeypatch.setattr(config_gen.subprocess, "run", fake)

    with pytest.raises(ConfigGenError, match="no hosts supplied"):
        ConfigGen().generate_config(
            "cssh-rs.exe",
            str(tmp_path),
            ssh_config,
            ["h1"],
        )


def test_generate_config_raises_when_binary_missing(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, ssh_config: str
) -> None:
    fake = _FakeRun(raises=FileNotFoundError("no such file"))
    monkeypatch.setattr(config_gen.subprocess, "run", fake)

    with pytest.raises(ConfigGenError, match="failed to run"):
        ConfigGen().generate_config(
            "missing-binary",
            str(tmp_path),
            ssh_config,
            ["h1"],
        )


def test_generate_config_raises_when_file_not_written(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, ssh_config: str
) -> None:
    fake = _FakeRun(write_file=False)
    monkeypatch.setattr(config_gen.subprocess, "run", fake)

    with pytest.raises(ConfigGenError, match="did not write"):
        ConfigGen().generate_config(
            "cssh-rs.exe",
            str(tmp_path),
            ssh_config,
            ["h1"],
        )
