/// Options controlling the output PDF layout.
#[derive(Clone, Debug)]
pub struct LayoutOptions {
    /// Page margin on all four sides, in PDF points (default: 72 pt = 1 inch).
    pub margin: f32,
    /// Line height as a multiple of font_size (default: 1.4).
    pub line_height_ratio: f32,
    /// Extra vertical gap inserted after each paragraph block, as a multiple of font_size (default: 0.8).
    pub paragraph_gap_ratio: f32,
    /// Font size for H1 headings (default: 24 pt).
    pub h1_size: f32,
    /// Font size for H2 headings (default: 20 pt).
    pub h2_size: f32,
    /// Font size for H3 headings (default: 16 pt).
    pub h3_size: f32,
    /// Font size for H4 headings (default: 14 pt).
    pub h4_size: f32,
    /// Font size for body paragraphs (default: 11 pt).
    pub body_size: f32,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            margin: 72.0,
            line_height_ratio: 1.4,
            paragraph_gap_ratio: 0.8,
            h1_size: 24.0,
            h2_size: 20.0,
            h3_size: 16.0,
            h4_size: 14.0,
            body_size: 11.0,
        }
    }
}

impl LayoutOptions {
    pub(crate) fn font_size_for_type(&self, block_type: &str) -> f32 {
        match block_type {
            "h1" => self.h1_size,
            "h2" => self.h2_size,
            "h3" => self.h3_size,
            "h4" | "h5" | "h6" => self.h4_size,
            _ => self.body_size,
        }
    }
}
