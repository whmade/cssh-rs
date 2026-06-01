//! Windows implementations of the `cssh-rs-platform` traits.
//!
//! The impls wrap the existing [`WindowsApi`] helpers and
//! `tokio::net::windows::named_pipe` so the daemon and client can be
//! ported to the platform-agnostic surface in later milestones. They are
//! not consumed by the M0 daemon/client yet; the trait surface is what
//! Linux and macOS stubs need to mirror.
//!
//! [`WindowsApi`]: crate::api::WindowsApi

use std::ffi::{OsStr, OsString};
use std::io;
use std::sync::Arc;

use cssh_rs_platform::{
    ControlChannelClient, ControlChannelServer, LaunchContext, ProcessSpawner, WindowHandleProbe,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt, Interest};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, PipeMode, ServerOptions,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::PROCESS_INFORMATION;

use crate::api::{DefaultWindowsApi, WindowsApi};

/// `Send + Sync` wrapper around [`PROCESS_INFORMATION`].
///
/// [`PROCESS_INFORMATION`] is `!Send` because its `HANDLE` fields wrap raw
/// pointers. The handles are owned by the spawning process and remain valid
/// across thread boundaries, so the wrapper asserts thread safety with
/// `unsafe impl`. The daemon already uses the same pattern for
/// `Client::process_handle` via its private `HWNDWrapper`.
#[derive(Debug)]
pub struct SendProcessInformation(pub PROCESS_INFORMATION);

// SAFETY: PROCESS_INFORMATION's HANDLE fields are opaque kernel handles,
// not pointers into the calling thread's address space; the OS allows the
// owning process to use them from any thread.
unsafe impl Send for SendProcessInformation {}
// SAFETY: same justification as for `Send` - the handle values are immutable
// once `CreateProcessW` returns, so shared references can travel freely.
unsafe impl Sync for SendProcessInformation {}

/// `Send + Sync` wrapper around [`HWND`].
///
/// Mirrors the daemon's private `HWNDWrapper` so the platform-trait
/// surface can be `Send + Sync` without consumers having to define their
/// own newtype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendHwnd(pub HWND);

// SAFETY: HWND is an opaque user-mode kernel object identifier the OS lets
// any thread in the owning process consult.
unsafe impl Send for SendHwnd {}
// SAFETY: same justification as for `Send` - the underlying identifier is
// immutable and read-only from Rust's perspective.
unsafe impl Sync for SendHwnd {}

/// Windows-side [`LaunchContext`].
///
/// Carries the focus-handoff hint the daemon uses when spawning client
/// consoles. The current Win32 spawn path inspects the same flag to apply
/// `STARTF_USESHOWWINDOW` + `SW_SHOWNOACTIVATE`, so this field is the
/// minimum needed to round-trip today's behaviour through the trait.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsLaunchContext {
    /// Whether the spawned console is allowed to take foreground focus.
    pub with_keyboard_focus: bool,
}

impl LaunchContext for WindowsLaunchContext {}

/// Process spawner backed by [`WindowsApi::create_process_with_args`].
///
/// Holds an [`Arc`] over a [`WindowsApi`] implementation so callers can
/// inject [`crate::api::MockWindowsApi`] in tests; production code uses
/// [`Self::default`] which selects [`DefaultWindowsApi`].
pub struct WindowsProcessSpawner<A: WindowsApi = DefaultWindowsApi> {
    api: Arc<A>,
}

impl<A: WindowsApi> WindowsProcessSpawner<A> {
    /// Construct a spawner around a specific [`WindowsApi`] implementation.
    ///
    /// # Arguments
    /// * `api` - Windows API operations implementation.
    pub fn new(api: Arc<A>) -> Self {
        return Self { api };
    }
}

impl Default for WindowsProcessSpawner<DefaultWindowsApi> {
    fn default() -> Self {
        return Self::new(Arc::new(DefaultWindowsApi));
    }
}

impl<A: WindowsApi + 'static> ProcessSpawner for WindowsProcessSpawner<A> {
    type Context = WindowsLaunchContext;
    type Handle = SendProcessInformation;
    type Error = io::Error;

    fn spawn(
        &self,
        program: &OsStr,
        args: &[OsString],
        context: &Self::Context,
    ) -> Result<Self::Handle, Self::Error> {
        let program = program.to_string_lossy().into_owned();
        let args = args
            .iter()
            .map(|a| return a.to_string_lossy().into_owned())
            .collect();
        return self
            .api
            .create_process_with_args(&program, args, context.with_keyboard_focus)
            .map(SendProcessInformation)
            .ok_or_else(|| {
                return io::Error::other("CreateProcessW failed");
            });
    }
}

/// Daemon-side control channel backed by a Windows named pipe server.
///
/// One instance owns one [`NamedPipeServer`] endpoint. `accept`
/// waits for the matching client process to connect; `send` / `recv`
/// then exchange already-framed bytes produced by `cssh-rs-protocol`.
pub struct WindowsControlChannelServer {
    pipe: Option<NamedPipeServer>,
    endpoint: OsString,
}

