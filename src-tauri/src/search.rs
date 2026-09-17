use crate::edit_buffer::EditBuffer;
use crate::error::{AppError, ErrorCode};
use crate::session::{FileSession, MAX_READ_RANGE};
use std::io::{self, Read, Seek, SeekFrom};

const MAX_PATTERN_LEN: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub matches: Vec<u64>,
    pub truncated: bool,
}

pub fn parse_hex_pattern(input: &str) -> Result<Vec<u8>, AppError> {
    let tokens: Vec<&str> = input.split_ascii_whitespace().collect();
    if tokens.is_empty() || tokens.len() > MAX_PATTERN_LEN {
        return Err(invalid_search(
            "Search patterns must contain between 1 and 4096 bytes.",
        ));
    }

    let mut pattern = Vec::with_capacity(tokens.len());
    for token in tokens {
        if token.len() != 2 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid_search(
                "Each search token must contain exactly two hexadecimal digits.",
            ));
        }
        let byte = u8::from_str_radix(token, 16).map_err(|_| {
            invalid_search("Each search token must contain exactly two hexadecimal digits.")
        })?;
        pattern.push(byte);
    }
    Ok(pattern)
}

pub fn search_session(
    session: &mut FileSession,
    pattern: &[u8],
    chunk_size: usize,
    max_results: usize,
) -> Result<SearchResult, AppError> {
    let size = session.info().size;
    let mut reader = SessionReader {
        session,
        position: 0,
    };
    search_reader(
        &mut reader,
        size,
        pattern,
        chunk_size,
        max_results,
        &EditBuffer::default(),
    )
}

fn search_reader<R: Read + Seek>(
    reader: &mut R,
    size: u64,
    pattern: &[u8],
    chunk_size: usize,
    max_results: usize,
    edits: &EditBuffer,
) -> Result<SearchResult, AppError> {
    validate_search(pattern, chunk_size, max_results)?;

    let mut position = 0u64;
    let mut examined_until = 0u64;
    let mut tail = Vec::new();
    let mut matches = Vec::new();

    while position < size {
        let remaining = size - position;
        let requested = remaining.min(chunk_size as u64) as usize;
        let mut current = vec![0u8; requested];
        reader
            .seek(SeekFrom::Start(position))
            .map_err(AppError::from)?;
        let read = reader.read(&mut current).map_err(AppError::from)?;
        if read == 0 {
            break;
        }
        current.truncate(read);
        edits.overlay(position, &mut current);

        let combined_start = position - tail.len() as u64;
        let mut combined = Vec::with_capacity(tail.len() + current.len());
        combined.extend_from_slice(&tail);
        combined.extend_from_slice(&current);

        if combined.len() >= pattern.len() {
            let window_count = combined.len() - pattern.len() + 1;
            for index in 0..window_count {
                let offset = combined_start + index as u64;
                if offset < examined_until {
                    continue;
                }
                if combined[index..].starts_with(pattern) {
                    if matches.len() == max_results {
                        return Ok(SearchResult {
                            matches,
                            truncated: true,
                        });
                    }
                    matches.push(offset);
                }
            }
            examined_until = examined_until.max(combined_start + window_count as u64);
        }

        let retained = (pattern.len() - 1).min(combined.len());
        tail = combined[combined.len() - retained..].to_vec();
        position += read as u64;
    }

    Ok(SearchResult {
        matches,
        truncated: false,
    })
}

fn validate_search(pattern: &[u8], chunk_size: usize, max_results: usize) -> Result<(), AppError> {
    if pattern.is_empty() || pattern.len() > MAX_PATTERN_LEN {
        return Err(invalid_search(
            "Search patterns must contain between 1 and 4096 bytes.",
        ));
    }
    if chunk_size == 0 || max_results == 0 {
        return Err(invalid_search(
            "Search chunk size and result limit must be greater than zero.",
        ));
    }
    if chunk_size > MAX_READ_RANGE as usize {
        return Err(AppError::new(
            ErrorCode::InvalidLength,
            "The search chunk is too large.",
            None,
        ));
    }
    Ok(())
}

fn invalid_search(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidSearch, message, None)
}

struct SessionReader<'a> {
    session: &'a mut FileSession,
    position: u64,
}

impl Read for SessionReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self
            .session
            .read_effective_chunk(self.position, buffer)
            .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?;
        self.position += read as u64;
        Ok(read)
    }
}

