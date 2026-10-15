//! Client implementation

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]
#![warn(missing_docs)]

mod console_mode;
mod dispatch;
pub(crate) mod input_bytes;
mod pty;

use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, Read, Write};
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
use cssh_rs_protocol::v1::message::DaemonToClient;
use cssh_rs_protocol::v1::version::{ProtocolVersion, Role};
use cssh_rs_protocol::ClientState;

use crate::client::console_mode::ConsoleModeGuard;
use crate::client::dispatch::dispatch_daemon_message;
use crate::client::input_bytes::key_event_record_to_bytes;
use crate::client::pty::{scan_and_answer_dsr, spawn_client_pty, write_to_master, SharedWriter};
use crate::utils::config::ClientConfig;
use crate::utils::windows::{
    get_console_title, read_keyboard_input, set_console_palette, snapshot_console_palette,
    tinted_palette, ConsolePaletteSnapshot, WindowsApi, WindowsControlChannelClient,
};

/// Length in bytes of the big-endian `u32` frame length prefix.
const LENGTH_PREFIX_LEN: usize = 4;

// Fixed until the PTY follows the console viewport (GitHub #306).
const PTY_ROWS: u16 = 50;
const PTY_COLS: u16 = 200;

/// Duration of the action-feedback flash painted on a highlighted client
/// when the user toggles the state.
const HIGHLIGHT_FLASH_DURATION: Duration = Duration::from_millis(250);

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

/// Connect to the daemon control channel, retrying while the daemon binds it.
///
/// # Arguments
///
/// * `client`   - The control-channel client to connect.
/// * `endpoint` - Named-pipe path of the daemon control channel.
///
/// # Returns
///
/// `true` once connected, `false` after about 30 seconds of failed attempts.
#[cfg_attr(coverage_nightly, coverage(off))]
async fn connect_with_retry(client: &mut WindowsControlChannelClient, endpoint: &OsStr) -> bool {
    for _ in 0..600 {
        if client.connect(endpoint).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    return false;
}

/// Exchange `Hello`s and receive the `Welcome`.
///
/// # Arguments
///
/// * `client` - The connected control-channel client.
///
/// # Returns
///
/// The negotiated maximum frame length and the bytes received past the
/// `Welcome`.
#[cfg_attr(coverage_nightly, coverage(off))]
pub(crate) async fn client_handshake(client: &mut WindowsControlChannelClient) -> (u32, Vec<u8>) {
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

    let mut pending = Vec::new();
    let max_frame_len =
        match read_message::<Hello>(client, &mut pending, DEFAULT_MAX_FRAME_LEN).await {
            Some(daemon_hello) => {
                negotiate_max_frame_len(DEFAULT_MAX_FRAME_LEN, daemon_hello.max_frame_len)
            }
            None => DEFAULT_MAX_FRAME_LEN,
        };
    let _ = read_message::<Welcome>(client, &mut pending, max_frame_len).await;
    return (max_frame_len, pending);
}

/// Receive until `pending` holds a complete frame, then split it off.
///
/// # Arguments
///
/// * `client`        - The connected control-channel client.
/// * `pending`       - Bytes received but not yet returned as a frame.
/// * `max_frame_len` - Largest frame body to accept.
///
/// # Returns
///
/// The frame including its length prefix, or `None` on disconnect, error, or
/// an oversized frame.
#[cfg_attr(coverage_nightly, coverage(off))]
async fn read_frame(
    client: &mut WindowsControlChannelClient,
    pending: &mut Vec<u8>,
    max_frame_len: u32,
) -> Option<Vec<u8>> {
    let mut buffer = [0u8; 8192];
    loop {
        if let Some(prefix) = pending.first_chunk::<LENGTH_PREFIX_LEN>() {
            let body_length = u32::from_be_bytes(*prefix);
            if body_length > max_frame_len {
                error!("Control frame length {} exceeds the maximum", body_length);
                return None;
            }
            let frame_length = LENGTH_PREFIX_LEN + body_length as usize;
            if pending.len() >= frame_length {
                return Some(pending.drain(..frame_length).collect());
            }
        }
        match client.recv(&mut buffer).await {
            Ok(0) => return None,
            Ok(received) => pending.extend_from_slice(&buffer[..received]),
            Err(err) => {
                error!("Control channel receive error: {}", err);
                return None;
            }
        }
    }
}

/// Read one frame and decode it as `T`.
#[cfg_attr(coverage_nightly, coverage(off))]
pub(crate) async fn read_message<T: serde::de::DeserializeOwned>(
    client: &mut WindowsControlChannelClient,
    pending: &mut Vec<u8>,
    max_frame_len: u32,
) -> Option<T> {
    let frame = read_frame(client, pending, max_frame_len).await?;
    let decoded = decode_frames::<T>(frame, max_frame_len).ok()?;
    return decoded.messages.into_iter().next();
}

/// Dispatch daemon messages until the daemon terminates or disconnects, or the
/// SSH child exits.
///
/// # Arguments
///
/// * `api`              - The Windows API implementation to use.
/// * `client`           - The connected control-channel client.
/// * `master`           - Shared PTY master writer.
/// * `child_pid`        - Process id of the SSH child.
/// * `state_sender`     - Watch sender for the client state.
/// * `highlight_sender` - Watch sender for the highlight flag.
/// * `max_frame_len`    - The negotiated maximum frame length.
/// * `pending`          - Bytes received during the handshake but not consumed.
/// * `child_exit`       - Receives the SSH child's exit code.
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
    mut pending: Vec<u8>,
    mut child_exit: oneshot::Receiver<i32>,
) {
    loop {
        tokio::select! {
            frame = read_frame(&mut client, &mut pending, max_frame_len) => {
                let Some(frame) = frame else {
                    return;
                };
                let Ok(decoded) = decode_frames::<DaemonToClient>(frame, max_frame_len) else {
                    return;
                };
                for message in decoded.messages {
                    let flow = dispatch_daemon_message(
                        api,
                        message,
                        &*master,
                        child_pid,
                        state_sender,
                        highlight_sender,
                    );
                    if flow.is_break() {
                        return;
                    }
                }
            }
            exit_code = &mut child_exit => {
                if let Ok(code) = exit_code {
                    info!("SSH child exited with code {}", code);
                }
                return;
            }
        }
    }
}

