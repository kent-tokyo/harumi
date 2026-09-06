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
    document::{glyph_advance_pt, helpers::wrap_paragraph_with_fallback},
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
    /// Baseline offset in PDF points applied consistently to flow text.
    /// Positive values move the baseline upward. Default: `0.0`.
    pub baseline_offset: f32,
    /// Extra vertical space added after each block element in PDF points. Default: 6.0.
    pub paragraph_spacing: f32,
    /// Horizontal alignment for body and styled paragraphs. Default: left.
    pub body_alignment: FlowTextAlignment,
    /// Minimum number of lines kept at the top or bottom of a page for a
    /// multi-line paragraph. `1` disables extra widow/orphan protection.
    /// Default: `2`.
    pub paragraph_min_lines: usize,
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
    /// Keep a heading with at least the first body line that follows it when
    /// the combined block fits on one page. Default: `false` for compatibility.
    pub keep_headings_with_next: bool,
    /// Keep a figure with the first following body line when the combined block
    /// fits on one page. Default: `false` for compatibility.
    pub keep_figures_with_next: bool,
    /// Optional font bytes for headings (h1–h6). If `None`, body font is used.
    /// Enables visual distinction of headings via a different typeface.
    /// Default: `None`.
    pub heading_font_bytes: Option<Vec<u8>>,
    /// Optional font bytes for code blocks. If `None`, body font is used.
    /// Enables monospace or special rendering for code via [`push_code_block`](FlowDocument::push_code_block).
    /// Default: `None`.
    pub code_font_bytes: Option<Vec<u8>>,
    /// Optional font bytes used for body characters missing from the primary font.
    ///
    /// The fallback is selected per Unicode scalar value and is intended for
    /// mixed Latin/CJK or symbol-heavy paragraphs. It is only used when the
    /// primary body font has no glyph for a character. Default: `None`.
    pub fallback_font_bytes: Option<Vec<u8>>,
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
            baseline_offset: 0.0,
            paragraph_spacing: 6.0,
            body_alignment: FlowTextAlignment::Left,
            paragraph_min_lines: 2,
            table_key_ratio: 0.3,
            max_pages: 2000,
            header: None,
            footer: None,
            auto_bookmarks: true,
            keep_headings_with_next: false,
            keep_figures_with_next: false,
            heading_font_bytes: None,
            code_font_bytes: None,
            fallback_font_bytes: None,
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

/// Horizontal alignment for Flow body paragraphs.
#[derive(Clone, Copy, Debug, Default)]
pub enum FlowTextAlignment {
    /// Align text to the left content edge.
    #[default]
    Left,
    /// Center each measured line within the content width.
    Center,
    /// Align each measured line to the right content edge.
    Right,
}

/// Strategy used to determine the widths of columns in [`TableOptions`].
#[derive(Clone, Debug)]
pub enum TableColumnWidths {
    /// Measure the widest unwrapped value in each column, then scale down if needed.
    Intrinsic,
    /// Explicit widths in PDF points, one value per column.
    Fixed(Vec<f32>),
    /// Relative weights, one value per column. Values are normalized to content width.
    Fractions(Vec<f32>),
}

/// Layout options for [`FlowDocument::push_table`].
#[derive(Clone, Debug)]
pub struct TableOptions {
    /// Column-width strategy. Defaults to [`TableColumnWidths::Intrinsic`].
    pub column_widths: TableColumnWidths,
    /// Padding on each side of every cell, in PDF points. Default: `4.0`.
    pub cell_padding: f32,
    /// Number of leading rows to repeat after a page break. Default: `0`.
    pub header_rows: usize,
    /// Keep the complete table with the first following body line when the
    /// table fits on one page. Default: `false`.
    pub keep_with_next: bool,
    /// Optional minimum width in points for each column. Default: `None`.
    pub min_column_widths: Option<Vec<f32>>,
    /// Optional maximum width in points for each column. Default: `None`.
    pub max_column_widths: Option<Vec<f32>>,
    /// RGB border color. Default: light gray `[0.7, 0.7, 0.7]`.
    pub border_color: [f32; 3],
    /// Border line width in PDF points. Set to `0.0` to omit borders. Default: `0.5`.
    pub border_width: f32,
}

/// Resolved column widths used by a table layout.
#[derive(Clone, Debug)]
pub struct TableWidthAllocation {
    /// Width of each column in PDF points.
    pub widths: Vec<f32>,
    /// Available width between the document's left and right margins.
    pub content_width: f32,
}

/// A flow-layout table cell with an optional horizontal span.
#[derive(Clone, Debug)]
pub struct FlowTableCell {
    /// Cell text. Newlines are treated as explicit line breaks.
    pub text: String,
    /// Number of adjacent columns occupied by this cell. Default: `1`.
    pub colspan: usize,
    /// Number of adjacent rows occupied by this cell. Default: `1`.
    pub rowspan: usize,
    /// Optional per-cell padding. When `None`, [`TableOptions::cell_padding`] is used.
    pub padding: Option<f32>,
    /// Horizontal alignment within the cell.
    pub alignment: TableCellAlignment,
}

/// Horizontal alignment for a flow-layout table cell.
#[derive(Clone, Copy, Debug, Default)]
pub enum TableCellAlignment {
    /// Align text to the left padding edge.
    #[default]
    Left,
    /// Center text within the cell.
    Center,
    /// Align text to the right padding edge.
    Right,
}

impl FlowTableCell {
    /// Creates a single-column cell.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            colspan: 1,
            rowspan: 1,
            padding: None,
            alignment: TableCellAlignment::Left,
        }
    }

    /// Creates a cell spanning `colspan` adjacent columns.
    pub fn spanning(text: impl Into<String>, colspan: usize) -> Self {
        Self {
            text: text.into(),
            colspan,
            rowspan: 1,
            padding: None,
            alignment: TableCellAlignment::Left,
        }
    }

    /// Sets the horizontal alignment for this cell.
    pub fn with_alignment(mut self, alignment: TableCellAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// Sets per-cell padding in PDF points.
    pub fn with_padding(mut self, padding: f32) -> Self {
        self.padding = Some(padding);
        self
    }

    /// Sets the number of adjacent rows occupied by this cell.
    pub fn with_rowspan(mut self, rowspan: usize) -> Self {
        self.rowspan = rowspan;
        self
    }

    /// Sets the number of adjacent columns occupied by this cell.
    pub fn with_colspan(mut self, colspan: usize) -> Self {
        self.colspan = colspan;
        self
    }
}

