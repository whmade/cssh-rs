"""Per-run user-mode sshd fixture for the cssh-rs Windows E2E suite.

This Robot Framework library starts a fresh OpenSSH ``sshd`` instance on
``127.0.0.1`` at a high TCP port for the duration of a test suite. Each
logical host alias gets its own Ed25519 keypair, and the matching
``authorized_keys`` entry pins a ``command="..."`` restriction that pipes
the SSH channel's stdin into ``markers/<alias>.log``. Test suites assert
fan-out by reading those marker files after exercising ``cssh-rs``.

Everything lives inside a per-run temp directory: host key, per-alias
keypairs, ``authorized_keys``, the generated ``sshd_config`` and
``ssh_config``, a ``known_hosts`` file, the markers directory, and the
sshd log. The fixture does not touch any system path and needs no
elevation.

The library exposes the keywords ``Start Sshd``, ``Stop Sshd``,
``Read Marker``, ``Markers Dir``, ``Ssh Config Path``, ``Port`` and
``Host Aliases`` to Robot Framework suites. A ``__main__`` smoke entry
brings the fixture up locally, drives a real ``ssh`` against each
alias, prints the resulting marker contents and tears down again, so
the library can be sanity-checked outside of Robot.
"""

from __future__ import annotations

import contextlib
import getpass
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

DEFAULT_SSHD_LOCATIONS = (
    r"C:\Windows\System32\OpenSSH\sshd.exe",
    r"C:\Program Files\OpenSSH\sshd.exe",
)
"""Fallback locations searched for ``sshd.exe`` on Windows."""

READINESS_TIMEOUT_SECONDS = 10.0
"""Maximum time spent polling sshd's listening port for readiness."""

READINESS_POLL_INTERVAL_SECONDS = 0.1
"""Delay between consecutive TCP-connect probes during readiness polling."""

STOP_GRACE_SECONDS = 3.0
"""Seconds granted to sshd to exit after ``terminate()`` before ``kill()``."""

ALIAS_PATTERN = re.compile(r"^[A-Za-z0-9._-]+$")
"""Allowed shape for a host alias - a single safe token with no path
separators, whitespace, or control characters, since each alias is
interpolated into filenames and the generated ``ssh_config``."""

MIN_TCP_PORT = 1
MAX_TCP_PORT = 65535


class SshdFixtureError(RuntimeError):
    """Raised when the fixture cannot start or shut down sshd cleanly."""


