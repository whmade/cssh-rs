//! Client implementation.
//!
//! The client runs `ssh` inside a ConPTY via `portable-pty` and feeds it two
//! merged input sources - keystrokes broadcast from the daemon over the v1
//! control channel, and keystrokes typed directly at this window - while
//! rendering the PTY output to its own console.

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]
#![warn(missing_docs)]

mod console_mode;
mod dispatch;
mod input_bytes;
mod pty;

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use log::{error, info, warn};
use ssh2_config::{ParseRule, SshConfig};
use tokio::sync::{oneshot, watch};
use windows::Win32::System::Console::CONSOLE_CHARACTER_ATTRIBUTES;

use cssh_rs_meta::PACKAGE_NAME;
use cssh_rs_platform::ControlChannelClient;
use cssh_rs_protocol::v1::capability::negotiate_max_frame_len;
use cssh_rs_protocol::v1::codec::{decode_frames, encode_frame};
use cssh_rs_protocol::v1::handshake::{Hello, Welcome};
use cssh_rs_protocol::v1::limits::DEFAULT_MAX_FRAME_LEN;
use cssh_rs_protocol::v1::message::{ClientToDaemon, DaemonToClient};
use cssh_rs_protocol::v1::version::{ProtocolVersion, Role};
use cssh_rs_protocol::v1::window::WindowHandle;

use crate::client::console_mode::ConsoleModeGuard;
use crate::client::dispatch::{dispatch_daemon_message, Dispatch};
use crate::client::input_bytes::key_event_record_to_bytes;
use crate::client::pty::{
    resize_pty, scan_and_answer_dsr, spawn_client_pty, ClientPty, SharedMaster, SharedWriter,
};
use crate::utils::config::ClientConfig;
use crate::utils::windows::{
    console_viewport_size, get_console_title, read_console_input, set_console_palette,
    snapshot_console_palette, tinted_palette, ConsolePaletteSnapshot, WindowsApi,
    WindowsControlChannelClient, KEY_EVENT, WINDOW_BUFFER_SIZE_EVENT,
};

/// Length in bytes of the frame length prefix (`u32`, big-endian).
const LENGTH_PREFIX_LEN: usize = 4;

/// Fallback PTY dimensions used only when the console viewport size cannot be
/// read; otherwise the PTY is seeded from and resized to the live viewport.
const DEFAULT_PTY_ROWS: u16 = 50;
/// Fallback PTY dimensions used only when the console viewport size cannot be
/// read; otherwise the PTY is seeded from and resized to the live viewport.
const DEFAULT_PTY_COLS: u16 = 200;

/// Duration of the action-feedback flash painted on a highlighted client
/// when the user toggles the state.
const HIGHLIGHT_FLASH_DURATION: Duration = Duration::from_millis(250);

/// Client run-state that drives the per-client console visuals. Mirrors the
/// two operative states of the wire `ClientRunState` (a forward-compatible
/// `Unknown` is folded into `Active` at the dispatch boundary).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ClientState {
    /// The client receives and replays broadcast input.
    Active,
    /// The daemon has suppressed input forwarding to the client.
    Disabled,
}

/// A state repaint: `Tint` swaps the console palette so the window background
/// takes on the state color, `Restore` writes the saved palette back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ConsolePaint {
    Restore,
    Tint(CONSOLE_CHARACTER_ATTRIBUTES),
}

/// Resolve the steady-state paint intent for `(state, highlighted)`; the
/// highlight overlays the disabled color.
fn get_effective_color(
    state: ClientState,
    highlighted: bool,
    disabled_console_color: CONSOLE_CHARACTER_ATTRIBUTES,
    highlighted_console_color: CONSOLE_CHARACTER_ATTRIBUTES,
) -> ConsolePaint {
    if highlighted {
        return ConsolePaint::Tint(highlighted_console_color);
    }
    match state {
        ClientState::Active => return ConsolePaint::Restore,
        ClientState::Disabled => return ConsolePaint::Tint(disabled_console_color),
    }
}

