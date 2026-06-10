Client console windows now always close when the daemon window is
closed. Previously a child process that handles `Ctrl + C` itself
(e.g. `cmd.exe`) survived the client's shutdown signal and kept the
client window open indefinitely; the client now force-kills the
child if it has not exited shortly after the `Ctrl + C` event.
