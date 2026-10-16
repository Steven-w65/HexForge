use crate::controller::ByteSelection;
#[derive(Debug, Clone, Copy)]
pub enum SelectionInput {
    Begin(u64),
    Move(u64),
    End,
    Cancel,
    Clear,
}
#[derive(Debug, Clone)]
pub struct SelectionUpdate {
    pub selection: Option<ByteSelection>,
    pub captured: bool,
}
#[derive(Debug, Default)]
pub struct Selection {
    anchor: Option<u64>,
    selection: Option<ByteSelection>,
}
pub fn normalize_selection(anchor: u64, focus: u64) -> ByteSelection {
    ByteSelection {
        start: anchor.min(focus),
        end: anchor.max(focus),
    }
}
pub fn contains_offset(selection: Option<&ByteSelection>, offset: u64) -> bool {
    selection.is_some_and(|s| offset >= s.start && offset <= s.end)
}
impl Selection {
    pub fn captured(&self) -> bool {
        self.anchor.is_some()
    }
    pub fn handle(&mut self, input: SelectionInput, file_size: u64) -> SelectionUpdate {
        match input {
            SelectionInput::Begin(offset) if offset < file_size => {
                self.anchor = Some(offset);
                self.selection = Some(normalize_selection(offset, offset));
            }
            SelectionInput::Move(offset) if offset < file_size => {
                if let Some(anchor) = self.anchor {
                    self.selection = Some(normalize_selection(anchor, offset));
                }
            }
            SelectionInput::End | SelectionInput::Cancel => self.anchor = None,
            SelectionInput::Clear => {
                self.anchor = None;
                self.selection = None;
            }
            _ => {}
        }
        SelectionUpdate {
            selection: self.selection.clone(),
            captured: self.captured(),
        }
    }
}
