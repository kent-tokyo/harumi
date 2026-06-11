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
mod html_tokenizer;

#[cfg(feature = "html")]
pub mod html;

use ttf_parser::Face;

use crate::{
    Document, FontHandle, Result,
    document::{glyph_advance_pt, wrap_paragraph},
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
        Margins {
            top: pt,
            right: pt,
            bottom: pt,
            left: pt,
        }
    }

    /// Standard 20 mm (≈ 56.7 pt) margins suitable for A4 documents.
    pub fn a4_standard() -> Self {
        Margins::uniform(56.7)
    }
}

/// Header or footer text rendered on every page of a [`FlowDocument`].
///
/// Set via [`FlowOptions::header`] and [`FlowOptions::footer`]. The placeholder
/// strings `{{page}}` and `{{total}}` are substituted with the current page number
/// and total page count at render time.
///
/// # Example
/// ```no_run
/// # #[cfg(feature = "flow")]
/// # fn main() -> harumi::Result<()> {
/// use harumi::{FlowDocument, FlowOptions, HeaderFooter};
///
/// let font = include_bytes!("../../tests/fixtures/NotoSansJP-Regular.ttf");
/// let mut doc = FlowDocument::new(font.as_ref(), FlowOptions {
///     footer: Some(HeaderFooter {
///         center: Some("{{page}} / {{total}}".into()),
///         ..Default::default()
///     }),
///     ..Default::default()
/// })?;
/// doc.push_paragraph("Hello!")?;
/// let pdf = doc.render()?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct HeaderFooter {
    /// Text aligned to the left of the area. `None` = no left text.
    pub left: Option<String>,
    /// Text centered horizontally. `None` = no center text.
    pub center: Option<String>,
    /// Text aligned to the right. `None` = no right text.
    pub right: Option<String>,
    /// Font size in PDF points. Default: `9.0`.
    pub font_size: f32,
    /// Color (RGB or CMYK) in `0.0..=1.0`. Default: dark gray.
    pub color: crate::Color,
}

impl Default for HeaderFooter {
    fn default() -> Self {
        HeaderFooter {
            left: None,
            center: None,
            right: None,
            font_size: 9.0,
            color: crate::Color::Rgb([0.3, 0.3, 0.3]),
        }
    }
}

impl HeaderFooter {
    /// A centered footer showing `"page / total"` in dark gray at 9 pt.
    pub fn page_number() -> Self {
        HeaderFooter {
            center: Some("{{page}} / {{total}}".into()),
            ..Default::default()
        }
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
    /// Optional header rendered at the top margin of every page. Default: `None`.
    pub header: Option<HeaderFooter>,
    /// Optional footer rendered at the bottom margin of every page. Default: `None`.
    pub footer: Option<HeaderFooter>,
    /// Auto-generate PDF bookmarks from headings pushed via [`FlowDocument::push_heading`].
    /// Default: `true`.
    pub auto_bookmarks: bool,
    /// Optional font bytes for headings (h1–h6). If `None`, body font is used.
    /// Enables visual distinction of headings via a different typeface.
    /// Default: `None`.
    pub heading_font_bytes: Option<Vec<u8>>,
    /// Optional font bytes for code blocks. If `None`, body font is used.
    /// Enables monospace or special rendering for code via [`push_code_block`](FlowDocument::push_code_block).
    /// Default: `None`.
    pub code_font_bytes: Option<Vec<u8>>,
    /// Optional background color for code blocks as RGB `[r, g, b]` in `0.0..=1.0`.
    /// When set, code blocks are drawn with a background rectangle of this color.
    /// Requires the **`draw`** feature (implied by **`flow`**). Default: `None`.
    pub code_background: Option<[f32; 3]>,
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
            header: None,
            footer: None,
            auto_bookmarks: true,
            heading_font_bytes: None,
            code_font_bytes: None,
            code_background: None,
        }
    }
}

/// A styled text run for use with [`FlowDocument::push_paragraph_styled`].
///
/// Bold and italic are *synthetic* effects: bold uses PDF fill+stroke mode; italic
/// applies a 12° shear matrix.  Both work with any single-font TTF/OTF including
/// CJK fonts — no separate bold/italic font file is required.
#[derive(Clone, Debug)]
pub struct InlineSpan {
    /// The text for this run.
    pub text: String,
    /// Synthetic bold (fill+stroke render mode with proportional stroke width).
    pub bold: bool,
    /// Synthetic italic (12° horizontal shear via text matrix).
    pub italic: bool,
    /// Fill color (RGB or CMYK) in `0.0..=1.0`. Default: black.
    pub color: crate::Color,
}

impl InlineSpan {
    /// Plain black text.
    pub fn plain(text: impl Into<String>) -> Self {
        InlineSpan {
            text: text.into(),
            bold: false,
            italic: false,
            color: crate::Color::Rgb([0.0; 3]),
        }
    }
    /// Bold text.
    pub fn bold(text: impl Into<String>) -> Self {
        InlineSpan {
            text: text.into(),
            bold: true,
            italic: false,
            color: crate::Color::Rgb([0.0; 3]),
        }
    }
    /// Italic text.
    pub fn italic(text: impl Into<String>) -> Self {
        InlineSpan {
            text: text.into(),
            bold: false,
            italic: true,
            color: crate::Color::Rgb([0.0; 3]),
        }
    }
    /// Colored text (bold and italic both false).
    pub fn colored(text: impl Into<String>, color: impl Into<crate::Color>) -> Self {
        InlineSpan {
            text: text.into(),
            bold: false,
            italic: false,
            color: color.into(),
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
    heading_font: Option<FontHandle>,
    heading_font_bytes: Option<Vec<u8>>,
    code_font: Option<FontHandle>,
    code_font_bytes: Option<Vec<u8>>,
    options: FlowOptions,
    current_page: u32,
    /// Distance from the top of the content area (positive = downward).
    content_y: f32,
    /// Pending bookmark entries collected from push_heading calls.
    /// Each entry is (title, page, pdf_y, level) where pdf_y is at the top of the heading.
    outline_entries: Vec<(String, u32, f32, u8)>,
}

impl FlowDocument {
    /// Creates a new single-page document.
    ///
    /// `font_bytes` is the raw TTF/OTF data for the body font;
    /// CJK fonts such as NotoSansCJK are fully supported.
    ///
    /// If `options.heading_font_bytes` or `options.code_font_bytes` are set,
    /// those fonts are embedded into the document as well. The body font is
    /// always embedded; the optional heading and code fonts are only embedded
    /// if present.
    pub fn new(font_bytes: impl Into<Vec<u8>>, options: FlowOptions) -> Result<Self> {
        let font_bytes: Vec<u8> = font_bytes.into();
        let mut inner = Document::new(options.page_size)?;
        let body_font = inner.embed_font(&font_bytes)?;

        // Embed optional heading and code fonts
        let (heading_font, heading_font_bytes) = if let Some(bytes) = &options.heading_font_bytes {
            let handle = inner.embed_font(bytes)?;
            (Some(handle), Some(bytes.clone()))
        } else {
            (None, None)
        };

        let (code_font, code_font_bytes) = if let Some(bytes) = &options.code_font_bytes {
            let handle = inner.embed_font(bytes)?;
            (Some(handle), Some(bytes.clone()))
        } else {
            (None, None)
        };

        Ok(FlowDocument {
            inner,
            body_font,
            body_font_bytes: font_bytes,
            heading_font,
            heading_font_bytes,
            code_font,
            code_font_bytes,
            options,
            current_page: 1,
            content_y: 0.0,
            outline_entries: Vec::new(),
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

    fn measure_lines(
        &self,
        text: &str,
        font_size: f32,
        width: f32,
        font_bytes: &[u8],
    ) -> Vec<String> {
        match Face::parse(font_bytes, 0) {
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
        let font_bytes = self
            .heading_font_bytes
            .as_deref()
            .unwrap_or(&self.body_font_bytes);
        let lines = self.measure_lines(text, font_size, self.content_width(), font_bytes);

        // Keep pre-heading spacing + the full block together on one page.
        // Compute spacing BEFORE ensure_space so that the heading is not orphaned at the
        // bottom of a page with only its spacing above it.
        let block_h = lines.len() as f32 * line_h;
        let pre_spacing = if self.content_y > 0.0 {
            self.options.paragraph_spacing * 1.5
        } else {
            0.0
        };
        self.ensure_space(pre_spacing + block_h)?;
        // After a potential page break content_y resets to 0; only add spacing when still
        // on the same page (content_y > 0 means we didn't just start a fresh page).
        if self.content_y > 0.0 {
            self.content_y += pre_spacing;
        }

        // Record a bookmark anchored at the top of this heading block (before rendering).
        if self.options.auto_bookmarks {
            let bm_y = self.pdf_top_y(self.content_y);
            let bm_page = self.current_page;
            self.outline_entries
                .push((text.to_owned(), bm_page, bm_y, level as u8));
        }

        let x = self.options.margins.left;
        let font = self.heading_font.unwrap_or(self.body_font);
        let current_page = self.current_page;

        for line in &lines {
            let y = self.pdf_baseline_y(self.content_y, font_size);
            self.inner.page(current_page)?.add_text(
                line,
                font,
                [x, y],
                font_size,
                [0.0, 0.0, 0.0],
            )?;
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
        let lines =
            self.measure_lines(text, font_size, self.content_width(), &self.body_font_bytes);

        let x = self.options.margins.left;
        let font = self.body_font;

        for line in &lines {
            self.ensure_space(line_h)?;
            let current_page = self.current_page;
            let y = self.pdf_baseline_y(self.content_y, font_size);
            self.inner.page(current_page)?.add_text(
                line,
                font,
                [x, y],
                font_size,
                [0.0, 0.0, 0.0],
            )?;
            self.content_y += line_h;
        }

        self.content_y += self.options.paragraph_spacing;
        Ok(())
    }

    /// Appends a paragraph with mixed inline styling (bold, italic, color) to the document.
    ///
    /// Each [`InlineSpan`] runs inline on the same line as the previous one. Word wrapping
    /// is performed across the full concatenated text so visual line breaks are natural.
    ///
    /// # Example
    /// ```no_run
    /// # #[cfg(feature = "flow")]
    /// # fn main() -> harumi::Result<()> {
    /// use harumi::{FlowDocument, FlowOptions, InlineSpan};
    /// let font = include_bytes!("../../tests/fixtures/NotoSansJP-Regular.ttf");
    /// let mut doc = FlowDocument::new(font.as_ref(), FlowOptions::default())?;
    /// doc.push_paragraph_styled(&[
    ///     InlineSpan::plain("Normal "),
    ///     InlineSpan::bold("bold "),
    ///     InlineSpan::colored("red", [0.8, 0.0, 0.0]),
    /// ])?;
    /// # Ok(()) }
    /// ```
    pub fn push_paragraph_styled(&mut self, spans: &[InlineSpan]) -> Result<()> {
        let non_empty: Vec<&InlineSpan> = spans.iter().filter(|s| !s.text.is_empty()).collect();
        if non_empty.is_empty() {
            return Ok(());
        }

        let font_size = self.options.body_font_size;
        let line_h = font_size * self.options.line_height_factor;
        let content_w = self.content_width();
        // Clone font bytes so the borrow doesn't conflict with ensure_space's &mut self.
        let font_bytes_owned = self.body_font_bytes.clone();
        let face: Option<Face<'_>> = Face::parse(&font_bytes_owned, 0).ok();
        let font = self.body_font;

        // Build flat (char, span_index) list.
        let mut char_spans: Vec<(char, usize)> = Vec::new();
        for (i, span) in non_empty.iter().enumerate() {
            for ch in span.text.chars() {
                char_spans.push((ch, i));
            }
        }

        // Wrap the full text.
        let full_text: String = char_spans.iter().map(|(ch, _)| ch).collect();
        let line_strings = match face.as_ref() {
            Some(f) => full_text
                .split('\n')
                .flat_map(|p| crate::document::wrap_paragraph(p, f, font_size, content_w))
                .collect::<Vec<_>>(),
            None => full_text.lines().map(str::to_owned).collect(),
        };

        let mut char_cursor = 0usize; // position in char_spans

        for line_str in &line_strings {
            let line_len = line_str.chars().count();

            self.ensure_space(line_h)?;
            let y = self.pdf_baseline_y(self.content_y, font_size);
            let mut x = self.options.margins.left;
            let current_page = self.current_page;

            // Group consecutive chars with the same span style.
            let mut run_start = char_cursor;
            while run_start < char_cursor + line_len {
                let span_idx = char_spans[run_start].1;
                let mut run_end = run_start + 1;
                while run_end < char_cursor + line_len && char_spans[run_end].1 == span_idx {
                    run_end += 1;
                }

                let run_text: String = char_spans[run_start..run_end]
                    .iter()
                    .map(|(ch, _)| ch)
                    .collect();
                let span = non_empty[span_idx];

                self.inner.page(current_page)?.add_text_styled(
                    &run_text,
                    font,
                    [x, y],
                    font_size,
                    span.color,
                    span.bold,
                    span.italic,
                )?;

                // Advance x by measured width of the run.
                if let Some(ref f) = face {
                    x += run_text
                        .chars()
                        .map(|ch| {
                            crate::document::glyph_advance_pt(f, ch, font_size)
                                .unwrap_or(font_size * 0.5)
                        })
                        .sum::<f32>();
                }

                run_start = run_end;
            }

            // Advance char cursor, skipping any trailing space/newline consumed by wrapping.
            char_cursor += line_len;
            // Skip one space between lines if the full text has one at this position.
            if char_cursor < char_spans.len() {
                let next_ch = char_spans[char_cursor].0;
                if next_ch == ' ' || next_ch == '\n' {
                    char_cursor += 1;
                }
            }

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
            let key_lines = self.measure_lines(key, font_size, inner_key_w, &self.body_font_bytes);
            let val_lines = self.measure_lines(val, font_size, inner_val_w, &self.body_font_bytes);
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
                page.add_line(
                    [x_left, row_top_y],
                    [x_right, row_top_y],
                    border_color,
                    border_lw,
                    1.0,
                )?;

                // Key cell text
                for (i, line) in key_lines.iter().enumerate() {
                    let y = row_top_y - cell_pad - font_size - i as f32 * line_h;
                    page.add_text(
                        line,
                        font,
                        [x_left + cell_pad, y],
                        font_size,
                        [0.0, 0.0, 0.0],
                    )?;
                }

                // Value cell text
                for (i, line) in val_lines.iter().enumerate() {
                    let y = row_top_y - cell_pad - font_size - i as f32 * line_h;
                    page.add_text(line, font, [x_val, y], font_size, [0.0, 0.0, 0.0])?;
                }

                // Vertical divider between key and value columns
                page.add_line(
                    [x_divider, row_top_y],
                    [x_divider, row_bot_y],
                    border_color,
                    border_lw,
                    1.0,
                )?;

                // Bottom border on last row
                if idx == last_idx {
                    page.add_line(
                        [x_left, row_bot_y],
                        [x_right, row_bot_y],
                        border_color,
                        border_lw,
                        1.0,
                    )?;
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

    /// Appends a code block with an optional background color to the document.
    ///
    /// If `options.code_font_bytes` is set, the code is rendered using that font;
    /// otherwise the body font is used. If `options.code_background` is set,
    /// a background rectangle is drawn behind the code at that color.
    ///
    /// Code blocks use monospace or fixed-width presentation via font selection only;
    /// no syntax highlighting is performed. Newlines in the text are preserved.
    pub fn push_code_block(&mut self, text: &str) -> Result<()> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }

        let font_size = self.options.body_font_size;
        let line_h = font_size * self.options.line_height_factor;
        let font_bytes = self
            .code_font_bytes
            .as_deref()
            .unwrap_or(&self.body_font_bytes);
        let lines = self.measure_lines(text, font_size, self.content_width(), font_bytes);

        let block_h = lines.len() as f32 * line_h;
        let pre_spacing = if self.content_y > 0.0 {
            self.options.paragraph_spacing
        } else {
            0.0
        };
        self.ensure_space(pre_spacing + block_h)?;
        if self.content_y > 0.0 {
            self.content_y += pre_spacing;
        }

        let x = self.options.margins.left;
        let font = self.code_font.unwrap_or(self.body_font);
        let current_page = self.current_page;
        let y_top = self.pdf_top_y(self.content_y);
        let y_bottom = y_top - block_h;

        // Draw background rectangle if configured (requires draw feature, which flow implies)
        if let Some(bg_color) = self.options.code_background {
            let right_x = self.options.page_size.0 - self.options.margins.right;
            let padding = 2.0;
            self.inner.page(current_page)?.add_rect(
                [
                    x - padding,
                    y_bottom - padding,
                    right_x - x + 2.0 * padding,
                    block_h + 2.0 * padding,
                ],
                bg_color,
                1.0,
            )?;
        }

        // Render the text lines
        for line in &lines {
            let y = self.pdf_baseline_y(self.content_y, font_size);
            self.inner.page(current_page)?.add_text(
                line,
                font,
                [x, y],
                font_size,
                [0.0, 0.0, 0.0],
            )?;
            self.content_y += line_h;
        }

        self.content_y += self.options.paragraph_spacing;
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
    ///
    /// Headers, footers, and bookmarks accumulated during content-push calls are
    /// written to the document at this point.
    pub fn render(mut self) -> Result<Vec<u8>> {
        let total_pages = self.inner.page_count();
        let body_font = self.body_font;

        // Parse the face once for text-width measurement in header/footer.
        let font_bytes_owned: Vec<u8> = self.body_font_bytes.clone();
        let face: Option<Face<'_>> = Face::parse(&font_bytes_owned, 0).ok();

        // Render header on every page.
        if let Some(ref hdr) = self.options.header.clone() {
            for pg in 1..=total_pages {
                render_hf_on_page(
                    &mut self.inner,
                    pg,
                    hdr,
                    total_pages,
                    true,
                    body_font,
                    self.options.page_size,
                    self.options.margins,
                    face.as_ref(),
                )?;
            }
        }
        // Render footer on every page.
        if let Some(ref ftr) = self.options.footer.clone() {
            for pg in 1..=total_pages {
                render_hf_on_page(
                    &mut self.inner,
                    pg,
                    ftr,
                    total_pages,
                    false,
                    body_font,
                    self.options.page_size,
                    self.options.margins,
                    face.as_ref(),
                )?;
            }
        }

        // Register bookmarks gathered from push_heading.
        for (title, page, y, level) in self.outline_entries.drain(..) {
            self.inner.add_outline_item(&title, page, y, level)?;
        }

        self.inner.save_to_bytes()
    }
}

// ---------------------------------------------------------------------------
// Header/footer renderer (free function to avoid borrow-checker conflicts)
// ---------------------------------------------------------------------------

/// Substitute `{{page}}` and `{{total}}` in a template string.
fn hf_subst(tmpl: &str, page: u32, total: u32) -> String {
    tmpl.replace("{{page}}", &page.to_string())
        .replace("{{total}}", &total.to_string())
}

/// Measure the rendered width of `text` in PDF points given a parsed face.
fn hf_measure(face: Option<&Face<'_>>, text: &str, font_size: f32) -> f32 {
    match face {
        Some(f) => text
            .chars()
            .map(|ch| glyph_advance_pt(f, ch, font_size).unwrap_or(font_size * 0.5))
            .sum(),
        // Fallback: use character count (not byte length) so CJK multi-byte chars don't
        // over-estimate the width and mis-position right-aligned / centered text.
        None => text.chars().count() as f32 * font_size * 0.5,
    }
}

/// Renders one header or footer row onto a single page.
#[allow(clippy::too_many_arguments)]
fn render_hf_on_page(
    inner: &mut Document,
    page_num: u32,
    hf: &HeaderFooter,
    total_pages: u32,
    is_header: bool,
    font: FontHandle,
    page_size: (f32, f32),
    margins: Margins,
    face: Option<&Face<'_>>,
) -> Result<()> {
    let fs = if hf.font_size > 0.0 {
        hf.font_size
    } else {
        9.0
    };
    let color = hf.color;
    let margin_left = margins.left;
    let margin_right = margins.right;
    let content_w = page_size.0 - margin_left - margin_right;

    // Vertical position: centered in the top/bottom margin band.
    let y = if is_header {
        page_size.1 - margins.top * 0.5
    } else {
        margins.bottom * 0.5
    };

    if let Some(ref tmpl) = hf.left {
        let text = hf_subst(tmpl, page_num, total_pages);
        inner
            .page(page_num)?
            .add_text(&text, font, [margin_left, y], fs, color)?;
    }
    if let Some(ref tmpl) = hf.center {
        let text = hf_subst(tmpl, page_num, total_pages);
        let w = hf_measure(face, &text, fs);
        let x = margin_left + (content_w - w) / 2.0;
        inner
            .page(page_num)?
            .add_text(&text, font, [x, y], fs, color)?;
    }
    if let Some(ref tmpl) = hf.right {
        let text = hf_subst(tmpl, page_num, total_pages);
        let w = hf_measure(face, &text, fs);
        let x = page_size.0 - margin_right - w;
        inner
            .page(page_num)?
            .add_text(&text, font, [x, y], fs, color)?;
    }

    Ok(())
}
