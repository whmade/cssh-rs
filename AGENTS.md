# cssh-rs - Agent Instructions

## Project Overview

cssh-rs is a Rust-based cross-platform cluster SSH tool. It enables users to SSH into
multiple hosts simultaneously with synchronized keystroke distribution.

## Architecture

- **Daemon-Client Model**: One daemon process coordinates multiple client processes
- **Process Isolation**: Each SSH connection runs in its own console window
- **Focus-Based Input**: Keystrokes go to all clients when daemon focused, single client when client focused
- **Windows-Native**: Deep integration with Windows APIs for terminal and registry management

## Key Design Philosophy

- **Cross-Platform via Trait Abstraction**: Platform-specific behaviour is hidden behind the `cssh-rs-platform` trait, with per-OS implementations in `cssh-rs-platform-windows`, `cssh-rs-platform-linux`, and `cssh-rs-platform-macos`
- **User Experience**: Automatic configuration generation, sensible defaults, graceful degradation
- **Configuration-Driven**: TOML-based configuration with auto-generation of defaults
- **Safety First**: Extensive use of Result types and proper error handling

## Project Structure

- **Binary**: `cssh-rs.exe` - Main executable with CLI interface (`cssh-rs/src/main.rs`)
- **Library**: `cssh_rs_core` - Core functionality (`cssh-rs-core/src/lib.rs`, `cssh-rs-core/src/cli.rs`)
- **Protocol crate**: `cssh-rs-protocol` - wire protocol between daemon and clients (`cssh-rs-protocol/src/lib.rs`)
- **Platform crate**: `cssh-rs-platform` - platform-abstraction trait definitions (`cssh-rs-platform/src/lib.rs`)
- **Linux platform crate**: `cssh-rs-platform-linux` - Linux trait implementations; scaffolded with `unimplemented!()` stubs until M4 (`cssh-rs-platform-linux/src/lib.rs`)
- **macOS platform crate**: `cssh-rs-platform-macos` - macOS trait implementations; scaffolded with `unimplemented!()` stubs until M5 (`cssh-rs-platform-macos/src/lib.rs`)
- **Modules**: `cssh-rs-core/src/client/`, `cssh-rs-core/src/daemon/`, `cssh-rs-core/src/utils/`
- **Tests**: `cssh-rs-core/src/tests/` with component-based organization (`test_*.rs` naming)
- **xtask**: `xtask/` - Developer automation tasks (README checks, release, changelog, social preview)
- **Config**: `.config/` - grouped, shared marker files consumed by both
  `xtask` and CI. Each area gets a `.config/<group>/` subdirectory holding
  `<identifier>.<kind>` files, where `<kind>` is `version` or `regex` and
  each file contains a single trimmed line (a pinned tool version or a
  regex). When pinning tools for a new area, add a new group subdirectory
  rather than overloading an existing one. Cross-cutting toolchain pins
  consumed by multiple groups live at `.config/<identifier>.<kind>`
  directly (e.g. `.config/python.version`) so consumers share a single
  source of truth.

## Build & Test Commands

```sh
cargo build                 # build
cargo fmt                   # format (run before submitting)
cargo lint                  # clippy (alias defined in .cargo/config.toml)
cargo test                  # unit + integration tests
cargo doc-tests             # documentation tests
cargo xtask check-typography # ASCII-punctuation lint
```

Always run `cargo fmt`, `cargo lint`, and both test commands before considering any task complete.

## Workflow skills

The `/workflows:commit`, `/workflows:github-pr`, and `/workflows:scrutinize`
skills come from the `workflows@whmade` marketplace plugin, enabled in
`.claude/settings.json`. They are generic and read the repo-specific
conventions documented below (`## Code style`, `## Testing`,
`## Commit conventions`, `## Pull requests`).

## Code style

Do NOT use decorative or "smart" Unicode punctuation anywhere in the
repo - not in code, comments, docstrings, commit messages, PR
descriptions, or markdown docs. Use the ASCII equivalent:

