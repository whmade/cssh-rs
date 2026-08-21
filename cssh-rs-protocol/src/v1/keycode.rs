//! Key codes and modifier flags for a decoded keypress.

use std::ops::BitOr;

use serde_derive::{Deserialize, Serialize};

/// Logical key pressed, mirroring the shape of `crossterm::event::KeyCode`.
///
/// Adjacently tagged so a key kind added in a later protocol minor version
/// decodes to [`KeyCode::Unknown`] instead of failing the whole frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "k", content = "v", rename_all = "snake_case")]
pub enum KeyCode {
    /// The composed character(s) produced by a single keypress.
    Char(String),
    /// The Enter / Return key.
    Enter,
    /// The Escape key.
    Escape,
    /// The Tab key.
    Tab,
    /// The Backspace key.
    Backspace,
    /// The Delete key.
    Delete,
    /// The Insert key.
    Insert,
    /// The Up arrow key.
    Up,
    /// The Down arrow key.
    Down,
    /// The Left arrow key.
    Left,
    /// The Right arrow key.
    Right,
    /// The Home key.
    Home,
    /// The End key.
    End,
    /// The Page Up key.
    PageUp,
    /// The Page Down key.
    PageDown,
    /// A function key; the payload is the `n` in `Fn`.
    F(u8),
    /// A modifier key pressed on its own, distinguishing left from right.
    Modifier(ModifierKeyCode),
    /// A key not defined in this protocol minor version.
    #[serde(other)]
    Unknown,
}

/// A modifier key pressed on its own, distinguishing the left and right sides.
///
/// Mirrors `crossterm::event::ModifierKeyCode`. `Modifiers` reports which
/// modifiers were held during another keypress; this reports the modifier key
/// itself being pressed or released.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierKeyCode {
    /// Left Shift.
    LeftShift,
    /// Right Shift.
    RightShift,
    /// Left Control.
    LeftControl,
    /// Right Control.
    RightControl,
    /// Left Alt.
    LeftAlt,
    /// Right Alt (AltGr).
    RightAlt,
    /// Left Meta / Super / Windows.
    LeftMeta,
    /// Right Meta / Super / Windows.
    RightMeta,
    /// A modifier key not defined in this protocol minor version.
    #[serde(other)]
    Unknown,
}

/// Bitflags of the keyboard modifiers held during a keypress.
///
/// Encodes on the wire as a single CBOR unsigned integer (the raw bits), so
/// modifier bits added in a later minor version round-trip unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Modifiers(
    /// The raw modifier bits; test membership with the [`Modifiers::SHIFT`],
    /// [`Modifiers::CTRL`], [`Modifiers::ALT`], and [`Modifiers::META`]
    /// constants.
    pub u8,
);

impl Modifiers {
    /// No modifiers held.
    pub const NONE: Modifiers = Modifiers(0);
    /// The Shift modifier bit.
    pub const SHIFT: Modifiers = Modifiers(1);
    /// The Control modifier bit.
    pub const CTRL: Modifiers = Modifiers(2);
    /// The Alt modifier bit.
    pub const ALT: Modifiers = Modifiers(4);
    /// The Meta / Super / Windows modifier bit.
    pub const META: Modifiers = Modifiers(8);

    /// Return true if every bit set in `other` is also set in `self`.
    ///
    /// # Arguments
    /// * `other` - Modifier bits to test for.
    ///
    /// # Returns
    /// `true` when `self` contains all bits of `other`.
    pub const fn contains(self, other: Modifiers) -> bool {
        return self.0 & other.0 == other.0;
    }
}

impl BitOr for Modifiers {
    type Output = Modifiers;

    fn bitor(self, rhs: Modifiers) -> Modifiers {
        return Modifiers(self.0 | rhs.0);
    }
}

#[cfg(test)]
#[path = "../tests/v1/test_keycode.rs"]
mod tests;
