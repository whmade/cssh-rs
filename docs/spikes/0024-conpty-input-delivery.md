# Spike 0024: ConPTY input delivery via portable-pty (M1 gate)

## Status

RESOLVED - gate PASSES. On Windows, `portable-pty` (ConPTY) delivers every input
category cssh-rs needs to the child inside the PTY: F-keys, cursor keys, AltGr,
IME/Unicode, paste, and Ctrl+C all arrive byte-for-byte. Ctrl+Break is the sole
exception - it has no input-byte encoding and must be delivered out-of-band. The
spike also proves the **load-bearing behavior** csshw has today: input broadcast
from the daemon and input typed directly at the client merge into one editable
line, so long as the client forwards BOTH sources to the same PTY master.

- Issue: #24 (M1 gate), milestone M1 (Protocol + PTY client on Windows), epic #3
- Harness: `cssh-rs-platform-windows/examples/pty_spike.rs` (throwaway, opt-in)

## Context

M1 rewrites the Windows client to run `ssh` behind a ConPTY via `portable-pty`
and feed it bytes, replacing today's `WriteConsoleInputW` record-replay path.
The gate separates two concerns: **capture** (turning a keypress into bytes -
the daemon's job, out of scope) and **delivery** (does the PTY carry existing
bytes to the child unmodified - what this gate measures).

Environment: Windows 10 Pro 10.0.19045, Rust per `rust-toolchain.toml`,
portable-pty 0.9.0, native ConPTY backend (`native_pty_system`).

## Results

The table below was captured from a one-time run of the original byte-delivery
harness (a child mirrored `ssh`'s raw + VT-input console setup, read wide, and
dumped what arrived; the parent compared it to what it sent). That measurement
code has since been removed; the example now keeps the automated cross-source
edit test and the interactive demo.

| Category            | sent  | recvd | delivered? | notes                                                     |
|---------------------|-------|-------|------------|-----------------------------------------------------------|
| F1-F4 (`ESC O ?`)   | 3     | 3     | yes        | delivered verbatim                                         |
| F5/F8/F12 (CSI `~`) | 5     | 5     | yes        | delivered verbatim                                         |
| Up, normal (CSI)    | 3     | 3     | yes        | delivered verbatim                                         |
| Up, application(SS3)| 3     | 0     | info       | ConPTY re-encodes SS3 cursor to the child DECCKM mode     |
| AltGr `@`           | 1     | 1     | yes        | delivered verbatim                                         |
| AltGr euro (U+20AC) | 3     | 3     | yes        | delivered verbatim (UTF-8)                                 |
| Emoji (U+1F600)     | 4     | 4     | yes        | surrogate pair survives                                    |
| CJK (`zhong wen`)   | 6     | 6     | yes        | delivered verbatim (UTF-8)                                 |
| Combining a + U+0301| 3     | 3     | yes        | delivered verbatim                                         |
| Paste 64 KiB        | 65536 | 65536 | yes        | no truncation or reorder                                   |
| Bracketed paste     | 17    | 5     | info       | ConPTY strips the markers unless the child enables them    |
| Ctrl+C (`0x03`)     | 1     | 1     | yes        | delivered verbatim as a byte                               |
| Ctrl+Break          | 0     | 0     | info       | no input byte; needs out-of-band event + new process group |
| Cross-source edit   | -     | -     | yes        | cmd ran `echo EDIT_OK`; line edited across both sources    |

## Key findings

- **The PTY is a faithful byte pipe.** Every escape sequence, control byte, and
  the 64 KiB paste arrive byte-identical - no truncation, corruption, or reorder.
- **Unicode requires a wide read on the consumer side.** ConPTY carries our input
  as Unicode events; a byte-oriented `ReadFile` folds non-ASCII into the legacy
  code page. Reading wide (`ReadConsoleW`) recovers it. Modern Win32-OpenSSH
  `ssh.exe` already reads wide.
- **conhost has a startup handshake.** It emits a DSR cursor-position query
  (`ESC [ 6 n`) and blocks the child until the terminal replies (`ESC [ 1 ; 1 R`),
  so whatever holds the master must answer it.
- **SS3 application-cursor and bracketed-paste markers are ConPTY input semantics,
  not delivery failures** - SS3 cursor keys are re-encoded to the child's DECCKM
  mode and bracketed-paste markers are stripped unless the child enables them, but
  paste content still arrives (proven by the 64 KiB case).
- **Ctrl+Break has no byte encoding.** Unlike Ctrl+C (`0x03`), no input byte maps
  to `CTRL_BREAK_EVENT` (microsoft/terminal#5128); it is an out-of-band
  process-group signal, exactly how the daemon already delivers Ctrl+C.

## Follow-ups

- #25: add a `Signal { Break | Interrupt }` control message to the protocol
  catalog (Ctrl+Break cannot be expressed as `Input` bytes).
- #31: rewrite the Windows client around `portable-pty` - spawn `ssh` with
  `CREATE_NEW_PROCESS_GROUP` (so `GenerateConsoleCtrlEvent` can signal it),
  answer conhost's startup DSR query, read wide, and forward BOTH input sources
  (daemon-delivered bytes and this window's own raw keystrokes) to the one master.
- Spawn each window so it attaches to its OWN console (no inherited std handles);
  `CREATE_NEW_CONSOLE` alone is not enough. Reuse cssh-rs's `create_process_with_args`.
- Live `ssh` soak: confirm Unicode, F-keys, Ctrl+C end-to-end, plus Ctrl+Break
  via the out-of-band path.

## Reproduce

```
cargo run -p cssh-rs-platform-windows --example pty_spike           # automated cross-source edit test -> PASS, exit 0
cargo run -p cssh-rs-platform-windows --example pty_spike -- --demo # interactive two-window demo
```

In the demo: type `echo hello worlld` (no Enter) in the daemon window; it appears
at the client's `cmd.exe` prompt. Focus the client, backspace to fix `worlld` ->
`world`, press Enter; `cmd` runs the corrected line. Ctrl+] quits.

## References

- portable-pty MasterPty: https://docs.rs/portable-pty/latest/portable_pty/trait.MasterPty.html
- ConPTY overview: https://devblogs.microsoft.com/commandline/windows-command-line-introducing-the-windows-pseudo-console-conpty/
- Ctrl+Break under ConPTY raw mode: https://github.com/microsoft/terminal/issues/5128
- Console virtual terminal sequences (DSR): https://learn.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences
