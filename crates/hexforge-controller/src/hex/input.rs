//! Shared strict decimal/hex parsers: no platform or float coercion.
pub use crate::goto::parse_goto as parse_offset;
pub use crate::search::{
    parse_edit_byte as parse_single_byte, parse_search_pattern as parse_hex_bytes,
};
