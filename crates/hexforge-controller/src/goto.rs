//! Anchored decimal/0x input, validated before any global coordinate becomes f64.
use crate::dialog::ControllerError;
pub type GotoError = ControllerError;
#[derive(Debug, Clone, Default)]
pub struct GotoState {
    pub open: bool,
    pub raw: String,
    pub error: Option<String>,
    pub focus_key: u64,
}
pub fn parse_goto(raw: &str, file_size: u64) -> Result<u64, GotoError> {
    let (digits, radix) = if raw.starts_with("0x") || raw.starts_with("0X") {
        (&raw[2..], 16)
    } else {
        (raw, 10)
    };
    let valid = !digits.is_empty()
        && digits.bytes().all(|b| {
            if radix == 16 {
                b.is_ascii_hexdigit()
            } else {
                b.is_ascii_digit()
            }
        });
    if !valid {
        return Err(ControllerError::invalid_input(
            "Offset must be a decimal or 0x-prefixed hexadecimal integer",
        ));
    }
    let value = u64::from_str_radix(digits, radix)
        .ok()
        .filter(|value| *value < file_size);
    value.ok_or_else(|| ControllerError::invalid_input("Offset is outside the file"))
}