/// Resolve the flash paint intent for `state`, bypassing the highlight overlay.
fn get_flash_color(
    state: ClientState,
    disabled_console_color: CONSOLE_CHARACTER_ATTRIBUTES,
) -> ConsolePaint {
    match state {
        ClientState::Active => return ConsolePaint::Restore,
        ClientState::Disabled => return ConsolePaint::Tint(disabled_console_color),
    }
}

/// Bundle of the state colors [`run_visuals_loop`] chooses between when
/// repainting the per-client console.
struct ConsolePalette {
    /// Pristine palette captured at startup; `None` (the palette could not be
    /// read) degrades every paint to a no-op.
    base: Option<ConsolePaletteSnapshot>,
    /// Color applied while [`ClientState::Disabled`].
    disabled: CONSOLE_CHARACTER_ATTRIBUTES,
    /// Color applied while the client is the highlighted submenu target.
    highlighted: CONSOLE_CHARACTER_ATTRIBUTES,
}

/// Repaint the console to the steady-state look for `(state, highlighted)`.
fn paint_steady(
    api: &dyn WindowsApi,
    state: ClientState,
    highlighted: bool,
    palette: &ConsolePalette,
    last: &mut Option<ConsolePaint>,
) {
    paint_console_color(
        api,
        get_effective_color(state, highlighted, palette.disabled, palette.highlighted),
        palette.base.as_ref(),
        last,
    );
}

/// Paint the action-feedback flash and return the deadline to clear it at.
fn start_flash(
    api: &dyn WindowsApi,
    state: ClientState,
    palette: &ConsolePalette,
    last: &mut Option<ConsolePaint>,
) -> tokio::time::Instant {
    paint_console_color(
        api,
        get_flash_color(state, palette.disabled),
        palette.base.as_ref(),
        last,
    );
    return tokio::time::Instant::now() + HIGHLIGHT_FLASH_DURATION;
}

/// Apply `target` if it differs from `last`, then update `last`.
///
/// `Tint` recolors the window by swapping the console palette; `Restore` writes
/// the pristine `base` palette back. Neither touches the screen-buffer cells, so
/// VT/24-bit text the palette cannot hold survives.
fn paint_console_color(
    api: &dyn WindowsApi,
    target: ConsolePaint,
    base: Option<&ConsolePaletteSnapshot>,
    last: &mut Option<ConsolePaint>,
) {
    // An unchanged repaint still costs a conhost LPC roundtrip and a WM_PAINT.
    if *last == Some(target) {
        return;
    }
    // No captured palette (unreadable at startup) degrades every paint to a no-op.
    let Some(base) = base else {
        return;
    };
    let palette = match target {
        ConsolePaint::Tint(color) => tinted_palette(base, color),
        ConsolePaint::Restore => base.color_table,
    };
    // Leave `last` untouched on a failed write so the next transition retries
    // instead of recording a paint that never reached the screen.
    if !set_console_palette(api, &palette) {
        return;
    }
    *last = Some(target);
}

/// Resolve the username from the provided value or SSH config.
///
/// # Arguments
///
/// * `username` - Optional username to use. If None, will try to resolve from SSH config.
/// * `host` - The hostname (without port) to connect to.
/// * `config` - The client configuration containing SSH config path.
///
/// # Returns
///
/// The resolved username.
fn resolve_username(username: Option<String>, host: &str, config: &ClientConfig) -> String {
    if let Some(val) = username {
        return val;
    }

    let mut ssh_config = SshConfig::default();
    let ssh_config_path = Path::new(config.ssh_config_path.as_str());
    if ssh_config_path.exists() {
        let mut reader = BufReader::new(
            File::open(ssh_config_path).expect("Could not open SSH configuration file."),
        );
        ssh_config = SshConfig::default()
            .parse(&mut reader, ParseRule::ALLOW_UNKNOWN_FIELDS)
            .expect("Failed to parse SSH configuration file");
    }
    return ssh_config
        .query(<&str>::clone(&host))
        .user
        .unwrap_or_default();
}