class SshdFixture:
    """Robot Framework library that owns a per-run user-mode sshd."""

    ROBOT_LIBRARY_SCOPE = "SUITE"
    ROBOT_LIBRARY_VERSION = "0.1.0"

    def __init__(self) -> None:
        self._tempdir: Path | None = None
        self._process: subprocess.Popen[bytes] | None = None
        self._port: int | None = None
        self._aliases: list[str] = []
        self._ssh_config_path: Path | None = None
        self._markers_dir: Path | None = None

    def start_sshd(
        self,
        host_aliases: list[str] | tuple[str, ...],
        port: int | None = None,
    ) -> dict[str, object]:
        """Bring up sshd for ``host_aliases`` and return runtime paths.

        Args:
            host_aliases: Logical host names exposed in the generated
                ``ssh_config``. Each alias gets its own keypair and its
                own ``markers/<alias>.log`` marker file.
            port: Specific TCP port to bind. ``None`` picks a free
                ephemeral high port.

        Returns:
            A dict with keys ``port`` (int), ``ssh_config`` (str),
            ``markers_dir`` (str), ``aliases`` (list of str) and
            ``tempdir`` (str), all populated after sshd is accepting
            connections.
        """
        if self._process is not None:
            raise SshdFixtureError("sshd fixture already running")
        aliases = list(host_aliases)
        if not aliases:
            raise SshdFixtureError("host_aliases must be non-empty")
        if len(set(aliases)) != len(aliases):
            raise SshdFixtureError("host_aliases must be unique")
        for alias in aliases:
            if not ALIAS_PATTERN.fullmatch(alias):
                raise SshdFixtureError(
                    f"host alias must match {ALIAS_PATTERN.pattern} "
                    f"(no whitespace, path separators or control characters): {alias!r}"
                )
        if port is not None and not MIN_TCP_PORT <= port <= MAX_TCP_PORT:
            raise SshdFixtureError(
                f"port must be between {MIN_TCP_PORT} and {MAX_TCP_PORT}: {port}"
            )

        tempdir = Path(tempfile.mkdtemp(prefix="cssh-e2e-sshd-"))
        process: subprocess.Popen[bytes] | None = None
        try:
            markers_dir = tempdir / "markers"
            keys_dir = tempdir / "keys"
            markers_dir.mkdir()
            keys_dir.mkdir()

            host_key_path = tempdir / "host_ed25519"
            _write_openssh_keypair(host_key_path)

            authorized_keys_path = tempdir / "authorized_keys"
            with authorized_keys_path.open("w", encoding="utf-8", newline="\n") as handle:
                for alias in aliases:
                    alias_key_path = keys_dir / f"{alias}_ed25519"
                    _write_openssh_keypair(alias_key_path)
                    pubkey = alias_key_path.with_suffix(".pub").read_text(encoding="utf-8").strip()
                    marker_path = markers_dir / f"{alias}.log"
                    forced = _build_forced_command(sys.executable, str(marker_path))
                    handle.write(
                        f'command="{forced}",no-port-forwarding,no-x11-forwarding,'
                        f"no-pty,no-agent-forwarding,no-user-rc {pubkey}\n"
                    )

            chosen_port = port if port is not None else _pick_free_port()
            sshd_config_path = tempdir / "sshd_config"
            sshd_log_path = tempdir / "sshd.log"
            pid_path = tempdir / "sshd.pid"
            sshd_config_path.write_text(
                _render_sshd_config(
                    port=chosen_port,
                    host_key=host_key_path,
                    authorized_keys=authorized_keys_path,
                    pid_file=pid_path,
                ),
                encoding="utf-8",
            )

            known_hosts_path = tempdir / "known_hosts"
            known_hosts_path.touch()
            ssh_config_path = tempdir / "ssh_config"
            ssh_config_path.write_text(
                _render_ssh_config(
                    aliases=aliases,
                    port=chosen_port,
                    keys_dir=keys_dir,
                    known_hosts=known_hosts_path,
                    user=getpass.getuser(),
                ),
                encoding="utf-8",
            )

            sshd_path = _resolve_sshd_path()
            # sshd's own log goes to sshd_log_path via -E; the process's own
            # stdout/stderr are discarded so no file handle outlives the
            # subprocess and blocks tempdir removal on Windows.
            process = subprocess.Popen(
                [
                    sshd_path,
                    "-f",
                    str(sshd_config_path),
                    "-D",
                    "-E",
                    str(sshd_log_path),
                ],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                stdin=subprocess.DEVNULL,
            )

            _wait_for_listening(chosen_port, process, sshd_log_path)
        except BaseException:
            _terminate(process)
            _remove_tempdir(tempdir)
            raise

        self._tempdir = tempdir
        self._process = process
        self._port = chosen_port
        self._aliases = aliases
        self._ssh_config_path = ssh_config_path
        self._markers_dir = markers_dir

        return {
            "port": chosen_port,
            "ssh_config": str(ssh_config_path),
            "markers_dir": str(markers_dir),
            "aliases": list(aliases),
            "tempdir": str(tempdir),
        }

    def stop_sshd(self) -> None:
        """Terminate sshd and remove the per-run temp directory."""
        _terminate(self._process)
        _remove_tempdir(self._tempdir)
        self._process = None
        self._tempdir = None
        self._port = None
        self._aliases = []
        self._ssh_config_path = None
        self._markers_dir = None

    def read_marker(self, alias: str) -> str:
        """Return the contents of ``markers/<alias>.log`` as UTF-8 text.

        Args:
            alias: Host alias whose marker is read.

        Returns:
            Marker file contents, decoded with ``errors='replace'``.
            Empty string if the file does not exist yet.
        """
        markers_dir = self._require_markers_dir()
        marker = markers_dir / f"{alias}.log"
        if not marker.exists():
            return ""
        return marker.read_bytes().decode("utf-8", errors="replace")

    def markers_dir(self) -> str:
        """Return the absolute path of the markers directory."""
        return str(self._require_markers_dir())

    def ssh_config_path(self) -> str:
        """Return the absolute path of the generated ``ssh_config``."""
        if self._ssh_config_path is None:
            raise SshdFixtureError("sshd fixture is not running")
        return str(self._ssh_config_path)

    def port(self) -> int:
        """Return the TCP port sshd is listening on."""
        if self._port is None:
            raise SshdFixtureError("sshd fixture is not running")
        return self._port

    def host_aliases(self) -> list[str]:
        """Return the list of configured host aliases."""
        return list(self._aliases)

    def _require_markers_dir(self) -> Path:
        if self._markers_dir is None:
            raise SshdFixtureError("sshd fixture is not running")
        return self._markers_dir


