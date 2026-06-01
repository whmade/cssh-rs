//! Utilities shared by daemon and client.

#![deny(clippy::implicit_return)]
#![allow(
    clippy::needless_return,
    clippy::doc_overindented_list_items,
    rustdoc::private_intra_doc_links
)]

pub mod config;
pub mod constants;
pub mod debug;

/// Re-exports the Windows API abstraction from `cssh-rs-platform-windows`.
///
/// Existing daemon and client modules import from `crate::utils::windows`;
/// the re-export preserves those paths while the implementation lives in
/// the platform crate.
pub mod windows {
    pub use cssh_rs_platform_windows::*;
}

#[cfg(test)]
#[path = "../tests/utils/test_mod.rs"]
mod test_mod;
