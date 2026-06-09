//! Shared metadata constants for the cssh-rs workspace.
//!
//! Single source of truth for identifiers - such as the binary name - that
//! several workspace members would otherwise duplicate as string literals.
//! `env!("CARGO_PKG_NAME")` expands to the *containing* crate's name and is
//! not a substitute, since the binary name (`cssh-rs`) differs from the
//! library and helper crate names (`cssh-rs-core`, `xtask`, ...).

#![doc(html_no_source)]

/// Name of the cssh-rs binary and the user-facing package.
pub const PACKAGE_NAME: &str = "cssh-rs";
