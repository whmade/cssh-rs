//! Client implementation

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]
#![warn(missing_docs)]

use log::{error, info, warn};
use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;
use std::process::ExitStatus;
use std::time::Duration;
use windows::Win32::UI::Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_C, VK_CANCEL};

use crate::utils::config::ClientConfig;
use crate::utils::windows::{
    get_console_title, set_console_palette, snapshot_console_palette, tinted_palette,
    ConsolePaletteSnapshot, WindowsApi,
};
use ssh2_config::{ParseRule, SshConfig};
use tokio::net::windows::named_pipe::NamedPipeClient;
use tokio::process::{Child, Command};
use tokio::sync::watch;
use tokio::{io::Interest, net::windows::named_pipe::ClientOptions};
use windows::Win32::System::Console::{
    CONSOLE_CHARACTER_ATTRIBUTES, ENABLE_PROCESSED_INPUT, INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT,
    KEY_EVENT_RECORD, LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED, RIGHT_ALT_PRESSED, RIGHT_CTRL_PRESSED,
    SHIFT_PRESSED, STD_INPUT_HANDLE,
};

use cssh_rs_protocol::{
    deserialization::parse_daemon_to_client_messages, serialization::serialize_pid, ClientState,
    DaemonToClientMessage, SERIALIZED_INPUT_RECORD_0_LENGTH, SERIALIZED_PID_LENGTH,
};

use cssh_rs_meta::PACKAGE_NAME;

use crate::utils::constants::PIPE_NAME;

/// Possible results when reading from the named pipe and writing to the
/// current process's stdinput.
enum ReadWriteResult {
    /// We wrote all complete [INPUT_RECORD_0] sequences we read from
    /// the named pipe to stdin.
    Success {
        /// Incomplete [INPUT_RECORD_0] sequence.
        ///
        /// What we read from the named pipe is a serialized [INPUT_RECORD_0].`KeyEvent`.
        /// As this is simply a [`SERIALIZED_INPUT_RECORD_0_LENGTH`] byte long sequence and we try to read from the pipe until we
        /// have some of the data it can happen that during any one read/write iteration we don't
        /// read the full sequence so we must keep track of what we read for next iterations
        /// where we will be able to read the remainder of the sequence.
        remainder: Vec<u8>,
        /// List of [KEY_EVENT_RECORD]s we have read from the named pipe.
        ///
        /// Used to detect the `Alt + Shift + C` key combination used
        /// to close the console window after the client process encountered an unexpected error.
        key_event_records: Vec<KEY_EVENT_RECORD>,
    },
    /// Trying to read from the pipe would require us to wait for data.
    WouldBlock,
    /// Something went wrong.
    Err,
    /// The pipe was closed.
    Disconnect,
}

/// Duration of the action-feedback flash painted on a highlighted client
/// when the user toggles the state.
const HIGHLIGHT_FLASH_DURATION: Duration = Duration::from_millis(250);

/// Grace period for the SSH child to exit after the console interrupt before
/// [`shutdown_child`] force-kills it.
const CHILD_EXIT_GRACE_PERIOD: Duration = Duration::from_millis(500);

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

/// Write the given [INPUT_RECORD_0] to the console input buffer using the provided API.
///
/// # Arguments
///
/// * `api` - The Windows API implementation to use.
/// * `input_record` - The [INPUT_RECORD_0].`KeyEvent` input record to write.
fn write_console_input(api: &dyn WindowsApi, input_record: INPUT_RECORD_0) {
    let buffer: [INPUT_RECORD; 1] = [INPUT_RECORD {
        EventType: KEY_EVENT as u16,
        Event: input_record,
    }];
    let mut nb_of_events_written = 0u32;
    match api.write_console_input(&buffer, &mut nb_of_events_written) {
        Ok(_) => {
            if nb_of_events_written == 0 {
                error!("Failed to write console input");
                error!("{:?}", api.get_last_error());
            }
        }
        Err(_) => {
            error!("Failed to write console input");
            error!("{:?}", api.get_last_error());
        }
    };
}

