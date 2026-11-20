# cssh-rs

Cluster SSH: a daemon window distributes keystrokes to many client windows, each running its own SSH session.

## Language

**Keystroke**:
A single press of one key, as the remote shell receives it; a held key yields one keystroke per auto-repeat.
_Avoid_: keypress, key event
