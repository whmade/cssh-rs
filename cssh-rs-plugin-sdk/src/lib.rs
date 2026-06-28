//! Optional Rust convenience layer for plugin authors.
//!
//! Scaffold for the plugin SDK. The typed handler traits
//! (`SpawnerPlugin`, `WindowManagerPlugin`), the `run()` stdio
//! entrypoint, and the golden-trace test helpers land in M2; this crate
//! exists so plugin authors have a stable dependency edge ahead of time.
//! Non-Rust plugins can skip this crate and speak the wire protocol
//! directly. It builds on the manifest schema from `cssh-rs-plugin-api`.

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]
#![warn(missing_docs)]
#![doc(html_no_source)]
