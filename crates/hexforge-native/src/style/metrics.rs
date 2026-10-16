#[derive(Debug, Clone, PartialEq)]
pub struct Metrics {
    pub font_support: f32,
    pub font_body: f32,
    pub font_heading: f32,
    pub font_title: f32,
    pub button_height: f32,
    pub input_height: f32,
    pub button_radius: u8,
    pub input_radius: u8,
    pub row_height: f32,
    pub hex_header: f32,
    pub horizontal_scrollbar: f32,
    pub top_menu: f32,
    pub file_strip: f32,
    pub status: f32,
    pub results_header: f32,
    pub result_row: f32,
    pub splitter: f32,
    pub editor_padding: f32,
    pub editor_gap: f32,
    pub field_padding: f32,
    pub dialog_padding: f32,
    pub dialog_width: f32,
}
impl Default for Metrics {
    fn default() -> Self {
        Self {
            font_support: 11.0,
            font_body: 12.0,
            font_heading: 13.0,
            font_title: 14.0,
            button_height: 27.0,
            input_height: 28.0,
            button_radius: 3,
            input_radius: 4,
            row_height: 22.0,
            hex_header: 32.0,
            // Actual retained Windows thin-scrollbar content-box deduction.
            horizontal_scrollbar: 10.0,
            top_menu: 34.0,
            file_strip: 42.0,
            status: 24.0,
            results_header: 28.0,
            result_row: 28.0,
            splitter: 4.0,
            editor_padding: 12.0,
            editor_gap: 12.0,
            field_padding: 10.0,
            dialog_padding: 15.0,
            dialog_width: 420.0,
        }
    }
}