/// Build the SSH arguments from the username, host, port, and config.
///
/// # Arguments
///
/// * `username`    - The username to connect with.
/// * `host`        - The hostname to connect to.
/// * `port`        - Optional port number (0-65535).
/// * `config`      - The client config indicating how to call the SSH program.
///
/// # Returns
///
/// A vector of arguments ready to be passed to the SSH command.
fn build_ssh_arguments(
    username: &str,
    host: &str,
    port: Option<u16>,
    config: &ClientConfig,
) -> Vec<String> {
    let username_host = format!("{username}@{host}");

    let mut arguments = replace_argument_placeholders(
        &config.arguments,
        &config.username_host_placeholder,
        &username_host,
    );

    // Add port arguments if port was specified
    if let Some(port) = port {
        arguments.push("-p".to_string());
        arguments.push(port.to_string());
    }

    return arguments;
}

/// Replaces placeholders in SSH command arguments.
///
/// # Arguments
///
/// * `arguments` - The argument templates.
/// * `placeholder` - The placeholder string to replace.
/// * `replacement` - The value to replace the placeholder with.
///
/// # Returns
///
/// A vector of arguments with placeholders replaced.
fn replace_argument_placeholders(
    arguments: &[String],
    placeholder: &str,
    replacement: &str,
) -> Vec<String> {
    return arguments
        .iter()
        .map(|arg| return arg.replace(placeholder, replacement))
        .collect();
}

