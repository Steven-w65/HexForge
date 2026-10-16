use eframe::egui::Color32;
use hexforge_controller::controller::Theme;
#[derive(Debug, Clone)]
pub struct Palette {
    pub theme: Theme,
    pub bg: Color32,
    pub text: Color32,
    pub address: Color32,
    pub selection: Color32,
    pub selection_text: Color32,
    pub modified: Color32,
    pub muted: Color32,
    pub surface: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub input: Color32,
    pub button: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    // Actual Windows thin-scrollbar hover/pressed colors with source tokens.
    pub scrollbar_hover: Color32,
    // HexCanvas.vue owns distinct light-theme canvas colors. Do not substitute
    // the application's general background/text tokens for the byte grid.
    pub hex_background: Color32,
    pub hex_text: Color32,
    pub hex_divider: Color32,
}
fn color(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}
impl Palette {
    pub fn new(theme: Theme) -> Self {
        let a = if theme == Theme::Dark {
            [
                0x111418, 0xc9d1d9, 0x8b949e, 0x1f6feb, 0xffffff, 0xf0883e, 0x8b949e, 0x171b21,
                0x15191e, 0x191e24, 0x111418, 0x222831, 0x252c35, 0x2a313a, 0x3b4551,
            ]
        } else {
            [
                0xf6f8fa, 0x24292f, 0x57606a, 0x0969da, 0xffffff, 0xbc4c00, 0x57606a, 0xffffff,
                0xf3f4f6, 0xffffff, 0xffffff, 0xeaeef2, 0xdfe5eb, 0xd0d7de, 0xafb8c1,
            ]
        }
        .map(color);
        Self {
            theme,
            bg: a[0],
            text: a[1],
            address: a[2],
            selection: a[3],
            selection_text: a[4],
            modified: a[5],
            muted: a[6],
            surface: a[7],
            panel: a[8],
            card: a[9],
            input: a[10],
            button: a[11],
            hover: a[12],
            border: a[13],
            border_strong: a[14],
            scrollbar_hover: color(if theme == Theme::Dark {
                0x636b74
            } else {
                0x80878e
            }),
            hex_background: color(if theme == Theme::Dark {
                0x111418
            } else {
                0xffffff
            }),
            hex_text: color(if theme == Theme::Dark {
                0xc9d1d9
            } else {
                0x1f2328
            }),
            hex_divider: color(if theme == Theme::Dark {
                0x30363d
            } else {
                0xd0d7de
            }),
        }
    }
    pub fn token(&self, name: &str) -> Option<Color32> {
        Some(match name {
            "bg" => self.bg,
            "text" => self.text,
            "address" => self.address,
            "selection" => self.selection,
            "selection-text" => self.selection_text,
            "modified" => self.modified,
            "muted" => self.muted,
            "surface" => self.surface,
            "panel" => self.panel,
            "card" => self.card,
            "input" => self.input,
            "button" => self.button,
            "hover" => self.hover,
            "border" => self.border,
            "border-strong" => self.border_strong,
            _ => return None,
        })
    }
}
