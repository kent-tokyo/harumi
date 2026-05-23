//! High-level flow-based document builder for generating structured PDFs.
//!
//! Enabled by the `flow` feature flag (implies `draw`).
//!
//! # Example
//! ```no_run
//! # #[cfg(feature = "flow")]
//! # fn main() -> harumi::Result<()> {
//! use harumi::{FlowDocument, FlowOptions};
//!
//! let font = include_bytes!("../../tests/fixtures/NotoSansJP-Regular.ttf");
//! let mut doc = FlowDocument::new(font.as_ref(), FlowOptions::default())?;
//!
//! doc.push_heading("Annual Report", 1)?;
//! doc.push_paragraph("This document summarizes the year.")?;
//! doc.push_key_value_table(&[("Revenue", "$1M"), ("Profit", "$200K")])?;
//!
//! let pdf_bytes = doc.render()?;
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "html")]
pub mod html;

use ttf_parser::Face;

use crate::{
    document::wrap_paragraph,
    Document, FontHandle, Result,
};

/// Page margin settings in PDF points.
#[derive(Clone, Copy, Debug)]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Margins {
    /// All four margins set to the same value.
    pub fn uniform(pt: f32) -> Self {
        Margins { top: pt, right: pt, bottom: pt, left: pt }
    }

    /// Standard 20 mm (≈ 56.7 pt) margins suitable for A4 documents.
    pub fn a4_standard() -> Self {
        Margins::uniform(56.7)
    }
}

/// Layout options for [`FlowDocument`].
pub struct FlowOptions {
    /// Page width and height in PDF points. Default: A4 (595 × 842).
    pub page_size: (f32, f32),
    /// Page margins in PDF points.
    pub margins: Margins,
    /// Body text font size in PDF points. Default: 11.0.
    pub body_font_size: f32,
    /// Scale factors for headings h1–h6 relative to body font size.
    /// Default: `[2.0, 1.6, 1.3, 1.1, 1.0, 0.9]`.
    pub heading_size_scale: [f32; 6],
    /// Multiplier for line height relative to font size. Default: 1.4.
    pub line_height_factor: f32,
    /// Extra vertical space added after each block element in PDF points. Default: 6.0.
    pub paragraph_spacing: f32,
    /// Fraction of content width used for the key column in tables. Default: 0.3.
    pub table_key_ratio: f32,
    /// Maximum number of pages the document may contain.
    ///
    /// `ensure_space` returns [`crate::Error::InvalidInput`] if this limit would be exceeded.
    /// Prevents unbounded page creation when rendering untrusted HTML.
    /// Default: 2000. Set to `u32::MAX` to disable.
    pub max_pages: u32,
}

impl Default for FlowOptions {
    fn default() -> Self {
        FlowOptions {
            page_size: (595.0, 842.0),
            margins: Margins::a4_standard(),
            body_font_size: 11.0,
            heading_size_scale: [2.0, 1.6, 1.3, 1.1, 1.0, 0.9],
            line_height_factor: 1.4,
            paragraph_spacing: 6.0,
            table_key_ratio: 0.3,
            max_pages: 2000,
        }
    }
}

/// A push-style document builder that generates a PDF with automatic pagination.
///
/// Push block elements (headings, paragraphs, tables, lists) in order;
/// page breaks are inserted automatically when content overflows a page.
///
/// Call [`render`](FlowDocument::render) to finalize and obtain the PDF bytes.
pub struct FlowDocument {
    inner: Document,
    body_font: FontHandle,
    body_font_bytes: Vec<u8>,
    options: FlowOptions,
    current_page: u32,
    /// Distance from the top of the content area (positive = downward).
    content_y: f32,
}

impl FlowDocument {
    /// Creates a new single-page document.
    ///
    /// `font_bytes` is the raw TTF/OTF data for the body font;
    /// CJK fonts such as NotoSansCJK are fully supported.
    pub fn new(font_bytes: impl Into<Vec<u8>>, options: FlowOptions) -> Result<Self> {
        let font_bytes: Vec<u8> = font_bytes.into();
        let mut inner = Document::new(options.page_size)?;
        let body_font = inner.embed_font(&font_bytes)?;
        Ok(FlowDocument {
            inner,
            body_font,
            body_font_bytes: font_bytes,
            options,
            current_page: 1,
            content_y: 0.0,
        })
    }

    // ── Geometry helpers ────────────────────────────────────────────────────