def _terminate(process: subprocess.Popen[bytes] | None) -> None:
    """Stop ``process`` gracefully, escalating to kill after the grace period."""
    if process is None or process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=STOP_GRACE_SECONDS)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=STOP_GRACE_SECONDS)


def _remove_tempdir(tempdir: Path | None) -> None:
    """Remove ``tempdir`` unless ``CSSH_E2E_KEEP_TEMP=1`` keeps it for debugging."""
    if tempdir is not None and os.environ.get("CSSH_E2E_KEEP_TEMP") != "1":
        shutil.rmtree(tempdir, ignore_errors=True)


def _write_openssh_keypair(private_path: Path) -> None:
    """Write a fresh Ed25519 keypair in OpenSSH format next to ``private_path``."""
    private_key = Ed25519PrivateKey.generate()
    private_bytes = private_key.private_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PrivateFormat.OpenSSH,
        encryption_algorithm=serialization.NoEncryption(),
    )
    public_bytes = private_key.public_key().public_bytes(
        encoding=serialization.Encoding.OpenSSH,
        format=serialization.PublicFormat.OpenSSH,
    )
    private_path.write_bytes(private_bytes)
    private_path.with_suffix(".pub").write_bytes(public_bytes + b"\n")
    _harden_private_key_permissions(private_path)


def _harden_private_key_permissions(private_path: Path) -> None:
    """Lock down ``private_path`` so OpenSSH accepts it as a private key.

    On POSIX this is a ``chmod 0600``. On Windows OpenSSH ignores the
    POSIX mode and instead rejects any key file whose ACL grants access
    beyond the owner - under the per-run temp tree it specifically flags
    the inherited ``OWNER RIGHTS`` ACE and then exits with ``no hostkeys
    available``. Strip inheritance and grant the current user sole access
    via ``icacls`` so both sshd (host key) and the ssh client (per-alias
    identity keys) load the key instead of ignoring it.

    Args:
        private_path: Path of the private key file to lock down.
    """
    if os.name != "nt":
        with contextlib.suppress(OSError):
            private_path.chmod(0o600)
        return
    user = getpass.getuser()
    for args in (
        ["icacls", str(private_path), "/inheritance:r"],
        ["icacls", str(private_path), "/grant:r", f"{user}:F"],
    ):
        result = subprocess.run(args, capture_output=True, check=False)
        if result.returncode != 0:
            detail = result.stderr.decode("utf-8", errors="replace").strip()
            raise SshdFixtureError(f"failed to harden key permissions on {private_path}: {detail}")


def _build_forced_command(executable: str, marker: str) -> str:
    """Return the ``command="..."`` payload for an authorized_keys line.

    Runs the marker writer as ``<python> -m cssh_rs_e2e._marker_writer
    <marker>`` so it resolves through the installed package rather than a
    filesystem path.

    OpenSSH parses the value as a double-quoted string in which ``\\"``
    becomes a literal double quote and ``\\\\`` becomes a literal
    backslash. We therefore double every backslash and escape every
    inner quote so the interpreter and marker paths survive both sshd's
    parser and the downstream shell exec even when they contain spaces
    or backslashes.

    Args:
        executable: Python interpreter that runs the marker writer.
        marker: Absolute path of the alias's ``markers/<alias>.log``.

    Returns:
        The escaped ``command`` payload, without the enclosing quotes.
    """
    return (
        f"{_quote_authorized_keys_arg(executable)} -m cssh_rs_e2e._marker_writer "
        f"{_quote_authorized_keys_arg(marker)}"
    )


