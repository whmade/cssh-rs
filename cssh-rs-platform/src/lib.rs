//! Platform-abstraction traits for cssh-rs.
//!
//! Defines the surface that per-platform crates
//! (`cssh-rs-platform-windows`, `-linux`, `-macos`) implement so the
//! daemon, client, and plugin host stay platform-agnostic. M0 scope is
//! the trait definitions only; concrete impls land alongside the
//! per-platform crate extractions.
//!
//! The five traits exposed here cover the entire surface that varies
//! across host operating systems:
//!
//! - [`LaunchContext`] - platform-supplied state attached to every
//!   child-process spawn (Wayland activation token, Windows console
//!   session id, macOS Apple Event session, ...).
//! - [`ProcessSpawner`] - launches the daemon, client consoles, and
//!   plugin hosts.
//! - [`ControlChannelServer`] - server side of one daemon-to-client
//!   transport (named pipe on Windows, UNIX socket elsewhere).
//! - [`ControlChannelClient`] - client side of the same transport.
//! - [`WindowHandleProbe`] - looks up the window handle owned by a
//!   given process id, used by focus tracking.
//!
//! The control-channel traits use return-position `impl Future`, which
//! makes them not `dyn`-compatible. Platform selection is a compile-time
//! choice in this crate's consumers, so the traits are intended for
//! static dispatch (generic parameters / `impl Trait`) rather than
//! `Box<dyn ...>` storage.

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]
#![warn(missing_docs)]
#![doc(html_no_source)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::future::Future;

/// Platform-supplied context required when launching a child process.
///
/// Carries whatever state the host platform needs to attribute the
/// child to the correct user session: a Wayland activation token, a
/// Windows console session id, a macOS Apple Event session, etc. Each
/// platform crate defines a concrete type that implements this trait
/// and threads it through [`ProcessSpawner::spawn`].
pub trait LaunchContext: Send + Sync + 'static {}

/// Spawns child processes (daemon, client consoles, plugin hosts) on
/// the host platform.
pub trait ProcessSpawner: Send + Sync {
    /// Per-spawn context type; see [`LaunchContext`].
    type Context: LaunchContext;
    /// Opaque handle to a successfully spawned child.
    ///
    /// Only `Send + 'static` is required; the bound omits `Sync` so
    /// platform implementations can return handles intended for
    /// single-owner mutation (e.g. `tokio::process::Child`) without
    /// wrapping them in an interior-mutability container.
    type Handle: Send + 'static;
    /// Error type returned when spawning fails.
    type Error: Error + Send + Sync + 'static;

    /// Spawn `program` with `args` under the supplied `context`.
    ///
    /// # Arguments
    /// * `program` - Executable to launch (absolute path or a
    ///   `PATH`-resolvable name).
    /// * `args` - Command-line arguments passed to the child.
    /// * `context` - Platform-specific launch context.
    ///
    /// # Returns
    /// Platform-specific handle to the spawned process, or a platform
    /// error if the spawn failed.
    fn spawn(
        &self,
        program: &OsStr,
        args: &[OsString],
        context: &Self::Context,
    ) -> Result<Self::Handle, Self::Error>;
}

/// Server endpoint of the daemon-to-client control channel for one
/// client connection.
///
/// The daemon owns one `ControlChannelServer` per connected client and
/// uses it to push framed messages defined in `cssh-rs-protocol`.
pub trait ControlChannelServer: Send {
    /// Error type returned by control-channel operations.
    type Error: Error + Send + Sync + 'static;

    /// Wait for the client process to connect.
    ///
    /// # Returns
    /// `Ok(())` once a client has connected, or an error if the
    /// underlying endpoint failed.
    fn accept(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Send an already-framed control message to the connected client.
    ///
    /// # Arguments
    /// * `frame` - Bytes produced by `cssh-rs-protocol` serialization
    ///   helpers; the trait performs no framing of its own.
    fn send(&mut self, frame: &[u8]) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Read up to `buf.len()` bytes from the connected client into
    /// `buf`.
    ///
    /// # Arguments
    /// * `buf` - Destination buffer.
    ///
    /// # Returns
    /// Number of bytes written into `buf`. `Ok(0)` signals EOF (the
    /// peer closed the connection cleanly); any transport-level
    /// failure is reported as `Err`.
    fn recv(&mut self, buf: &mut [u8]) -> impl Future<Output = Result<usize, Self::Error>> + Send;
}

/// Client endpoint of the daemon-to-client control channel.
///
/// Each client process owns one `ControlChannelClient` connected to the
/// daemon-side [`ControlChannelServer`].
pub trait ControlChannelClient: Send {
    /// Error type returned by control-channel operations.
    type Error: Error + Send + Sync + 'static;

    /// Connect to the daemon's control-channel endpoint identified by
    /// `endpoint`.
    ///
    /// # Arguments
    /// * `endpoint` - Platform-specific endpoint identifier (named pipe
    ///   name on Windows, UNIX socket path on Linux/macOS).
    fn connect(&mut self, endpoint: &OsStr)
        -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Send raw bytes to the daemon.
    ///
    /// # Arguments
    /// * `bytes` - Payload to write; the trait performs no framing.
    fn send(&mut self, bytes: &[u8]) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Read up to `buf.len()` bytes from the daemon into `buf`.
    ///
    /// # Arguments
    /// * `buf` - Destination buffer.
    ///
    /// # Returns
    /// Number of bytes written into `buf`. `Ok(0)` signals EOF (the
    /// daemon closed the connection cleanly); any transport-level
    /// failure is reported as `Err`.
    fn recv(&mut self, buf: &mut [u8]) -> impl Future<Output = Result<usize, Self::Error>> + Send;
}

/// Looks up an opaque platform window handle owned by a given process.
pub trait WindowHandleProbe: Send + Sync {
    /// Opaque platform-specific window handle type
    /// (`HWND` on Windows, X11 window XID, Wayland surface id, ...).
    type Handle: Send + Sync + 'static;

    /// Return a window handle owned by `pid`, if one exists.
    ///
    /// # Arguments
    /// * `pid` - Operating-system process id of the window's owner.
    ///
    /// # Returns
    /// `Some(handle)` when a window owned by `pid` is found, otherwise
    /// `None`. If `pid` owns multiple windows it is unspecified which
    /// handle is returned.
    fn window_handle_for_process(&self, pid: u32) -> Option<Self::Handle>;
}
