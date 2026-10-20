//! Names of controller buttons and keyboard keys used in key mappings.

/// Keyboard keys offered for mapping: (HID usage, label).
pub const KEYBOARD: &[(u8, &str)] = &[
    (0x04, "A"), (0x05, "B"), (0x06, "C"), (0x07, "D"), (0x08, "E"), (0x09, "F"),
    (0x0a, "G"), (0x0b, "H"), (0x0c, "I"), (0x0d, "J"), (0x0e, "K"), (0x0f, "L"),
    (0x10, "M"), (0x11, "N"), (0x12, "O"), (0x13, "P"), (0x14, "Q"), (0x15, "R"),
    (0x16, "S"), (0x17, "T"), (0x18, "U"), (0x19, "V"), (0x1a, "W"), (0x1b, "X"),
    (0x1c, "Y"), (0x1d, "Z"),
    (0x1e, "1"), (0x1f, "2"), (0x20, "3"), (0x21, "4"), (0x22, "5"),
    (0x23, "6"), (0x24, "7"), (0x25, "8"), (0x26, "9"), (0x27, "0"),
    (0x28, "Enter"), (0x29, "Esc"), (0x2a, "Backspace"), (0x2b, "Tab"), (0x2c, "Space"),
    (0x2d, "-"), (0x2e, "="), (0x2f, "["), (0x30, "]"), (0x31, "\\"), (0x33, ";"),
    (0x34, "'"), (0x35, "`"), (0x36, ","), (0x37, "."), (0x38, "/"), (0x39, "Caps Lock"),
    (0x3a, "F1"), (0x3b, "F2"), (0x3c, "F3"), (0x3d, "F4"), (0x3e, "F5"), (0x3f, "F6"),
    (0x40, "F7"), (0x41, "F8"), (0x42, "F9"), (0x43, "F10"), (0x44, "F11"), (0x45, "F12"),
    (0x46, "Print Screen"), (0x49, "Insert"), (0x4a, "Home"), (0x4b, "Page Up"),
    (0x4c, "Delete"), (0x4d, "End"), (0x4e, "Page Down"),
    (0x4f, "Right"), (0x50, "Left"), (0x51, "Down"), (0x52, "Up"),
    (0xe0, "L Ctrl"), (0xe1, "L Shift"), (0xe2, "L Alt"), (0xe3, "L Win"),
    (0xe4, "R Ctrl"), (0xe5, "R Shift"), (0xe6, "R Alt"), (0xe7, "R Win"),
];

/// Modifier bits accepted in a keyboard mapping.
pub const MODIFIERS: &[(u8, &str)] = &[
    (0x00, "—"),
    (0x01, "Ctrl"),
    (0x02, "Shift"),
    (0x04, "Alt"),
    (0x10, "R Ctrl"),
    (0x20, "R Shift"),
    (0x40, "R Alt"),
];

pub fn key_name(usage: u8) -> Option<&'static str> {
    KEYBOARD.iter().find(|(u, _)| *u == usage).map(|(_, n)| *n)
}

pub fn modifier_name(bits: u8) -> Option<&'static str> {
    MODIFIERS.iter().find(|(m, _)| *m == bits).map(|(_, n)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usages_are_unique() {
        let mut seen = std::collections::HashSet::new();
        assert!(KEYBOARD.iter().all(|(u, _)| seen.insert(*u)));
        assert_eq!(key_name(0x2c), Some("Space"));
        assert_eq!(modifier_name(0x02), Some("Shift"));
    }
}