/// Connect the control-channel client to `endpoint`, retrying while the daemon
/// races to bind its end.
///
/// # Arguments
///
/// * `client`   - The control-channel client to connect.
/// * `endpoint` - The daemon-supplied named-pipe path.
///
/// # Returns
///
/// `true` once connected, `false` if the daemon never became reachable.
#[cfg_attr(coverage_nightly, coverage(off))]
async fn connect_with_retry(client: &mut WindowsControlChannelClient, endpoint: &OsStr) -> bool {
    // Many clients race the daemon's per-instance bind; keep retrying while it
    // catches up. Bounded so a never-appearing daemon does not hang forever.
    for _ in 0..600u32 {
        if client.connect(endpoint).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    return false;
}

/// Perform the v1 handshake: send our `Hello`, consume the daemon's `Hello`
/// and `Welcome`, and negotiate the send frame cap.
///
/// # Arguments
///
/// * `client` - The connected control-channel client.
///
/// # Returns
///
/// The negotiated maximum frame length and any control bytes read past the
/// handshake (initial `StateChange`/`Highlight` frames the daemon sends
/// immediately).
#[cfg_attr(coverage_nightly, coverage(off))]
async fn client_handshake(client: &mut WindowsControlChannelClient) -> (u32, Vec<u8>) {
    let hello = Hello {
        protocol_version: ProtocolVersion::new(1, 0),
        role: Role::Client,
        pid: std::process::id(),
        capabilities: Vec::new(),
        max_frame_len: DEFAULT_MAX_FRAME_LEN,
    };
    if let Ok(frame) = encode_frame(&hello, DEFAULT_MAX_FRAME_LEN) {
        if let Err(err) = client.send(&frame).await {
            warn!("Failed to send client Hello: {}", err);
        }
    }

    let mut acc: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    let max_frame_len =
        match read_typed::<Hello>(client, &mut acc, &mut buf, DEFAULT_MAX_FRAME_LEN).await {
            Some(daemon_hello) => {
                negotiate_max_frame_len(DEFAULT_MAX_FRAME_LEN, daemon_hello.max_frame_len)
            }
            None => DEFAULT_MAX_FRAME_LEN,
        };
    // Consume the Welcome; client_id/server_capabilities are unused for now.
    let _ = read_typed::<Welcome>(client, &mut acc, &mut buf, max_frame_len).await;
    return (max_frame_len, acc);
}

/// Send `Ready` so the daemon can correlate this connection and learn the
/// client window handle.
///
/// # Arguments
///
/// * `client`         - The connected control-channel client.
/// * `child_pid`      - Pid of the spawned SSH child.
/// * `window_hwnd`    - This window's console handle as a raw `u64`.
/// * `max_frame_len`  - The negotiated frame cap.
#[cfg_attr(coverage_nightly, coverage(off))]
async fn send_ready(
    client: &mut WindowsControlChannelClient,
    child_pid: u32,
    window_hwnd: u64,
    max_frame_len: u32,
) {
    let handle = WindowHandle::windows_hwnd(window_hwnd);
    let ready = ClientToDaemon::Ready {
        child_pid,
        window: handle,
    };
    match encode_frame(&ready, max_frame_len) {
        Ok(frame) => {
            if let Err(err) = client.send(&frame).await {
                warn!("Failed to send Ready to daemon: {}", err);
            }
        }
        Err(err) => warn!("Failed to encode Ready: {}", err),
    }
}

/// Read one complete length-prefixed frame, recv-ing more bytes as needed.
///
/// # Arguments
///
/// * `client` - The connected control-channel client.
/// * `acc`    - Buffer of bytes read but not yet framed; drained in place.
/// * `buf`    - Scratch receive buffer.
/// * `max`    - Maximum permitted frame length.
///
/// # Returns
///
/// The full frame (prefix included), or `None` on EOF/oversized frame/error.
#[cfg_attr(coverage_nightly, coverage(off))]
async fn read_one_frame(
    client: &mut WindowsControlChannelClient,
    acc: &mut Vec<u8>,
    buf: &mut [u8],
    max: u32,
) -> Option<Vec<u8>> {
    loop {
        if acc.len() >= LENGTH_PREFIX_LEN {
            let len = u32::from_be_bytes([acc[0], acc[1], acc[2], acc[3]]);
            if len > max {
                error!("control frame length {} exceeds the maximum", len);
                return None;
            }
            let total = LENGTH_PREFIX_LEN + len as usize;
            if acc.len() >= total {
                return Some(acc.drain(..total).collect());
            }
        }
        match client.recv(buf).await {
            Ok(0) => return None,
            Ok(n) => acc.extend_from_slice(&buf[..n]),
            Err(err) => {
                error!("control channel recv error: {}", err);
                return None;
            }
        }
    }
}

/// Read a single frame and decode it as `T`.
#[cfg_attr(coverage_nightly, coverage(off))]
async fn read_typed<T: serde::de::DeserializeOwned>(
    client: &mut WindowsControlChannelClient,
    acc: &mut Vec<u8>,
    buf: &mut [u8],
    max: u32,
) -> Option<T> {
    let frame = read_one_frame(client, acc, buf, max).await?;
    let decoded = decode_frames::<T>(frame, max).ok()?;
    return decoded.messages.into_iter().next();
}

/// Drive the control channel: decode daemon messages and dispatch them, and on
/// SSH-child exit report `ChildExited` before returning.
///
/// # Arguments
///
/// * `api`              - The Windows API implementation to use.
/// * `client`           - The connected control-channel client.
/// * `master`           - Shared PTY master writer.
/// * `child_pid`        - Pid of the SSH child (signal target).
/// * `state_sender`     - Watch sender for the authoritative visual state.
/// * `highlight_sender` - Watch sender for the highlight flag.
/// * `max_frame_len`    - The negotiated frame cap.
/// * `initial`          - Bytes read past the handshake to process first.
/// * `exit_rx`          - Receives the SSH child's exit code when it exits.
#[allow(clippy::too_many_arguments)]
#[cfg_attr(coverage_nightly, coverage(off))]
async fn run_control_channel(
    api: &dyn WindowsApi,
    mut client: WindowsControlChannelClient,
    master: SharedWriter,
    child_pid: u32,
    state_sender: &watch::Sender<ClientState>,
    highlight_sender: &watch::Sender<bool>,
    max_frame_len: u32,
    initial: Vec<u8>,
    mut exit_rx: oneshot::Receiver<i32>,
) {
    let mut acc = initial;
    let mut buf = [0u8; 8192];
    loop {
        tokio::select! {
            frame = read_one_frame(&mut client, &mut acc, &mut buf, max_frame_len) => {
                let Some(frame) = frame else {
                    return;
                };
                let decoded = match decode_frames::<DaemonToClient>(frame, max_frame_len) {
                    Ok(decoded) => decoded,
                    Err(err) => {
                        error!("failed to decode daemon frame: {}", err);
                        return;
                    }
                };
                for message in decoded.messages {
                    if let Dispatch::Terminate = dispatch_daemon_message(
                        api,
                        message,
                        &master,
                        child_pid,
                        state_sender,
                        highlight_sender,
                    ) {
                        return;
                    }
                }
            }
            code = &mut exit_rx => {
                if let Ok(code) = code {
                    info!("ssh child exited with code {}", code);
                    let exited = ClientToDaemon::ChildExited { code };
                    if let Ok(frame) = encode_frame(&exited, max_frame_len) {
                        let _ = client.send(&frame).await;
                    }
                }
                return;
            }
        }
    }
}

/// Drain the PTY master in a dedicated thread: answer the conhost DSR query and
/// render the child's output to this window's stdout.
///
/// # Arguments
///
/// * `reader` - Reader over the PTY master.
/// * `master` - Shared PTY master writer (for the DSR reply).
#[cfg_attr(coverage_nightly, coverage(off))]
fn spawn_output_reader(mut reader: Box<dyn std::io::Read + Send>, master: SharedWriter) {
    std::thread::spawn(move || {
        let mut carry: Vec<u8> = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let rendered = scan_and_answer_dsr(&buf[..n], &mut carry, &master);
                    let mut stdout = std::io::stdout();
                    let _ = stdout.write_all(&rendered);
                    let _ = stdout.flush();
                }
            }
        }
    });
}

