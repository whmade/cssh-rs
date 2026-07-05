cssh-rs now works on Windows profiles that have never selected a default
terminal application. Previously the daemon and client windows opened under
Windows Terminal instead of the Windows Console Host on such profiles, so
cssh-rs could not manage them.
