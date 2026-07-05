cssh-rs now forces `conhost.exe` as the default terminal application even
on Windows profiles that never chose one. Previously, when the
`Console\%%Startup` registry key did not exist, cssh-rs silently left the
system default in place, so on a fresh profile the daemon and client
consoles were hosted by Windows Terminal instead of conhost. The guard now
creates the missing key and values, and fully reverts on exit - restoring
values it overwrote, deleting values it created, and deleting a key it had
to create.