/// Read this window's console input on a dedicated thread: forward locally
/// typed keystrokes to the PTY master (so a daemon-broadcast line can be edited
/// directly here) and resize the PTY when the console window changes size.
///
/// The thread blocks in `ReadConsoleInputW`; it is reclaimed by process exit at
/// shutdown rather than joined.
///
/// # Arguments
///
/// * `api`        - The Windows API implementation to use (cloned into the thread).
/// * `writer`     - Shared PTY master writer for locally typed keystrokes.
/// * `pty_master` - Shared PTY master, resized on window-size-change events.
#[cfg_attr(coverage_nightly, coverage(off))]
fn spawn_local_input<A: WindowsApi + Clone + 'static>(
    api: A,
    writer: SharedWriter,
    pty_master: SharedMaster,
) {
    std::thread::spawn(move || loop {
        let record = read_console_input(&api);
        match record.EventType {
            WINDOW_BUFFER_SIZE_EVENT => {
                // The event carries the new buffer size, but the PTY tracks the
                // visible viewport, so re-read the window rectangle instead.
                if let Some((cols, rows)) = console_viewport_size(&api) {
                    resize_pty(&pty_master, cols, rows);
                }
            }
            KEY_EVENT => {
                let key = unsafe { record.Event.KeyEvent };
                if !key.bKeyDown.as_bool() {
                    continue;
                }
                let bytes = key_event_record_to_bytes(&key);
                if bytes.is_empty() {
                    continue;
                }
                if let Ok(mut writer) = writer.lock() {
                    let _ = writer.write_all(&bytes);
                    let _ = writer.flush();
                }
            }
            _ => {}
        }
    });
}

/// Splits `host` on its trailing `:port` suffix (if any) and parses
/// the port. An invalid `:port` is logged and treated as absent so
/// the CLI port can still apply.
///
/// # Arguments
///
/// * `host` - Raw host argument, optionally with `:port` suffix.
///
/// # Returns
///
/// `(host_without_port, inline_port)`.
fn split_host_and_inline_port(host: &str) -> (&str, Option<u16>) {
    let (bare_host, port_str) = host
        .rsplit_once(':')
        .map_or((host, None), |(h, p)| return (h, Some(p)));
    let inline_port = port_str.and_then(|p| {
        return p
            .parse::<u16>()
            .map_err(|e| {
                warn!("Invalid port '{}': {}. Using default SSH port.", p, e);
            })
            .ok();
    });
    return (bare_host, inline_port);
}

