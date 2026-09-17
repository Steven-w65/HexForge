use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
struct EditAction {
    offset: u64,
    previous_effective: u8,
}

#[derive(Debug, Default)]
pub struct EditBuffer {
    edits: BTreeMap<u64, u8>,
    history: Vec<EditAction>,
}

impl EditBuffer {
    /// Applies an effective byte value, returning whether the sparse edit state changed.
    pub fn apply(&mut self, offset: u64, source: u8, value: u8) -> bool {
        let previous_effective = self.effective_byte(offset, source);
        if value == previous_effective {
            return false;
        }

        self.history.push(EditAction {
            offset,
            previous_effective,
        });
        if value == source {
            self.edits.remove(&offset);
        } else {
            self.edits.insert(offset, value);
        }
        true
    }

    /// Restores the preceding effective byte for the most recent edit.
    pub fn undo(&mut self, source: u8) -> Option<(u64, u8)> {
        let action = self.history.pop()?;
        if action.previous_effective == source {
            self.edits.remove(&action.offset);
        } else {
            self.edits.insert(action.offset, action.previous_effective);
        }
        Some((action.offset, action.previous_effective))
    }

    pub fn overlay(&self, start: u64, bytes: &mut [u8]) {
        for (&offset, &value) in self.edits.range(start..) {
            let index = offset - start;
            if index >= bytes.len() as u64 {
                break;
            }
            bytes[index as usize] = value;
        }
    }

    pub fn is_dirty(&self) -> bool {
        !self.edits.is_empty()
    }

    pub fn effective_byte(&self, offset: u64, source: u8) -> u8 {
        self.edits.get(&offset).copied().unwrap_or(source)
    }

    pub fn modified_offsets(&self, start: u64, len: u64) -> Vec<u64> {
        self.edits
            .range(start..)
            .take_while(|(&offset, _)| offset - start < len)
            .map(|(&offset, _)| offset)
            .collect()
    }

    /// Clears all sparse edits and undo history, returning whether state changed.
    pub fn clear(&mut self) -> bool {
        let changed = !self.edits.is_empty() || !self.history.is_empty();
        self.edits.clear();
        self.history.clear();
        changed
    }

    pub(crate) fn last_offset(&self) -> Option<u64> {
        self.history.last().map(|action| action.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::EditBuffer;

    #[test]
    fn applying_source_value_removes_dirty_edit() {
        let mut edits = EditBuffer::default();
        edits.apply(7, 0xaa, 0xbb);
        assert!(edits.is_dirty());
        edits.apply(7, 0xaa, 0xaa);
        assert!(!edits.is_dirty());
    }

    #[test]
    fn undo_restores_prior_effective_value_in_lifo_order() {
        let mut edits = EditBuffer::default();
        edits.apply(2, 0x10, 0x20);
        edits.apply(2, 0x10, 0x30);
        assert_eq!(edits.undo(0x10), Some((2, 0x20)));
        assert_eq!(edits.effective_byte(2, 0x10), 0x20);
    }
}