/// Report whether a forwarded key event is Ctrl+C or Ctrl+Break.
///
/// # Arguments
///
/// * `key_event` - The key event record forwarded by the daemon.
fn is_console_signal_key(key_event: &KEY_EVENT_RECORD) -> bool {
    return key_event.dwControlKeyState & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0
        && matches!(VIRTUAL_KEY(key_event.wVirtualKeyCode), VK_C | VK_CANCEL);
}

/// Report whether the console input buffer has `ENABLE_PROCESSED_INPUT` set,
/// treating any query failure as not set.
///
/// # Arguments
///
/// * `api` - The Windows API implementation to use.
fn processed_input_enabled(api: &dyn WindowsApi) -> bool {
    let Ok(handle) = api.get_std_handle(STD_INPUT_HANDLE) else {
        return false;
    };
    return match api.get_console_mode(handle) {
        Ok(mode) => mode.0 & ENABLE_PROCESSED_INPUT.0 != 0,
        Err(_) => false,
    };
}

/// Replay one daemon-forwarded key event into the client console.
///
/// Ctrl+C and Ctrl+Break injected via `WriteConsoleInputW` never raise a
/// console control signal, so when the input buffer has processed input
/// enabled they are re-raised via `GenerateConsoleCtrlEvent`; all other
/// records (and raw-mode consoles) are written verbatim.
///
/// # Arguments
///
/// * `api`          - The Windows API implementation to use.
/// * `input_record` - The [INPUT_RECORD_0].`KeyEvent` record to replay.
fn replay_input_record(api: &dyn WindowsApi, input_record: INPUT_RECORD_0) {
    let key_event = unsafe { input_record.KeyEvent };
    if is_console_signal_key(&key_event) && processed_input_enabled(api) {
        // Re-raise on key-down only, dropping both records so no literal Ctrl+C
        // reaches the child. Group 0 signals this client too, but the startup
        // handler shields it.
        if key_event.bKeyDown.as_bool() {
            if let Err(err) = api.interrupt_console_process_group() {
                warn!("Failed to relay console control event: {}", err);
            }
        }
        return;
    }
    write_console_input(api, input_record);
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

/// Launch the SSH process.
///
/// The process might overwrite the console title once it launched, so we wait for that
/// to happen and set the title again.
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
/// The handle to created [Child] process.
async fn launch_ssh_process(
    username: &str,
    host: &str,
    port: Option<u16>,
    config: &ClientConfig,
) -> Child {
    let arguments = build_ssh_arguments(username, host, port, config);
    let child = Command::new(&config.program)
        .args(arguments.clone())
        // Backstop for paths that bypass `shutdown_child` (e.g. a panic).
        .kill_on_drop(true)
        .spawn()
        .unwrap_or_else(|err| {
            let args: String = arguments.join(" ");
            error!("{}", err);
            panic!(
                "Failed to launch process `{}` with arguments `{}`",
                config.program, args
            )
        });
    return child;
}

/// Testable view of the SSH child process used by [`shutdown_child`].
trait ChildProcess {
    /// Wait until the child exits.
    ///
    /// # Returns
    ///
    /// The child's [`ExitStatus`], or the error reported while waiting.
    async fn wait(&mut self) -> io::Result<ExitStatus>;

    /// Force-kill the child and wait until it has terminated.
    ///
    /// # Returns
    ///
    /// `Ok(())` once terminated, or the error reported by the kill.
    async fn kill(&mut self) -> io::Result<()>;
}

impl ChildProcess for Child {
    async fn wait(&mut self) -> io::Result<ExitStatus> {
        return Child::wait(self).await;
    }

    async fn kill(&mut self) -> io::Result<()> {
        Child::start_kill(self)?;
        Child::wait(self).await?;
        return Ok(());
    }
}

/// Guarantee the SSH child terminates once the client's run loop has ended.
///
/// Interrupts the console process group for a graceful exit, then force-kills
/// after [`CHILD_EXIT_GRACE_PERIOD`]; children that handle the signal themselves
/// (e.g. cmd.exe) would otherwise keep the client window open forever.
///
/// # Arguments
///
/// * `api`   - The Windows API implementation to use.
/// * `child` - Handle to the SSH child process.
async fn shutdown_child(api: &dyn WindowsApi, child: &mut impl ChildProcess) {
    // Interrupt the whole group so even a child that ignores Ctrl+C exits; the
    // startup handler shields this client so it survives to force the kill.
    if let Err(err) = api.interrupt_console_process_group() {
        warn!("Failed to interrupt console process group: {}", err);
    }
    match tokio::time::timeout(CHILD_EXIT_GRACE_PERIOD, child.wait()).await {
        Ok(Ok(exit_status)) => {
            info!("Child exited after console interrupt: {}", exit_status);
            return;
        }
        Ok(Err(err)) => {
            warn!("Failed to wait for child exit: {}", err);
        }
        Err(_) => {
            warn!(
                "Child still running {}ms after console interrupt; force-killing it",
                CHILD_EXIT_GRACE_PERIOD.as_millis()
            );
        }
    }
    if let Err(err) = child.kill().await {
        error!("Failed to kill child process: {}", err);
    }
    return;
}

/// Read all available daemon-to-client messages from the named pipe and apply them.
///
/// Input records are written to the console input buffer using the provided API
/// and their key-event payloads are returned via `ReadWriteResult::Success` so
/// the caller can detect the Alt+Shift+C close combination. State-change frames
/// are forwarded via [`watch::Sender::send_replace`] on `state_sender`, making the
/// authoritative [`ClientState`] visible to every watch subscriber (currently
/// the visuals task in [`main`]) without coupling this loop to any
/// state-dependent rendering. Keep-alive frames are ignored. Partial trailing
/// frames are returned as `remainder` for the next call to prepend.
///
/// # Arguments
///
/// * `api`                 - The Windows API implementation to use.
/// * `named_pipe_client`   - The [Windows named pipe][1] client that has successfully connected to
///                           the named pipe created by the daemon.
/// * `internal_buffer`     - Vector containing the unconsumed bytes (possibly an
///                           incomplete trailing frame) from a previous call.
/// * `state_sender`            - Watch sender used to broadcast every
///                           [`DaemonToClientMessage::StateChange`] payload as
///                           the client's authoritative [`ClientState`].
/// * `highlight_sender`        - Watch sender used to broadcast every
///                           [`DaemonToClientMessage::Highlight`] payload as
///                           the client's current highlight flag.
/// # Returns
///
/// A `ReadWriteResult` indicating whether we were able to read from the named pipe and write the available INPUT_RECORDs
/// to the console input buffer or not.
///
/// [1]: https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipes
async fn read_write_loop(
    api: &dyn WindowsApi,
    named_pipe_client: &NamedPipeClient,
    internal_buffer: &mut Vec<u8>,
    state_sender: &watch::Sender<ClientState>,
    highlight_sender: &watch::Sender<bool>,
) -> ReadWriteResult {
    let mut buf: [u8; SERIALIZED_INPUT_RECORD_0_LENGTH * 10] =
        [0; SERIALIZED_INPUT_RECORD_0_LENGTH * 10];
    match named_pipe_client.try_read(&mut buf) {
        Ok(0) => {
            // Seems to only happen if the pipe is closed/server disconnects
            // indicating that the daemon has been closed.
            // Exit the client too in that case.
            return ReadWriteResult::Disconnect;
        }
        Ok(n) => {
            internal_buffer.extend_from_slice(&buf[..n]);
            let (messages, remainder) = parse_daemon_to_client_messages(internal_buffer);
            let mut key_event_records: Vec<KEY_EVENT_RECORD> = Vec::new();
            for message in messages {
                match message {
                    DaemonToClientMessage::InputRecord(input_record) => {
                        replay_input_record(api, input_record);
                        key_event_records.push(unsafe { input_record.KeyEvent });
                    }
                    DaemonToClientMessage::StateChange(state) => {
                        state_sender.send_replace(state);
                    }
                    DaemonToClientMessage::Highlight(highlighted) => {
                        highlight_sender.send_replace(highlighted);
                    }
                    DaemonToClientMessage::KeepAlive => {}
                }
            }
            return ReadWriteResult::Success {
                remainder,
                key_event_records,
            };
        }
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
            return ReadWriteResult::WouldBlock;
        }
        Err(e) => {
            error!("{}", e);
            return ReadWriteResult::Err;
        }
    }
}