- em-dash and en-dash -> single `-` (NEVER `--`)
- smart quotes        -> `'` or `"`
- ellipsis            -> `...`
- arrows              -> `->`, `<-`, `=>`, etc.
- bullet / middle-dot -> `-` or `*`
- non-breaking space  -> regular space
- math glyphs         -> ASCII operators (`x`, `/`, `>=`, `<=`, `!=`)

Emoji in user-visible output (e.g. CI workflow logs) are fine.

This is enforced by `cargo xtask check-typography`, which runs in the
pre-commit hook and CI. If the check fails, fix the offending
characters - do NOT add to the allowlist.

### Docstrings

- **Public items** (`pub fn`, `pub struct`, public consts) and trait methods:
  one-sentence imperative summary (`Return the ...`, not `This function
  returns ...`), plus `# Arguments` and `# Returns`. The `# Arguments` block
  is load-bearing - keep it even when trimming other parts of a docstring.
- **`# Examples`**: only for reusable utilities a caller invokes in isolation
  (see `cssh-rs-core/src/utils/windows.rs`). Do NOT add `# Examples` to trait methods, CLI
  entrypoints, protocol handlers, or any function whose behaviour is only
  meaningful inside its module.
- **`# Panics` / `# Errors`**: only when they actually apply. Omit otherwise.
- **Private helpers**: one-line doc if the purpose is non-obvious; skip
  entirely for trivial helpers, simple getters, single-expression wrappers.
- **Test functions, closures, trivial trait impls**: no docs.
- **Module docs** (`//!`): one line for typical modules. Multi-paragraph only
  when the module defines a protocol or wire format (see
  `cssh-rs-protocol/src/lib.rs`). All library modules use
  `#![doc(html_no_source)]`.

````rust
// GOOD
/// Return the console window handle for the current process.
///
/// # Arguments
/// * `pid` - Process ID whose console is being queried.
///
/// # Returns
/// `HWND` to the attached console, or `null` if none is attached.
pub fn get_console_window_handle(pid: u32) -> HWND { ... }

// BAD - narrates, restates the signature, invents an `# Examples` block for
// a function nobody calls in isolation.
/// This is a function that gets the console window handle. It takes a
/// process ID (a u32) and returns an HWND, which is a Windows handle to
/// the console window.
///
/// # Arguments
/// * `pid` - The process ID. This is a u32 representing the process.
///
/// # Returns
/// Returns the HWND.
///
/// # Examples
/// ```ignore
/// let hwnd = get_console_window_handle(std::process::id());
/// ```
pub fn get_console_window_handle(pid: u32) -> HWND { ... }
````

### Inline comments

Default to not writing inline commends.
Add a `//` comment only for:

- Windows / platform quirks - cite the MS Learn URL or equivalent.
- Non-obvious async ordering, race conditions, or shared-state invariants.
- Magic numbers, protocol byte layout, named-pipe contracts.
- `// SAFETY:` justifications for `unsafe` blocks.

Never paraphrase the next line, narrate steps (`// Step 1: ...`,
`// First, ... // Then, ...`), add banner dividers (`// ----- Helpers -----`),
or commit commented-out code.

```rust
// GOOD - cites a platform quirk and explains the workaround.
// conhost leaves the bottom row stale after a bulk attribute fill until
// something forces a redraw; invalidate to force one.
nudge_cursor(handle)?;

// BAD - paraphrases the call.
// Set the console title.
set_console_title(handle, &title)?;
```

## Development Patterns

### RAII Resource Management
- Use guard structs that restore Windows state on `Drop`
- Example: `WindowsSettingsDefaultTerminalApplicationGuard` restores registry on cleanup

### Async-First Architecture
- All I/O operations are async via Tokio to prevent blocking
- Use `#[tokio::main]` for async entry points
- Spawn separate tasks for independent operations