    fn content_width(&self) -> f32 {
        self.options.page_size.0 - self.options.margins.left - self.options.margins.right
    }

    fn content_height(&self) -> f32 {
        self.options.page_size.1 - self.options.margins.top - self.options.margins.bottom
    }

    /// PDF y coordinate of the text baseline, given logical `content_y` and `font_size`.
    /// PDF origin is bottom-left; `content_y` grows downward from the content area top.
    fn pdf_baseline_y(&self, content_y: f32, font_size: f32) -> f32 {
        self.options.page_size.1 - self.options.margins.top - content_y - font_size
    }

    /// PDF y coordinate of the top edge of the block at `content_y`.
    fn pdf_top_y(&self, content_y: f32) -> f32 {
        self.options.page_size.1 - self.options.margins.top - content_y
    }

    // ── Measurement ─────────────────────────────────────────────────────────

    fn measure_lines(&self, text: &str, font_size: f32, width: f32) -> Vec<String> {
        match Face::parse(&self.body_font_bytes, 0) {
            Ok(face) => text
                .split('\n')
                .flat_map(|para| wrap_paragraph(para, &face, font_size, width))
                .collect(),
            Err(_) => text.lines().map(str::to_owned).collect(),
        }
    }

    // ── Pagination ──────────────────────────────────────────────────────────

    /// Ensures at least `height` points of vertical space remain on the current page.
    /// If not, appends a new blank page and resets `content_y` to 0.
    /// Returns `Error::InvalidInput` if `max_pages` would be exceeded.
    fn ensure_space(&mut self, height: f32) -> Result<()> {
        if self.content_y > 0.0 && self.content_y + height > self.content_height() + 0.1 {
            let n = self.inner.page_count();
            if n >= self.options.max_pages {
                return Err(crate::Error::InvalidInput(format!(
                    "document exceeds max_pages limit of {}",
                    self.options.max_pages
                )));
            }
            self.inner.insert_blank_page(n, self.options.page_size)?;
            self.current_page = n + 1;
            self.content_y = 0.0;
        }
        Ok(())
    }

    // ── Public API ───────────────────────────────────────────────────────────

