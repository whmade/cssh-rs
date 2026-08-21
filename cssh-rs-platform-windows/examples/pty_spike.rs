//! M1 gate spike (GitHub #24): does `portable-pty` (ConPTY) preserve csshw's
//! load-bearing behavior - input broadcast from the daemon and input typed
//! directly at the client merge into ONE editable line for the child?
//!
//! This is a THROWAWAY, Windows-only, opt-in harness
//! (`cargo run -p cssh-rs-platform-windows --example pty_spike`). It never runs
//! under `cargo test`, so it does not violate the repo's side-effect-free test
//! convention. The default run is the automated cross-source edit test; `--demo`
//! launches an interactive two-window version that reuses cssh-rs's own spawner
//! (`create_process_with_args`) and keyboard capture (`read_keyboard_input`).
//!
//! The per-category byte-delivery results that justified the gate are recorded
//! in `docs/spikes/0024-conpty-input-delivery.md`.

// The example is Windows-only; keep a trivial entry point for other targets so
// `cargo build --all-targets` off Windows does not fail on a missing `main`.
#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(windows)]
mod win {
    #![allow(clippy::needless_return)]

    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use cssh_rs_platform_windows::{read_keyboard_input, DefaultWindowsApi, WindowsApi};
    use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
    use windows::Win32::System::Console::{
        ENABLE_PROCESSED_INPUT, ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
        STD_INPUT_HANDLE,
    };

    /// TCP port the interactive `--daemon`/`--client` demo uses on loopback.
    const DEMO_PORT: u16 = 5599;
    /// Ctrl+] - the quit key for the interactive demo windows.
    const QUIT_BYTE: u8 = 0x1d;

    /// A PTY master writer shared between the daemon source, the local source,
    /// and the DSR-reply thread - all input converges on this one handle.
    type SharedWriter = Arc<Mutex<Box<dyn std::io::Write + Send>>>;

