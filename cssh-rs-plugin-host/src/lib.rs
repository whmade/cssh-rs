//! Daemon-side plugin runtime.
//!
//! Scaffold for the plugin host. Plugin discovery, the NDJSON RPC
//! lifecycle over stdio, capability matching, and the hard-fail crash
//! policy land in M2; this crate exists so the workspace has a home for
//! that work and so consumers can depend on it ahead of time. It
//! consumes the manifest schema from `cssh-rs-plugin-api`.

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]
#![warn(missing_docs)]
#![doc(html_no_source)]
