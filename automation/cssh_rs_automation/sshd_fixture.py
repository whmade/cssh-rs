"""Per-run user-mode sshd fixture for the cssh-rs Windows E2E suite.

This Robot Framework library starts a fresh OpenSSH ``sshd`` on
``127.0.0.1`` at a high TCP port for one test suite, with no elevation and
no shared system state. Each host alias gets its own Ed25519 keypair whose
``authorized_keys`` entry pins a ``command="..."`` restriction piping the
SSH channel's stdin into ``markers/<alias>.log``; suites assert cssh-rs
keystroke broadcast by reading those marker files.

All per-run state lives in one temp directory that ``Stop Sshd`` removes.
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
from dataclasses import dataclass
from pathlib import Path

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

# Fallback sshd.exe locations on Windows, tried when it is not on PATH.
DEFAULT_SSHD_LOCATIONS = (
    r"C:\Windows\System32\OpenSSH\sshd.exe",
    r"C:\Program Files\OpenSSH\sshd.exe",
)

# Fallback Git-for-Windows bash locations on Windows, tried as a last resort.
DEFAULT_BASH_LOCATIONS = (
    r"C:\Program Files\Git\bin\bash.exe",
    r"C:\Program Files (x86)\Git\bin\bash.exe",
)

READINESS_TIMEOUT_SECONDS = 10.0
READINESS_POLL_INTERVAL_SECONDS = 0.1
STOP_GRACE_SECONDS = 3.0
MAX_LOG_TAIL_BYTES = 4096

REMOVE_TEMPDIR_ATTEMPTS = 5
REMOVE_TEMPDIR_RETRY_SECONDS = 0.2

# Aliases are interpolated into filenames and the generated ssh_config, so
# reject path separators, whitespace and control characters.
ALIAS_PATTERN = re.compile(r"^[A-Za-z0-9._-]+$")

# A caller-supplied port must be unprivileged so the fixture binds without admin.
MIN_UNPRIVILEGED_PORT = 1024
MAX_TCP_PORT = 65535


class SshdFixtureError(RuntimeError):
    """Raised when the fixture cannot start or shut down sshd cleanly."""


@dataclass(frozen=True)
class MarkerMode:
    """Force a command piping each session's stdin into its marker file (default)."""


@dataclass(frozen=True)
class ShellMode:
    """Drop the forced command so sessions land in the user's real login shell."""


@dataclass(frozen=True)
class ScriptedShellMode:
    """Run interactive bash from a generated rc that signals readiness once set up.

    Args:
        rc_lines: Extra bash appended to each session's rc, e.g. shell aliases.
    """

    rc_lines: str = ""


SshdMode = MarkerMode | ShellMode | ScriptedShellMode