### Error Handling
- Use `Result<T, E>` for all fallible operations
- Log warnings for non-critical failures and continue execution
- Panic with descriptive messages only for unrecoverable errors
- Registry failures are logged but do not stop execution

## Windows-Specific Implementation

### String Conversion
- **Rust -> Windows API**: `OsString::encode_wide()` for UTF-16 encoding
- **Windows API -> Rust**: `to_string_lossy()` for safe conversion back
- Always ensure proper null termination for C-style strings

### Windows API Integration
- Check all Windows API return values with descriptive error messages
- Apply RAII patterns to all Windows resources (handles, registry keys, etc.)
- Use `unsafe` blocks sparingly with proper validation
- Use `mockall` for testing Windows API calls without system side-effects

## Testing

- **Naming**: `test_*.rs` files in `cssh-rs-core/src/tests/`, descriptive test function names
- **Pattern**: Arrange-Act-Assert for all tests
- **Mocking**: Use `mockall` for all Windows API interactions - tests must have zero side-effects on the system
- **No external state**: tests must not modify registry, filesystem, or process state

## Commit conventions

Use the `/workflows:commit` skill to draft messages. Repo specifics:

- **Subject**: imperative mood, first word capitalized; optional lowercase
  `scope:` prefix mirroring the scopes already in `git log` (e.g. `client:`,
  `control mode:`) - do not invent new ones. No trailing period; keep under
  ~72 characters.
- Do NOT pre-append a PR number in parentheses (`(#123)`) - GitHub's
  squash-merge adds it automatically when the PR lands.
- **Issue/PR references**: use a `GitHub: #<number>` trailer in the footer,
  one per line, never in the subject or body prose. Do not use `Fixes:`.
- **AI co-authorship (MANDATORY for AI-generated commits)**: include exactly
  one `Co-authored-by: <Model Name> <noreply@anthropic.com>` trailer (e.g.
  `Co-authored-by: Claude Opus 4.6 <noreply@anthropic.com>`), using the
  git-canonical casing `Co-authored-by:`.

## User Interaction

- Clarify open questions before starting work
- Identify and resolve all ambiguities and assumptions up front
- Evaluate trade-offs before choosing an approach

## Pull requests

Use the `/workflows:github-pr` skill for both creating PRs and addressing
review feedback. Repo specifics:

- Create PRs from the commit with `gh pr create --fill`; add
  `--label no-news-fragment-needed` when the change has no user-facing
  effect.
- **The PR title and body ARE the commit message.** `--fill` copies the
  commit subject into the title and the commit body into the description;
  they must stay byte-for-byte identical to it and meet the same
  `## Commit conventions` bar. Do NOT hand-edit the PR to add an intro, a
  stacking note, a `> [!WARNING]`, a summary of the diff, or a
  generated-by footer - none of that is in the commit message. Anything
  worth saying on the PR belongs in the commit message (subject, body,
  trailers) so both carry it. If the PR text drifts from the commit, fix
  the commit and re-run `--fill`; never patch the PR description on its own.
- **News fragments**: user-facing changes need a fragment at
  `news/<name>.<type>.md`, where `<type>` is one of `feature`, `bugfix`,
  `security`, `deprecation`, or `removal`. This is enforced by
  `.github/workflows/news-fragment-check.yml` and can be waived with the
  `no-news-fragment-needed` label.
- When addressing feedback: reply to every unresolved review thread, resolve
  each one only after its fix is pushed, and push to update the PR.

## Completion Checklist

Before considering any task complete, first self-review your changes by
running `/workflows:scrutinize` on them. Then confirm:

1. Documentation is complete and accurate
2. All tests pass (`cargo doc-tests && cargo test`)
3. Code is formatted (`cargo fmt`)
4. No clippy warnings (`cargo lint`)
5. No forbidden Unicode (`cargo xtask check-typography`)
6. All interactions with external systems are mocked in tests
7. Configuration changes maintain backwards compatibility
