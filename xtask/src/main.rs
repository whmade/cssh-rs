//! xtask - developer automation tasks for cssh-rs.
//!
//! Invoke via `cargo xtask <subcommand>`.
//! See each subcommand's module for details.

#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod changelog;
mod coverage;
mod cross_build;
mod cross_test;
mod inject_agent_token;
mod readme;
mod record_demo;
mod release;
mod social_preview;
mod typography;
mod vm;
mod worktree_teardown;

#[cfg(test)]
#[path = "tests/mock_system.rs"]
mod mock_system;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use log::LevelFilter;
use simplelog::{format_description, ColorChoice, ConfigBuilder, TermLogger, TerminalMode};

/// Developer automation tasks for cssh-rs.
#[derive(Parser)]
#[clap(name = "xtask")]
struct Args {
    /// Increase log verbosity (`-v` = debug, `-vv` = trace). Overridden
    /// by `RUST_LOG` when that env var parses as a log level.
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[clap(subcommand)]
    command: Command,
}

/// Available xtask subcommands.
#[derive(Subcommand)]
enum Command {
    /// Verify the README help section matches `cargo run --package cssh-rs -- --help`.
    CheckReadmeHelp,
    /// Update the README help section to match `cargo run --package cssh-rs -- --help`.
    UpdateReadmeHelp,
    /// Generate changelog for the current version from news fragments.
    GenerateChangelog,
    /// Prepare a new release: bump version, create maintenance branch, commit, push.
    PrepareRelease,
    /// Create and push an annotated git release tag for the current version.
    CreateReleaseTag,
    /// Render the 1280x640 social preview PNG from templates/social-preview.html
    /// using headless Chromium via the pinned Playwright Docker image.
    GenerateSocialPreview {
        /// Output path for the generated PNG. Relative paths resolve
        /// against the workspace root; absolute paths are accepted as
        /// long as they live under the workspace root so the container
        /// bind mount can reach them. Defaults to
        /// `target/social-preview/social-preview.png`.
        #[arg(long)]
        out: Option<PathBuf>,
        /// GitHub token used for authenticated API requests. Falls back to
        /// the `GITHUB_TOKEN` environment variable, then to unauthenticated
        /// access (rate-limited to 60 requests/hour).
        #[arg(long)]
        token: Option<String>,
    },
    /// Run coverage analysis using a pinned nightly toolchain.
    Coverage,
    /// Inject a contributor-supplied fine-grained GitHub PAT into the
    /// current worktree's `.claude/settings.local.json` so paseo-spawned
    /// agents act with a least-privilege token instead of the user's
    /// full `gh` login. A no-op when `.paseo/gh-token` is absent.
    InjectAgentToken,
    /// Scan tracked text files for forbidden decorative Unicode
    /// punctuation and fail with a list of offending locations.
    CheckTypography,
    /// Tear down a paseo worktree: detach `HEAD` in the worktree and
    /// force-delete its branch from the source checkout. Reads
    /// `PASEO_WORKTREE_PATH`, `PASEO_SOURCE_CHECKOUT_PATH`, and
    /// `PASEO_BRANCH_NAME` from the environment. Designed to be
    /// invoked from `paseo.json`'s `worktree.teardown` so the same
    /// entry works on Windows (PowerShell) and Linux/macOS (bash).
    WorktreeTeardown,
    /// Cross-build cssh-rs, or the packages that the arguments after `--`
    /// select, for the given target.
    /// Detects the host and dispatches to the right toolchain (native
    /// `cargo build`, with `--target`, or `cargo xwin build`); installs
    /// the Rust target and any required helper crate on first use.
    /// Defaults to a debug build; pass `--release` for an optimized
    /// binary.
    CrossBuild {
        /// Target triple to build for. Run with `--help` for the
        /// list of supported targets.
        #[arg(value_enum)]
        target: cross_build::Target,
        /// Build with `--release` for an optimized binary. Defaults
        /// to a debug build so contributor iteration stays fast.
        #[arg(short, long)]
        release: bool,
        /// Arguments for `cargo build`, after `--`, for example
        /// `-- --workspace --all-targets`. If they select packages, they
        /// replace the default `--package cssh-rs`.
        #[arg(last = true)]
        cargo_args: Vec<String>,
    },
    /// Lint the workspace for the given target. On a Windows x64 host
    /// with the MSVC toolchain this is `cargo lint`; other hosts add
    /// `--target` or go through cargo-xwin.
    CrossLint {
        /// Target triple to lint for.
        #[arg(value_enum)]
        target: cross_build::Target,
    },
    /// Document the workspace for the given target. On a Windows x64
    /// host with the MSVC toolchain this is `cargo docs`; other hosts add
    /// `--target` or go through cargo-xwin.
    CrossDoc {
        /// Target triple to document for.
        #[arg(value_enum)]
        target: cross_build::Target,
    },
    /// Run the workspace's tests for the given target. A Windows host
    /// runs native `cargo test`; a Linux host builds the tests with
    /// cargo-xwin and runs each test binary in the test VM of
    /// `cargo xtask vm start`, which must be running unless `--with-vm`
    /// is passed.
    CrossTest {
        /// Target triple to test. Run with `--help` for the list of
        /// supported targets.
        #[arg(value_enum)]
        target: cross_build::Target,
        /// Start the test VM for this run and stop it afterwards. A VM
        /// that is already running is reused and left running.
        #[arg(long)]
        with_vm: bool,
        /// Arguments for `cargo test`, after `--`, for example
        /// `-- -p cssh-rs-core input`.
        #[arg(last = true)]
        cargo_args: Vec<String>,
    },
    /// Run one test binary in the test VM. `cross-test` sets this as
    /// cargo's target runner.
    #[command(name = cross_test::RUNNER_SUBCOMMAND, hide = true)]
    CrossTestRunner {
        /// Target the test binary was built for.
        #[arg(value_enum)]
        target: cross_build::Target,
        /// The test binary's path, followed by its arguments.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Start or stop the VM that `cross-test` runs a target's tests in on
    /// a Linux host.
    Vm {
        #[clap(subcommand)]
        action: VmAction,
    },
    /// Record the cssh-rs demo GIF into `target/demo/cssh-rs.gif` by
    /// building the binary, driving real console windows and capturing
    /// the desktop. Windows only.
    RecordDemo,
}

/// Actions on the test VM.
#[derive(Subcommand)]
enum VmAction {
    /// Start the test VM for the given target and wait until it accepts
    /// SSH. The first start installs Windows, which takes 20 to 60
    /// minutes; later starts reuse the installed disk.
    Start {
        /// Target whose tests the VM runs.
        #[arg(value_enum)]
        target: cross_build::Target,
    },
    /// Stop the test VM for the given target. Its disk persists for the
    /// next start.
    Stop {
        /// Target whose tests the VM runs.
        #[arg(value_enum)]
        target: cross_build::Target,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // {:#} prints the full anyhow error chain. TermLogger colors
            // the ERROR level red on TTYs and emits plain text on non-TTYs.
            log::error!("{err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    init_logger(args.verbose);
    match args.command {
        Command::CheckReadmeHelp => {
            readme::check_readme_help(&readme::RealSystem)?;
        }
        Command::UpdateReadmeHelp => {
            let changed = readme::update_readme_help(&readme::RealSystem)?;
            if changed {
                // Exit 1 to abort the pre-commit hook when the README was modified.
                std::process::exit(1);
            }
        }
        Command::GenerateChangelog => {
            changelog::generate_changelog(&changelog::RealSystem)?;
        }
        Command::PrepareRelease => {
            release::prepare_release(&release::RealSystem)?;
        }
        Command::CreateReleaseTag => {
            release::create_release_tag(&release::RealSystem)?;
        }
        Command::GenerateSocialPreview { out, token } => {
            social_preview::generate_social_preview(&social_preview::RealSystem, out, token)?;
        }
        Command::Coverage => {
            coverage::run_coverage(&coverage::RealSystem)?;
        }
        Command::InjectAgentToken => {
            inject_agent_token::inject_agent_token(&inject_agent_token::RealSystem)?;
        }
        Command::CheckTypography => {
            typography::check_typography(&typography::RealSystem)?;
        }
        Command::WorktreeTeardown => {
            worktree_teardown::worktree_teardown(&worktree_teardown::RealSystem)?;
        }
        Command::CrossBuild {
            target,
            release,
            cargo_args,
        } => {
            cross_build::run_cross_build(&cross_build::RealSystem, target, release, &cargo_args)?;
        }
        Command::CrossLint { target } => {
            cross_build::run_cross_lint(&cross_build::RealSystem, target)?;
        }
        Command::CrossDoc { target } => {
            cross_build::run_cross_doc(&cross_build::RealSystem, target)?;
        }
        Command::CrossTest {
            target,
            with_vm,
            cargo_args,
        } => {
            cross_test::run_cross_test(&cross_build::RealSystem, target, with_vm, &cargo_args)?;
        }
        Command::CrossTestRunner { target, command } => {
            let code = cross_test::run_test_binary(&cross_build::RealSystem, target, &command)?;
            if code != 0 {
                std::process::exit(code);
            }
        }
        Command::Vm { action } => match action {
            VmAction::Start { target } => {
                vm::start(&cross_build::RealSystem, target)?;
            }
            VmAction::Stop { target } => vm::stop(&cross_build::RealSystem, target)?,
        },
        Command::RecordDemo => {
            record_demo::record_demo(&record_demo::RealSystem)?;
        }
    }
    Ok(())
}

/// Initialize the terminal logger.
///
/// Reuses the `ConfigBuilder` setup from `cssh-rs-core` so xtask output
/// and the app's log files share one `HH:MM:SS.subsecond` timestamp
/// format. Precedence: `RUST_LOG` (if parses as a level) > `-v` count >
/// `Info` default.
///
/// # Arguments
/// * `verbose` - Count of `-v` flags passed on the command line.
fn init_logger(verbose: u8) {
    let level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|s| s.parse::<LevelFilter>().ok())
        .unwrap_or(match verbose {
            0 => LevelFilter::Info,
            1 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        });
    let _ = TermLogger::init(
        level,
        ConfigBuilder::new()
            .set_time_format_custom(format_description!("[hour]:[minute]:[second].[subsecond]"))
            .build(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    );
    log_panics::init();
}
