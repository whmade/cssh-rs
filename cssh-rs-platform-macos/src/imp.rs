//! macOS implementations of the `cssh-rs-platform` traits.

use std::ffi::{OsStr, OsString};

use cssh_rs_platform::{
    ControlChannelClient, ControlChannelServer, LaunchContext, ProcessSpawner, WindowHandleProbe,
};

/// Placeholder macOS launch context.
///
/// The real type will carry the Apple Event session identifier (or
/// equivalent NSWorkspace activation token) once M5 lands.
#[derive(Debug, Default)]
pub struct MacOsLaunchContext;

impl LaunchContext for MacOsLaunchContext {}

/// Placeholder macOS process spawner.
#[derive(Debug, Default)]
pub struct MacOsProcessSpawner;

impl ProcessSpawner for MacOsProcessSpawner {
    type Context = MacOsLaunchContext;
    type Handle = std::process::Child;
    type Error = std::io::Error;

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn spawn(
        &self,
        _program: &OsStr,
        _args: &[OsString],
        _context: &Self::Context,
    ) -> Result<Self::Handle, Self::Error> {
        unimplemented!("cssh-rs-platform-macos: process spawn lands in M5");
    }
}

/// Placeholder macOS control-channel server endpoint.
#[derive(Debug, Default)]
pub struct MacOsControlChannelServer;

impl ControlChannelServer for MacOsControlChannelServer {
    type Error = std::io::Error;

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn accept(&mut self) -> Result<(), Self::Error> {
        unimplemented!("cssh-rs-platform-macos: control-channel accept lands in M5");
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn send(&mut self, _frame: &[u8]) -> Result<(), Self::Error> {
        unimplemented!("cssh-rs-platform-macos: control-channel send lands in M5");
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn recv(&mut self, _buf: &mut [u8]) -> Result<usize, Self::Error> {
        unimplemented!("cssh-rs-platform-macos: control-channel recv lands in M5");
    }
}

/// Placeholder macOS control-channel client endpoint.
#[derive(Debug, Default)]
pub struct MacOsControlChannelClient;

impl ControlChannelClient for MacOsControlChannelClient {
    type Error = std::io::Error;

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn connect(&mut self, _endpoint: &OsStr) -> Result<(), Self::Error> {
        unimplemented!("cssh-rs-platform-macos: control-channel connect lands in M5");
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn send(&mut self, _bytes: &[u8]) -> Result<(), Self::Error> {
        unimplemented!("cssh-rs-platform-macos: control-channel send lands in M5");
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn recv(&mut self, _buf: &mut [u8]) -> Result<usize, Self::Error> {
        unimplemented!("cssh-rs-platform-macos: control-channel recv lands in M5");
    }
}

/// Placeholder macOS window-handle probe.
///
/// `Handle` is `u64` to carry an `AXUIElement`-derived window number
/// (or equivalent Quartz `CGWindowID`); the concrete representation is
/// settled in M5 once the focus-tracking implementation is in place.
#[derive(Debug, Default)]
pub struct MacOsWindowHandleProbe;

impl WindowHandleProbe for MacOsWindowHandleProbe {
    type Handle = u64;

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn window_handle_for_process(&self, _pid: u32) -> Option<Self::Handle> {
        unimplemented!("cssh-rs-platform-macos: window-handle probe lands in M5");
    }
}