class SshdFixture:
    """Robot Framework library that owns a per-run user-mode sshd."""

    ROBOT_LIBRARY_SCOPE = "SUITE"
    ROBOT_LIBRARY_VERSION = "0.1.0"

    def __init__(self) -> None:
        self._tempdir: Path | None = None
        self._process: subprocess.Popen[bytes] | None = None

    def start_sshd(
        self,
        host_aliases: list[str] | tuple[str, ...],
        port: int | None = None,
        mode: SshdMode | None = None,
    ) -> dict[str, object]:
        """Bring up sshd for ``host_aliases`` and return runtime paths.

        Args:
            host_aliases: Logical host names exposed in the generated
                ``ssh_config``. Each alias gets its own keypair and its
                own ``markers/<alias>.log`` marker file.
            port: Specific TCP port to bind. ``None`` picks a free
                ephemeral high port.
            mode: Session mode: ``MarkerMode`` (default) pipes each session's
                stdin into its marker file, ``ShellMode`` yields the user's
                real login shell, ``ScriptedShellMode`` runs a scripted bash
                that signals readiness. Defaults to ``MarkerMode``.

        Returns:
            A dict with keys ``port`` (int), ``ssh_config`` (str),
            ``markers_dir`` (str), ``aliases`` (list of str), ``tempdir``
            (str) and ``homes`` (alias -> home directory, empty unless
            ``ScriptedShellMode``), populated after sshd is accepting connections.
        """
        if self._process is not None:
            raise SshdFixtureError("sshd fixture already running")
        if mode is None:
            mode = MarkerMode()
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
        if port is not None and not MIN_UNPRIVILEGED_PORT <= port <= MAX_TCP_PORT:
            raise SshdFixtureError(
                f"port must be between {MIN_UNPRIVILEGED_PORT} and {MAX_TCP_PORT}: {port}"
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
            homes = _write_authorized_keys(
                authorized_keys_path,
                aliases,
                keys_dir=keys_dir,
                markers_dir=markers_dir,
                homes_dir=tempdir / "homes",
                mode=mode,
            )

            chosen_port = port if port is not None else _pick_free_port()
            sshd_config_path = tempdir / "sshd_config"
            sshd_log_path = tempdir / "sshd.log"
            sshd_config_path.write_text(
                _render_sshd_config(
                    port=chosen_port,
                    host_key=host_key_path,
                    authorized_keys=authorized_keys_path,
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

        return {
            "port": chosen_port,
            "ssh_config": str(ssh_config_path),
            "markers_dir": str(markers_dir),
            "aliases": aliases,
            "tempdir": str(tempdir),
            "homes": homes,
        }

    def stop_sshd(self) -> None:
        """Terminate the sshd process tree and remove the per-run temp directory."""
        _terminate_tree(self._process)
        _remove_tempdir(self._tempdir)
        self._process = None
        self._tempdir = None

    def count_connected_markers(self) -> int:
        """Return how many host markers exist, one per connected ssh session.

        A marker exists once the session's forced command runs (the marker
        writer, or the interactive shell's readiness line), so its presence
        proves the session authenticated; this globs marker files only, never
        the read-share-locked sshd.log.

        Returns:
            Count of existing marker files, or 0 before any session connects.
        """
        if self._tempdir is None:
            raise SshdFixtureError("sshd fixture is not running")
        markers_dir = self._tempdir / "markers"
        return len(list(markers_dir.glob("*.log")))

    def read_marker(self, alias: str) -> str:
        """Return the contents of ``markers/<alias>.log`` as UTF-8 text.

        Args:
            alias: Host alias whose marker is read.

        Returns:
            Marker file contents decoded with ``errors='replace'``, or the
            empty string if the file does not exist yet.
        """
        if self._tempdir is None:
            raise SshdFixtureError("sshd fixture is not running")
        marker = self._tempdir / "markers" / f"{alias}.log"
        if not marker.exists():
            return ""
        return marker.read_bytes().decode("utf-8", errors="replace")


def _terminate(process: subprocess.Popen[bytes] | None) -> None:
    """Stop ``process`` gracefully, escalating to kill after the grace period."""
    if process is None or process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=STOP_GRACE_SECONDS)
    except subprocess.TimeoutExpired:
        process.kill()
        # A process stuck in uninterruptible sleep may not reap even after
        # kill; teardown still removes the temp tree, so a lingering timeout
        # here must not abort it.
        with contextlib.suppress(subprocess.TimeoutExpired):
            process.wait(timeout=STOP_GRACE_SECONDS)


def _terminate_tree(process: subprocess.Popen[bytes] | None) -> None:
    """Kill ``process`` and its children.

    The per-connection sshd children hold the marker files open, so on Windows
    ``taskkill /F /T`` must tear down the whole tree; a plain ``terminate`` on
    the listener PID would leave them running.
    """
    if process is None or process.poll() is not None:
        return
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/F", "/T", "/PID", str(process.pid)],
            capture_output=True,
            check=False,
        )
        with contextlib.suppress(subprocess.TimeoutExpired):
            process.wait(timeout=STOP_GRACE_SECONDS)
        if process.poll() is not None:
            return
    _terminate(process)


def _remove_tempdir(tempdir: Path | None) -> None:
    """Remove ``tempdir`` unless ``CSSH_E2E_KEEP_TEMP=1`` keeps it for debugging.

    Retries because Windows releases a killed child's file handles asynchronously,
    so the first rmtree can still hit a locked marker.
    """
    if tempdir is None or os.environ.get("CSSH_E2E_KEEP_TEMP") == "1":
        return
    for attempt in range(REMOVE_TEMPDIR_ATTEMPTS):
        shutil.rmtree(tempdir, ignore_errors=True)
        if not tempdir.exists():
            return
        if attempt < REMOVE_TEMPDIR_ATTEMPTS - 1:
            time.sleep(REMOVE_TEMPDIR_RETRY_SECONDS)


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

    POSIX uses ``chmod 0600``. Windows ignores the POSIX mode, so ``icacls``
    strips inheritance and the temp tree's OWNER RIGHTS (``S-1-3-4``) ACE and
    grants the current user sole access, then re-reads the ACL to fail loudly
    if any wider grant survives.
    """
    if os.name != "nt":
        with contextlib.suppress(OSError):
            private_path.chmod(0o600)
        return
    target = str(private_path)
    user = getpass.getuser()
    for args in (
        ["icacls", target, "/inheritance:r"],
        ["icacls", target, "/remove:g", "*S-1-3-4"],
        ["icacls", target, "/grant:r", f"{user}:F"],
    ):
        result = subprocess.run(args, capture_output=True, check=False)
        if result.returncode != 0:
            detail = result.stderr.decode("utf-8", errors="replace").strip()
            raise SshdFixtureError(f"failed to harden key permissions on {target}: {detail}")

    listing = subprocess.run(["icacls", target], capture_output=True, check=False)
    acl = listing.stdout.decode("utf-8", errors="replace")
    if "S-1-3-4" in acl or "OWNER RIGHTS" in acl:
        raise SshdFixtureError(
            f"OWNER RIGHTS ACE still present on {target} after hardening:\n{acl}"
        )


def _write_authorized_keys(
    path: Path,
    aliases: list[str],
    *,
    keys_dir: Path,
    markers_dir: Path,
    homes_dir: Path,
    mode: SshdMode,
) -> dict[str, str]:
    """Write one authorized_keys line per alias; return scripted-shell home dirs by alias."""
    homes: dict[str, str] = {}
    with path.open("w", encoding="utf-8", newline="\n") as handle:
        for alias in aliases:
            alias_key_path = keys_dir / f"{alias}_ed25519"
            _write_openssh_keypair(alias_key_path)
            pubkey = alias_key_path.with_suffix(".pub").read_text(encoding="utf-8").strip()
            marker_path = markers_dir / f"{alias}.log"
            match mode:
                case ScriptedShellMode(rc_lines=rc_lines):
                    home = homes_dir / alias
                    home.mkdir(parents=True)
                    bash_rc = home / ".bashrc"
                    _write_bash_rc(
                        bash_rc,
                        prompt_host=alias,
                        home=home,
                        marker_path=marker_path,
                        rc_lines=rc_lines,
                    )
                    homes[alias] = str(home)
                    forced_command = _build_bash_command(_resolve_bash_path(), bash_rc)
                case ShellMode():
                    forced_command = None
                case MarkerMode():
                    forced_command = _build_forced_command(sys.executable, str(marker_path))
            handle.write(_authorized_keys_line(pubkey, forced_command))
    return homes


def _authorized_keys_line(pubkey: str, forced_command: str | None) -> str:
    """Return the ``authorized_keys`` line, prefixing ``command="..."`` when forced.

    ``forced_command`` is ``None`` for a real login shell; otherwise it is the
    marker-writer or interactive-bash command to force.
    """
    # No no-pty: keeping the PTY lets a relayed Ctrl+C interrupt the remote.
    options = "no-port-forwarding,no-x11-forwarding,no-agent-forwarding,no-user-rc"
    if forced_command is None:
        return f"{options} {pubkey}\n"
    return f'command="{forced_command}",{options} {pubkey}\n'


def _build_forced_command(executable: str, marker: str) -> str:
    """Return the authorized_keys ``command="..."`` payload (without the outer
    quotes) that runs ``<executable> -m cssh_rs_automation._marker_writer <marker>``.
    """
    return (
        f"{_quote_authorized_keys_arg(executable)} -m cssh_rs_automation._marker_writer "
        f"{_quote_authorized_keys_arg(marker)}"
    )


def _build_bash_command(bash_path: str, rc_path: Path) -> str:
    """Return the command= payload running ``bash_path`` interactively with ``rc_path``."""
    quoted_bash = _quote_authorized_keys_arg(bash_path.replace("\\", "/"))
    return f"{quoted_bash} --rcfile {_quote_authorized_keys_arg(_as_forward_slash(rc_path))} -i"


def _write_bash_rc(
    rc_path: Path, *, prompt_host: str, home: Path, marker_path: Path, rc_lines: str
) -> None:
    """Write a per-host bash rc: root@<host> prompt, host home as ~, extra lines, ready marker."""
    # Set HOME to $PWD after cd so \w abbreviates the canonicalized home to ~.
    body = [
        f'cd "{_as_forward_slash(home)}"',
        'export HOME="$PWD"',
        f"export PS1='root@{prompt_host}:\\w\\$ '",
    ]
    if rc_lines:
        body.append(rc_lines)
    # Mark ready from the first prompt, not mid-rc, so early keystrokes are not lost.
    marker = _as_forward_slash(marker_path)
    body.append(f"""PROMPT_COMMAND='echo ready > "{marker}"; unset PROMPT_COMMAND'""")
    rc_path.write_text("\n".join(body) + "\n", encoding="utf-8", newline="\n")


def _resolve_bash_path() -> str:
    """Return an absolute bash path for ScriptedShellMode, honoring ``CSSH_E2E_BASH``.

    On Windows a bare ``bash`` resolves via PATH to WSL's ``System32\\bash.exe``,
    which ignores the Windows-path ``--rcfile`` and never writes the readiness
    marker, so PATH ``bash`` is never trusted there.
    """
    override = os.environ.get("CSSH_E2E_BASH")
    if override:
        if not Path(override).is_file():
            raise SshdFixtureError(f"CSSH_E2E_BASH points at non-existent path: {override}")
        return str(Path(override).resolve())
    if os.name != "nt":
        resolved = shutil.which("bash")
        if resolved:
            return resolved
        raise SshdFixtureError("could not locate bash; set CSSH_E2E_BASH or install bash")
    candidates: list[str] = []
    git = shutil.which("git")
    if git:
        # <Git>\cmd\git.exe or <Git>\bin\git.exe -> <Git>\bin\bash.exe
        candidates.append(str(Path(git).resolve().parent.parent / "bin" / "bash.exe"))
    local_app_data = os.environ.get("LOCALAPPDATA")
    if local_app_data:
        candidates.append(str(Path(local_app_data) / "Programs" / "Git" / "bin" / "bash.exe"))
    candidates.extend(DEFAULT_BASH_LOCATIONS)
    for candidate in candidates:
        if Path(candidate).is_file():
            return candidate
    raise SshdFixtureError(
        "could not locate Git for Windows bash (WSL bash is unsuitable for "
        "ScriptedShellMode); set CSSH_E2E_BASH or install Git for Windows"
    )


def _quote_authorized_keys_arg(value: str) -> str:
    """Quote ``value`` as one token inside sshd's double-quoted command= string.

    The command= value is itself double-quoted, so backslashes are doubled and
    inner quotes escaped; paths with spaces or backslashes then survive intact.
    """
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return f'\\"{escaped}\\"'


def _render_sshd_config(
    *,
    port: int,
    host_key: Path,
    authorized_keys: Path,
) -> str:
    return (
        f"Port {port}\n"
        "ListenAddress 127.0.0.1\n"
        f"HostKey {_as_forward_slash(host_key)}\n"
        f"AuthorizedKeysFile {_as_forward_slash(authorized_keys)}\n"
        "PubkeyAuthentication yes\n"
        "PasswordAuthentication no\n"
        "PermitRootLogin no\n"
        # StrictModes is disabled so the fixture works from any user-owned
        # temp directory; the per-run tree is unique and isolated anyway.
        "StrictModes no\n"
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
            # Force a PTY (the server grants one, as in real use) so a relayed
            # Ctrl+C forwards to the remote as a real interrupt. The server
            # granting it avoids the "PTY allocation request failed" noise a
            # refusal would print (PR #232).
            "    RequestTTY force\n"
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


def _read_log_tail(log_path: Path) -> str:
    """Return up to the last ``MAX_LOG_TAIL_BYTES`` of ``log_path``, or ``""`` on error."""
    try:
        with log_path.open("rb") as handle:
            handle.seek(0, os.SEEK_END)
            handle.seek(max(0, handle.tell() - MAX_LOG_TAIL_BYTES))
            data = handle.read()
    except OSError:
        return ""
    return data.decode("utf-8", errors="replace")