/// Render the PTY output to this window from a dedicated thread.
///
/// # Arguments
///
/// * `reader` - Reader over the PTY master.
/// * `master` - Shared PTY master writer, for the DSR reply.
#[cfg_attr(coverage_nightly, coverage(off))]
fn spawn_output_reader(mut reader: Box<dyn Read + Send>, master: SharedWriter) {
    std::thread::spawn(move || {
        let mut carry = Vec::new();
        let mut buffer = [0u8; 8192];
        let mut stdout = std::io::stdout();
        while let Ok(received @ 1..) = reader.read(&mut buffer) {
            let rendered = scan_and_answer_dsr(&buffer[..received], &mut carry, &*master);
            let _ = stdout.write_all(&rendered);
            let _ = stdout.flush();
        }
    });
}

/// Forward this window's own keystrokes to the PTY master from a dedicated
/// thread, so a broadcast line stays editable here.
///
/// # Arguments
///
/// * `api`    - The Windows API implementation to use.
/// * `master` - Shared PTY master writer.
#[cfg_attr(coverage_nightly, coverage(off))]
fn spawn_local_input<A: WindowsApi + Clone + 'static>(api: A, master: SharedWriter) {
    std::thread::spawn(move || loop {
        let key = unsafe { read_keyboard_input(&api).KeyEvent };
        if key.bKeyDown.as_bool() {
            write_to_master(&*master, &key_event_record_to_bytes(&key));
        }
    });
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
/// Runs `ssh` inside a ConPTY that receives both the daemon-broadcast and this
/// window's own keystrokes, and renders its output to this window.
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
    daemon_channel: Option<String>,
    config: &ClientConfig,
) {
    // Shield this client from relayed CTRL+C/CTRL+Break so only the SSH child reacts.
    if let Err(err) = api.install_console_ctrl_handler() {
        warn!("Failed to install console control handler: {}", err);
    }

    let Some(daemon_channel) = daemon_channel else {
        error!("The client requires --daemon-channel");
        return;
    };

    // Snapshot the palette before the SSH child writes, so a later tint restores
    // the pristine colors and reads the true default attribute rather than one
    // the child may have changed.
    let base_palette = snapshot_console_palette(api);
    let _console_mode_guard = ConsoleModeGuard::apply(api);

    let (state_sender, state_receiver) = watch::channel(ClientState::Active);
    let (highlight_sender, highlight_receiver) = watch::channel(false);

    let (host, inline_port) = split_host_and_inline_port(&host);
    let port = inline_port.or(cli_port);
    let resolved_username = resolve_username(username, host, config);
    let console_title = build_console_title(&resolved_username, host, port);

    let mut control_channel = WindowsControlChannelClient::new();
    if !connect_with_retry(&mut control_channel, OsStr::new(&daemon_channel)).await {
        error!("Failed to connect to the daemon control channel");
        return;
    }
    let (max_frame_len, pending) = client_handshake(&mut control_channel).await;

    let ssh_arguments = build_ssh_arguments(&resolved_username, host, port, config);
    let mut pty = match spawn_client_pty(&config.program, &ssh_arguments, PTY_ROWS, PTY_COLS) {
        Ok(pty) => pty,
        Err(err) => {
            error!("Failed to spawn `{}` under a PTY: {}", config.program, err);
            return;
        }
    };

    spawn_output_reader(pty.reader, Arc::clone(&pty.writer));
    spawn_local_input(api.clone(), Arc::clone(&pty.writer));

    let mut child_killer = pty.child.clone_killer();
    let (child_exit_sender, child_exit_receiver) = oneshot::channel();
    tokio::task::spawn_blocking(move || {
        let exit_code = pty
            .child
            .wait()
            .map_or(1, |status| return status.exit_code() as i32);
        let _ = child_exit_sender.send(exit_code);
    });

    let control_task = run_control_channel(
        api,
        control_channel,
        pty.writer,
        pty.child_pid,
        &state_sender,
        &highlight_sender,
        max_frame_len,
        pending,
        child_exit_receiver,
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

    // The title and visuals tasks are infinite by construction; if either
    // ever completes, that is a logic bug, not a shutdown path.
    tokio::select! {
        _ = control_task => {}
        _ = title_task => {
            panic!("Title task should never complete");
        }
        _ = visuals_task => {
            panic!("Visuals task should never complete");
        }
    }

    if let Err(err) = child_killer.kill() {
        warn!("Failed to kill SSH child on shutdown: {}", err);
    }
}

#[cfg(test)]
#[path = "../tests/client/test_mod.rs"]
mod test_mod;