impl Default for TableOptions {
    fn default() -> Self {
        Self {
            column_widths: TableColumnWidths::Intrinsic,
            cell_padding: 4.0,
            header_rows: 0,
            keep_with_next: false,
            min_column_widths: None,
            max_column_widths: None,
            border_color: [0.7, 0.7, 0.7],
            border_width: 0.5,
        }
    }
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

/// Shared page and text geometry used by the flow planner and renderer.
///
/// Keeping these calculations in one value prevents pagination decisions from
/// drifting away from the coordinates used when operators are emitted.
#[derive(Clone, Copy, Debug)]
struct GeometryPlanner {
    page_size: (f32, f32),
    margins: Margins,
    line_height_factor: f32,
    baseline_offset: f32,
}

/// The measured result shared by Flow and HTML paragraph entry points.
///
/// Keeping the wrapped lines and vertical rhythm together prevents pagination
/// from measuring one geometry and rendering another.
#[derive(Clone, Debug)]
struct MeasuredParagraph {
    lines: Vec<String>,
    line_height: f32,
}

fn apply_table_width_constraints(
    mut widths: Vec<f32>,
    target: f32,
    min_widths: Option<&[f32]>,
    max_widths: Option<&[f32]>,
    columns: usize,
) -> Result<Vec<f32>> {
    let mins = min_widths.unwrap_or(&[]);
    let maxs = max_widths.unwrap_or(&[]);
    if (!mins.is_empty() && mins.len() != columns) || (!maxs.is_empty() && maxs.len() != columns) {
        return Err(crate::Error::InvalidInput(
            "table min/max width lists must match the column count".into(),
        ));
    }
    let min_at = |i: usize| mins.get(i).copied().unwrap_or(0.0);
    let max_at = |i: usize| maxs.get(i).copied().unwrap_or(f32::INFINITY);
    for i in 0..columns {
        let min = min_at(i);
        let max = max_at(i);
        if !min.is_finite()
            || min < 0.0
            || (!max.is_infinite() && (!max.is_finite() || max <= 0.0))
            || min > max
        {
            return Err(crate::Error::InvalidInput(
                "table min/max widths must be finite, positive where set, and ordered".into(),
            ));
        }
    }
    if !target.is_finite() || target <= 0.0 {
        return Err(crate::Error::InvalidInput(
            "table width target must be finite and positive".into(),
        ));
    }
    let min_total = (0..columns).map(min_at).sum::<f32>();
    let max_total = (0..columns).map(max_at).sum::<f32>();
    if min_total > target + 0.1 || max_total < target - 0.1 {
        return Err(crate::Error::InvalidInput(
            "table min/max widths cannot satisfy the requested table width".into(),
        ));
    }
    for (i, width) in widths.iter_mut().enumerate().take(columns) {
        *width = (*width).clamp(min_at(i), max_at(i));
    }

    for _ in 0..2 {
        let delta = target - widths.iter().sum::<f32>();
        if delta.abs() <= 0.01 {
            break;
        }
        if delta > 0.0 {
            let unbounded = (0..columns).filter(|&i| max_at(i).is_infinite()).count();
            if unbounded > 0 {
                let addition = delta / unbounded as f32;
                for (i, width) in widths.iter_mut().enumerate().take(columns) {
                    if max_at(i).is_infinite() {
                        *width += addition;
                    }
                }
                continue;
            }
            let capacity = (0..columns)
                .map(|i| (max_at(i) - widths[i]).max(0.0))
                .sum::<f32>();
            if capacity <= 0.0 {
                return Err(crate::Error::InvalidInput(
                    "table max widths leave no room for allocation".into(),
                ));
            }
            for (i, width) in widths.iter_mut().enumerate().take(columns) {
                *width += delta * (max_at(i) - *width).max(0.0) / capacity;
            }
        } else {
            let capacity = (0..columns)
                .map(|i| (widths[i] - min_at(i)).max(0.0))
                .sum::<f32>();
            if capacity <= 0.0 {
                return Err(crate::Error::InvalidInput(
                    "table min widths leave no room for allocation".into(),
                ));
            }
            for (i, width) in widths.iter_mut().enumerate().take(columns) {
                *width += delta * (*width - min_at(i)).max(0.0) / capacity;
            }
        }
    }
    Ok(widths)
}

impl GeometryPlanner {
    fn new(options: &FlowOptions) -> Self {
        Self {
            page_size: options.page_size,
            margins: options.margins,
            line_height_factor: options.line_height_factor,
            baseline_offset: options.baseline_offset,
        }
    }

    fn content_width(self) -> f32 {
        self.page_size.0 - self.margins.left - self.margins.right
    }

    fn content_height(self) -> f32 {
        self.page_size.1 - self.margins.top - self.margins.bottom
    }

    fn line_height(self, font_size: f32) -> f32 {
        font_size * self.line_height_factor
    }

    /// PDF y coordinate of a text baseline at logical top-down `content_y`.
    fn baseline_y(self, content_y: f32, font_size: f32) -> f32 {
        self.page_size.1 - self.margins.top - content_y - font_size + self.baseline_offset
    }

    /// PDF y coordinate of the top edge of a block at logical top-down `content_y`.
    fn top_y(self, content_y: f32) -> f32 {
        self.page_size.1 - self.margins.top - content_y
    }