impl WindowsControlChannelServer {
    /// Open a new named-pipe server endpoint at `endpoint`.
    ///
    /// # Arguments
    /// * `endpoint` - Fully-qualified Windows named-pipe name
    ///   (e.g. `\\.\pipe\cssh-rs-named-pipe-for-ipc`).
    ///
    /// # Returns
    /// Ready-to-`accept` server, or the underlying I/O error from
    /// [`ServerOptions::create`].
    pub fn bind(endpoint: &OsStr) -> io::Result<Self> {
        let pipe = ServerOptions::new()
            .access_inbound(true)
            .access_outbound(true)
            .pipe_mode(PipeMode::Message)
            .create(endpoint)?;
        return Ok(Self {
            pipe: Some(pipe),
            endpoint: endpoint.to_owned(),
        });
    }
}

impl ControlChannelServer for WindowsControlChannelServer {
    type Error = io::Error;

    async fn accept(&mut self) -> Result<(), Self::Error> {
        let pipe = self.pipe.as_mut().ok_or_else(|| {
            return io::Error::other("control channel server has no pending pipe");
        })?;
        return pipe.connect().await;
    }

    async fn send(&mut self, frame: &[u8]) -> Result<(), Self::Error> {
        let pipe = self.pipe.as_mut().ok_or_else(|| {
            return io::Error::other("control channel server is not bound");
        })?;
        return pipe.write_all(frame).await;
    }

    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let pipe = self.pipe.as_mut().ok_or_else(|| {
            return io::Error::other("control channel server is not bound");
        })?;
        return pipe.read(buf).await;
    }
}

impl WindowsControlChannelServer {
    /// Endpoint name the server is bound to.
    ///
    /// # Returns
    /// The named-pipe path supplied to [`Self::bind`].
    pub fn endpoint(&self) -> &OsStr {
        return self.endpoint.as_os_str();
    }

    /// Test for pending readable bytes without blocking.
    ///
    /// # Returns
    /// `Ok(true)` when the pipe has bytes ready to read, `Ok(false)`
    /// otherwise, or the underlying I/O error.
    pub async fn ready_to_read(&mut self) -> io::Result<bool> {
        let pipe = self.pipe.as_mut().ok_or_else(|| {
            return io::Error::other("control channel server is not bound");
        })?;
        let ready = pipe.ready(Interest::READABLE).await?;
        return Ok(ready.is_readable());
    }
}

/// Client-side control channel backed by a Windows named pipe client.
pub struct WindowsControlChannelClient {
    pipe: Option<NamedPipeClient>,
}

impl WindowsControlChannelClient {
    /// Construct an unconnected control-channel client.
    ///
    /// # Returns
    /// A client whose `connect` has not yet run.
    pub fn new() -> Self {
        return Self { pipe: None };
    }
}

impl Default for WindowsControlChannelClient {
    fn default() -> Self {
        return Self::new();
    }
}

impl ControlChannelClient for WindowsControlChannelClient {
    type Error = io::Error;

    async fn connect(&mut self, endpoint: &OsStr) -> Result<(), Self::Error> {
        let pipe = ClientOptions::new().open(endpoint)?;
        self.pipe = Some(pipe);
        return Ok(());
    }

    async fn send(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        let pipe = self.pipe.as_mut().ok_or_else(|| {
            return io::Error::other("control channel client is not connected");
        })?;
        return pipe.write_all(bytes).await;
    }

    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let pipe = self.pipe.as_mut().ok_or_else(|| {
            return io::Error::other("control channel client is not connected");
        })?;
        return pipe.read(buf).await;
    }
}

/// Window-handle probe backed by [`WindowsApi::get_window_handle_for_process`].
///
/// Holds an [`Arc`] over a [`WindowsApi`] for the same testability reasons
/// as [`WindowsProcessSpawner`].
pub struct WindowsWindowHandleProbe<A: WindowsApi = DefaultWindowsApi> {
    api: Arc<A>,
}

impl<A: WindowsApi> WindowsWindowHandleProbe<A> {
    /// Construct a probe around a specific [`WindowsApi`] implementation.
    ///
    /// # Arguments
    /// * `api` - Windows API operations implementation.
    pub fn new(api: Arc<A>) -> Self {
        return Self { api };
    }
}

impl Default for WindowsWindowHandleProbe<DefaultWindowsApi> {
    fn default() -> Self {
        return Self::new(Arc::new(DefaultWindowsApi));
    }
}

impl<A: WindowsApi + 'static> WindowHandleProbe for WindowsWindowHandleProbe<A> {
    type Handle = SendHwnd;

    fn window_handle_for_process(&self, pid: u32) -> Option<Self::Handle> {
        let handle = self.api.get_window_handle_for_process(pid);
        if self.api.is_window(handle) {
            return Some(SendHwnd(handle));
        }
        return None;
    }
}
