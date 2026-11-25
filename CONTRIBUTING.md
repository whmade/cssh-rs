# Contributing to cssh-rs

Thank you for considering contributing to cssh-rs! It's people like you that make cssh-rs a robust and reliable cross-platform cluster SSH tool.

Following these guidelines helps to communicate that you respect the time of the developers managing and developing this open source project. In return, they should reciprocate that respect in addressing your issue, assessing changes, and helping you finalize your pull requests.

### What kinds of contributions we're looking for

cssh-rs is an open source project and we love to receive contributions from our community - you! There are many ways to contribute, from writing blog posts, improving the documentation, submitting bug reports and feature requests or writing code which can be incorporated into cssh-rs itself.

## Ground Rules

### Technical Responsibilities

Before contributing code to cssh-rs, please ensure you understand and can meet these requirements:

- **Complete Documentation**: All code must be documented - modules, functions, structs, constants, everything
- **Testing Requirements**: All tests must pass (`cargo doc-tests && cargo test`)
- **Code Quality**: Code must be formatted (`cargo fmt`) and pass linting (`cargo lint`)
- **Backwards Compatibility**: Configuration changes must maintain backwards compatibility
- **Comments**: Comments should explain *why*, not *what* - the code should be self-documenting

### Behavioral Expectations

- **Be respectful and considerate** in all interactions
- **Create issues for major changes** before implementing them to discuss the approach
- **Keep contributions focused** - one feature or fix per pull request
- **Test thoroughly** by ensuring high code coverage and manually testing on Windows systems before submitting
- **Follow the existing code patterns** and architectural decisions
- **Be welcoming to newcomers** and encourage diverse new contributors from all backgrounds.

## Your First Contribution

Unsure where to begin contributing to cssh-rs? Here are some suggestions:

- **Documentation improvements** - Look for areas where explanations could be clearer
- **Test coverage** - Add tests for edge cases or improve existing test patterns
- **Bug fixes** - Check the Issues tab for reported bugs
- **Configuration enhancements** - Improve what can be configured and how

## Getting Started

### Development Environment Setup

