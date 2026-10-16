//! A displayed pattern and its completed scan are distinct. Editing the pattern
//! disables old navigation/markers without clearing byte or modification state.
use crate::{controller::ByteSelection, dialog::ControllerError};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchDirection {
    Next,
    Previous,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}
#[derive(Debug, Clone)]
pub struct SearchState {
    pub raw: String,
    pub query: String,
    pub matches: Vec<u64>,
    pub match_length: u64,
    pub truncated: bool,
    pub open: bool,
    pub error: Option<String>,
    pub focus_key: u64,
    anchor: Option<ByteSelection>,
    viewport: u64,
}
impl Default for SearchState {
    fn default() -> Self {
        Self {
            raw: String::new(),
            query: String::new(),
            matches: vec![],
            match_length: 1,
            truncated: false,
            open: false,
            error: None,
            focus_key: 0,
            anchor: None,
            viewport: 0,
        }
    }
}
impl SearchState {
    pub fn set_pattern(&mut self, raw: &str) {
        self.raw = raw.to_owned();
    }
    pub fn install(&mut self, query: &str, matches: Vec<u64>, length: u64, truncated: bool) {
        self.query = query.trim().to_owned();
        self.matches = matches;
        self.match_length = length;
        self.truncated = truncated;
    }
    pub fn is_current(&self) -> bool {
        if self.query.is_empty() {
            return false;
        }
        match (
            parse_search_pattern(&self.raw),
            parse_search_pattern(&self.query),
        ) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }
    pub fn clear(&mut self) {
        self.raw.clear();
        self.query.clear();
        self.matches.clear();
        self.match_length = 1;
        self.truncated = false;
        self.error = None;
    }
    /// Content changes reset completed search state; the visible text remains
    /// available for re-scanning, exactly like the non-modal reference bar.
    pub fn invalidate(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.match_length = 1;
        self.truncated = false;
    }
    pub fn warning(&self) -> Option<&'static str> {
        (self.truncated && self.is_current())
            .then_some("Search results were limited; refine the byte pattern.")
    }
    pub fn set_anchor(&mut self, selected: Option<ByteSelection>, viewport: u64) {
        self.anchor = selected;
        self.viewport = viewport;
    }
    pub fn active_index(&self, selected: Option<&ByteSelection>) -> Option<usize> {
        selected
            .filter(|s| s.count() == u128::from(self.match_length))
            .and_then(|s| self.matches.iter().position(|offset| *offset == s.start))
    }
    pub fn next(&mut self, direction: MatchDirection) -> Option<ByteRange> {
        if !self.is_current() || self.matches.is_empty() {
            return None;
        }
        let index = if let Some(index) = self.active_index(self.anchor.as_ref()) {
            match direction {
                MatchDirection::Next => (index + 1) % self.matches.len(),
                MatchDirection::Previous => {
                    if index == 0 {
                        self.matches.len() - 1
                    } else {
                        index - 1
                    }
                }
            }
        } else {
            let anchor = self.anchor.as_ref().map_or(self.viewport, |s| s.start);
            match direction {
                MatchDirection::Next => self
                    .matches
                    .iter()
                    .position(|offset| *offset > anchor)
                    .unwrap_or(0),
                MatchDirection::Previous => self
                    .matches
                    .iter()
                    .rposition(|offset| *offset < anchor)
                    .unwrap_or(self.matches.len() - 1),
            }
        };
        let start = self.matches[index];
        let end = start.checked_add(self.match_length.checked_sub(1)?)?;
        self.anchor = Some(ByteSelection { start, end });
        self.viewport = start;
        Some(ByteRange { start, end })
    }
}
pub fn parse_search_pattern(raw: &str) -> Result<Vec<u8>, ControllerError> {
    let ascii_space = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{000b}' | '\u{000c}');
    let raw = raw.trim_matches(ascii_space);
    if raw.is_empty() {
        return Err(ControllerError::invalid_input(
            "At least one byte is required",
        ));
    }
    raw.split(ascii_space)
        .filter(|token| !token.is_empty())
        .map(|token| {
            if token.len() != 2 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(ControllerError::invalid_input(
                    "Bytes must be two hexadecimal digits separated by ASCII whitespace",
                ));
            }
            u8::from_str_radix(token, 16).map_err(|_| {
                ControllerError::invalid_input(
                    "Bytes must be two hexadecimal digits separated by ASCII whitespace",
                )
            })
        })
        .collect()
}
pub fn parse_edit_byte(raw: &str) -> Result<u8, ControllerError> {
    if raw.len() != 2 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ControllerError::invalid_input(
            "Byte must be exactly two hexadecimal digits",
        ));
    }
    u8::from_str_radix(raw, 16)
        .map_err(|_| ControllerError::invalid_input("Byte must be exactly two hexadecimal digits"))
}