impl Seek for SessionReader<'_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            SeekFrom::End(offset) => self.session.info().size.checked_add_signed(offset),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid seek position"))?;
        self.position = next;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_hex_pattern, search_reader, search_session};
    use crate::edit_buffer::EditBuffer;
    use crate::session::FileSession;
    use std::io::Cursor;

    #[test]
    fn rejects_empty_and_partial_hex_tokens() {
        assert_eq!(parse_hex_pattern(" ").unwrap_err().code(), "invalid_search");
        assert_eq!(
            parse_hex_pattern("4 42").unwrap_err().code(),
            "invalid_search"
        );
    }

    #[test]
    fn parses_uppercase_and_lowercase_hex_tokens() {
        assert_eq!(parse_hex_pattern("00 Af FF").unwrap(), vec![0, 0xaf, 0xff]);
    }

    #[test]
    fn rejects_tokens_that_are_not_exactly_two_ascii_hex_digits() {
        for pattern in ["A", "AAA", "GG", "é1"] {
            assert_eq!(
                parse_hex_pattern(pattern).unwrap_err().code(),
                "invalid_search"
            );
        }
    }

    #[test]
    fn caps_patterns_at_4096_bytes() {
        let valid = std::iter::repeat("AA")
            .take(4096)
            .collect::<Vec<_>>()
            .join(" ");
        let too_large = std::iter::repeat("AA")
            .take(4097)
            .collect::<Vec<_>>()
            .join(" ");

        assert_eq!(parse_hex_pattern(&valid).unwrap().len(), 4096);
        assert_eq!(
            parse_hex_pattern(&too_large).unwrap_err().code(),
            "invalid_search"
        );
    }

    #[test]
    fn finds_match_crossing_chunk_boundary() {
        let mut source = Cursor::new(b"xxxxABCxxxx".to_vec());
        let offsets =
            search_reader(&mut source, 11, b"ABC", 5, 100, &EditBuffer::default()).unwrap();
        assert_eq!(offsets.matches, vec![4]);
    }

    #[test]
    fn search_uses_sparse_edits() {
        let mut edits = EditBuffer::default();
        edits.apply(1, b'X', b'B');
        let mut source = Cursor::new(b"AXC".to_vec());
        let result = search_reader(&mut source, 3, b"ABC", 2, 10, &edits).unwrap();
        assert_eq!(result.matches, vec![0]);
    }

    #[test]
    fn rejects_zero_chunk_size_and_result_limit() {
        let mut source = Cursor::new(b"ABC".to_vec());
        let edits = EditBuffer::default();
        assert_eq!(
            search_reader(&mut source, 3, b"A", 0, 10, &edits)
                .unwrap_err()
                .code(),
            "invalid_search"
        );
        assert_eq!(
            search_reader(&mut source, 3, b"A", 1, 0, &edits)
                .unwrap_err()
                .code(),
            "invalid_search"
        );
    }

    #[test]
    fn finds_overlapping_matches() {
        let mut source = Cursor::new(b"AAAA".to_vec());
        let result = search_reader(&mut source, 4, b"AAA", 2, 10, &EditBuffer::default()).unwrap();
        assert_eq!(result.matches, vec![0, 1]);
        assert!(!result.truncated);
    }

    #[test]
    fn finds_pattern_longer_than_chunk() {
        let mut source = Cursor::new(b"xxABCDEyy".to_vec());
        let result =
            search_reader(&mut source, 9, b"ABCDE", 2, 10, &EditBuffer::default()).unwrap();
        assert_eq!(result.matches, vec![2]);
    }

    #[test]
    fn returns_no_match_when_eof_is_shorter_than_pattern() {
        let mut source = Cursor::new(b"AB".to_vec());
        let result = search_reader(&mut source, 2, b"ABC", 2, 10, &EditBuffer::default()).unwrap();
        assert!(result.matches.is_empty());
        assert!(!result.truncated);
    }

    #[test]
    fn truncates_when_an_additional_match_would_exceed_the_cap() {
        let mut source = Cursor::new(b"ABABAB".to_vec());
        let result = search_reader(&mut source, 6, b"AB", 2, 2, &EditBuffer::default()).unwrap();
        assert_eq!(result.matches, vec![0, 2]);
        assert!(result.truncated);
    }

    #[test]
    fn does_not_mark_truncated_when_matches_end_at_the_cap() {
        let mut source = Cursor::new(b"ABABAB".to_vec());
        let result = search_reader(&mut source, 6, b"AB", 2, 3, &EditBuffer::default()).unwrap();
        assert_eq!(result.matches, vec![0, 2, 4]);
        assert!(!result.truncated);
    }

    #[test]
    fn searches_effective_session_bytes() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"AXC").unwrap();
        let mut session = FileSession::open(file.path().to_path_buf(), 2, 2).unwrap();
        session.edit_byte(1, b'B').unwrap();

        let result = search_session(&mut session, b"ABC", 2, 10).unwrap();

        assert_eq!(result.matches, vec![0]);
    }
}