    fn text_width(self, text: &str, face: &Face<'_>, font_size: f32) -> f32 {
        text.chars()
            .map(|ch| glyph_advance_pt(face, ch, font_size).unwrap_or(font_size * 0.5))
            .sum()
    }
}

fn split_missing_glyph_runs(
    text: &str,
    body_face: Option<&Face<'_>>,
    fallback_face: Option<&Face<'_>>,
) -> Vec<(String, bool)> {
    let mut runs: Vec<(String, bool)> = Vec::new();
    for ch in text.chars() {
        let use_fallback = body_face
            .zip(fallback_face)
            .is_some_and(|(body, fallback)| {
                body.glyph_index(ch).is_none() && fallback.glyph_index(ch).is_some()
            });
        if let Some((run, previous_fallback)) = runs.last_mut()
            && *previous_fallback == use_fallback
        {
            run.push(ch);
        } else {
            runs.push((ch.to_string(), use_fallback));
        }
    }
    runs
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
    fallback_font: Option<FontHandle>,
    fallback_font_bytes: Option<Vec<u8>>,
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
        if !options.baseline_offset.is_finite() {
            return Err(crate::Error::InvalidInput(
                "baseline_offset must be finite".into(),
            ));
        }
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

        let (fallback_font, fallback_font_bytes) = if let Some(bytes) = &options.fallback_font_bytes
        {
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
            fallback_font,
            fallback_font_bytes,
            options,
            current_page: 1,
            content_y: 0.0,
            outline_entries: Vec::new(),
        })
    }

    // ── Geometry helpers ────────────────────────────────────────────────────

    fn geometry(&self) -> GeometryPlanner {
        GeometryPlanner::new(&self.options)
    }

    fn body_text_width(&self, text: &str, font_size: f32) -> f32 {
        let body_bytes = self.body_font_bytes.clone();
        let body_face = Face::parse(&body_bytes, 0).ok();
        let fallback_bytes = self.fallback_font_bytes.clone().unwrap_or_default();
        let fallback_face = Face::parse(&fallback_bytes, 0).ok();
        split_missing_glyph_runs(text, body_face.as_ref(), fallback_face.as_ref())
            .into_iter()
            .map(|(run, use_fallback)| {
                let face = if use_fallback {
                    fallback_face.as_ref()
                } else {
                    body_face.as_ref()
                };
                face.map(|face| self.geometry().text_width(&run, face, font_size))
                    .unwrap_or(run.chars().count() as f32 * font_size * 0.5)
            })
            .sum()
    }

    fn body_text_x(&self, text: &str, font_size: f32) -> f32 {
        self.body_text_x_with_alignment(text, font_size, self.options.body_alignment)
    }

    fn body_text_x_with_alignment(
        &self,
        text: &str,
        font_size: f32,
        alignment: FlowTextAlignment,
    ) -> f32 {
        let left = self.options.margins.left;
        let remaining =
            (self.geometry().content_width() - self.body_text_width(text, font_size)).max(0.0);
        left + match alignment {
            FlowTextAlignment::Left => 0.0,
            FlowTextAlignment::Center => remaining / 2.0,
            FlowTextAlignment::Right => remaining,
        }
    }

    #[cfg(feature = "html")]
    pub(crate) fn default_body_alignment(&self) -> FlowTextAlignment {
        self.options.body_alignment
    }

    // ── Measurement ─────────────────────────────────────────────────────────

    fn measure_lines(
        &self,
        text: &str,
        font_size: f32,
        width: f32,
        font_bytes: &[u8],
    ) -> Vec<String> {
        self.measure_paragraph(text, font_size, width, font_bytes)
            .lines
    }

    fn measure_paragraph(
        &self,
        text: &str,
        font_size: f32,
        width: f32,
        font_bytes: &[u8],
    ) -> MeasuredParagraph {
        let line_height = self.geometry().line_height(font_size);
        let fallback_face = self
            .fallback_font_bytes
            .as_deref()
            .and_then(|bytes| Face::parse(bytes, 0).ok());
        let lines = match Face::parse(font_bytes, 0) {
            Ok(face) => text
                .split('\n')
                .flat_map(|para| {
                    wrap_paragraph_with_fallback(
                        para,
                        &face,
                        fallback_face.as_ref(),
                        font_size,
                        width,
                    )
                })
                .collect(),
            Err(_) => text.lines().map(str::to_owned).collect(),
        };
        MeasuredParagraph { lines, line_height }
    }

    /// Draws a body line as primary/fallback font runs while preserving the
    /// measured advance of each run. Missing glyphs remain in the primary font
    /// when the fallback is absent or also lacks the character.
    fn add_body_text_with_fallback(
        &mut self,
        page_num: u32,
        text: &str,
        position: [f32; 2],
        font_size: f32,
    ) -> Result<()> {
        let Some(fallback_font) = self.fallback_font else {
            return self.inner.page(page_num)?.add_text(
                text,
                self.body_font,
                position,
                font_size,
                [0.0, 0.0, 0.0],
            );
        };

        let body_bytes = self.body_font_bytes.clone();
        let fallback_bytes = self.fallback_font_bytes.clone().unwrap_or_default();
        let body_face = Face::parse(&body_bytes, 0).ok();
        let fallback_face = Face::parse(&fallback_bytes, 0).ok();
        let can_select_fallback = body_face.is_some() && fallback_face.is_some();

        let runs = if can_select_fallback {
            split_missing_glyph_runs(text, body_face.as_ref(), fallback_face.as_ref())
        } else {
            vec![(text.to_owned(), false)]
        };

        let mut x = position[0];
        let mut page = self.inner.page(page_num)?;
        for (run, use_fallback) in runs {
            let (font, face) = if use_fallback {
                (fallback_font, fallback_face.as_ref())
            } else {
                (self.body_font, body_face.as_ref())
            };
            page.add_text(&run, font, [x, position[1]], font_size, [0.0, 0.0, 0.0])?;
            if let Some(face) = face {
                x += glyph_advance_pt(face, run.chars().next().unwrap(), font_size)
                    .unwrap_or(font_size * 0.5);
                for ch in run.chars().skip(1) {
                    x += glyph_advance_pt(face, ch, font_size).unwrap_or(font_size * 0.5);
                }
            } else {
                x += run.chars().count() as f32 * font_size * 0.5;
            }
        }
        Ok(())
    }

    // ── Pagination ──────────────────────────────────────────────────────────

    /// Ensures at least `height` points of vertical space remain on the current page.
    /// If not, appends a new blank page and resets `content_y` to 0.
    /// Returns `Error::InvalidInput` if `max_pages` would be exceeded.
    fn ensure_space(&mut self, height: f32) -> Result<()> {
        let geometry = self.geometry();
        if self.content_y > 0.0 && self.content_y + height > geometry.content_height() + 0.1 {
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
        let geometry = self.geometry();
        let line_h = geometry.line_height(font_size);
        let font_bytes = self
            .heading_font_bytes
            .as_deref()
            .unwrap_or(&self.body_font_bytes);
        let lines = self.measure_lines(text, font_size, geometry.content_width(), font_bytes);

        // Keep pre-heading spacing + the full block together on one page when it fits.
        // An unusually tall heading still needs to be split rather than overflowing
        // below the page when its wrapped text exceeds the content area.
        // Compute spacing BEFORE ensure_space so that the heading is not orphaned at the
        // bottom of a page with only its spacing above it.
        let block_h = lines.len() as f32 * line_h;
        let pre_spacing = if self.content_y > 0.0 {
            self.options.paragraph_spacing * 1.5
        } else {
            0.0
        };
        let keep_together = pre_spacing + block_h <= geometry.content_height() + 0.1;
        let keep_with_next = self.options.keep_headings_with_next
            && keep_together
            && pre_spacing + block_h + line_h <= geometry.content_height() + 0.1;
        if keep_with_next {
            self.ensure_space(pre_spacing + block_h + line_h)?;
        } else if keep_together {
            self.ensure_space(pre_spacing + block_h)?;
        } else {
            self.ensure_space(pre_spacing + line_h)?;
        }
        // After a potential page break content_y resets to 0; only add spacing when still
        // on the same page (content_y > 0 means we didn't just start a fresh page).
        if self.content_y > 0.0 {
            self.content_y += pre_spacing;
        }

        // Record a bookmark anchored at the top of this heading block (before rendering).
        if self.options.auto_bookmarks {
            let bm_y = geometry.top_y(self.content_y);
            let bm_page = self.current_page;
            self.outline_entries
                .push((text.to_owned(), bm_page, bm_y, level as u8));
        }

        let x = self.options.margins.left;
        let font = self.heading_font.unwrap_or(self.body_font);
        for line in &lines {
            self.ensure_space(line_h)?;
            let current_page = self.current_page;
            let y = geometry.baseline_y(self.content_y, font_size);
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

    /// Appends a centered raster figure as a flow block.
    ///
    /// `width` and `height` are PDF points. PNG and JPEG bytes are accepted.
    /// When [`FlowOptions::keep_figures_with_next`] is enabled, the figure is
    /// moved to the next page if reserving one following body line would
    /// otherwise split the figure from its context.
    pub fn push_figure(&mut self, image_bytes: &[u8], width: f32, height: f32) -> Result<()> {
        if image_bytes.is_empty() {
            return Err(crate::Error::InvalidInput(
                "figure image bytes must not be empty".into(),
            ));
        }
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(crate::Error::InvalidInput(
                "figure dimensions must be finite and positive".into(),
            ));
        }

        let geometry = self.geometry();
        if width > geometry.content_width() + 0.1 {
            return Err(crate::Error::InvalidInput(
                "figure width exceeds the flow content width".into(),
            ));
        }
        if height > geometry.content_height() + 0.1 {
            return Err(crate::Error::InvalidInput(
                "figure height exceeds the flow content height".into(),
            ));
        }

        let pre_spacing = if self.content_y > 0.0 {
            self.options.paragraph_spacing
        } else {
            0.0
        };
        let following_line = geometry.line_height(self.options.body_font_size);
        let keep_with_next = self.options.keep_figures_with_next;
        let reservation = if keep_with_next { following_line } else { 0.0 };
        let required = pre_spacing + height + self.options.paragraph_spacing + reservation;
        if required <= geometry.content_height() + 0.1 {
            self.ensure_space(required)?;
        } else {
            self.ensure_space(pre_spacing + height + self.options.paragraph_spacing)?;
        }
        if self.content_y > 0.0 {
            self.content_y += pre_spacing;
        }

        let x = self.options.margins.left + (geometry.content_width() - width) * 0.5;
        let y = geometry.top_y(self.content_y) - height;
        self.inner
            .page(self.current_page)?
            .add_image(image_bytes, [x, y, width, height])?;
        self.content_y += height + self.options.paragraph_spacing;
        Ok(())
    }

    /// Appends a body-text paragraph to the document, with automatic word wrapping.
    ///
    /// CJK text breaks at any character; Latin text breaks at word boundaries.
    /// Newlines (`\n`) in `text` produce explicit line breaks.
    pub fn push_paragraph(&mut self, text: &str) -> Result<()> {
        self.push_paragraph_with_spacing(text, self.options.paragraph_spacing)
    }

    /// Appends a body-text paragraph with explicit trailing spacing in points.
    ///
    /// This overrides [`FlowOptions::paragraph_spacing`] for this paragraph only.
    /// The value must be finite and non-negative.
    pub fn push_paragraph_with_spacing(&mut self, text: &str, spacing: f32) -> Result<()> {
        if !spacing.is_finite() || spacing < 0.0 {
            return Err(crate::Error::InvalidInput(
                "paragraph spacing must be finite and non-negative".into(),
            ));
        }
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }

        let font_size = self.options.body_font_size;
        let geometry = self.geometry();
        let layout = self.measure_paragraph(
            text,
            font_size,
            geometry.content_width(),
            &self.body_font_bytes,
        );
        let line_h = layout.line_height;
        let lines = &layout.lines;

        // Keep a configurable minimum number of lines together. This prevents a
        // paragraph from leaving a short orphan at the bottom of a page.
        let keep_lines = self.options.paragraph_min_lines.max(1).min(lines.len());
        if lines.len() >= keep_lines && self.content_y > 0.0 {
            self.ensure_space(line_h * keep_lines as f32)?;
        }

        for (line_index, line) in lines.iter().enumerate() {
            let remaining = lines.len() - line_index;
            if line_index > 0
                && remaining <= keep_lines
                && self.content_y > 0.0
                && self.content_y + remaining as f32 * line_h > geometry.content_height() + 0.1
            {
                self.push_page_break()?;
            }
            self.ensure_space(line_h)?;
            let current_page = self.current_page;
            let y = geometry.baseline_y(self.content_y, font_size);
            let x = self.body_text_x(line, font_size);
            self.add_body_text_with_fallback(current_page, line, [x, y], font_size)?;
            self.content_y += line_h;
        }

        self.content_y += spacing;
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
        let alignment = self.options.body_alignment;
        self.push_paragraph_styled_with_spacing_and_alignment(
            spans,
            alignment,
            self.options.paragraph_spacing,
        )
    }

    /// Appends a styled paragraph with explicit trailing spacing in points.
    ///
    /// This overrides [`FlowOptions::paragraph_spacing`] for this paragraph only.
    /// The value must be finite and non-negative.
    pub fn push_paragraph_styled_with_spacing(
        &mut self,
        spans: &[InlineSpan],
        spacing: f32,
    ) -> Result<()> {
        self.push_paragraph_styled_with_spacing_and_alignment(
            spans,
            self.options.body_alignment,
            spacing,
        )
    }

    /// Appends a styled paragraph using an explicit horizontal alignment.
    pub fn push_paragraph_styled_with_alignment(
        &mut self,
        spans: &[InlineSpan],
        alignment: FlowTextAlignment,
    ) -> Result<()> {
        self.push_paragraph_styled_with_spacing_and_alignment(
            spans,
            alignment,
            self.options.paragraph_spacing,
        )
    }

    fn push_paragraph_styled_with_spacing_and_alignment(
        &mut self,
        spans: &[InlineSpan],
        alignment: FlowTextAlignment,
        spacing: f32,
    ) -> Result<()> {
        if !spacing.is_finite() || spacing < 0.0 {
            return Err(crate::Error::InvalidInput(
                "paragraph spacing must be finite and non-negative".into(),
            ));
        }
        let non_empty: Vec<&InlineSpan> = spans.iter().filter(|s| !s.text.is_empty()).collect();
        if non_empty.is_empty() {
            return Ok(());
        }

        let font_size = self.options.body_font_size;
        let geometry = self.geometry();
        let line_h = geometry.line_height(font_size);
        let content_w = geometry.content_width();
        // Clone font bytes so the borrow doesn't conflict with ensure_space's &mut self.
        let font_bytes_owned = self.body_font_bytes.clone();
        let face: Option<Face<'_>> = Face::parse(&font_bytes_owned, 0).ok();
        let fallback_bytes_owned = self.fallback_font_bytes.clone().unwrap_or_default();
        let fallback_face: Option<Face<'_>> = Face::parse(&fallback_bytes_owned, 0).ok();
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
        let line_strings = self
            .measure_paragraph(&full_text, font_size, content_w, &font_bytes_owned)
            .lines;

        let keep_lines = self
            .options
            .paragraph_min_lines
            .max(1)
            .min(line_strings.len());
        if line_strings.len() >= keep_lines && self.content_y > 0.0 {
            self.ensure_space(line_h * keep_lines as f32)?;
        }

        let mut char_cursor = 0usize; // position in char_spans

        for (line_index, line_str) in line_strings.iter().enumerate() {
            let line_len = line_str.chars().count();

            let remaining = line_strings.len() - line_index;
            if line_index > 0
                && remaining <= keep_lines
                && self.content_y > 0.0
                && self.content_y + remaining as f32 * line_h > geometry.content_height() + 0.1
            {
                self.push_page_break()?;
            }

            self.ensure_space(line_h)?;
            let y = geometry.baseline_y(self.content_y, font_size);
            let mut x = self.body_text_x_with_alignment(line_str, font_size, alignment);
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

                for (font_run, use_fallback) in
                    split_missing_glyph_runs(&run_text, face.as_ref(), fallback_face.as_ref())
                {
                    let run_font = if use_fallback {
                        self.fallback_font.unwrap_or(font)
                    } else {
                        font
                    };
                    self.inner.page(current_page)?.add_text_styled(
                        &font_run,
                        run_font,
                        [x, y],
                        font_size,
                        span.color,
                        span.bold,
                        span.italic,
                    )?;

                    // Advance x using the same face that drew the run.
                    let run_face = if use_fallback {
                        fallback_face.as_ref()
                    } else {
                        face.as_ref()
                    };
                    if let Some(run_face) = run_face {
                        x += geometry.text_width(&font_run, run_face, font_size);
                    }
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

        self.content_y += spacing;
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

        let geometry = self.geometry();
        let content_w = geometry.content_width();
        let key_w = content_w * self.options.table_key_ratio;
        let val_w = content_w - key_w;
        let font_size = self.options.body_font_size;
        let line_h = geometry.line_height(font_size);
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
            let max_lines_per_page =
                ((geometry.content_height() - cell_pad * 2.0) / line_h).floor() as usize;
            if max_lines_per_page == 0 {
                return Err(crate::Error::InvalidInput(format!(
                    "table row {idx} cannot fit one line in the page content area"
                )));
            }

            // Split an oversized row into continuation chunks instead of drawing
            // below the page. Each chunk has its own top separator; the final
            // chunk receives the table's bottom border.
            let mut line_offset = 0usize;
            while line_offset < row_lines {
                let chunk_lines = (row_lines - line_offset).min(max_lines_per_page);
                let row_h = chunk_lines as f32 * line_h + cell_pad * 2.0;
                self.ensure_space(row_h)?;

                let row_top_y = geometry.top_y(self.content_y);
                self.content_y += row_h;
                let row_bot_y = geometry.top_y(self.content_y);
                let page_num = self.current_page;
                let font = self.body_font;
                let final_chunk = line_offset + chunk_lines >= row_lines;

                {
                    let mut page = self.inner.page(page_num)?;
                    page.add_line(
                        [x_left, row_top_y],
                        [x_right, row_top_y],
                        border_color,
                        border_lw,
                        1.0,
                    )?;

                    for (i, line) in key_lines
                        .iter()
                        .skip(line_offset)
                        .take(chunk_lines)
                        .enumerate()
                    {
                        let y = row_top_y - cell_pad - font_size - i as f32 * line_h;
                        page.add_text(
                            line,
                            font,
                            [x_left + cell_pad, y],
                            font_size,
                            [0.0, 0.0, 0.0],
                        )?;
                    }

                    for (i, line) in val_lines
                        .iter()
                        .skip(line_offset)
                        .take(chunk_lines)
                        .enumerate()
                    {
                        let y = row_top_y - cell_pad - font_size - i as f32 * line_h;
                        page.add_text(line, font, [x_val, y], font_size, [0.0, 0.0, 0.0])?;
                    }

                    page.add_line(
                        [x_divider, row_top_y],
                        [x_divider, row_bot_y],
                        border_color,
                        border_lw,
                        1.0,
                    )?;

                    if idx == last_idx && final_chunk {
                        page.add_line(
                            [x_left, row_bot_y],
                            [x_right, row_bot_y],
                            border_color,
                            border_lw,
                            1.0,
                        )?;
                    }
                }
                line_offset += chunk_lines;
            }
        }

        self.content_y += self.options.paragraph_spacing;
        Ok(())
    }

    /// Appends a table with an arbitrary number of columns.
    ///
    /// Rows must have the same number of cells. Cells wrap using the shared
    /// paragraph line breaker, and oversized rows continue on later pages.
    /// Header repetition, spans, and nested blocks are intentionally not part
    /// of this first table-engine contract.
    pub fn measure_table_widths(
        &self,
        rows: &[Vec<String>],
        options: &TableOptions,
    ) -> Result<TableWidthAllocation> {
        self.resolve_table_widths(rows, options)
    }

    fn resolve_table_widths(
        &self,
        rows: &[Vec<String>],
        options: &TableOptions,
    ) -> Result<TableWidthAllocation> {
        if rows.is_empty() {
            return Ok(TableWidthAllocation {
                widths: Vec::new(),
                content_width: self.geometry().content_width(),
            });
        }
        let columns = rows[0].len();
        if columns == 0 || rows.iter().any(|row| row.len() != columns) {
            return Err(crate::Error::InvalidInput(
                "table rows must have the same non-zero column count".into(),
            ));
        }
        if !options.cell_padding.is_finite() || options.cell_padding < 0.0 {
            return Err(crate::Error::InvalidInput(
                "table cell_padding must be finite and non-negative".into(),
            ));
        }
        if options
            .border_color
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0 || *value > 1.0)
            || !options.border_width.is_finite()
            || options.border_width < 0.0
        {
            return Err(crate::Error::InvalidInput(
                "table border color must be RGB in 0..=1 and width non-negative".into(),
            ));
        }
        let content_w = self.geometry().content_width();
        let font_size = self.options.body_font_size;
        let (widths, target) = match &options.column_widths {
            TableColumnWidths::Fixed(widths) => {
                if widths.len() != columns
                    || widths.iter().any(|w| !w.is_finite() || *w <= 0.0)
                    || widths.iter().sum::<f32>() > content_w + 0.1
                {
                    return Err(crate::Error::InvalidInput(
                        "fixed table widths must be positive, fit the content area, and match the column count".into(),
                    ));
                }
                (widths.clone(), widths.iter().sum())
            }
            TableColumnWidths::Fractions(fractions) => {
                if fractions.len() != columns
                    || fractions.iter().any(|w| !w.is_finite() || *w < 0.0)
                {
                    return Err(crate::Error::InvalidInput(
                        "fractional table widths must be finite, non-negative, and match the column count".into(),
                    ));
                }
                let total = fractions.iter().sum::<f32>();
                if total <= 0.0 {
                    return Err(crate::Error::InvalidInput(
                        "fractional table widths must contain a positive total".into(),
                    ));
                }
                (
                    fractions
                        .iter()
                        .map(|fraction| content_w * fraction / total)
                        .collect(),
                    content_w,
                )
            }
            TableColumnWidths::Intrinsic => {
                let face = Face::parse(&self.body_font_bytes, 0).map_err(|_| {
                    crate::Error::InvalidInput(
                        "body font cannot be measured for table sizing".into(),
                    )
                })?;
                let mut natural = vec![options.cell_padding * 2.0; columns];
                for row in rows {
                    for (column, cell) in row.iter().enumerate() {
                        for line in cell.lines() {
                            natural[column] = natural[column].max(
                                self.geometry().text_width(line, &face, font_size)
                                    + options.cell_padding * 2.0,
                            );
                        }
                    }
                }
                let total = natural.iter().sum::<f32>();
                if total > content_w {
                    let scale = content_w / total;
                    natural.iter_mut().for_each(|width| *width *= scale);
                }
                let target = natural.iter().sum();
                (natural, target)
            }
        };
        let widths = apply_table_width_constraints(
            widths,
            target,
            options.min_column_widths.as_deref(),
            options.max_column_widths.as_deref(),
            columns,
        )?;
        Ok(TableWidthAllocation {
            widths,
            content_width: content_w,
        })
    }

    pub fn push_table(&mut self, rows: &[Vec<String>], options: TableOptions) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let columns = rows[0].len();
        if columns == 0 || rows.iter().any(|row| row.len() != columns) {
            return Err(crate::Error::InvalidInput(
                "table rows must have the same non-zero column count".into(),
            ));
        }
        if options.header_rows > rows.len() {
            return Err(crate::Error::InvalidInput(
                "table header_rows cannot exceed the row count".into(),
            ));
        }
        if !options.cell_padding.is_finite() || options.cell_padding < 0.0 {
            return Err(crate::Error::InvalidInput(
                "table cell_padding must be finite and non-negative".into(),
            ));
        }

        let geometry = self.geometry();
        let allocation = self.resolve_table_widths(rows, &options)?;
        let font_size = self.options.body_font_size;
        let line_h = geometry.line_height(font_size);
        let widths = allocation.widths;

        let mut x_boundaries = Vec::with_capacity(columns + 1);
        x_boundaries.push(self.options.margins.left);
        for width in &widths {
            x_boundaries.push(x_boundaries.last().copied().unwrap() + *width);
        }
        let border_color = options.border_color;
        let border_lw = options.border_width;
        let max_lines_per_page =
            ((geometry.content_height() - options.cell_padding * 2.0) / line_h).floor() as usize;
        if max_lines_per_page == 0 {
            return Err(crate::Error::InvalidInput(
                "table row cannot fit one line in the page content area".into(),
            ));
        }

        let estimated_table_height = rows
            .iter()
            .map(|row| {
                let row_lines = row
                    .iter()
                    .enumerate()
                    .map(|(column, cell)| {
                        self.measure_lines(
                            cell.trim(),
                            font_size,
                            (widths[column] - options.cell_padding * 2.0).max(1.0),
                            &self.body_font_bytes,
                        )
                        .len()
                    })
                    .max()
                    .unwrap_or(1)
                    .max(1);
                row_lines as f32 * line_h + options.cell_padding * 2.0
            })
            .sum::<f32>();
        if options.keep_with_next
            && estimated_table_height <= geometry.content_height() + 0.1
            && estimated_table_height + self.options.paragraph_spacing + line_h
                <= geometry.content_height() + 0.1
            && self.content_y > 0.0
        {
            self.ensure_space(estimated_table_height + self.options.paragraph_spacing + line_h)?;
        }

        for (row_index, row) in rows.iter().enumerate() {
            let cell_lines: Vec<Vec<String>> = row
                .iter()
                .enumerate()
                .map(|(column, cell)| {
                    self.measure_lines(
                        cell.trim(),
                        font_size,
                        (widths[column] - options.cell_padding * 2.0).max(1.0),
                        &self.body_font_bytes,
                    )
                })
                .collect();
            let row_lines = cell_lines.iter().map(Vec::len).max().unwrap_or(1).max(1);
            let mut line_offset = 0;
            while line_offset < row_lines {
                let chunk_lines = (row_lines - line_offset).min(max_lines_per_page);
                let row_h = chunk_lines as f32 * line_h + options.cell_padding * 2.0;
                let previous_page = self.current_page;
                self.ensure_space(row_h)?;
                if self.current_page != previous_page
                    && options.header_rows > 0
                    && row_index >= options.header_rows
                {
                    let header_rows = rows[..options.header_rows].to_vec();
                    self.push_table(
                        &header_rows,
                        TableOptions {
                            column_widths: TableColumnWidths::Fixed(widths.clone()),
                            cell_padding: options.cell_padding,
                            header_rows: 0,
                            keep_with_next: false,
                            min_column_widths: None,
                            max_column_widths: None,
                            border_color: options.border_color,
                            border_width: options.border_width,
                        },
                    )?;
                    // A repeated header is part of this table, not a separate
                    // block. Remove the nested call's trailing block spacing.
                    self.content_y = (self.content_y - self.options.paragraph_spacing).max(0.0);
                }
                let row_top_y = geometry.top_y(self.content_y);
                self.content_y += row_h;
                let row_bottom_y = geometry.top_y(self.content_y);
                let page_num = self.current_page;
                let final_chunk = line_offset + chunk_lines >= row_lines;
                let mut page = self.inner.page(page_num)?;
                page.add_line(
                    [x_boundaries[0], row_top_y],
                    [*x_boundaries.last().unwrap(), row_top_y],
                    border_color,
                    border_lw,
                    1.0,
                )?;
                for boundary in &x_boundaries {
                    page.add_line(
                        [*boundary, row_top_y],
                        [*boundary, row_bottom_y],
                        border_color,
                        border_lw,
                        1.0,
                    )?;
                }
                for (column, lines) in cell_lines.iter().enumerate() {
                    let x = x_boundaries[column] + options.cell_padding;
                    for (line, text) in lines.iter().skip(line_offset).take(chunk_lines).enumerate()
                    {
                        let y = row_top_y - options.cell_padding - font_size - line as f32 * line_h;
                        page.add_text(text, self.body_font, [x, y], font_size, [0.0, 0.0, 0.0])?;
                    }
                }
                if row_index + 1 == rows.len() && final_chunk {
                    page.add_line(
                        [x_boundaries[0], row_bottom_y],
                        [*x_boundaries.last().unwrap(), row_bottom_y],
                        border_color,
                        border_lw,
                        1.0,
                    )?;
                }
                line_offset += chunk_lines;
            }
        }
        self.content_y += self.options.paragraph_spacing;
        Ok(())
    }

    /// Appends a table whose cells may span multiple adjacent columns.
    ///
    /// Every row must occupy the same total number of columns. This first span
    /// implementation supports horizontal and vertical spans; nested blocks
    /// remain a separate layout feature.
    pub fn push_table_cells(
        &mut self,
        rows: &[Vec<FlowTableCell>],
        options: TableOptions,
    ) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        if options.header_rows > rows.len() {
            return Err(crate::Error::InvalidInput(
                "table header_rows cannot exceed the row count".into(),
            ));
        }
        let column_count = rows[0]
            .iter()
            .try_fold(0usize, |sum, cell| sum.checked_add(cell.colspan))
            .ok_or_else(|| crate::Error::InvalidInput("table column count overflow".into()))?;
        let has_rowspans = rows.iter().flatten().any(|cell| cell.rowspan > 1);
        if column_count == 0
            || rows.iter().any(|row| {
                row.is_empty()
                    || row
                        .iter()
                        .any(|cell| cell.colspan == 0 || cell.rowspan == 0)
                    || row.iter().any(|cell| {
                        cell.padding
                            .is_some_and(|padding| !padding.is_finite() || padding < 0.0)
                    })
                    || (!has_rowspans
                        && row.iter().map(|cell| cell.colspan).sum::<usize>() != column_count)
            })
        {
            return Err(crate::Error::InvalidInput(
                "spanned table rows must have the same non-zero column count".into(),
            ));
        }

        if has_rowspans {
            return self.push_table_cells_with_rowspans(rows, options);
        }

        let shadow_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                let mut shadow = vec![String::new(); column_count];
                let mut column = 0;
                for cell in row {
                    shadow[column] = cell.text.clone();
                    column += cell.colspan;
                }
                shadow
            })
            .collect();
        let geometry = self.geometry();
        let allocation = self.resolve_table_widths(&shadow_rows, &options)?;
        let widths = allocation.widths;
        let font_size = self.options.body_font_size;
        let line_h = geometry.line_height(font_size);
        let mut x_boundaries = Vec::with_capacity(column_count + 1);
        x_boundaries.push(self.options.margins.left);
        for width in &widths {
            x_boundaries.push(x_boundaries.last().copied().unwrap() + *width);
        }
        let border_color = options.border_color;
        let border_lw = options.border_width;
        let font_bytes_for_measurement = self.body_font_bytes.clone();
        let face = Face::parse(&font_bytes_for_measurement, 0).ok();
        let max_lines_per_page =
            ((geometry.content_height() - options.cell_padding * 2.0) / line_h).floor() as usize;
        if max_lines_per_page == 0 {
            return Err(crate::Error::InvalidInput(
                "table row cannot fit one line in the page content area".into(),
            ));
        }

        let estimated_table_height = rows
            .iter()
            .map(|row| {
                let mut column = 0usize;
                let row_lines = row
                    .iter()
                    .map(|cell| {
                        let start = column;
                        let end = column + cell.colspan;
                        column = end;
                        let padding = cell.padding.unwrap_or(options.cell_padding);
                        let width =
                            (x_boundaries[end] - x_boundaries[start] - padding * 2.0).max(1.0);
                        self.measure_lines(
                            cell.text.trim(),
                            font_size,
                            width,
                            &self.body_font_bytes,
                        )
                        .len()
                    })
                    .max()
                    .unwrap_or(1)
                    .max(1);
                row_lines as f32 * line_h + options.cell_padding * 2.0
            })
            .sum::<f32>();
        if options.keep_with_next
            && estimated_table_height <= geometry.content_height() + 0.1
            && estimated_table_height + self.options.paragraph_spacing + line_h
                <= geometry.content_height() + 0.1
            && self.content_y > 0.0
        {
            self.ensure_space(estimated_table_height + self.options.paragraph_spacing + line_h)?;
        }

        for (row_index, row) in rows.iter().enumerate() {
            let mut column = 0usize;
            let cell_lines: Vec<(usize, usize, f32, TableCellAlignment, Vec<String>)> = row
                .iter()
                .map(|cell| {
                    let start = column;
                    let end = column + cell.colspan;
                    column = end;
                    let padding = cell.padding.unwrap_or(options.cell_padding);
                    let width = (x_boundaries[end] - x_boundaries[start] - padding * 2.0).max(1.0);
                    (
                        start,
                        end,
                        padding,
                        cell.alignment,
                        self.measure_lines(
                            cell.text.trim(),
                            font_size,
                            width,
                            &self.body_font_bytes,
                        ),
                    )
                })
                .collect();
            let row_lines = cell_lines
                .iter()
                .map(|(_, _, _, _, lines)| lines.len())
                .max()
                .unwrap_or(1)
                .max(1);
            let mut line_offset = 0;
            while line_offset < row_lines {
                let chunk_lines = (row_lines - line_offset).min(max_lines_per_page);
                let row_h = chunk_lines as f32 * line_h + options.cell_padding * 2.0;
                let previous_page = self.current_page;
                self.ensure_space(row_h)?;
                if self.current_page != previous_page
                    && options.header_rows > 0
                    && row_index >= options.header_rows
                {
                    let header_rows = rows[..options.header_rows].to_vec();
                    self.push_table_cells(
                        &header_rows,
                        TableOptions {
                            column_widths: TableColumnWidths::Fixed(widths.clone()),
                            cell_padding: options.cell_padding,
                            header_rows: 0,
                            keep_with_next: false,
                            min_column_widths: None,
                            max_column_widths: None,
                            border_color: options.border_color,
                            border_width: options.border_width,
                        },
                    )?;
                    self.content_y = (self.content_y - self.options.paragraph_spacing).max(0.0);
                }
                let row_top_y = geometry.top_y(self.content_y);
                self.content_y += row_h;
                let row_bottom_y = geometry.top_y(self.content_y);
                let page_num = self.current_page;
                let final_chunk = line_offset + chunk_lines >= row_lines;
                let mut page = self.inner.page(page_num)?;
                page.add_line(
                    [x_boundaries[0], row_top_y],
                    [*x_boundaries.last().unwrap(), row_top_y],
                    border_color,
                    border_lw,
                    1.0,
                )?;
                for (start, end, padding, alignment, lines) in &cell_lines {
                    page.add_line(
                        [x_boundaries[*start], row_top_y],
                        [x_boundaries[*start], row_bottom_y],
                        border_color,
                        border_lw,
                        1.0,
                    )?;
                    page.add_line(
                        [x_boundaries[*end], row_top_y],
                        [x_boundaries[*end], row_bottom_y],
                        border_color,
                        border_lw,
                        1.0,
                    )?;
                    for (line, text) in lines.iter().skip(line_offset).take(chunk_lines).enumerate()
                    {
                        let y = row_top_y - *padding - font_size - line as f32 * line_h;
                        let cell_width = x_boundaries[*end] - x_boundaries[*start];
                        let inner_width = (cell_width - *padding * 2.0).max(0.0);
                        let text_width = face
                            .as_ref()
                            .map(|face| geometry.text_width(text, face, font_size))
                            .unwrap_or(text.chars().count() as f32 * font_size * 0.5);
                        let align_offset = match alignment {
                            TableCellAlignment::Left => 0.0,
                            TableCellAlignment::Center => (inner_width - text_width).max(0.0) / 2.0,
                            TableCellAlignment::Right => (inner_width - text_width).max(0.0),
                        };
                        page.add_text(
                            text,
                            self.body_font,
                            [x_boundaries[*start] + *padding + align_offset, y],
                            font_size,
                            [0.0, 0.0, 0.0],
                        )?;
                    }
                }
                if row_index + 1 == rows.len() && final_chunk {
                    page.add_line(
                        [x_boundaries[0], row_bottom_y],
                        [*x_boundaries.last().unwrap(), row_bottom_y],
                        border_color,
                        border_lw,
                        1.0,
                    )?;
                }
                line_offset += chunk_lines;
            }
        }
        self.content_y += self.options.paragraph_spacing;
        Ok(())
    }

    fn push_table_cells_with_rowspans(
        &mut self,
        rows: &[Vec<FlowTableCell>],
        options: TableOptions,
    ) -> Result<()> {
        #[derive(Clone)]
        struct PlacedCell {
            cell: FlowTableCell,
            row: usize,
            column: usize,
        }

        let column_count = rows[0].iter().map(|cell| cell.colspan).sum::<usize>();
        let mut occupied = vec![vec![false; column_count]; rows.len()];
        let mut placed = Vec::new();
        for (row_index, row) in rows.iter().enumerate() {
            let mut cursor = 0usize;
            for cell in row {
                while cursor < column_count && occupied[row_index][cursor] {
                    cursor += 1;
                }
                let col_end = cursor.checked_add(cell.colspan).ok_or_else(|| {
                    crate::Error::InvalidInput("table column span overflow".into())
                })?;
                let row_end = row_index
                    .checked_add(cell.rowspan)
                    .ok_or_else(|| crate::Error::InvalidInput("table row span overflow".into()))?;
                if col_end > column_count || row_end > rows.len() {
                    return Err(crate::Error::InvalidInput(
                        "table cell span exceeds the table grid".into(),
                    ));
                }
                for row_slot in occupied.iter().take(row_end).skip(row_index) {
                    if row_slot[cursor..col_end].iter().any(|occupied| *occupied) {
                        return Err(crate::Error::InvalidInput(
                            "table cell spans overlap another cell".into(),
                        ));
                    }
                }
                for row_slot in occupied.iter_mut().take(row_end).skip(row_index) {
                    for occupied in &mut row_slot[cursor..col_end] {
                        *occupied = true;
                    }
                }
                placed.push(PlacedCell {
                    cell: cell.clone(),
                    row: row_index,
                    column: cursor,
                });
                cursor = col_end;
            }
        }
        if occupied
            .iter()
            .any(|row| row.iter().any(|occupied| !occupied))
        {
            return Err(crate::Error::InvalidInput(
                "table cell spans leave an uncovered grid position".into(),
            ));
        }
        if options.header_rows > 0 {
            return Err(crate::Error::InvalidInput(
                "vertical-span tables do not support repeated header rows yet".into(),
            ));
        }

        let shadow_rows: Vec<Vec<String>> = (0..rows.len())
            .map(|row| {
                let mut shadow = vec![String::new(); column_count];
                for cell in placed.iter().filter(|cell| cell.row == row) {
                    shadow[cell.column] = cell.cell.text.clone();
                }
                shadow
            })
            .collect();
        let geometry = self.geometry();
        let allocation = self.resolve_table_widths(&shadow_rows, &options)?;
        let widths = allocation.widths;
        let font_size = self.options.body_font_size;
        let line_h = geometry.line_height(font_size);
        let mut x_boundaries = Vec::with_capacity(column_count + 1);
        x_boundaries.push(self.options.margins.left);
        for width in &widths {
            x_boundaries.push(x_boundaries.last().copied().unwrap() + *width);
        }
        let font_bytes_owned = self.body_font_bytes.clone();
        let face = Face::parse(&font_bytes_owned, 0).ok();
        let mut row_heights = vec![line_h + options.cell_padding * 2.0; rows.len()];
        let mut measured = Vec::new();
        for cell in &placed {
            let end = cell.column + cell.cell.colspan;
            let padding = cell.cell.padding.unwrap_or(options.cell_padding);
            let width = (x_boundaries[end] - x_boundaries[cell.column] - padding * 2.0).max(1.0);
            let lines =
                self.measure_lines(cell.cell.text.trim(), font_size, width, &font_bytes_owned);
            let needed = lines.len().max(1) as f32 * line_h + padding * 2.0;
            let span_height: f32 = row_heights[cell.row..cell.row + cell.cell.rowspan]
                .iter()
                .sum();
            if cell.cell.rowspan == 1 {
                row_heights[cell.row] = row_heights[cell.row].max(needed);
            } else if span_height < needed {
                row_heights[cell.row + cell.cell.rowspan - 1] += needed - span_height;
            }
            measured.push((cell.clone(), padding, lines));
        }

        let table_height: f32 = row_heights.iter().sum();
        if table_height > geometry.content_height() + 0.1 {
            return Err(crate::Error::InvalidInput(
                "vertical-span table exceeds one page; split it before layout".into(),
            ));
        }
        let pre_spacing = if self.content_y > 0.0 {
            self.options.paragraph_spacing
        } else {
            0.0
        };
        let following_line = if options.keep_with_next { line_h } else { 0.0 };
        self.ensure_space(
            pre_spacing + table_height + self.options.paragraph_spacing + following_line,
        )?;
        if self.content_y > 0.0 {
            self.content_y += pre_spacing;
        }
        let table_top = geometry.top_y(self.content_y);
        self.content_y += table_height;
        let mut row_tops = Vec::with_capacity(rows.len() + 1);
        row_tops.push(table_top);
        for height in &row_heights {
            row_tops.push(row_tops.last().copied().unwrap() - *height);
        }
        let mut page = self.inner.page(self.current_page)?;
        for (boundary, y) in row_tops.iter().copied().enumerate() {
            let mut start = 0usize;
            while start < column_count {
                let covered = if boundary == rows.len() {
                    false
                } else {
                    placed.iter().any(|cell| {
                        cell.row < boundary
                            && boundary < cell.row + cell.cell.rowspan
                            && cell.column <= start
                            && start < cell.column + cell.cell.colspan
                    })
                };
                let mut end = start + 1;
                while end < column_count {
                    let next_covered = if boundary == rows.len() {
                        false
                    } else {
                        placed.iter().any(|cell| {
                            cell.row < boundary
                                && boundary < cell.row + cell.cell.rowspan
                                && cell.column <= end
                                && end < cell.column + cell.cell.colspan
                        })
                    };
                    if next_covered != covered {
                        break;
                    }
                    end += 1;
                }
                if !covered {
                    page.add_line(
                        [x_boundaries[start], y],
                        [x_boundaries[end], y],
                        options.border_color,
                        options.border_width,
                        1.0,
                    )?;
                }
                start = end;
            }
        }
        for cell in &placed {
            let x0 = x_boundaries[cell.column];
            let x1 = x_boundaries[cell.column + cell.cell.colspan];
            let y0 = row_tops[cell.row];
            let y1 = row_tops[cell.row + cell.cell.rowspan];
            page.add_line(
                [x0, y0],
                [x0, y1],
                options.border_color,
                options.border_width,
                1.0,
            )?;
            page.add_line(
                [x1, y0],
                [x1, y1],
                options.border_color,
                options.border_width,
                1.0,
            )?;
        }
        for (cell, padding, lines) in measured {
            let x0 = x_boundaries[cell.column];
            let x1 = x_boundaries[cell.column + cell.cell.colspan];
            let y0 = row_tops[cell.row];
            let inner_width = (x1 - x0 - padding * 2.0).max(0.0);
            for (line_index, line) in lines.iter().enumerate() {
                let text_width = face
                    .as_ref()
                    .map(|face| geometry.text_width(line, face, font_size))
                    .unwrap_or(line.chars().count() as f32 * font_size * 0.5);
                let align_offset = match cell.cell.alignment {
                    TableCellAlignment::Left => 0.0,
                    TableCellAlignment::Center => (inner_width - text_width).max(0.0) / 2.0,
                    TableCellAlignment::Right => (inner_width - text_width).max(0.0),
                };
                page.add_text(
                    line,
                    self.body_font,
                    [
                        x0 + padding + align_offset,
                        y0 - padding - font_size - line_index as f32 * line_h,
                    ],
                    font_size,
                    [0.0, 0.0, 0.0],
                )?;
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
        let geometry = self.geometry();
        let line_h = geometry.line_height(font_size);
        let font_bytes = self
            .code_font_bytes
            .as_deref()
            .unwrap_or(&self.body_font_bytes);
        let lines = self.measure_lines(text, font_size, geometry.content_width(), font_bytes);

        let block_h = lines.len() as f32 * line_h;
        let pre_spacing = if self.content_y > 0.0 {
            self.options.paragraph_spacing
        } else {
            0.0
        };
        // A code block may be taller than a page. Reserve the whole block only
        // when it fits; otherwise each line participates in normal pagination.
        let keep_together = pre_spacing + block_h <= geometry.content_height() + 0.1;
        if keep_together {
            self.ensure_space(pre_spacing + block_h)?;
        } else {
            self.ensure_space(pre_spacing + line_h)?;
        }
        if self.content_y > 0.0 {
            self.content_y += pre_spacing;
        }

        let x = self.options.margins.left;
        let font = self.code_font.unwrap_or(self.body_font);
        if keep_together && let Some(bg_color) = self.options.code_background {
            let right_x = self.options.page_size.0 - self.options.margins.right;
            let padding = 2.0;
            let y_top = geometry.top_y(self.content_y);
            self.inner.page(self.current_page)?.add_rect(
                [
                    x - padding,
                    y_top - block_h - padding,
                    right_x - x + 2.0 * padding,
                    block_h + 2.0 * padding,
                ],
                bg_color,
                1.0,
            )?;
        }

        // Render the text lines. For a split block, draw one background strip per
        // line so the background remains bounded to the page it occupies.
        for line in &lines {
            self.ensure_space(line_h)?;
            let current_page = self.current_page;
            let y_top = geometry.top_y(self.content_y);

            if !keep_together && let Some(bg_color) = self.options.code_background {
                let right_x = self.options.page_size.0 - self.options.margins.right;
                let padding = 2.0;
                self.inner.page(current_page)?.add_rect(
                    [
                        x - padding,
                        y_top - line_h - padding,
                        right_x - x + 2.0 * padding,
                        line_h + 2.0 * padding,
                    ],
                    bg_color,
                    1.0,
                )?;
            }

            let y = geometry.baseline_y(self.content_y, font_size);
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
fn hf_measure(
    geometry: GeometryPlanner,
    face: Option<&Face<'_>>,
    text: &str,
    font_size: f32,
) -> f32 {
    match face {
        Some(f) => geometry.text_width(text, f, font_size),
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
    let geometry = GeometryPlanner {
        page_size,
        margins,
        line_height_factor: 1.0,
        baseline_offset: 0.0,
    };
    let margin_left = geometry.margins.left;
    let margin_right = geometry.margins.right;
    let content_w = geometry.content_width();

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
        let w = hf_measure(geometry, face, &text, fs);
        let x = margin_left + (content_w - w) / 2.0;
        inner
            .page(page_num)?
            .add_text(&text, font, [x, y], fs, color)?;
    }
    if let Some(ref tmpl) = hf.right {
        let text = hf_subst(tmpl, page_num, total_pages);
        let w = hf_measure(geometry, face, &text, fs);
        let x = page_size.0 - margin_right - w;
        inner
            .page(page_num)?
            .add_text(&text, font, [x, y], fs, color)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_planner_is_single_source_for_page_coordinates() {
        let options = FlowOptions {
            page_size: (200.0, 300.0),
            margins: Margins {
                top: 10.0,
                right: 20.0,
                bottom: 30.0,
                left: 40.0,
            },
            line_height_factor: 1.5,
            ..FlowOptions::default()
        };
        let geometry = GeometryPlanner::new(&options);

        assert_eq!(geometry.content_width(), 140.0);
        assert_eq!(geometry.content_height(), 260.0);
        assert_eq!(geometry.line_height(10.0), 15.0);
        assert_eq!(geometry.top_y(12.0), 278.0);
        assert_eq!(geometry.baseline_y(12.0, 10.0), 268.0);
    }
}
