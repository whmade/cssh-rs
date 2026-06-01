//! Unit tests for the Windows implementations of the `cssh-rs-platform`
//! traits.

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]

use std::ffi::{OsStr, OsString};
use std::sync::Arc;

use cssh_rs_platform::{
    ControlChannelClient, ControlChannelServer, ProcessSpawner, WindowHandleProbe,
};
use mockall::predicate::{always, eq};
use tokio::runtime::Builder;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::PROCESS_INFORMATION;

use crate::api::MockWindowsApi;
use crate::traits::{
    SendHwnd, SendProcessInformation, WindowsControlChannelClient, WindowsControlChannelServer,
    WindowsLaunchContext, WindowsProcessSpawner, WindowsWindowHandleProbe,
};

/// Construct a unique named-pipe endpoint per test invocation so concurrent
/// tests cannot collide.
fn unique_pipe_name(tag: &str) -> OsString {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let mut name = OsString::from(r"\\.\pipe\cssh-rs-platform-windows-test-");
    name.push(tag);
    name.push("-");
    name.push(std::process::id().to_string());
    name.push("-");
    name.push(n.to_string());
    return name;
}
}

mod process_spawner_tests {
    use super::*;

    /// Spawner forwards `program`, `args`, and the focus flag through to
    /// `WindowsApi::create_process_with_os_args` and wraps the returned
    /// `PROCESS_INFORMATION`.
    #[test]
    fn spawn_forwards_arguments_and_wraps_handle() {
        let mut mock_api = MockWindowsApi::new();
        mock_api
            .expect_create_process_with_os_args()
            .withf(|application, args, focus| {
                return application == OsStr::new("ssh.exe")
                    && args == [OsString::from("user@host"), OsString::from("-p 22")].as_slice()
                    && *focus;
            })
            .times(1)
            .returning(|_, _, _| return Ok(PROCESS_INFORMATION::default()));

        let spawner = WindowsProcessSpawner::new(Arc::new(mock_api));
        let ctx = WindowsLaunchContext {
            with_keyboard_focus: true,
        };
        let handle = spawner
            .spawn(
                OsStr::new("ssh.exe"),
                &[OsString::from("user@host"), OsString::from("-p 22")],
                &ctx,
            )
            .expect("spawn must succeed when the API returns Ok");

        // Default PROCESS_INFORMATION has null handles - Drop is a no-op.
        assert!(handle.0.hProcess.is_invalid());
        assert!(handle.0.hThread.is_invalid());
    }

    /// Non-UTF-8 byte sequences in `program` / `args` reach
    /// `create_process_with_os_args` untouched - no lossy replacement.
    #[test]
    fn spawn_preserves_non_utf8_arguments() {
        use std::os::windows::ffi::OsStringExt;

        let lone_surrogate = OsString::from_wide(&[0xD800u16, b'a' as u16]);
        let expected = lone_surrogate.clone();

        let mut mock_api = MockWindowsApi::new();
        mock_api
            .expect_create_process_with_os_args()
            .withf(move |application, args, _| {
                return application == OsStr::new("foo.exe") && args == [expected.clone()];
            })
            .times(1)
            .returning(|_, _, _| return Ok(PROCESS_INFORMATION::default()));

        let spawner = WindowsProcessSpawner::new(Arc::new(mock_api));
        spawner
            .spawn(
                OsStr::new("foo.exe"),
                &[lone_surrogate],
                &WindowsLaunchContext::default(),
            )
            .expect("spawn must succeed");
    }

    /// When the API returns `Err`, `spawn` surfaces it as `io::Error`
    /// rather than collapsing to a generic "CreateProcessW failed" string.
    #[test]
    fn spawn_propagates_api_error() {
        let mut mock_api = MockWindowsApi::new();
        mock_api
            .expect_create_process_with_os_args()
            .times(1)
            .returning(|_, _, _| {
                return Err(windows::core::Error::from_thread());
            });

        let spawner = WindowsProcessSpawner::new(Arc::new(mock_api));
        let err = spawner
            .spawn(
                OsStr::new("missing.exe"),
                &[],
                &WindowsLaunchContext::default(),
            )
            .expect_err("spawn must surface API errors");
        assert!(!err.to_string().is_empty());
    }

    /// `Default for WindowsProcessSpawner` selects `DefaultWindowsApi` so
    /// production callers do not have to wire the Arc themselves.
    #[test]
    fn default_uses_default_windows_api() {
        let _spawner: WindowsProcessSpawner = WindowsProcessSpawner::default();
    }
}

mod window_handle_probe_tests {
    use super::*;