    pub fn run() {
        let args: Vec<String> = std::env::args().collect();
        let port_after = |flag: &str| -> Option<u16> {
            let pos = args.iter().position(|a| a == flag)?;
            return Some(
                args.get(pos + 1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(DEMO_PORT),
            );
        };
        if let Some(port) = port_after("--demo") {
            demo_mode(port);
            return;
        }
        if let Some(port) = port_after("--daemon") {
            daemon_mode(port);
            return;
        }
        if let Some(port) = port_after("--client") {
            client_mode(port);
            return;
        }

        println!("cssh-rs M1 ConPTY spike (GitHub #24): cross-source edit test");
        let pass = cross_source_edit_case();
        println!(
            "Cross-source edit (daemon broadcast + local typing merge into one line): {}",
            if pass { "PASS" } else { "FAIL" }
        );
        std::process::exit(if pass { 0 } else { 1 });
    }

    fn flush() {
        let _ = std::io::stdout().flush();
    }

    fn send(writer: &SharedWriter, bytes: &[u8]) {
        if let Ok(mut w) = writer.lock() {
            let _ = w.write_all(bytes);
            let _ = w.flush();
        }
    }

    fn open_pty(rows: u16, cols: u16) -> PtyPair {
        return native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");
    }

    /// Drain the PTY master in a thread, answering conhost's startup DSR query
    /// (`ESC [ 6 n` -> `ESC [ 1 ; 1 R`) so the child leaves console init, and
    /// optionally capturing bytes and/or rendering them (with the DSR query
    /// stripped, so this window does not auto-reply it back into our input).
    /// https://learn.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences
    fn spawn_output_reader(
        mut reader: Box<dyn Read + Send>,
        writer: SharedWriter,
        capture: Option<Arc<Mutex<Vec<u8>>>>,
        render: bool,
    ) {
        thread::spawn(move || {
            let dsr: &[u8] = b"\x1b[6n";
            let mut sink = [0u8; 8192];
            loop {
                match reader.read(&mut sink) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let chunk = &sink[..n];
                        let has_dsr = chunk.windows(dsr.len()).any(|w| w == dsr);
                        if has_dsr {
                            send(&writer, b"\x1b[1;1R");
                        }
                        if let Some(buf) = &capture {
                            buf.lock().expect("capture").extend_from_slice(chunk);
                        }
                        if render {
                            let out = if has_dsr {
                                strip_bytes(chunk, dsr)
                            } else {
                                chunk.to_vec()
                            };
                            let mut so = std::io::stdout();
                            let _ = so.write_all(&out);
                            let _ = so.flush();
                        }
                    }
                }
            }
        });
    }

    /// Prove the load-bearing behavior automatically: daemon-broadcast input and
    /// input typed directly at the client must merge into ONE editable line for
    /// the child. A real `cmd.exe` does the editing; a "daemon" source types
    /// `echo EDIT_FAILED` (no Enter), then a "local" source backspaces over
    /// `FAILED`, types `OK`, and presses Enter. If both sources share one input
    /// stream, `cmd` runs `echo EDIT_OK` - returning true.
    fn cross_source_edit_case() -> bool {
        let pair = open_pty(50, 200);
        let mut cmd = CommandBuilder::new("cmd.exe");
        cmd.arg("/q");
        inherit_env(&mut cmd);
        let child = pair.slave.spawn_command(cmd).expect("spawn cmd");
        drop(pair.slave);

        let writer: SharedWriter = Arc::new(Mutex::new(pair.master.take_writer().expect("writer")));
        let output = Arc::new(Mutex::new(Vec::<u8>::new()));
        let reader = pair.master.try_clone_reader().expect("reader");
        spawn_output_reader(
            reader,
            Arc::clone(&writer),
            Some(Arc::clone(&output)),
            false,
        );

        thread::sleep(Duration::from_millis(900)); // let cmd print its first prompt
        send(&writer, b"echo EDIT_FAILED"); // Source A: daemon broadcast, no Enter
        thread::sleep(Duration::from_millis(300));
        send(&writer, &[0x08, 0x08, 0x08, 0x08, 0x08, 0x08]); // Source B: local edit
        thread::sleep(Duration::from_millis(150));
        send(&writer, b"OK\r");
        thread::sleep(Duration::from_millis(700));

        let out = output.lock().expect("output").clone();
        let text = String::from_utf8_lossy(&out);
        let pass = text.contains("EDIT_OK") && !text.contains("EDIT_FAILEDOK");

        let mut killer = child.clone_killer();
        let _ = killer.kill();
        drop(pair.master);
        return pass;
    }

    // portable-pty's CommandBuilder starts from an empty environment, but a
    // Windows process needs at least SystemRoot and PATH to initialize.
    fn inherit_env(cmd: &mut CommandBuilder) {
        for (key, value) in std::env::vars() {
            cmd.env(key, value);
        }
    }

    /// Clear `ENABLE_PROCESSED_INPUT` on this window's console input so Ctrl+C is
    /// delivered as a `0x03` byte we can forward to cmd, instead of being turned
    /// into a signal. Mirrors cssh-rs's `toggle_processed_input_mode`, which is
    /// private to the daemon crate that depends on this one, so it cannot be
    /// reused across the crate boundary.
    fn enable_ctrl_c_forwarding() {
        let api = DefaultWindowsApi;
        if let Ok(stdin) = api.get_std_handle(STD_INPUT_HANDLE) {
            if let Ok(mode) = api.get_console_mode(stdin) {
                let _ = api.set_console_mode(stdin, mode & !ENABLE_PROCESSED_INPUT);
            }
        }
    }

    fn strip_bytes(input: &[u8], pat: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(input.len());
        let mut i = 0;
        while i < input.len() {
            if input[i..].starts_with(pat) {
                i += pat.len();
            } else {
                out.push(input[i]);
                i += 1;
            }
        }
        return out;
    }

    /// Read local keystrokes and hand each composed character to `on_bytes` as
    /// UTF-8, returning true when it (or Ctrl+]) asks to quit. On a real console
    /// this reuses cssh-rs's `read_keyboard_input` (naturally raw, non-character
    /// events ignored); off a TTY it falls back to piped stdin.
    fn forward_console_input<F: FnMut(&[u8]) -> bool>(mut on_bytes: F) -> bool {
        let api = DefaultWindowsApi;
        let on_console = api
            .get_std_handle(STD_INPUT_HANDLE)
            .map(|h| api.get_console_mode(h).is_ok())
            .unwrap_or(false);
        if on_console {
            loop {
                let key = unsafe { read_keyboard_input(&api).KeyEvent };
                if !key.bKeyDown.as_bool() {
                    continue;
                }
                let ch = unsafe { key.uChar.UnicodeChar };
                let Some(ch) = char::from_u32(u32::from(ch)).filter(|c| *c != '\0') else {
                    continue;
                };
                let mut buf = [0u8; 4];
                let text = ch.encode_utf8(&mut buf);
                if text.as_bytes().contains(&QUIT_BYTE) {
                    return true;
                }
                if !on_bytes(text.as_bytes()) {
                    return true;
                }
            }
        }
        let mut input = std::io::stdin().lock();
        let mut buf = [0u8; 4096];
        loop {
            match input.read(&mut buf) {
                Ok(0) | Err(_) => return false,
                Ok(n) => {
                    if !on_bytes(&buf[..n]) {
                        return true;
                    }
                }
            }
        }
    }

    fn connect_retry(port: u16) -> std::net::TcpStream {
        for _ in 0..50 {
            if let Ok(stream) = std::net::TcpStream::connect(("127.0.0.1", port)) {
                return stream;
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("could not connect to daemon on 127.0.0.1:{port}");
    }

    /// Interactive demo, one command: spawn BOTH a daemon window and a client
    /// window in their own new consoles. Type in the daemon window to broadcast;
    /// the client window runs cmd.exe in a ConPTY. Ctrl+] in either window quits.
    fn demo_mode(port: u16) {
        // Reuse cssh-rs's own spawner (create_process_with_args -> CreateProcessW
        // + CREATE_NEW_CONSOLE, no inherited handles) so each side attaches to its
        // OWN console window. Spawning with std's Command inherits this launcher's
        // std handles, so the child would read/write this console instead of its
        // window - "typing in the daemon did nothing".
        let api = DefaultWindowsApi;
        let exe = std::env::current_exe().expect("current_exe");
        let exe = exe.to_string_lossy().into_owned();
        let port = port.to_string();
        api.create_process_with_args(&exe, vec!["--daemon".to_string(), port.clone()], true);
        thread::sleep(Duration::from_millis(400));
        api.create_process_with_args(&exe, vec!["--client".to_string(), port], false);
        println!("[demo] launched a daemon window and a client window.");
        println!("[demo] type in the daemon window; Ctrl+] in either window quits.");
        flush();
    }

    /// Interactive demo, daemon side: capture this window's keystrokes and
    /// broadcast them to the connected client over loopback TCP.
    fn daemon_mode(port: u16) {
        // Swallow Ctrl+C / Ctrl+Break so they do not close this window (reuses
        // cssh-rs's handler); Ctrl+] quits. Then make Ctrl+C readable so it is
        // broadcast to the client (and on to cmd) as a 0x03 byte.
        let _ = DefaultWindowsApi.install_console_ctrl_handler();
        enable_ctrl_c_forwarding();
        println!("[daemon] listening on 127.0.0.1:{port}; start the client now...");
        flush();
        let listener = std::net::TcpListener::bind(("127.0.0.1", port)).expect("bind daemon port");
        let (mut stream, _) = listener.accept().expect("accept client");
        println!("[daemon] client connected. Type to broadcast; Ctrl+] quits.");
        flush();

        forward_console_input(move |bytes| {
            if bytes.contains(&QUIT_BYTE) {
                return false;
            }
            if stream.write_all(bytes).is_err() {
                return false;
            }
            let _ = stream.flush();
            return true;
        });
        println!("\n[daemon] bye.");
    }

    /// Interactive demo, client side: run cmd.exe in a ConPTY and feed it BOTH the
    /// daemon broadcast (from the socket) and this window's own keystrokes. Both
    /// sources converge on the one PTY master, so a line typed via the daemon can
    /// be focused here and edited directly - the M1 behavior we must preserve.
    fn client_mode(port: u16) {
        let stream = connect_retry(port);
        let api = DefaultWindowsApi;
        // Swallow Ctrl+C / Ctrl+Break so they do not close this window, and make
        // Ctrl+C readable so we forward it to cmd (interrupt / cancel the line).
        let _ = api.install_console_ctrl_handler();
        enable_ctrl_c_forwarding();
        if let Ok(stdout) = api.get_stdout_handle() {
            if let Ok(mode) = api.get_console_mode(stdout) {
                let _ = api.set_console_mode(
                    stdout,
                    mode | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING,
                );
            }
        }

        let pair = open_pty(30, 120);
        let mut cmd = CommandBuilder::new("cmd.exe");
        inherit_env(&mut cmd);
        let mut child = pair.slave.spawn_command(cmd).expect("spawn cmd");
        drop(pair.slave);
        let writer: SharedWriter = Arc::new(Mutex::new(pair.master.take_writer().expect("writer")));

        // Render cmd's output and answer conhost's DSR query on the master.
        let reader = pair.master.try_clone_reader().expect("reader");
        spawn_output_reader(reader, Arc::clone(&writer), None, true);

        // Daemon source: socket bytes -> PTY master.
        {
            let writer = Arc::clone(&writer);
            let mut sock = stream.try_clone().expect("clone stream");
            thread::spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match sock.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => send(&writer, &buf[..n]),
                    }
                }
            });
        }

        // Local source: this window's keystrokes -> PTY master, in a thread so
        // that stdin ending does not tear down the session; only Ctrl+] (or cmd
        // exiting, handled by the output thread) ends the demo.
        {
            let writer = Arc::clone(&writer);
            let killer = child.clone_killer();
            thread::spawn(move || {
                let quit = forward_console_input(move |bytes| {
                    if bytes.contains(&QUIT_BYTE) {
                        return false;
                    }
                    send(&writer, bytes);
                    return true;
                });
                if quit {
                    let mut killer = killer;
                    let _ = killer.kill();
                    std::process::exit(0);
                }
            });
        }

        // Keep the session alive until cmd exits (e.g. the user types `exit`);
        // the local-source thread exits the process early on Ctrl+].
        loop {
            if let Ok(Some(_)) = child.try_wait() {
                break;
            }
            thread::sleep(Duration::from_millis(150));
        }
        drop(pair.master);
    }
}
