# cssh-rs

Cluster SSH: a daemon window distributes keystrokes to many client windows, each running its own SSH session.

## Language

**Keystroke**:
A single press of one key, as the remote shell receives it; a held key yields one keystroke per auto-repeat.
_Avoid_: keypress, key event

**Composed text**:
The characters a keystroke types on the active keyboard layout, with Shift, AltGr and dead keys already applied.
_Avoid_: composed character, UnicodeChar

**Chord**:
A keystroke that holds Ctrl, Alt or Meta as a command rather than to type a character, such as Ctrl+C or Alt+X. Holding AltGr to type a character is not a chord.
_Avoid_: shortcut, key combination, hotkey