1. **Prerequisites**:
   - Rust (we use [`rust-toolchain.toml`](https://github.com/whmade/cssh-rs/blob/main/rust-toolchain.toml) to configure the desired rust version/toolchain)
   - Git
   - A Windows development environment

2. **Clone and Setup**:
   ```cmd
   git clone https://github.com/whmade/cssh-rs.git
   cd cssh-rs
   git config --local core.hooksPath .githooks/
   ```

### AI agent GitHub auth (optional)

If you use [paseo](https://paseo.dev) to spawn AI coding agents on this
repository, those agents inherit your full `gh` CLI login by default -
typically a classic `repo` scope, which can delete the repository or
force-push to `main`. You can scope an agent down by providing a
**fine-grained** Personal Access Token:

1. Generate a fine-grained PAT at
   <https://github.com/settings/personal-access-tokens/new> with
   `Contents`, `Pull requests`, and `Issues` set to *Read and write*
   (and only those - leave everything else at *No access*). Restrict it
   to your fork of `cssh-rs`. Set a short expiration.
2. Save the full token (including the `github_pat_` prefix) to
   `.paseo/gh-token` in your source checkout (not in a worktree). The
   `.paseo/` directory is checked into the repository so the file
   itself is gitignored.

`cargo xtask inject-agent-token` is wired into `paseo.json`'s
`worktree.setup` and will write the token into the worktree's
`.claude/settings.local.json` at creation time, where Claude Code
injects it as `GH_TOKEN` for the agent process. If the file is absent
the step is a no-op. Fine-grained PATs are the recommended shape;
classic (`ghp_...`) and OAuth (`gho_...`) tokens are also accepted,
but each triggers a warning at injection time because they cannot be
scoped to specific repositories or to a subset of repository
permissions. Any other content is rejected. To rotate, overwrite
`.paseo/gh-token` in the source checkout and either re-run
`cargo xtask inject-agent-token` from there or recreate the worktree.

### Development Workflow

cssh-rs uses cargo aliases and the [`xtask`](https://github.com/matklad/cargo-xtask) crate for development automation. Key commands:

- `cargo fmt` - Format code
- `cargo lint` - Run clippy linting
- `cargo test` - Run all tests
- `cargo build` - Build the project

### Cross-compiling

`cssh-rs`, `cssh-rs-core` and `cssh-rs-protocol` build only for Windows, so on a Linux or macOS host `cargo build`, `cargo lint` and `cargo docs` fail. Cross xtasks run them for a target from any supported host:

```sh
cargo xtask cross-build <target> [--release] [-- <cargo build arguments>]
cargo xtask cross-lint <target>
cargo xtask cross-doc <target>
```

On a Windows x64 host with the MSVC Rust toolchain they run the plain native commands; other hosts reach the target with `--target` or through cargo-xwin. Run `cargo xtask cross-build --help` for the list of supported targets. On first use the xtasks install the Rust target and cargo-xwin; on Linux and macOS, put LLVM's `clang`, `llvm-rc` and `lld-link` on PATH yourself.

`cross-build` builds the cssh-rs package unless the arguments after `--` select packages themselves, for example `-- --workspace --all-targets`.

`cargo xtask update-readme-help` and `cargo xtask check-readme-help` have no cross variant: they run cssh-rs to read its `--help` output, so they work only on a Windows host. Elsewhere they fail and leave the README unchanged.

### Windows test VM

On a Linux host, `cargo xtask vm start x86_64-pc-windows-msvc` runs a Windows 11 VM that xtasks reach over SSH: a [dockurr/windows](https://github.com/dockur/windows) container that you start once and reuse. `cargo xtask vm stop x86_64-pc-windows-msvc` shuts it down.

The VM needs Docker with access to its daemon, KVM (`/dev/kvm`) and the OpenSSH client (`ssh`, `scp`, `ssh-keygen`). The xtask names whichever is missing but installs none of them, as that needs root. macOS hosts have no KVM and are not supported.

The first `vm start` downloads Windows 11 (about 8 GB) and installs it unattended, which takes 20 to 60 minutes depending on the download; watch it at the web viewer URL the xtask logs. The VM's disk lives in the Docker volume `cssh-rs-windows-vm`, so `cargo clean` keeps it and every worktree shares it. After the install the volume holds about 20 GB: the installation image and the VM's disk, which grows with use up to 64 GB. Later starts boot in about 15 seconds. While it runs, the VM uses 4 GB of RAM and 2 CPU cores. Its SSH key lives in `$XDG_DATA_HOME/cssh-rs/windows-vm/` (by default `~/.local/share/cssh-rs/windows-vm/`). To reinstall Windows, stop the VM and remove its disk with `docker volume rm cssh-rs-windows-vm`.

### Pre-commit Hooks

cssh-rs uses pre-commit git hooks to enforce code quality. These are automatically installed when you set the hooks path as shown above. The hooks will:

- Format your code with `cargo fmt`
- Run linting with `cargo xtask cross-lint`
- Check for forbidden Unicode with `cargo xtask check-typography`
- Build the workspace, tests included, with `cargo xtask cross-build`; on a Windows x64 host with the MSVC toolchain the build leaves out xtask, which cannot relink itself while it runs
- Generate documentation with `cargo xtask cross-doc`
- On a Windows host, update README help output with `cargo xtask update-readme-help` if needed; other hosts skip this step and leave the README check to CI

The hook runs the cross xtasks for `x86_64-pc-windows-msvc` on every host. On Linux and macOS it therefore needs the LLVM tools listed under [Cross-compiling](#cross-compiling), and on first use the xtasks install the Rust target and cargo-xwin, which downloads the MSVC CRT and Windows SDK into your local cache; the xtasks accept their Microsoft license terms for you by setting `XWIN_ACCEPT_LICENSE=1`.

### For Small Changes

Small contributions can be submitted directly as pull requests without creating an issue first.

Examples of small changes:
- Spelling/grammar fixes
- Typo corrections and formatting improvements
- Comment cleanup
- Documentation clarifications
- Adding logging messages or debugging output
- Changes to metadata files like `.gitignore`, build scripts, etc.

### For Larger Changes

For anything more substantial:

1. **Create an issue first** to discuss the change
2. **Fork the repository** and create a feature branch
3. **Make your changes** following the coding standards
4. **Ensure all tests pass** and pre-commit hooks succeed (you can enable the github actions after forking to have the CI run on your fork)
5. **Submit a pull request** with a clear description

## How to Suggest a Feature or Enhancement

If you have an idea for a new feature:

1. **Check existing issues** to see if it's already been suggested (open and closed)
2. **Create a new issue** with the "enhancement" label
3. **Describe the feature** following the issue template
4. **Be prepared to discuss** the implementation approach
5. **Consider offering to implement** the feature yourself

## Code Review Process

### Automated Checks

All pull requests go through automated checks using GitHub Actions and must pass all checks.

### Review Criteria

Pull requests are reviewed based on:

- **Code quality** - follows project standards and patterns
- **Testing** - adequate test coverage with proper mocking
- **Documentation** - complete and accurate documentation
- **Windows compatibility** - works correctly on supported Windows versions
- **Backwards compatibility** - doesn't break existing functionality

### Review Timeline

- **Initial response** - within 1 week for most pull requests
- **Detailed review** - depends on complexity and current workload
- **Follow-up** - we expect responses to feedback within 2 weeks

If a pull request shows no activity for 2 weeks after feedback, it may be closed.

## Community

### Communication Channels

- **GitHub Issues** - for bug reports and feature requests
- **Pull Request Comments** - for code-specific discussions

### Maintainers

cssh-rs is maintained by [@whme](https://github.com/whme). Response times may vary based on availability and workload.

## Code, Commit Message and other Conventions

### Code Style

cssh-rs follows standard Rust conventions with some specific requirements:

- **Follow clippy suggestions** - all warnings must be resolved
- **Document everything** - modules, functions, structs, constants
- **Use meaningful names** - prefer clarity over brevity
- **Handle errors properly** - use `Result<T, E>` for fallible operations

### Documentation Style

- **Module documentation**: Use `//!` with `#![doc(html_no_source)]`
- **Function documentation**: Include `# Arguments` and  `# Returns` ( `# Examples` are optional)
- **Document panics** and error conditions explicitly
- **Provide examples** for complex functionality

### Testing Patterns

cssh-rs has no integration tests (yet). The following applies to unit tests.

- **Tests in `cssh-rs-core/src/tests/`** with `test_*.rs` naming convention
- **Use `mockall`** for Windows API mocking
- **Follow Arrange-Act-Assert** pattern
- **Use descriptive test names** that explain what is being tested
- **No side effects** - all external interactions must be mocked

For easy manual testing on Windows we recommend the following setup:
- Enable OpenSSH Server - [docs](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_install_firstuse?tabs=gui&pivots=windows-10)
- Run cssh-rs against `localhost`:

    ```powershell
    cargo run -- -u $env:USERNAME localhost localhost
    ```

### Regenerating golden snapshots

Some tests freeze exact bytes or output as golden fixtures with the `insta`
crate - for example the v1 protocol wire fixtures under
`cssh-rs-protocol/tests/golden/v1/*.snap`. A plain `cargo test` compares
against the committed snapshots, so an accidental change fails CI. When you
change the output *on purpose*, regenerate the snapshots and review the diff
before committing:

```sh
INSTA_UPDATE=always cargo test
```

(or `cargo insta review` if you have `cargo-insta` installed). Commit the
updated `.snap` files alongside the change. This applies to every `insta`
snapshot in the repo, not just the protocol fixtures.

### Commit Messages

- Use clear, descriptive commit messages
- Start with a verb in present tense (e.g., "Add", "Fix", "Update")
- Keep the first line under about 72 characters
- Provide additional details in the body

## Development Tools and Automation

### xtask Subcommands

cssh-rs uses `cargo xtask` subcommands for automation - run as part of the pre-commit githook, in the GitHub Actions CI, or locally. See `cargo xtask --help` for the full list.

### Regenerating the demo GIF

The demo clip in the README is generated by `cargo xtask record-demo` (Windows only) and published to the orphan `demo-assets` branch. It is never committed to a code branch, so clones stay lean, and the README links the clip directly from `demo-assets`.

To publish an updated clip, trigger the **Regenerate demo GIF** workflow (Actions tab -> "Run workflow") and pick the branch in the **"Use workflow from"** dropdown - the demo is generated for that branch. It only accepts `main` or an `X.Y-maintenance` branch. The workflow records the clip, validates the GIF, and opens a review PR against `demo-assets` so you can inspect the rendered old/new GIF diff before it goes live. Merging that PR publishes the clip.

Each branch keeps its own clip on the shared `demo-assets` branch, keyed by branch:

- `main` -> `cssh-rs.gif`
- an `X.Y-maintenance` branch -> `cssh-rs-X.Y.gif`

so regenerating a maintenance branch's demo never clobbers main's. A maintenance branch displays its clip by pointing its own README at the suffixed asset.

To preview the demo for a pull request's code without publishing it, comment `/record-demo` on the PR. Because this runs the PR's code on a Windows runner, only repository owners, members, and collaborators can trigger it. The command records the demo GIF from the PR head, uploads it as a build artifact, and links it back in a PR comment - so you can verify a demo-logic change before merging.

One-time setup: "Allow GitHub Actions to create and approve pull requests" must be enabled under Settings > Actions > General (org-level if the repo is in an org) for the review PR to be created.

### Release Process

cssh-rs follows a structured release process:

1. **Prepare release** with `cargo xtask prepare-release`
2. **Create pull request** from maintenance branch to main
3. **Create release tag** with `cargo xtask create-release-tag`
4. **Publish release** through GitHub Actions

Contributors don't need to worry about releases - maintainers handle this process.

---

Thank you for contributing to cssh-rs! Your efforts help make cluster management on Windows better for everyone.