/// Builds the console window title shown to the user.
///
/// # Arguments
///
/// * `resolved_username` - Username after SSH config resolution.
/// * `host`              - Bare hostname.
/// * `port`              - Effective port (inline or CLI), if any.
///
/// # Returns
///
/// The console title string in `cssh-rs - user@host[:port]` form.
fn build_console_title(resolved_username: &str, host: &str, port: Option<u16>) -> String {
    let title_host = if let Some(port) = port {
        format!("{host}:{port}")
    } else {
        host.to_string()
    };
    return format!("{PACKAGE_NAME} - {resolved_username}@{title_host}");
}

/// Keeps the console window title pinned to `console_title`, since
/// the SSH child can overwrite it on connect.
///
/// # Arguments
///
/// * `api`           - The Windows API implementation to use.
/// * `console_title` - The title to (re)apply.
async fn run_title_loop(api: &dyn WindowsApi, console_title: String) {
    loop {
        if console_title != get_console_title(api) {
            api.set_console_title(console_title.as_str())
                .unwrap_or_else(|err| {
                    error!("Failed to set console title: {}", err);
                });
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Drive the per-client console color: track `state_receiver` and
/// `highlight_receiver`, paint the steady-state combination, and flash the
/// underlying state color for [`HIGHLIGHT_FLASH_DURATION`]. A `None` `base`
/// (the palette could not be read) degrades all painting to a no-op.
async fn run_visuals_loop(
    api: &dyn WindowsApi,
    mut state_receiver: watch::Receiver<ClientState>,
    mut highlight_receiver: watch::Receiver<bool>,
    base: Option<ConsolePaletteSnapshot>,
    disabled_console_color: CONSOLE_CHARACTER_ATTRIBUTES,
    highlighted_console_color: CONSOLE_CHARACTER_ATTRIBUTES,
) {
    let palette = ConsolePalette {
        base,
        disabled: disabled_console_color,
        highlighted: highlighted_console_color,
    };
    let mut prev_state = *state_receiver.borrow_and_update();
    let mut prev_highlight = *highlight_receiver.borrow_and_update();
    // The console shows its pristine palette at startup, so the initial paint is
    // a no-op restore rather than an unknown state.
    let mut last_painted: Option<ConsolePaint> = Some(ConsolePaint::Restore);
    let mut flash_until: Option<tokio::time::Instant> = None;

    paint_steady(api, prev_state, prev_highlight, &palette, &mut last_painted);

    loop {
        // Independent watch channels: `state_receiver` and `highlight_receiver` may be observed out of send-order, so the flash branch can fire (or not) on stale `prev_highlight`.
        tokio::select! {
            state_changed = state_receiver.changed() => {
                if state_changed.is_err() {
                    return;
                }
                prev_state = *state_receiver.borrow_and_update();
                if prev_highlight {
                    flash_until =
                        Some(start_flash(api, prev_state, &palette, &mut last_painted));
                } else {
                    paint_steady(api, prev_state, prev_highlight, &palette, &mut last_painted);
                    flash_until = None;
                }
            }
            highlight_changed = highlight_receiver.changed() => {
                if highlight_changed.is_err() {
                    return;
                }
                let next_highlight = *highlight_receiver.borrow_and_update();
                if next_highlight == prev_highlight {
                    continue;
                }
                prev_highlight = next_highlight;
                flash_until = None;
                paint_steady(api, prev_state, prev_highlight, &palette, &mut last_painted);
            }
            _ = async {
                match flash_until {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                flash_until = None;
                paint_steady(api, prev_state, prev_highlight, &palette, &mut last_painted);
            }
        }
    }
}

/// The entrypoint for the `client` subcommand with API dependency injection.
///
/// Connects to the daemon's control channel, runs `ssh` inside a ConPTY, merges
/// daemon-broadcast and locally-typed input onto the one PTY master, and renders
/// the child's output to this window.
///
/// # Arguments
///
/// * `api`            - The Windows API implementation to use.
/// * `host`           - The name of the host to connect to, optionally with `:port` suffix.
/// * `username`       - The username to be used. Resolved from the SSH config if `None`.
/// * `cli_port`       - Optional port from CLI option. Inline port takes precedence.
/// * `daemon_channel` - Named-pipe path of the daemon control channel.
/// * `config`         - A reference to the `ClientConfig`.
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn main<A: WindowsApi + Clone + 'static>(
    api: &A,
    host: String,
    username: Option<String>,
    cli_port: Option<u16>,
    daemon_channel: Option<OsString>,
    config: &ClientConfig,
) {
    // Shield this client from relayed/typed CTRL+C/CTRL+Break so only the SSH
    // child reacts to them.
    if let Err(err) = api.install_console_ctrl_handler() {
        warn!("Failed to install console control handler: {}", err);
    }

    let Some(endpoint) = daemon_channel else {
        error!("client requires --daemon-channel; refusing to start");
        return;
    };

    // Snapshot the palette before the SSH child writes, so a later tint restores
    // the pristine colors and reads the true default attribute rather than one
    // the child may have changed.
    let base_palette = snapshot_console_palette(api);
    // Raw stdin + VT stdout for the session; restored when this guard drops.
    let _mode_guard = ConsoleModeGuard::apply(api);

    let (state_sender, state_receiver) = watch::channel(ClientState::Active);
    let (highlight_sender, highlight_receiver) = watch::channel(false);

    let (host, inline_port) = split_host_and_inline_port(&host);
    let port = inline_port.or(cli_port);
    let resolved_username = resolve_username(username, host, config);
    let console_title = build_console_title(&resolved_username, host, port);

    let mut control = WindowsControlChannelClient::new();
    if !connect_with_retry(&mut control, &endpoint).await {
        error!("Failed to connect to daemon control channel; giving up");
        return;
    }
    let (max_frame_len, initial) = client_handshake(&mut control).await;

    let ssh_args = build_ssh_arguments(&resolved_username, host, port, config);
    // Seed the PTY from the live viewport so `ssh` sees the real terminal size;
    // fall back to fixed dimensions only if the viewport cannot be read.
    let (init_cols, init_rows) =
        console_viewport_size(api).unwrap_or((DEFAULT_PTY_COLS, DEFAULT_PTY_ROWS));
    let pty = match spawn_client_pty(&config.program, &ssh_args, init_rows, init_cols) {
        Ok(pty) => pty,
        Err(err) => {
            error!("Failed to spawn `{}` under a PTY: {}", config.program, err);
            return;
        }
    };
    let ClientPty {
        writer,
        reader,
        mut child,
        child_pid,
        master: pty_master,
    } = pty;

    let window_hwnd = api.get_console_window().0 as usize as u64;
    send_ready(&mut control, child_pid, window_hwnd, max_frame_len).await;

    spawn_output_reader(reader, Arc::clone(&writer));
    spawn_local_input(api.clone(), Arc::clone(&writer), Arc::clone(&pty_master));

    // Watch the SSH child from a blocking thread; hand its exit code to the
    // control task so it can report `ChildExited` before shutting down.
    let mut killer = child.clone_killer();
    let (exit_tx, exit_rx) = oneshot::channel::<i32>();
    tokio::task::spawn_blocking(move || {
        let code = child
            .wait()
            .map(|status| return status.exit_code() as i32)
            .unwrap_or(1);
        let _ = exit_tx.send(code);
    });

    let control_task = run_control_channel(
        api,
        control,
        Arc::clone(&writer),
        child_pid,
        &state_sender,
        &highlight_sender,
        max_frame_len,
        initial,
        exit_rx,
    );
    let title_task = run_title_loop(api, console_title);
    let visuals_task = run_visuals_loop(
        api,
        state_receiver,
        highlight_receiver,
        base_palette,
        CONSOLE_CHARACTER_ATTRIBUTES(config.disabled_console_color),
        CONSOLE_CHARACTER_ATTRIBUTES(config.highlighted_console_color),
    );

    // The title and visuals tasks are infinite by construction; the control
    // task ends on daemon `Terminate`, disconnect, or SSH-child exit.
    tokio::select! {
        _ = control_task => {}
        _ = title_task => {
            panic!("Title task should never complete");
        }
        _ = visuals_task => {
            panic!("Visuals task should never complete");
        }
    }

    if let Err(err) = killer.kill() {
        warn!("Failed to kill SSH child on shutdown: {}", err);
    }
}

#[cfg(test)]
#[path = "../tests/client/test_mod.rs"]
mod test_mod;
