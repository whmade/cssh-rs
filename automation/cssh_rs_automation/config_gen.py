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
        ssh_config_path: str,
        aliases: list[str] | tuple[str, ...],
        cluster_name: str = "e2e",
        program: str = "ssh",
        arguments: list[str] | tuple[str, ...] | None = None,
    ) -> str:
        """Write ``cssh-rs-config.toml`` into ``output_dir`` and return its path.

        Args:
            cssh_rs_binary: Path to the cssh-rs executable to invoke.
            output_dir: Existing directory the config is written into. Only used
                to build the ``--output`` path; pass the directory holding the
                executable when you want cssh-rs to load the config on startup.
            ssh_config_path: OpenSSH client config the program runs against; becomes
                ``ssh -F <ssh_config_path>``. Must be an existing file.
            aliases: Host aliases that form the cluster's host list; each must
                resolve to a ``Host`` block in ``ssh_config_path``.
            cluster_name: Name of the generated cluster.
            program: SSH executable the config launches.
            arguments: Full program arguments that replace the default
                ``<user>@<host>`` placeholder; must include it, e.g.
                ``["-o", "User=deploy", "{{USERNAME_AT_HOST}}"]``.

        Returns:
            Absolute path to the written ``cssh-rs-config.toml`` as a str.
        """
        host_list = _validate(
            cssh_rs_binary=cssh_rs_binary,
            output_dir=output_dir,
            ssh_config_path=ssh_config_path,
            cluster_name=cluster_name,
            program=program,
            aliases=aliases,
        )
        arguments = _validate_arguments(arguments)
        config_path = Path(output_dir).resolve() / CONFIG_FILENAME
        argv = [
            cssh_rs_binary,
            GENERATE_CONFIG_SUBCOMMAND,
            "--ssh-config-path",
            ssh_config_path,
            "--program",
            program,
            "--cluster",
            cluster_name,
            "--output",
            str(config_path),
            # Attached form so values starting with `-` are not parsed as flags.
            *(f"--arguments={value}" for value in arguments),
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
    ssh_config_path: str,
    cluster_name: str,
    program: str,
    aliases: list[str] | tuple[str, ...],
) -> list[str]:
    """Validate ``generate_config`` arguments; return the aliases as a list."""
    if not cssh_rs_binary:
        raise ConfigGenError("cssh_rs_binary must be a non-empty path")
    if not ssh_config_path:
        raise ConfigGenError("ssh_config_path must be a non-empty path")
    if not Path(ssh_config_path).is_file():
        raise ConfigGenError(f"ssh_config_path is not an existing file: {ssh_config_path}")
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


def _validate_arguments(arguments: list[str] | tuple[str, ...] | None) -> list[str]:
    """Return ``arguments`` as a list; reject non-string or empty entries."""
    argument_list = list(arguments or ())
    if not all(isinstance(argument, str) and argument for argument in argument_list):
        raise ConfigGenError("each argument must be a non-empty string")
    return argument_list