/// Checks if a key event represents the Alt+Shift+C combination.
///
/// # Arguments
///
/// * `key_event` - The key event record to check.
///
/// # Returns
///
/// `true` if the key event represents Alt+Shift+C, `false` otherwise.
fn is_alt_shift_c_combination(key_event: &KEY_EVENT_RECORD) -> bool {
    return (key_event.dwControlKeyState & LEFT_ALT_PRESSED >= 1
        || key_event.dwControlKeyState & RIGHT_ALT_PRESSED == 1)
        && key_event.dwControlKeyState & SHIFT_PRESSED >= 1
        && key_event.wVirtualKeyCode == VK_C.0;
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

/// Send this process's id over the pipe to the daemon as a 4 byte
/// little-endian sequence.
///
/// The daemon uses the PID to match the pipe connection to the correct
/// [`crate::daemon`] `Client` entry. Without this handshake the daemon will
/// not forward any input records.
///
/// # Arguments
///
/// * `named_pipe_client` - The connected pipe client to write the PID to.
///
/// # Panics
///
/// Panics if the pipe write fails in a way that cannot be retried.
async fn send_pid_handshake(named_pipe_client: &NamedPipeClient) {
    let pid_bytes = serialize_pid(std::process::id());
    let mut written = 0usize;
    while written < SERIALIZED_PID_LENGTH {
        named_pipe_client.writable().await.unwrap_or_else(|err| {
            panic!("Named pipe client is not writable for PID handshake: {err}")
        });
        match named_pipe_client.try_write(&pid_bytes[written..]) {
            Ok(0) => {
                panic!("Named pipe closed before PID handshake could complete");
            }
            Ok(n) => {
                written += n;
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => {
                panic!("Failed to send PID handshake to daemon: {e}");
            }
        }
    }
    return;
}

/// The main run loop of the client.
///
/// Connects to the named pipe opened by the daemon, reads all input records from it
/// and replays them to the console input buffer of the given child process.
/// Handles the `Alt + Shift + C` key combination used to close the console window
/// after the child process encountered an unexpected error.
///
/// # Arguments
///
/// * `api`         - The Windows API implementation to use.
/// * `child`       - Handle to the running SSH process.
/// * `state_sender`    - Watch sender used by [`read_write_loop`] to broadcast the
///                   client's authoritative [`ClientState`] to subscribers
///                   such as the visuals task in [`main`].
/// * `highlight_sender`- Watch sender used by [`read_write_loop`] to broadcast the
///                   client's current highlight flag.
async fn run(
    api: &dyn WindowsApi,
    child: &mut Child,
    state_sender: &watch::Sender<ClientState>,
    highlight_sender: &watch::Sender<bool>,
) {
    // Many clients trying to open the pipe at the same time can cause
    // a file not found error, so keep trying until we managed to open it
    let named_pipe_client: NamedPipeClient = loop {
        match ClientOptions::new().open(PIPE_NAME) {
            Ok(named_pipe_client) => {
                break named_pipe_client;
            }
            Err(_) => {
                continue;
            }
        }
    };
    // Identify ourselves to the daemon's pipe server by sending our PID.
    // The daemon uses this to correlate this pipe connection to the corresponding
    // client in its internal bookkeeping.
    send_pid_handshake(&named_pipe_client).await;
    let mut child_error = false;
    let mut internal_buffer: Vec<u8> = Vec::new();
    loop {
        named_pipe_client
            .ready(Interest::READABLE)
            .await
            .unwrap_or_else(|err| {
                error!("{}", err);
                panic!("Named client pipe is not ready to be read",)
            });

        match read_write_loop(
            api,
            &named_pipe_client,
            &mut internal_buffer,
            state_sender,
            highlight_sender,
        )
        .await
        {
            ReadWriteResult::Success {
                remainder,
                key_event_records,
            } => {
                internal_buffer = remainder;
                if child_error {
                    for key_event in key_event_records.into_iter() {
                        if is_alt_shift_c_combination(&key_event) {
                            return;
                        }
                    }
                }
            }
            ReadWriteResult::WouldBlock | ReadWriteResult::Err => {
                // Sleep some time to avoid hogging 100% CPU usage.
                tokio::time::sleep(Duration::from_nanos(5)).await;
            }
            ReadWriteResult::Disconnect => {
                warn!("Encountered disconnect when trying to read from named pipe");
                break;
            }
        }
        match child.try_wait() {
            Ok(Some(exit_status)) => match exit_status.code().unwrap() {
                0 | 1 | 130 => {
                    // 0 -> last command successful
                    // 1 -> last command unsuccessful
                    // 130 -> last command cancelled (Ctrl + C)
                    info!(
                        "Application terminated, last exit code: {}",
                        exit_status.code().unwrap()
                    );
                    break;
                }
                _ => {
                    if !child_error {
                        println!("Failed to establish SSH connection: {exit_status}");
                        println!("Shift-Alt-C to exit");
                        child_error = true;
                    }
                }
            },
            Ok(None) => (
                // child is still running
            ),
            Err(e) => panic!("{}", e),
        }
    }
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
/// Spawns a tokio background thread to ensure the console window title is not replaced
/// by the name of the child process once its launched.
/// Starts the SSH process as child process.
/// Executes the main run loop.
///
/// # Arguments
///
/// * `api`         - The Windows API implementation to use.
/// * `host`        - The name of the host to connect to, optionally with `:port` suffix.
/// * `username`    - The username to be used.
///                   Will try to resolve the correct username from the ssh config
///                   if none is given.
/// * `cli_port`    - Optional port from CLI option. Inline port takes precedence.
/// * `config`      - A reference to the `ClientConfig`.
pub async fn main(
    api: &dyn WindowsApi,
    host: String,
    username: Option<String>,
    cli_port: Option<u16>,
    config: &ClientConfig,
) {
    // Shield this client from relayed CTRL+C/CTRL+Break so only the SSH child reacts.
    if let Err(err) = api.install_console_ctrl_handler() {
        warn!("Failed to install console control handler: {}", err);
    }

    // Snapshot the palette before the SSH child writes, so a later tint restores
    // the pristine colors and reads the true default attribute rather than one
    // the child may have changed.
    let base_palette = snapshot_console_palette(api);

    let (state_sender, state_receiver) = watch::channel(ClientState::Active);
    let (highlight_sender, highlight_receiver) = watch::channel(false);

    let (host, inline_port) = split_host_and_inline_port(&host);
    let port = inline_port.or(cli_port);

    let resolved_username = resolve_username(username, host, config);
    let console_title = build_console_title(&resolved_username, host, port);

    let title_task = run_title_loop(api, console_title);
    let child_task = async {
        let mut child = launch_ssh_process(&resolved_username, host, port, config).await;
        run(api, &mut child, &state_sender, &highlight_sender).await;
        return child;
    };
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
    let mut child = tokio::select! {
        child = child_task => child,
        _ = title_task => {
            panic!("Title task should never complete");
        }
        _ = visuals_task => {
            panic!("Visuals task should never complete");
        }
    };

    shutdown_child(api, &mut child).await;
}

#[cfg(test)]
#[path = "../tests/client/test_mod.rs"]
mod test_mod;