    /// Probe returns `Some(SendHwnd)` when `IsWindow` reports a valid handle.
    #[test]
    fn returns_handle_when_window_is_valid() {
        let mut mock_api = MockWindowsApi::new();
        // Use a usize sentinel and rebuild the HWND inside the closure so the
        // returning closure stays `Send` (HWND wraps `*mut c_void`).
        const HWND_BITS: usize = 0x1234;
        mock_api
            .expect_get_window_handle_for_process()
            .with(eq(4242u32))
            .times(1)
            .returning(|_| return HWND(HWND_BITS as *mut _));
        mock_api
            .expect_is_window()
            .with(always())
            .times(1)
            .returning(|_| return true);

        let probe = WindowsWindowHandleProbe::new(Arc::new(mock_api));
        let handle = probe
            .window_handle_for_process(4242)
            .expect("probe must return Some when IsWindow is true");
        assert_eq!(handle, SendHwnd(HWND(HWND_BITS as *mut _)));
    }

    /// Probe returns `None` when `IsWindow` rejects the handle returned by
    /// `get_window_handle_for_process`.
    #[test]
    fn returns_none_when_window_is_invalid() {
        let mut mock_api = MockWindowsApi::new();
        mock_api
            .expect_get_window_handle_for_process()
            .times(1)
            .returning(|_| return HWND::default());
        mock_api
            .expect_is_window()
            .times(1)
            .returning(|_| return false);

        let probe = WindowsWindowHandleProbe::new(Arc::new(mock_api));
        assert!(probe.window_handle_for_process(1).is_none());
    }

    /// `Default for WindowsWindowHandleProbe` selects `DefaultWindowsApi`.
    #[test]
    fn default_uses_default_windows_api() {
        let _probe: WindowsWindowHandleProbe = WindowsWindowHandleProbe::default();
    }
}

mod control_channel_tests {
    use super::*;

    /// End-to-end round trip: server binds, client connects, both sides
    /// exchange bytes. Covers `bind`, `accept`, `send`, `recv`,
    /// `endpoint`, `ready_to_read`, the client's `new`/`Default`,
    /// `connect`, `send`, and `recv`.
    #[test]
    fn round_trip_exchange() {
        let runtime = Builder::new_multi_thread()
            .enable_io()
            .enable_time()
            .worker_threads(2)
            .build()
            .expect("tokio runtime");
        let endpoint = unique_pipe_name("round-trip");

        runtime.block_on(async move {
            let mut server =
                WindowsControlChannelServer::bind(&endpoint).expect("server bind succeeds");
            assert_eq!(server.endpoint(), endpoint.as_os_str());

            let client_endpoint = endpoint.clone();
            let client_task = tokio::spawn(async move {
                let mut client = WindowsControlChannelClient::default();
                // Briefly wait for the server's connect call to be ready.
                for _ in 0..100u32 {
                    match client.connect(&client_endpoint).await {
                        Ok(()) => break,
                        Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
                    }
                }
                client
                    .send(b"hello-from-client")
                    .await
                    .expect("client send");
                let mut buf = [0u8; 32];
                let n = client.recv(&mut buf).await.expect("client recv");
                return buf[..n].to_vec();
            });

            server.accept().await.expect("server accept");

            let mut buf = [0u8; 64];
            let n = server.recv(&mut buf).await.expect("server recv");
            assert_eq!(&buf[..n], b"hello-from-client");

            server
                .send(b"hello-from-server")
                .await
                .expect("server send");

            let client_msg = client_task.await.expect("client task panicked");
            assert_eq!(client_msg, b"hello-from-server");

            // ready_to_read after the peer closed must not block forever -
            // a quick poll is sufficient to drive the coverage line.
            let _ =
                tokio::time::timeout(std::time::Duration::from_millis(50), server.ready_to_read())
                    .await;
        });
    }

    /// `WindowsControlChannelClient::new` produces an unconnected client
    /// whose `send` / `recv` fail with the documented "not connected"
    /// message rather than panicking.
    #[test]
    fn unconnected_client_send_recv_report_not_connected() {
        let runtime = Builder::new_current_thread()
            .enable_io()
            .build()
            .expect("tokio runtime");
        runtime.block_on(async {
            let mut client = WindowsControlChannelClient::new();
            let send_err = client.send(b"x").await.expect_err("send must error");
            assert!(send_err.to_string().contains("not connected"));
            let mut buf = [0u8; 1];
            let recv_err = client.recv(&mut buf).await.expect_err("recv must error");
            assert!(recv_err.to_string().contains("not connected"));
        });
    }
}

mod send_process_information_tests {
    use super::*;

    /// `Drop` on `SendProcessInformation` with null handles does not call
    /// `CloseHandle` and does not panic - exercises the `is_invalid` short
    /// circuit.
    #[test]
    fn drop_with_null_handles_is_noop() {
        let wrapper = SendProcessInformation(PROCESS_INFORMATION::default());
        drop(wrapper);
    }
}
