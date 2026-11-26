//! Run the workspace's tests for a target, in the test VM of [`crate::vm`] on a Linux host.

pub mod system;

use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::cross_build::{prepare_toolchain, run_cargo_for, Target, Toolchain};
use crate::vm::{self, SshEndpoint};
pub use system::CrossTestSystem;

/// xtask subcommand that cargo invokes as the target runner.
pub const RUNNER_SUBCOMMAND: &str = "cross-test-runner";

/// Directory in the VM that holds the test binaries while they run, under
/// the [`vm::VM_ROOT`] that Defender skips.
static REMOTE_ROOT: LazyLock<String> = LazyLock::new(|| format!(r"{}\cross-test", vm::VM_ROOT));

/// Directory in the VM that holds one mirror of the source files per
/// worktree, under the [`vm::VM_ROOT`] that Defender skips.
static MIRRORS_ROOT: LazyLock<String> = LazyLock::new(|| format!(r"{}\workspaces", vm::VM_ROOT));

const CLEANUP_ATTEMPTS: u32 = 5;

/// Host environment variables the runner sets for each test binary in the
/// VM, whose SSH session inherits none from the host.
const FORWARDED_VARS: [&str; 2] = ["RUST_BACKTRACE", "RUST_TEST_THREADS"];

/// Directory under the workspace root for the worktree lock and the
/// workspace archive.
const SCRATCH_DIR: &str = "target/cross-test";

/// Run the workspace's tests for `target`.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - Target to build and run the tests for.
/// * `with_vm` - Start the test VM if it is not running, to stop after the
///   last run using it.
/// * `cargo_args` - Arguments passed through to `cargo test`.
///
/// # Returns
/// `Ok(())` once every test passes.
///
/// # Errors
/// Returns an error if the host is unsupported, a prerequisite is missing,
/// the test VM is not running, or the tests fail.
pub fn run_cross_test<S: CrossTestSystem>(
    system: &S,
    target: Target,
    with_vm: bool,
    cargo_args: &[String],
) -> Result<()> {
    log::info!(
        "Cross-testing {} on {} host",
        target.triple(),
        system.host_os()
    );
    match target {
        Target::X86_64PcWindowsMsvc => cross_test_windows_msvc(system, with_vm, cargo_args),
    }
}

/// Run one test binary in the test VM and return its exit code.
///
/// `cross-test` registers this as cargo's runner for `target`, so cargo
/// calls it once per test binary with the binary's arguments.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - Target the binary was built for.
/// * `command` - The test binary's host path, followed by its arguments.
///
/// # Returns
/// The exit code of the test binary in the VM.
///
/// # Errors
/// Returns an error if an argument or a forwarded environment variable
/// cannot pass through `cmd.exe`, the VM is not running, the binary
/// cannot be copied into it, or cargo runs it outside the workspace.
pub fn run_test_binary<S: CrossTestSystem>(
    system: &S,
    target: Target,
    command: &[String],
) -> Result<i32> {
    let (binary, args) = command
        .split_first()
        .context("cargo passed the runner no test binary")?;
    let binary = Path::new(binary);
    let name = binary
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("{} has no file name", binary.display()))?;
    let args = args
        .iter()
        .map(|arg| quote_cmd_arg(arg))
        .collect::<Result<Vec<_>>>()?;
    let vars = forwarded_vars(system)?;
    let endpoint = vm::connect(system, target)?;
    let dir = format!(r"{}\{}", *REMOTE_ROOT, system.process_id());
    // A killed run leaves its directory behind, and process IDs are reused.
    run_remote(
        system,
        &endpoint,
        &format!(r#"if not exist "{dir}" mkdir "{dir}""#),
    )?;
    let remote_binary = format!(r"{dir}\{name}");
    let result = copy_and_run(system, &endpoint, binary, &remote_binary, &vars, &args);
    remove_remote_dir(system, &endpoint, &dir);
    result
}

fn cross_test_windows_msvc<S: CrossTestSystem>(
    system: &S,
    with_vm: bool,
    cargo_args: &[String],
) -> Result<()> {
    let target = Target::X86_64PcWindowsMsvc;
    if system.host_os() != "windows" {
        vm::require_vm_host(system, target)?;
        return test_in_vm(system, target, with_vm, cargo_args);
    }
    if with_vm {
        log::info!("A Windows host runs the tests natively; ignoring --with-vm");
    }
    let toolchain = prepare_toolchain(system, target)?;
    run_cargo_for(system, toolchain, "test", cargo_args, &[])
}

