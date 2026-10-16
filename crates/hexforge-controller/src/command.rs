use serde::{Deserialize, Serialize};

/// Exact reference command identities; menu, shortcut and action dispatch share them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Command {
    Open,
    CloseFile,
    SaveAs,
    Export,
    Exit,
    EditSelected,
    ToggleEdit,
    Undo,
    Goto,
    Search,
    SearchNext,
    SearchPrevious,
    TemplateEditor,
    ApplyTemplate,
    LoadTemplate,
    UnloadTemplate,
    SaveTemplate,
    SaveTemplateAs,
    ThemeToggle,
    #[serde(rename = "row-16")]
    Row16,
    #[serde(rename = "row-32")]
    Row32,
    ToggleRightPanel,
    MinimapToggle,
    MinimapFit,
    MinimapProportional,
    MinimapCharacters,
    MinimapBlocks,
    #[serde(rename = "minimap-scale-1")]
    MinimapScale1,
    #[serde(rename = "minimap-scale-2")]
    MinimapScale2,
    #[serde(rename = "minimap-scale-3")]
    MinimapScale3,
}

impl Command {
    pub const ALL: [Self; 30] = [
        Self::Open,
        Self::CloseFile,
        Self::SaveAs,
        Self::Export,
        Self::Exit,
        Self::EditSelected,
        Self::ToggleEdit,
        Self::Undo,
        Self::Goto,
        Self::Search,
        Self::SearchNext,
        Self::SearchPrevious,
        Self::TemplateEditor,
        Self::ApplyTemplate,
        Self::LoadTemplate,
        Self::UnloadTemplate,
        Self::SaveTemplate,
        Self::SaveTemplateAs,
        Self::ThemeToggle,
        Self::Row16,
        Self::Row32,
        Self::ToggleRightPanel,
        Self::MinimapToggle,
        Self::MinimapFit,
        Self::MinimapProportional,
        Self::MinimapCharacters,
        Self::MinimapBlocks,
        Self::MinimapScale1,
        Self::MinimapScale2,
        Self::MinimapScale3,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open File…",
            Self::CloseFile => "Close File",
            Self::SaveAs => "Save As…",
            Self::Export => "Export Parsed Results as CSV…",
            Self::Exit => "Exit",
            Self::EditSelected => "Edit Selected Byte…",
            Self::ToggleEdit => "Edit Mode",
            Self::Undo => "Undo Byte Edit",
            Self::Goto => "Go To Offset…",
            Self::Search => "Search Bytes…",
            Self::SearchNext => "Next Search Match",
            Self::SearchPrevious => "Previous Search Match",
            Self::TemplateEditor => "Template Editor…",
            Self::ApplyTemplate => "Apply Template",
            Self::LoadTemplate => "Load Template…",
            Self::UnloadTemplate => "Unload Template",
            Self::SaveTemplate => "Save Template",
            Self::SaveTemplateAs => "Save Template As…",
            Self::ThemeToggle => "Toggle Theme",
            Self::Row16 => "16 Bytes",
            Self::Row32 => "32 Bytes",
            Self::ToggleRightPanel => "Parsed Results",
            Self::MinimapToggle => "Show Minimap",
            Self::MinimapFit => "Fit File",
            Self::MinimapProportional => "Proportional",
            Self::MinimapCharacters => "Characters",
            Self::MinimapBlocks => "Color Blocks",
            Self::MinimapScale1 => "Scale 1×",
            Self::MinimapScale2 => "Scale 2×",
            Self::MinimapScale3 => "Scale 3×",
        }
    }
}
