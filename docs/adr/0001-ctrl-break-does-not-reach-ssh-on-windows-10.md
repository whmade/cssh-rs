# Ctrl+Break does not reach ssh on Windows 10

A client delivers Ctrl+Break to its `ssh` by writing the win32-input-mode
Ctrl+Break key to its ConPTY, which raises `CTRL_BREAK_EVENT` in every
process on the pseudoconsole. Windows 10's inbox ConPTY predates
win32-input-mode and drops that key, so on Windows 10 Ctrl+Break ends no
`ssh` session, whether typed in the daemon or in a client window. In
0.19.1, where `ssh` shared the client's console, Ctrl+Break in a client
window did end its session, so this is a Windows 10 regression. We accept
it as a known limitation: no issue tracks a fix, and the e2e cases that
need Ctrl+Break to reach `ssh` skip below Windows 11 build 22000.

## Considered Options

Set aside untried, not disproven:

- Ship `conpty.dll` and `OpenConsole.exe` from microsoft/terminal next to
  `cssh-rs.exe`. `portable-pty` loads a sideloaded `conpty.dll` in place
  of the inbox ConPTY, which would bring win32-input-mode to Windows 10.
- Raise the event from a helper process that attaches to the `ssh`
  pseudoconsole and calls `GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, 0)`.
  The client cannot attach itself, because freeing its own console
  destroys its window.