/// Build the tests with cargo-xwin and run them in the test VM, failing on
/// any missing prerequisite before the VM starts or the build begins.
fn test_in_vm<S: CrossTestSystem>(
    system: &S,
    target: Target,
    with_vm: bool,
    cargo_args: &[String],
) -> Result<()> {
    let root = system.workspace_root()?;
    let scratch = root.join(SCRATCH_DIR);
    system.create_dir_all(&scratch)?;
    // Runs in different worktrees share the VM, but two runs in one worktree
    // would mirror the workspace to the same path in it.
    let Some(_worktree_lock) = system.try_lock_exclusive(&scratch.join("worktree.lock"))? else {
        bail!(
            "a cross-test is already running in this worktree ({}); wait for it to finish",
            root.display()
        );
    };
    let runner = runner_command(system, target)?;
    vm::check_host_prerequisites(system)?;
    let lease = vm::acquire_lease(system)?;
    let result = prepare_and_test(system, target, with_vm, cargo_args, &runner);
    let released = vm::release_lease(system, target, lease);
    if let (Err(_), Err(err)) = (&result, &released) {
        log::error!("{err:#}");
    }
    result.and(released)
}

/// Prepare the toolchain and make sure the test VM runs, starting it if
/// `with_vm` and failing before the build otherwise, then run the tests.
fn prepare_and_test<S: CrossTestSystem>(
    system: &S,
    target: Target,
    with_vm: bool,
    cargo_args: &[String],
    runner: &str,
) -> Result<()> {
    let toolchain = if with_vm {
        let toolchain = prepare_toolchain(system, target)?;
        vm::start_for_run(system, target)?;
        toolchain
    } else {
        vm::connect(system, target)?;
        prepare_toolchain(system, target)?
    };
    test_in_running_vm(system, target, toolchain, cargo_args, runner)
}

/// Mirror the workspace into the running VM and run `cargo test` with
/// `runner` as cargo's target runner.
fn test_in_running_vm<S: CrossTestSystem>(
    system: &S,
    target: Target,
    toolchain: Toolchain,
    cargo_args: &[String],
    runner: &str,
) -> Result<()> {
    let endpoint = vm::connect(system, target)?;
    vm::wait_until_ready(system, target, &endpoint)?;
    mirror_workspace(system, &endpoint)?;
    // cargo-xwin sets this to `wine` only while it is unset:
    // https://github.com/rust-cross/cargo-xwin/blob/v0.23.0/src/test.rs#L44-L47
    let runner_var = format!(
        "CARGO_TARGET_{}_RUNNER",
        target.triple().to_uppercase().replace('-', "_")
    );
    log::info!("Running each test binary in the test VM");
    run_cargo_for(
        system,
        toolchain,
        "test",
        cargo_args,
        &[(runner_var, runner.to_owned())],
    )
}

/// Return the runner command that points cargo back at this xtask.
fn runner_command<S: CrossTestSystem>(system: &S, target: Target) -> Result<String> {
    let exe = system.current_exe()?;
    // cargo splits a runner given as a string, as this one is, at spaces:
    // https://doc.rust-lang.org/cargo/reference/config.html#executable-paths-with-arguments
    let path = exe
        .to_str()
        .filter(|path| !path.contains(char::is_whitespace))
        .with_context(|| {
            format!(
                "cargo cannot run {} as a test runner: its path holds whitespace \
                 or is not UTF-8. Move the checkout to a plain path",
                exe.display()
            )
        })?;
    Ok(format!("{path} {RUNNER_SUBCOMMAND} {}", target.triple()))
}

/// Copy the workspace's source files into the worktree's mirror in the VM,
/// replacing the previous copy.
fn mirror_workspace<S: CrossTestSystem>(system: &S, endpoint: &SshEndpoint) -> Result<()> {
    let root = system.workspace_root()?;
    let remote_root = vm_mirror(&root);
    let archive = root.join(SCRATCH_DIR).join("workspace.tar");
    let remote_archive = format!(r"{}\workspace-{}.tar", *REMOTE_ROOT, system.process_id());
    log::info!("Mirroring the workspace's source files to {remote_root} in the test VM");
    system.create_dir_all(archive.parent().context("archive path has no parent")?)?;
    system.pack_workspace(&root, &archive)?;
    run_remote(
        system,
        endpoint,
        &format!(
            r#"(if exist "{remote_root}" rmdir /s /q "{remote_root}") && mkdir "{remote_root}" && (if not exist "{binaries}" mkdir "{binaries}")"#,
            binaries = *REMOTE_ROOT,
        ),
    )?;
    system.scp(endpoint, &archive, &remote_archive)?;
    run_remote(
        system,
        endpoint,
        &format!(r#"tar -xf "{remote_archive}" -C "{remote_root}" && del "{remote_archive}""#),
    )
}

fn copy_and_run<S: CrossTestSystem>(
    system: &S,
    endpoint: &SshEndpoint,
    binary: &Path,
    remote_binary: &str,
    vars: &[(&str, String)],
    args: &[String],
) -> Result<i32> {
    system.scp(endpoint, binary, remote_binary)?;
    let root = system.workspace_root()?;
    let workspace = vm_mirror(&root);
    let cwd = vm_cwd(&workspace, &root, &system.current_dir()?)?;
    // insta finds the workspace root with `cargo metadata`, which the VM
    // lacks; https://insta.rs/docs/advanced/#workspace-root
    // The command must not start with a quote, which `cmd /c` would strip:
    // https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/cmd
    let mut command = format!(r#"set "INSTA_WORKSPACE_ROOT={workspace}""#);
    for (name, value) in vars {
        command.push_str(&format!(r#" && set "{name}={value}""#));
    }
    command.push_str(&format!(r#" && cd /d "{cwd}" && "{remote_binary}""#));
    for arg in args {
        command.push(' ');
        command.push_str(arg);
    }
    system.ssh(endpoint, &command)
}

/// Run `command` in the VM and fail unless it exits with 0.
fn run_remote<S: CrossTestSystem>(system: &S, endpoint: &SshEndpoint, command: &str) -> Result<()> {
    let code = system.ssh(endpoint, command)?;
    if code != 0 {
        bail!("`{command}` failed in the test VM with exit code {code}");
    }
    Ok(())
}

/// Remove `dir` from the VM, retrying while Windows still holds the file of
/// a test binary that just exited, and logging a warning if it stays.
fn remove_remote_dir<S: CrossTestSystem>(system: &S, endpoint: &SshEndpoint, dir: &str) {
    for attempt in 1..=CLEANUP_ATTEMPTS {
        match system.ssh(endpoint, &format!(r#"rmdir /s /q "{dir}" 2>nul"#)) {
            Ok(0) => return,
            Ok(_) if attempt < CLEANUP_ATTEMPTS => system.sleep(Duration::from_secs(1)),
            Ok(code) => log::warn!("failed to remove {dir} from the test VM (exit code {code})"),
            Err(err) => {
                log::warn!("failed to remove {dir} from the test VM: {err:#}");
                return;
            }
        }
    }
}

/// Return the directory in the VM that mirrors the worktree at `root`.
///
/// It is named by a hash of `root` rather than mirroring that path, where a
/// worktree nested in another, such as `.claude/worktrees/*`, would lie in
/// the other's mirror, which every run there replaces.
fn vm_mirror(root: &Path) -> String {
    // 64-bit FNV-1a: unlike std's hashers, stable across Rust releases, so a
    // worktree keeps its mirror.
    // http://www.isthe.com/chongo/tech/comp/fnv/index.html#FNV-param
    let hash = root
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
        });
    format!(r"{}\{hash:016x}", *MIRRORS_ROOT)
}

/// Return the directory in the mirror at `workspace` that matches `cwd` in
/// the worktree at `root`, where cargo runs a test binary.
fn vm_cwd(workspace: &str, root: &Path, cwd: &Path) -> Result<String> {
    let relative = cwd.strip_prefix(root).with_context(|| {
        format!(
            "cargo runs the test binary in {}, outside the workspace {}, which \
             the test VM does not mirror",
            cwd.display(),
            root.display()
        )
    })?;
    let relative = relative.to_string_lossy().replace('/', "\\");
    Ok(if relative.is_empty() {
        workspace.to_owned()
    } else {
        format!(r"{workspace}\{relative}")
    })
}

/// Return the [`FORWARDED_VARS`] set on the host, failing on a value
/// `cmd.exe` cannot pass.
fn forwarded_vars<S: CrossTestSystem>(system: &S) -> Result<Vec<(&'static str, String)>> {
    FORWARDED_VARS
        .into_iter()
        .filter_map(|name| Some((name, system.env_var(name)?)))
        .map(|(name, value)| {
            require_cmd_safe(&format!("{name}={value}"))?;
            Ok((name, value))
        })
        .collect()
}

/// Fail if `text` holds a character `cmd.exe` cannot pass through reliably.
fn require_cmd_safe(text: &str) -> Result<()> {
    if text.contains(['"', '%', '\n', '\r']) {
        bail!(
            "cannot pass {text:?} to a test binary in the test VM: cmd.exe has no \
             reliable escape for '\"', '%' or line breaks"
        );
    }
    Ok(())
}

/// Quote `arg` for a `cmd.exe` command line that the C runtime of the test
/// binary splits back into the same argument.
fn quote_cmd_arg(arg: &str) -> Result<String> {
    require_cmd_safe(arg)?;
    let plain =
        !arg.is_empty() && !arg.contains(|c: char| c.is_whitespace() || "&|<>^".contains(c));
    if plain {
        return Ok(arg.to_owned());
    }
    // Inside quotes, backslashes before the closing quote must be doubled.
    // https://learn.microsoft.com/en-us/cpp/c-language/parsing-c-command-line-arguments
    let trailing = arg.len() - arg.trim_end_matches('\\').len();
    Ok(format!("\"{arg}{}\"", "\\".repeat(trailing)))
}

#[cfg(test)]
#[path = "../tests/test_cross_test.rs"]
mod tests;
