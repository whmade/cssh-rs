# cssh-rs

Cluster SSH: a daemon window distributes keystrokes to many client windows, each running its own SSH session.

## Language

**Keystroke**:
A single press of one key, as the remote shell receives it; a held key yields one keystroke per auto-repeat.
_Avoid_: keypress, key event

**Composed text**:
The characters a keystroke produces with its layout, modifiers, dead keys or IME applied, control characters included; possibly none.
_Avoid_: composed character

**Chord**:
A keystroke with Ctrl, Alt or Meta held to send a command rather than to type a character, such as Ctrl+C or Alt+X. A Ctrl or Alt that only selects a character, as AltGr does, does not make a chord.
_Avoid_: shortcut, key combination, hotkey
