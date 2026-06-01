//! macOS implementations of the `cssh-rs-platform` traits.
//!
//! Scaffolds for the macOS port. Every method panics with
//! `unimplemented!()` until M5 lands the concrete impls; the crate
//! exists so workspace consumers compile against the platform-
//! abstraction surface on macOS targets.
//!
//! All items in this crate are only available on macOS targets; on other
//! targets the crate compiles to an empty library so the workspace stays
//! buildable for `cargo check --target` of Linux and Windows hosts.

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]
#![warn(missing_docs)]
#![doc(html_no_source)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

#[cfg(target_os = "macos")]
mod imp;

#[cfg(target_os = "macos")]
pub use imp::{
    MacOsControlChannelClient, MacOsControlChannelServer, MacOsLaunchContext, MacOsProcessSpawner,
    MacOsWindowHandleProbe,
};
