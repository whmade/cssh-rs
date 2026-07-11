"""Generate a cssh-rs config for the Windows E2E suite via the real binary.

This Robot Framework library does not emit TOML itself; it shells out to the
shipped ``cssh-rs generate-config`` subcommand, the single source of truth for
the config schema. Suites pair it with the sshd fixture: they pass the
fixture's generated ``ssh_config`` and host aliases, and the produced
``cssh-rs-config.toml`` drives cssh-rs to launch ``ssh -F <ssh_config> <alias>``
through the existing ``program``/``arguments`` client settings.
"""

from __future__ import annotations

import subprocess
from pathlib import Path

CONFIG_FILENAME = "cssh-rs-config.toml"
GENERATE_CONFIG_SUBCOMMAND = "generate-config"


class ConfigGenError(RuntimeError):
    """Raised when the cssh-rs config cannot be generated."""


class ConfigGen:
    """Robot Framework library that generates a cssh-rs config via the binary."""

    ROBOT_LIBRARY_SCOPE = "SUITE"
    ROBOT_LIBRARY_VERSION = "0.1.0"

    def generate_config(
        self,
        cssh_rs_binary: str,
        output_dir: str,
        ssh_config: str,
        aliases: list[str] | tuple[str, ...],
        cluster_name: str = "e2e",
        program: str = "ssh",
    ) -> str:
        """Write ``cssh-rs-config.toml`` into ``output_dir`` and return its path.

        Args:
            cssh_rs_binary: Path to the cssh-rs executable to invoke.
            output_dir: Existing directory the config is written into. Only used
                to build the ``--output`` path; pass the directory holding the
                executable when you want cssh-rs to load the config on startup.
            ssh_config: OpenSSH client config the program runs against; becomes
                ``ssh -F <ssh_config>``. Must be an existing file.
            aliases: Host aliases that form the cluster's host list; each must
                resolve to a ``Host`` block in ``ssh_config``.
            cluster_name: Name of the generated cluster.
            program: SSH executable the config launches.

        Returns:
            Absolute path to the written ``cssh-rs-config.toml`` as a str.
        """
        host_list = _validate(
            cssh_rs_binary=cssh_rs_binary,
            output_dir=output_dir,
            ssh_config=ssh_config,
            cluster_name=cluster_name,
            program=program,
            aliases=aliases,
        )
        config_path = Path(output_dir).resolve() / CONFIG_FILENAME
        argv = [
            cssh_rs_binary,
            GENERATE_CONFIG_SUBCOMMAND,
            "--ssh-config",
            ssh_config,
            "--program",
            program,
            "--cluster",
            cluster_name,
            "--output",
            str(config_path),
            *host_list,
        ]
        try:
            result = subprocess.run(argv, capture_output=True, text=True, check=False)
        except OSError as exc:
            raise ConfigGenError(f"failed to run {cssh_rs_binary}: {exc}") from exc
        if result.returncode != 0:
            raise ConfigGenError(
                f"cssh-rs {GENERATE_CONFIG_SUBCOMMAND} exited with code "
                f"{result.returncode}: {result.stderr.strip()}"
            )
        if not config_path.is_file():
            raise ConfigGenError(
                f"cssh-rs {GENERATE_CONFIG_SUBCOMMAND} reported success but "
                f"did not write {config_path}"
            )
        return str(config_path)


def _validate(
    *,
    cssh_rs_binary: str,
    output_dir: str,
    ssh_config: str,
    cluster_name: str,
    program: str,
    aliases: list[str] | tuple[str, ...],
) -> list[str]:
    """Validate ``generate_config`` arguments; return the aliases as a list."""
    if not cssh_rs_binary:
        raise ConfigGenError("cssh_rs_binary must be a non-empty path")
    if not ssh_config:
        raise ConfigGenError("ssh_config must be a non-empty path")
    if not Path(ssh_config).is_file():
        raise ConfigGenError(f"ssh_config is not an existing file: {ssh_config}")
    if not cluster_name:
        raise ConfigGenError("cluster_name must be a non-empty string")
    if not program:
        raise ConfigGenError("program must be a non-empty string")
    host_list = list(aliases)
    if not host_list:
        raise ConfigGenError("aliases must be non-empty")
    if not all(isinstance(alias, str) and alias for alias in host_list):
        raise ConfigGenError("each alias must be a non-empty string")
    if not Path(output_dir).is_dir():
        raise ConfigGenError(f"output_dir is not an existing directory: {output_dir}")
    return host_list