    /// Appends a heading at the given level (1–6) to the document.
    ///
    /// The heading is kept on a single page whenever it fits. Font size is scaled
    /// by [`FlowOptions::heading_size_scale`] relative to the body font size.
    pub fn push_heading(&mut self, text: &str, level: u8) -> Result<()> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }

        let level = level.clamp(1, 6) as usize;
        let font_size = self.options.body_font_size * self.options.heading_size_scale[level - 1];
        let line_h = font_size * self.options.line_height_factor;
        let lines = self.measure_lines(text, font_size, self.content_width());

        // Keep pre-heading spacing + the full block together on one page.
        // Compute spacing BEFORE ensure_space so that the heading is not orphaned at the
        // bottom of a page with only its spacing above it.
        let block_h = lines.len() as f32 * line_h;
        let pre_spacing = if self.content_y > 0.0 { self.options.paragraph_spacing * 1.5 } else { 0.0 };
        self.ensure_space(pre_spacing + block_h)?;
        // After a potential page break content_y resets to 0; only add spacing when still
        // on the same page (content_y > 0 means we didn't just start a fresh page).
        if self.content_y > 0.0 {
            self.content_y += pre_spacing;
        }

        let x = self.options.margins.left;
        let font = self.body_font;
        let current_page = self.current_page;

        for line in &lines {
            let y = self.pdf_baseline_y(self.content_y, font_size);
            self.inner.page(current_page)?.add_text(line, font, [x, y], font_size, [0.0, 0.0, 0.0])?;
            self.content_y += line_h;
        }

        self.content_y += self.options.paragraph_spacing;
        Ok(())
    }

    /// Appends a body-text paragraph to the document, with automatic word wrapping.
    ///
    /// CJK text breaks at any character; Latin text breaks at word boundaries.
    /// Newlines (`\n`) in `text` produce explicit line breaks.
    pub fn push_paragraph(&mut self, text: &str) -> Result<()> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }

        let font_size = self.options.body_font_size;
        let line_h = font_size * self.options.line_height_factor;
        let lines = self.measure_lines(text, font_size, self.content_width());

        let x = self.options.margins.left;
        let font = self.body_font;

        for line in &lines {
            self.ensure_space(line_h)?;
            let current_page = self.current_page;
            let y = self.pdf_baseline_y(self.content_y, font_size);
            self.inner.page(current_page)?.add_text(line, font, [x, y], font_size, [0.0, 0.0, 0.0])?;
            self.content_y += line_h;
        }

        self.content_y += self.options.paragraph_spacing;
        Ok(())
    }

    /// Appends a two-column key/value table to the document.
    ///
    /// Each row has a key cell (left) and a value cell (right). The key column width
    /// is controlled by [`FlowOptions::table_key_ratio`]. Rows are separated by
    /// light-gray horizontal lines.
    pub fn push_key_value_table(&mut self, rows: &[(&str, &str)]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }

        let content_w = self.content_width();
        let key_w = content_w * self.options.table_key_ratio;
        let val_w = content_w - key_w;
        let font_size = self.options.body_font_size;
        let line_h = font_size * self.options.line_height_factor;
        let cell_pad = 4.0_f32;
        let inner_key_w = (key_w - cell_pad * 2.0).max(1.0);
        let inner_val_w = (val_w - cell_pad * 2.0).max(1.0);
        let border_color = [0.7_f32, 0.7, 0.7];
        let border_lw = 0.5_f32;
        let x_left = self.options.margins.left;
        let x_divider = x_left + key_w;
        let x_right = x_left + content_w;
        let x_val = x_left + key_w + cell_pad;

        let last_idx = rows.len() - 1;

        for (idx, (key, val)) in rows.iter().enumerate() {
            let key = key.trim();
            let val = val.trim();
            let key_lines = self.measure_lines(key, font_size, inner_key_w);
            let val_lines = self.measure_lines(val, font_size, inner_val_w);
            let row_lines = key_lines.len().max(val_lines.len()).max(1);
            let row_h = row_lines as f32 * line_h + cell_pad * 2.0;

            self.ensure_space(row_h)?;

            // All coordinates must be captured after ensure_space (which may change current_page).
            let row_top_y = self.pdf_top_y(self.content_y);
            self.content_y += row_h;
            let row_bot_y = self.pdf_top_y(self.content_y);
            let page_num = self.current_page;
            let font = self.body_font;

            {
                let mut page = self.inner.page(page_num)?;

                // Top separator (also acts as outer top border for first row)
                page.add_line([x_left, row_top_y], [x_right, row_top_y], border_color, border_lw, 1.0)?;

                // Key cell text
                for (i, line) in key_lines.iter().enumerate() {
                    let y = row_top_y - cell_pad - font_size - i as f32 * line_h;
                    page.add_text(line, font, [x_left + cell_pad, y], font_size, [0.0, 0.0, 0.0])?;
                }

                // Value cell text
                for (i, line) in val_lines.iter().enumerate() {
                    let y = row_top_y - cell_pad - font_size - i as f32 * line_h;
                    page.add_text(line, font, [x_val, y], font_size, [0.0, 0.0, 0.0])?;
                }

                // Vertical divider between key and value columns
                page.add_line([x_divider, row_top_y], [x_divider, row_bot_y], border_color, border_lw, 1.0)?;

                // Bottom border on last row
                if idx == last_idx {
                    page.add_line([x_left, row_bot_y], [x_right, row_bot_y], border_color, border_lw, 1.0)?;
                }
            }
        }

        self.content_y += self.options.paragraph_spacing;
        Ok(())
    }

    /// Appends a bulleted or numbered list to the document.
    ///
    /// Each item is formatted as `"• text"` (unordered) or `"N. text"` (ordered).
    pub fn push_list(&mut self, items: &[&str], ordered: bool) -> Result<()> {
        for (i, item) in items.iter().enumerate() {
            let bullet = if ordered {
                format!("{}. {}", i + 1, item.trim())
            } else {
                format!("\u{2022} {}", item.trim()) // U+2022 BULLET
            };
            self.push_paragraph(&bullet)?;
        }
        Ok(())
    }

    /// Inserts an explicit page break, starting subsequent content on a new page.
    pub fn push_page_break(&mut self) -> Result<()> {
        let n = self.inner.page_count();
        self.inner.insert_blank_page(n, self.options.page_size)?;
        self.current_page = n + 1;
        self.content_y = 0.0;
        Ok(())
    }

    /// Finalizes the document and returns the PDF as a byte vector.
    pub fn render(mut self) -> Result<Vec<u8>> {
        self.inner.save_to_bytes()
    }
}