def _quote_authorized_keys_arg(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return f'\\"{escaped}\\"'


def _render_sshd_config(
    *,
    port: int,
    host_key: Path,
    authorized_keys: Path,
    pid_file: Path,
) -> str:
    return (
        f"Port {port}\n"
        "ListenAddress 127.0.0.1\n"
        f"HostKey {_as_forward_slash(host_key)}\n"
        f"AuthorizedKeysFile {_as_forward_slash(authorized_keys)}\n"
        "PasswordAuthentication no\n"
        "PubkeyAuthentication yes\n"
        "PermitRootLogin no\n"
        "ChallengeResponseAuthentication no\n"
        "KbdInteractiveAuthentication no\n"
        # StrictModes is disabled so the fixture works from any user-owned
        # temp directory; the per-run tree is unique and isolated anyway.
        "StrictModes no\n"
        f"PidFile {_as_forward_slash(pid_file)}\n"
        "LogLevel VERBOSE\n"
    )


def _render_ssh_config(
    *,
    aliases: list[str],
    port: int,
    keys_dir: Path,
    known_hosts: Path,
    user: str,
) -> str:
    blocks = []
    for alias in aliases:
        identity = keys_dir / f"{alias}_ed25519"
        blocks.append(
            f"Host {alias}\n"
            "    HostName 127.0.0.1\n"
            f"    Port {port}\n"
            f"    User {user}\n"
            f"    IdentityFile {_as_forward_slash(identity)}\n"
            "    IdentitiesOnly yes\n"
            "    StrictHostKeyChecking no\n"
            f"    UserKnownHostsFile {_as_forward_slash(known_hosts)}\n"
            "    BatchMode yes\n"
            "    LogLevel ERROR\n"
        )
    return "\n".join(blocks)


def _as_forward_slash(path: Path) -> str:
    return str(path).replace("\\", "/")


def _pick_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def _resolve_sshd_path() -> str:
    override = os.environ.get("CSSH_E2E_SSHD")
    if override:
        if not Path(override).is_file():
            raise SshdFixtureError(f"CSSH_E2E_SSHD points at non-existent path: {override}")
        return override
    resolved = shutil.which("sshd")
    if resolved:
        return resolved
    for candidate in DEFAULT_SSHD_LOCATIONS:
        if Path(candidate).is_file():
            return candidate
    raise SshdFixtureError("could not locate sshd; set CSSH_E2E_SSHD or install OpenSSH server")


def _wait_for_listening(
    port: int,
    process: subprocess.Popen[bytes],
    log_path: Path,
) -> None:
    deadline = time.monotonic() + READINESS_TIMEOUT_SECONDS
    while time.monotonic() < deadline:
        if process.poll() is not None:
            log_tail = _read_log_tail(log_path)
            raise SshdFixtureError(
                f"sshd exited with code {process.returncode} before binding port {port}\n{log_tail}"
            )
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.5):
                return
        except OSError:
            time.sleep(READINESS_POLL_INTERVAL_SECONDS)
    log_tail = _read_log_tail(log_path)
    raise SshdFixtureError(
        f"sshd did not start listening on port {port} within "
        f"{READINESS_TIMEOUT_SECONDS}s\n{log_tail}"
    )


def _read_log_tail(log_path: Path, max_bytes: int = 4096) -> str:
    try:
        data = log_path.read_bytes()
    except OSError:
        return ""
    return data[-max_bytes:].decode("utf-8", errors="replace")


def _smoke() -> int:
    """Bring the fixture up, drive ssh against each alias, tear down."""
    aliases = sys.argv[1:] or ["h1", "h2"]
    fixture = SshdFixture()
    info = fixture.start_sshd(aliases)
    print(f"fixture: {info}")
    try:
        ssh = shutil.which("ssh") or r"C:\Windows\System32\OpenSSH\ssh.exe"
        if not Path(ssh).exists():
            print(f"ssh client not found at {ssh}; skipping drive step")
        else:
            for alias in aliases:
                payload = f"hello-{alias}\n".encode()
                result = subprocess.run(
                    [ssh, "-F", str(info["ssh_config"]), alias],
                    input=payload,
                    capture_output=True,
                    timeout=10,
                    check=False,
                )
                print(
                    f"ssh {alias}: rc={result.returncode}, "
                    f"stderr={result.stderr.decode('utf-8', 'replace').strip()}"
                )
        for alias in aliases:
            print(f"markers/{alias}.log -> {fixture.read_marker(alias)!r}")
    finally:
        fixture.stop_sshd()
    return 0


if __name__ == "__main__":
    raise SystemExit(_smoke())
