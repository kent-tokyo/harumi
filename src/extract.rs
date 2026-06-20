use std::collections::{BTreeMap, HashMap};

use lopdf::{Dictionary, Object, ObjectId};

use crate::error::Result;

/// A text fragment extracted from a page content stream.
///
/// Returned by [`crate::Document::extract_text_runs`].
///
/// ## Bounding box
///
/// The fields `x`, `y`, `width`, `height` form the text run's bounding box:
///
/// ```text
/// y + height  ┌──────────────────────────────┐
///             │   ascenders (cap/diacritic)  │
/// y (baseline)├──────────────────────────────│ ← text sits on this line
///             │   descenders (g, p, y…)      │
/// y - height×D└──────────────────────────────┘
///             x                    x + width
/// ```
///
/// * `(x, y)` — baseline origin in PDF points (bottom-left page origin).
/// * `width`  — advance-width sum; actual ink may be slightly narrower.
/// * `height` — full em height (`font_size`); actual ascent/descent split
///   depends on the typeface. For a typical Latin font, the cap top is
///   approximately `y + 0.7 * font_size`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct TextFragment {
    /// Decoded Unicode text.
    pub text: String,
    /// X coordinate of the text baseline in PDF points (origin: bottom-left of page).
    pub x: f32,
    /// Y coordinate of the text baseline in PDF points (origin: bottom-left of page).
    pub y: f32,
    /// Estimated text width in PDF points, computed from the font's advance widths.
    pub width: f32,
    /// Approximate text height in PDF points (equals `font_size`, the full em height).
    ///
    /// The baseline is at `y`; the em square extends from approximately
    /// `y - descender_fraction * font_size` to `y + ascender_fraction * font_size`.
    pub height: f32,
    /// Font size in PDF points.
    pub font_size: f32,
    /// PDF resource name of the font at this position (e.g. `"HR0"`, `"F1"`).
    pub font_name: String,
    /// RGB fill color at this position, each component in `0.0..=1.0`.
    /// Defaults to black `[0.0, 0.0, 0.0]` when no color operator precedes the text.
    pub color: [f32; 3],
    /// `true` if the text render mode is 3 (invisible / OCR search layer).
    pub invisible: bool,
    /// `true` when the font name indicates a bold weight
    /// (keywords: Bold, Heavy, Black, Semibold, Demibold, Extrabold).
    pub is_bold: bool,
    /// `true` when the font name indicates italic or oblique style
    /// (keywords: Italic, Oblique, Slanted).
    pub is_italic: bool,
    /// Font family name derived from the PostScript `/BaseFont` entry,
    /// with subset prefix (e.g. `"ABCDEF+"`) and style suffixes stripped.
    /// Empty string when no `/BaseFont` is present in the font dictionary.
    pub font_family: String,
    /// Full PostScript base font name (subset prefix stripped).
    /// Examples: `"Helvetica-BoldOblique"`, `"NotoSansJP-Regular"`.
    /// Empty string when no `/BaseFont` is present in the font dictionary.
    pub base_font: String,
    /// Advance width of the space glyph (U+0020) in PDF points at this fragment's font size.
    /// Zero when the font has no space glyph mapped in its ToUnicode table.
    ///
    /// Callers can compare `next.x - (prev.x + prev.width)` against `prev.space_advance`
    /// to decide whether the gap between two adjacent fragments represents a word space
    /// (gap ≥ space_advance × threshold) or tight character spacing (no space needed).
    pub space_advance: f32,
    /// Raw font size from the `Tf` operator, before any `Tm` matrix scaling.
    /// Equals `font_size` when the active text matrix is a pure translation (scale = 1).
    pub tf_font_size: f32,
    /// Y-axis scale factor from the most recent `Tm` matrix: `√(c² + d²)`.
    /// `font_size ≈ tf_font_size × tm_y_scale` (CTM scaling is not included here).
    /// Useful when the PDF uses a pattern like `1 Tf  9 0 0 9 x y Tm` where `Tf`
    /// emits size 1 and the actual visual size comes entirely from the Tm matrix.
    pub tm_y_scale: f32,
    /// Zero-based index into the page `/Contents` array identifying which content
    /// stream produced this fragment.  `None` for fragments extracted from Form
    /// XObjects or whenever source tracking is unavailable.
    ///
    /// Use together with [`source_op_start`](Self::source_op_start) and
    /// [`source_op_end`](Self::source_op_end) to locate the originating `Tj`/`TJ`
    /// operator for [`PageHandle::replace_text_fragments`].
    pub source_stream: Option<usize>,
    /// Byte offset of the first byte of the `Tj` or `TJ` keyword in the
    /// decompressed content stream identified by `source_stream`.
    pub source_op_start: Option<usize>,
    /// Byte offset one past the last byte of the `Tj`/`TJ` keyword
    /// (i.e. `source_op_start + 2` for both operators).
    ///
    /// ## When `source_op_end` is `None`
    ///
    /// This field is `None` in two situations:
    ///
    /// 1. **Per-character encoding** — the PDF encodes each character with its own
    ///    `Td`/`Tj` pair (common in some Japanese generators).  Because there is no
    ///    single operator to suppress, batch suppression silently skips these
    ///    fragments (the returned count `n` is not incremented).
    ///
    /// 2. **Unsupported XObject nesting** — the fragment came from a deeply-nested
    ///    Form XObject whose stream could not be located during extraction.
    ///
    /// Use [`PageHandle::can_suppress_fragment`] to detect unsuppressible fragments
    /// before calling [`PageHandle::replace_text_fragments_batch`] or
    /// [`PageHandle::replace_text_fragments_batch_opts`].
    /// For per-character PDFs, fall back to an **overlay approach**: draw a cover
    /// rectangle with [`PageHandle::add_rect`] and place translated text on top
    /// with [`PageHandle::add_text`].
    pub source_op_end: Option<usize>,
    /// `lopdf` `ObjectId` `(object_number, generation_number)` of the Form XObject
    /// stream that produced this fragment.  `None` for fragments extracted from page
    /// content streams.  When set, `source_stream` is `None`.
    ///
    /// Pass to [`PageHandle::replace_text_fragments`] alongside `source_op_start` /
    /// `source_op_end` to suppress this fragment's originating operator inside the
    /// XObject stream.
    pub source_xobject: Option<(u32, u16)>,
    /// X coordinate of the most recent `Tm` operator in the enclosing BT block,
    /// in PDF points (page space, same coordinate system as [`x`](Self::x)).
    ///
    /// Unlike `x`, this value **does not advance** after `Tj`/`TJ` rendering or
    /// `Td`/`TD` relative moves — it is updated only when a new `Tm` sets an
    /// absolute text position.
    ///
    /// **Use case — column alignment:** PDFs that lay out vertically-aligned labels
    /// using a single BT block with `Td 0 -line_height` between rows accumulate
    /// glyph advances in `x`, causing row-by-row x drift.  All fragments in the
    /// same BT block share the same `tm_origin_x`, which is the intended left-margin
    /// anchor.  Use `tm_origin_x` instead of `x` when placing replacement text for
    /// column-aligned content.
    ///
    /// `None` when no `Tm` operator preceded the first `Tj` in this BT block.
    pub tm_origin_x: Option<f32>,
    /// Y coordinate of the most recent `Tm` operator.
    /// Paired with [`tm_origin_x`](Self::tm_origin_x); see its documentation.
    pub tm_origin_y: Option<f32>,
    /// X-scale from the most recent `Tm` matrix: √(a² + b²), where `a b c d e f Tm`.
    ///
    /// For axis-aligned Tm (no rotation) this equals the horizontal scaling factor applied
    /// to glyph advances and `Td` offsets.  Combined with `tm_origin_x`, it lets callers
    /// recover the logical column position of a fragment even when the PDF uses `font_size=1`
    /// with a large Tm scale (a common pattern in typesetting software).
    ///
    /// `None` when no `Tm` operator preceded the first `Tj` in this BT block (same guard
    /// as `tm_origin_x`).
    pub tm_x_scale: Option<f32>,
    /// X position of the text line matrix (T_lm) at the start of this `Tj`.
    ///
    /// Unlike `tm_origin_x` (set only by `Tm` and never changed by `Td`), this field
    /// reflects the T_lm after every `Td` operator, giving the **row anchor** for each
    /// Td-based line in a BT block.
    ///
    /// **Coordinate layer summary:**
    /// - `tm_origin_x` — BT-block column anchor; set only by `Tm`
    /// - `tm_lm_x` — row anchor; updated by both `Tm` and `Td`; use for in-place translation
    /// - `x` — visual glyph-start position; equals `tm_lm_x` for the first `Tj` after each
    ///   `Td`, then advances as subsequent `Tj` operators accumulate
    ///
    /// `None` when no `Tm` preceded the first `Tj` in this BT block.
    pub tm_lm_x: Option<f32>,
    /// Y position of the text line matrix (T_lm). Paired with [`tm_lm_x`](Self::tm_lm_x).
    pub tm_lm_y: Option<f32>,
}

// ---------------------------------------------------------------------------
// Extraction diagnostics
// ---------------------------------------------------------------------------

/// Why a content stream or Form XObject was not fully decoded during text extraction.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum WarningKind {
    /// `decompress()` failed; raw stream content was used as a best-effort fallback.
    /// This can occur with AES-256 encrypted PDFs where lopdf has already decoded the
    /// stream during password loading, leaving decoded bytes with the Filter entry intact.
    StreamDecompressFailed,
    /// A Form XObject could not be decoded (decompression failed and content was empty).
    XObjectSkipped,
}

/// A non-fatal issue encountered while extracting text from a page.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ExtractionWarning {
    /// Category of the warning.
    pub kind: WarningKind,
    /// PDF object ID of the problematic stream (`(object_number, generation_number)`).
    pub stream_id: Option<(u32, u16)>,
    /// Human-readable description.
    pub message: String,
}

// ---------------------------------------------------------------------------
// Internal font data
// ---------------------------------------------------------------------------

pub(crate) struct FontInfo {
    pub(crate) to_unicode: BTreeMap<u16, char>,
    pub(crate) dw: u32,
    pub(crate) w_runs: Vec<WidthRun>,
    /// 1 for simple fonts (Type1, TrueType), 2 for CID fonts (Type0).
    pub(crate) bytes_per_char: u8,
    /// For Type0 fonts with Identity-H/V encoding and no ToUnicode: treat the 2-byte GID
    /// directly as a Unicode scalar value (char::from_u32). Best-effort heuristic.
    pub(crate) identity_fallback: bool,
    pub(crate) base_font: String,
    pub(crate) is_bold: bool,
    pub(crate) is_italic: bool,
    pub(crate) font_family: String,
}

pub(crate) struct WidthRun {
    pub(crate) start_gid: u16,
    pub(crate) widths: Vec<u32>,
}

impl FontInfo {
    pub(crate) fn advance_width(&self, gid: u16) -> u32 {
        for run in &self.w_runs {
            if gid >= run.start_gid {
                let idx = (gid - run.start_gid) as usize;
                if idx < run.widths.len() {
                    return run.widths[idx];
                }
            }
        }
        self.dw
    }
}

// ---------------------------------------------------------------------------
// Public APIs for text extraction utilities
// ---------------------------------------------------------------------------

/// Return the axis-aligned bounding box that covers all fragments in `fragments`
/// as `[x, y, width, height]` in PDF points (origin: bottom-left of the page).
///
/// Each fragment's vertical extent is estimated from its baseline (`y`) and
/// `font_size`: ascender ≈ `font_size × 0.75` above the baseline, descender ≈
/// `font_size × 0.25` below.  This is a good practical approximation for most
/// Latin and CJK fonts; callers that need exact metrics can adjust the returned
/// rectangle manually.
///
/// Returns `None` when `fragments` is empty.
///
/// # Example
///
/// ```no_run
/// # use harumi::{Document, text_fragment_bounds};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let doc = Document::from_file("example.pdf")?;
/// let fragments = doc.extract_text_runs(1)?;
/// if let Some([x, y, w, h]) = text_fragment_bounds(&fragments) {
///     println!("Text occupies ({x}, {y}) size {w}×{h} pt");
/// }
/// # Ok(())
/// # }
/// ```
pub fn text_fragment_bounds(fragments: &[TextFragment]) -> Option<[f32; 4]> {
    let mut x_min = f32::INFINITY;
    let mut x_max = f32::NEG_INFINITY;
    let mut y_min = f32::INFINITY;
    let mut y_max = f32::NEG_INFINITY;

    for frag in fragments {
        if !frag.x.is_finite() || !frag.y.is_finite() || !frag.font_size.is_finite() {
            continue;
        }
        x_min = x_min.min(frag.x);
        x_max = x_max.max(frag.x + frag.width.max(0.0));
        // Baseline at frag.y; ascender ≈ 75 %, descender ≈ 25 % of em height.
        y_min = y_min.min(frag.y - frag.font_size * 0.25);
        y_max = y_max.max(frag.y + frag.font_size * 0.75);
    }

    if !x_min.is_finite() {
        return None;
    }
    Some([x_min, y_min, (x_max - x_min).max(0.0), (y_max - y_min).max(0.0)])
}

/// Sort text fragments by reading order: top-to-bottom, then left-to-right.
///
/// Fragments returned by [`crate::Document::extract_text_runs`] are in content-stream order.
/// This function reorders them for human-readable top-left-to-bottom-right scanning.
///
/// # Algorithm
///
/// * Groups by y-coordinate (descending, since PDF origin is bottom-left)
/// * Within each row, sorts by x-coordinate (ascending, left-to-right)
///
/// # Example
///
/// ```no_run
/// # use harumi::Document;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let doc = Document::from_file("example.pdf")?;
/// let mut fragments = doc.extract_text_runs(1)?;
/// harumi::sort_by_reading_order(&mut fragments);
/// for frag in fragments {
///     println!("{}", frag.text);
/// }
/// # Ok(())
/// # }
/// ```
pub fn sort_by_reading_order(fragments: &mut [TextFragment]) {
    use std::cmp::Ordering;
    fragments.sort_by(|a, b| {
        // Sort by y descending (top to bottom in PDF coords where bottom-left is origin).
        // Use finite() guard: NaN and Infinity values are treated as "greater than" finite values
        // so they sort to the end (bottom). Within NaN/Infinity, preserve input order.
        let y_cmp = match (a.y.is_finite(), b.y.is_finite()) {
            (true, true) => b.y.partial_cmp(&a.y).unwrap_or(Ordering::Equal),
            (true, false) => Ordering::Less, // finite < infinite
            (false, true) => Ordering::Greater,
            (false, false) => Ordering::Equal, // both infinite/NaN: preserve order
        };

        // If y is equal, sort by x ascending (left to right).
        if y_cmp != Ordering::Equal {
            return y_cmp;
        }

        match (a.x.is_finite(), b.x.is_finite()) {
            (true, true) => a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal),
            (true, false) => Ordering::Less, // finite < infinite
            (false, true) => Ordering::Greater,
            (false, false) => Ordering::Equal,
        }
    });
}

// ---------------------------------------------------------------------------
// Column detection
// ---------------------------------------------------------------------------

/// A horizontal text zone returned by [`detect_text_columns`].
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnZone {
    /// Left edge of the column in PDF points.
    pub x_start: f32,
    /// Right edge of the column in PDF points.
    pub x_end: f32,
}

/// Estimate column layout from a set of text fragments.
///
/// Builds an X-density histogram (5 pt buckets), then identifies empty gaps
/// of at least 15 pt as column separators.  Returns one [`ColumnZone`] per
/// detected column, ordered left to right.
///
/// When no clear gap exists (single-column page), returns one zone spanning
/// `[0, page_width]`.  Returns an empty slice when `fragments` is empty or
/// `page_width` is non-positive.
///
/// # Example
///
/// ```no_run
/// # use harumi::{Document, detect_text_columns};
/// # fn main() -> harumi::Result<()> {
/// let mut doc = Document::from_file("two_column.pdf")?;
/// let (w, _h) = doc.page(1)?.size()?;
/// let frags = doc.extract_text_runs(1)?;
/// let cols = detect_text_columns(&frags, w);
/// println!("{} column(s)", cols.len());
/// # Ok(())
/// # }
/// ```
pub fn detect_text_columns(fragments: &[TextFragment], page_width: f32) -> Vec<ColumnZone> {
    const BUCKET_PT: f32 = 5.0;
    const MIN_GAP_PT: f32 = 15.0;

    if fragments.is_empty() || page_width <= 0.0 {
        return vec![];
    }

    let n = (page_width / BUCKET_PT).ceil() as usize + 1;
    let mut occupied = vec![false; n];

    for frag in fragments {
        if frag.invisible {
            continue;
        }
        let lo = (frag.x / BUCKET_PT).floor() as usize;
        let hi = ((frag.x + frag.width.max(0.0)) / BUCKET_PT).ceil() as usize;
        let hi = hi.min(n - 1);
        for bucket in occupied.iter_mut().take(hi + 1).skip(lo) {
            *bucket = true;
        }
    }

    let min_gap_buckets = (MIN_GAP_PT / BUCKET_PT).ceil() as usize;

    // Collect empty runs wide enough to count as column separators.
    let mut gaps: Vec<(usize, usize)> = Vec::new();
    let mut gap_start: Option<usize> = None;
    for (i, &occ) in occupied.iter().enumerate() {
        if !occ {
            if gap_start.is_none() {
                gap_start = Some(i);
            }
        } else if let Some(gs) = gap_start.take()
            && i - gs >= min_gap_buckets
        {
            gaps.push((gs, i));
        }
    }
    if let Some(gs) = gap_start
        && n - gs >= min_gap_buckets
    {
        gaps.push((gs, n));
    }

    if gaps.is_empty() {
        return vec![ColumnZone { x_start: 0.0, x_end: page_width }];
    }

    // Column zones are the occupied ranges between (and around) the gaps.
    let mut zones = Vec::new();
    let mut col_start = 0usize;
    for (gap_s, gap_e) in &gaps {
        if col_start < *gap_s {
            zones.push(ColumnZone {
                x_start: col_start as f32 * BUCKET_PT,
                x_end: *gap_s as f32 * BUCKET_PT,
            });
        }
        col_start = *gap_e;
    }
    if col_start < n {
        zones.push(ColumnZone {
            x_start: col_start as f32 * BUCKET_PT,
            x_end: page_width,
        });
    }

    zones
}

// ---------------------------------------------------------------------------
// Text grouping
// ---------------------------------------------------------------------------

/// Controls how [`group_text_fragments`] merges individual [`TextFragment`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupingStrategy {
    /// No grouping: each [`TextFragment`] becomes its own [`TextGroup`].
    Raw,
    /// Merge fragments that share the same visual line
    /// (y-coordinate within ±½ font-size).
    Line,
    /// Group lines into paragraphs: a new paragraph starts when the vertical
    /// gap between consecutive lines exceeds 1.5 × the line height.
    Paragraph,
}

/// A group of [`TextFragment`]s merged into a single logical text block.
///
/// Returned by [`group_text_fragments`].  Primarily used to feed
/// paragraph-level context to a translation model instead of
/// per-character fragments.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct TextGroup {
    /// Combined Unicode text of all constituent fragments (space-separated
    /// within a line, newline-separated between lines for `Paragraph` groups).
    pub text: String,
    /// Source fragments in reading order.
    pub fragments: Vec<TextFragment>,
    /// X coordinate of the leftmost fragment (PDF points, bottom-left origin).
    pub x: f32,
    /// Baseline Y of the topmost (highest) line in the group.
    pub y: f32,
    /// Bounding-box width spanning all fragments.
    pub width: f32,
    /// Bounding-box height from the last line's baseline to the topmost line.
    pub height: f32,
}

/// Group text fragments into logical blocks according to `strategy`.
///
/// The input slice need not be sorted; a working copy is sorted by reading
/// order before grouping.
///
/// # Example
///
/// ```no_run
/// # use harumi::{Document, GroupingStrategy, group_text_fragments};
/// # fn main() -> harumi::Result<()> {
/// let doc = Document::from_file("doc.pdf")?;
/// let frags = doc.extract_text_runs(1)?;
/// let groups = group_text_fragments(&frags, GroupingStrategy::Paragraph);
/// for g in &groups { println!("{}", g.text); }
/// # Ok(())
/// # }
/// ```
pub fn group_text_fragments(
    fragments: &[TextFragment],
    strategy: GroupingStrategy,
) -> Vec<TextGroup> {
    if fragments.is_empty() {
        return vec![];
    }
    if matches!(strategy, GroupingStrategy::Raw) {
        return fragments
            .iter()
            .map(|f| TextGroup {
                text: f.text.clone(),
                fragments: vec![f.clone()],
                x: f.x,
                y: f.y,
                width: f.width.max(0.0),
                height: f.height.max(0.0),
            })
            .collect();
    }

    // Sort by reading order (top-to-bottom, then left-to-right).
    let mut sorted = fragments.to_vec();
    sort_by_reading_order(&mut sorted);

    // Phase 1: group into lines.
    let mut lines: Vec<TextGroup> = Vec::new();
    for frag in &sorted {
        let tol = (frag.font_size * 0.5).max(2.0);
        if let Some(last) = lines.last_mut()
            && last.y.is_finite()
            && (frag.y - last.y).abs() <= tol
        {
            // Same visual line — merge.
            if !last.text.is_empty() && !last.text.ends_with(' ') {
                last.text.push(' ');
            }
            last.text.push_str(&frag.text);
            last.fragments.push(frag.clone());
            let frag_right = frag.x + frag.width.max(0.0);
            let self_right = last.x + last.width;
            last.x = last.x.min(frag.x);
            last.width = frag_right.max(self_right) - last.x;
            last.height = last.height.max(frag.height);
            continue;
        }
        lines.push(TextGroup {
            text: frag.text.clone(),
            fragments: vec![frag.clone()],
            x: frag.x,
            y: frag.y,
            width: frag.width.max(0.0),
            height: frag.height.max(0.0),
        });
    }

    if matches!(strategy, GroupingStrategy::Line) {
        return lines;
    }

    // Phase 2: merge consecutive lines into paragraphs.
    let mut paragraphs: Vec<TextGroup> = Vec::new();
    for line in lines {
        if paragraphs.is_empty() {
            paragraphs.push(line);
            continue;
        }
        let prev = paragraphs.last().unwrap();
        let gap = (prev.y - line.y).abs();
        let line_h = prev.height.max(line.height);
        if gap > line_h * 1.5 {
            paragraphs.push(line);
        } else {
            let last = paragraphs.last_mut().unwrap();
            last.text.push('\n');
            last.text.push_str(&line.text);
            last.fragments.extend(line.fragments);
            let line_right = line.x + line.width;
            let self_right = last.x + last.width;
            last.x = last.x.min(line.x);
            last.width = line_right.max(self_right) - last.x;
            last.height = (last.y - line.y) + line.height.max(last.height);
        }
    }

    paragraphs
}

// ---------------------------------------------------------------------------
// Table cell detection
// ---------------------------------------------------------------------------

/// A text cell detected by [`extract_table_cells`].
///
/// Row and column indices are 0-based and derived from Y-coordinate clustering
/// (rows) and [`detect_text_columns`] zone assignment (columns).
///
/// > **Note:** Table detection without visible grid lines is heuristic.
/// > Complex layouts (merged cells, nested tables, irregular spacing) may
/// > produce unexpected row/column assignments.  Always validate the output
/// > before relying on it for layout-sensitive work.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    /// 0-based row index (top = 0).
    pub row: usize,
    /// 0-based column index (left = 0).
    pub col: usize,
    /// Merged text of all fragments in this cell, in left-to-right order.
    pub text: String,
    /// X coordinate of the cell's leftmost fragment (PDF points).
    pub x: f32,
    /// Y coordinate of the cell's topmost baseline (PDF points).
    pub y: f32,
    /// Bounding-box width of the cell.
    pub width: f32,
    /// Bounding-box height of the cell (baseline to bottom of em square).
    pub height: f32,
    /// Source fragments that compose this cell, in reading order.
    ///
    /// Pass `&cell.fragments` directly to
    /// [`replace_text_fragments_batch_opts`](crate::PageHandle::replace_text_fragments_batch_opts)
    /// or [`replace_fragments_fit_to_bbox`](crate::PageHandle::replace_fragments_fit_to_bbox)
    /// to suppress the original text and place a replacement within the cell bbox.
    pub fragments: Vec<TextFragment>,
}

impl TableCell {
    /// Returns `[x, y, width, height]` — a convenience alias for passing to
    /// [`replace_fragments_fit_to_bbox`](crate::PageHandle::replace_fragments_fit_to_bbox).
    pub fn bbox(&self) -> [f32; 4] {
        [self.x, self.y, self.width, self.height]
    }
}

/// Merge short CJK "tail" fragments into the preceding fragment.
///
/// CJK form PDFs often encode a single logical text run across many short `Tj`
/// operators, producing 1–4 character fragments ("る。", "界", "値）") that carry
/// no useful meaning in isolation.  When passed to a translation model as separate
/// units, these produce garbage or empty output.
///
/// A fragment is merged into its predecessor when both conditions hold:
/// 1. Its non-whitespace character count is ≤ `max_chars`.
/// 2. Its `y` baseline is within `line_height_ratio × predecessor.font_size` of
///    the predecessor's `y`.
///
/// The merged fragment inherits the predecessor's position and expands its `width`.
/// If the predecessor itself would be merged, the result is chained transitively.
///
/// Fragments with no predecessor (the first fragment) are never merged.
///
/// # Source-operator tracking after merge
///
/// A merged fragment retains only the **predecessor's** `source_op_start` /
/// `source_op_end`.  The tail fragment's operator offset is discarded.  If you
/// pass merged fragments to [`PageHandle::replace_text_fragments_batch`] or
/// [`PageHandle::suppress_text_where`], only the predecessor's `Tj` is
/// suppressed; the tail's `Tj` remains in the content stream.
///
/// To avoid incomplete suppression, apply suppression on the **original**
/// (pre-merge) fragment list and use the merged list only for translation
/// model input.
///
/// # Parameters
///
/// - `max_chars` — maximum non-whitespace character count to consider a fragment a
///   "tail".  Pass `0` to disable merging (returns a clone of `fragments`).
///   Typical value: `4`.
/// - `line_height_ratio` — maximum `|predecessor.y - fragment.y| / predecessor.font_size`
///   allowed for merging.  Typical value: `1.7` (merges continuation on the same
///   line or very close lines).
///
/// # Example
///
/// ```no_run
/// # use harumi::{Document, merge_short_cjk_tails};
/// # fn main() -> harumi::Result<()> {
/// let mut doc = Document::from_file("cjk_form.pdf")?;
/// let frags = doc.extract_text_runs(1)?;
/// let merged = merge_short_cjk_tails(&frags, 4, 1.7);
/// // `merged` has fewer entries; short tails are joined to their predecessors.
/// # Ok(())
/// # }
/// ```
pub fn merge_short_cjk_tails(
    fragments: &[TextFragment],
    max_chars: usize,
    line_height_ratio: f32,
) -> Vec<TextFragment> {
    if max_chars == 0 || fragments.is_empty() {
        return fragments.to_vec();
    }
    let mut out: Vec<TextFragment> = Vec::with_capacity(fragments.len());
    for frag in fragments {
        let non_ws = frag.text.chars().filter(|c| !c.is_whitespace()).count();
        let is_tail = non_ws > 0 && non_ws <= max_chars;
        if is_tail && let Some(prev) = out.last_mut() {
            let y_dist = (prev.y - frag.y).abs();
            let threshold = (prev.font_size * line_height_ratio).max(2.0);
            if y_dist <= threshold {
                // Merge: append text and extend bbox.
                prev.text.push_str(&frag.text);
                let new_right = (frag.x + frag.width).max(prev.x + prev.width);
                prev.width = new_right - prev.x;
                prev.height = prev.height.max(frag.height);
                continue;
            }
        }
        out.push(frag.clone());
    }
    out
}

/// Detect table structure in a flat list of text fragments.
///
/// The function uses two orthogonal passes:
/// - **Columns** — delegates to [`detect_text_columns`] (X-density gap detection).
/// - **Rows** — fragments whose Y baselines are within `½ × font_size` of the
///   row's first fragment are grouped into the same row; a larger gap starts a
///   new row.
///
/// Returns one [`TableCell`] per occupied (row, col) pair, sorted by row then
/// column.  Invisible fragments and empty fragments are excluded.
///
/// # Example
///
/// ```no_run
/// # use harumi::{Document, extract_table_cells};
/// # fn main() -> harumi::Result<()> {
/// let mut doc = Document::from_file("table.pdf")?;
/// let (w, h) = doc.page(1)?.size()?;
/// let frags = doc.extract_text_runs(1)?;
/// let cells = extract_table_cells(&frags, w, h);
/// for cell in &cells {
///     println!("({},{}) {}", cell.row, cell.col, cell.text);
/// }
/// # Ok(())
/// # }
/// ```
pub fn extract_table_cells(
    fragments: &[TextFragment],
    page_width: f32,
    _page_height: f32,
) -> Vec<TableCell> {
    if fragments.is_empty() || page_width <= 0.0 {
        return vec![];
    }

    // Work only with visible, non-empty fragments in reading order.
    let mut sorted: Vec<TextFragment> = fragments
        .iter()
        .filter(|f| !f.invisible && !f.text.trim().is_empty())
        .cloned()
        .collect();
    if sorted.is_empty() {
        return vec![];
    }
    sort_by_reading_order(&mut sorted);

    // Choose column-assignment strategy.
    //
    // When a majority of fragments have `tm_lm_x` (set by `Tm`+`Td` operators),
    // use those anchors directly — they are exact column starts per the PDF text
    // line matrix.  This is more accurate than the X-density histogram for form
    // PDFs that use a single BT block with Td jumps between label and value cols.
    //
    // Fall back to the histogram when `tm_lm_x` is absent (e.g., PDFs without
    // scaled Tm, or older content streams that only use `Td` with identity Tm).
    let tm_lm_count = sorted.iter().filter(|f| f.tm_lm_x.is_some()).count();
    let use_tm_lm_cols = tm_lm_count > sorted.len() / 2;

    // Build sorted, deduplicated column anchors for the tm_lm_x path
    // (cluster values within 2 pt to handle sub-pixel jitter).
    let tm_lm_anchors: Vec<f32> = if use_tm_lm_cols {
        let mut v: Vec<f32> = sorted.iter().filter_map(|f| f.tm_lm_x).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v.dedup_by(|a, b| (*a - *b).abs() < 2.0);
        v
    } else {
        vec![]
    };

    // Histogram-based column zones for the fallback path.
    let col_zones: Vec<ColumnZone> = if !use_tm_lm_cols {
        let z = detect_text_columns(fragments, page_width);
        if z.is_empty() {
            return vec![];
        }
        z
    } else {
        vec![]
    };

    // Map a fragment to its column index.
    let col_for_frag = |frag: &TextFragment| -> usize {
        if use_tm_lm_cols {
            let lm = frag.tm_lm_x.unwrap_or(frag.x);
            tm_lm_anchors
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    (lm - *a).abs().partial_cmp(&(lm - *b).abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
                .unwrap_or(0)
        } else {
            for (i, zone) in col_zones.iter().enumerate() {
                if frag.x >= zone.x_start && frag.x < zone.x_end {
                    return i;
                }
            }
            col_zones
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let da = (frag.x - (a.x_start + a.x_end) * 0.5).abs();
                    let db = (frag.x - (b.x_start + b.x_end) * 0.5).abs();
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
                .unwrap_or(0)
        }
    };

    // Row-grouping threshold: half the first (topmost) fragment's font size, at
    // least 2 pt.
    let row_tol = {
        let first_fs = sorted
            .iter()
            .find(|f| f.font_size.is_finite() && f.font_size > 0.0)
            .map(|f| f.font_size)
            .unwrap_or(12.0);
        (first_fs * 0.5).max(2.0)
    };

    // Group fragments into rows by Y proximity.
    let mut rows: Vec<Vec<&TextFragment>> = Vec::new();
    for frag in &sorted {
        let in_current_row = rows
            .last()
            .map(|r| (r[0].y - frag.y).abs() <= row_tol);
        if in_current_row == Some(true) {
            rows.last_mut().unwrap().push(frag);
        } else {
            rows.push(vec![frag]);
        }
    }

    // Collect fragments per (row, col) cell.
    let mut cell_map: std::collections::BTreeMap<(usize, usize), Vec<&TextFragment>> =
        std::collections::BTreeMap::new();
    for (row_idx, row_frags) in rows.iter().enumerate() {
        for frag in row_frags {
            let col_idx = col_for_frag(frag);
            cell_map.entry((row_idx, col_idx)).or_default().push(frag);
        }
    }

    // Build TableCell for each occupied (row, col).
    cell_map
        .into_iter()
        .map(|((row, col), mut frags)| {
            frags.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
            let text = frags
                .iter()
                .map(|f| f.text.trim())
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let x = frags.iter().map(|f| f.x).fold(f32::INFINITY, f32::min);
            let y = frags.iter().map(|f| f.y).fold(f32::NEG_INFINITY, f32::max);
            let right = frags
                .iter()
                .map(|f| f.x + f.width.max(0.0))
                .fold(f32::NEG_INFINITY, f32::max);
            let height = frags.iter().map(|f| f.height.max(0.0)).fold(0.0f32, f32::max);
            let fragments_owned: Vec<TextFragment> = frags.iter().map(|f| (*f).clone()).collect();
            TableCell {
                row,
                col,
                text,
                x,
                y,
                width: (right - x).max(0.0),
                height,
                fragments: fragments_owned,
            }
        })
        .collect()
    // BTreeMap iteration is already sorted by (row, col).
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub(crate) fn extract_text_runs_from_page(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> Result<Vec<TextFragment>> {
    let streams = page_content_streams(doc, page_id);
    let fonts = collect_fonts(doc, page_id);

    let mut fragments = Vec::new();
    // Carry graphics state (colour, render-mode) across streams on the same page.
    let mut carry = ParseCarryState::default();
    for (stream_idx, stream_bytes) in streams.iter().enumerate() {
        parse_content_stream(stream_bytes, &fonts, &mut carry, &mut fragments, Some(stream_idx), None);
    }
    // Also extract text from Form XObjects (headers, footers, watermarks).
    extract_text_from_xobjects(doc, page_id, &mut carry, &mut fragments, 0);
    Ok(fragments)
}

/// Extract text from Form XObjects referenced in the page content.
///
/// When the page content stream contains explicit `Do` invocations (recorded in
/// `carry.do_ctm_map`), each XObject is processed with the CTM that was active at
/// its specific `Do` call.  This fixes the multi-XObject case where a single
/// accumulated CTM would be applied to all objects.
///
/// When no `Do` operators were seen (legacy / test PDFs that put content in XObjects
/// without an explicit invocation in the main stream), we fall back to processing
/// every Form XObject in the inherited /Resources dict with the current CTM.
fn extract_text_from_xobjects(
    doc: &lopdf::Document,
    page_id: ObjectId,
    carry: &mut ParseCarryState,
    out: &mut Vec<TextFragment>,
    _depth: u8,
) {
    let saved_ctm = carry.ctm;
    // Save the page-level CTM stack: each XObject gets its own fresh stack starting
    // at its combined Do-time CTM × XObject matrix, independent of the page's state.
    let saved_ctm_stack = carry.ctm_stack.clone();

    if !carry.do_ctm_map.is_empty() {
        // Per-Do CTM path: process only explicitly invoked XObjects, each with its
        // own CTM captured at the time of the Do operator.
        let xobj_name_map = collect_inherited_xobject_name_map(doc, page_id);
        let do_ctm_map = std::mem::take(&mut carry.do_ctm_map);

        for (xobj_name, do_ctm) in &do_ctm_map {
            let Some(&xobj_id) = xobj_name_map.get(xobj_name.as_slice()) else { continue };
            if let Some(content) = decode_form_xobject(doc, xobj_id) {
                let xobj_fonts = xobject_fonts(doc, page_id, xobj_id);
                let xobj_matrix = xobject_matrix(doc, xobj_id);
                carry.ctm = multiply_ctm(*do_ctm, xobj_matrix);
                carry.ctm_stack = vec![carry.ctm];
                parse_content_stream(&content, &xobj_fonts, carry, out, None, Some(xobj_id));
            }
        }

        carry.do_ctm_map = do_ctm_map;
    } else {
        // Fallback path: no Do operators observed.  Process all Form XObjects from
        // the inherited /Resources dict using the identity (or current) CTM.
        let xobj_ids = collect_inherited_xobject_ids(doc, page_id);
        for xobj_id in xobj_ids {
            if let Some(content) = decode_form_xobject(doc, xobj_id) {
                let xobj_fonts = xobject_fonts(doc, page_id, xobj_id);
                let xobj_matrix = xobject_matrix(doc, xobj_id);
                carry.ctm = multiply_ctm(saved_ctm, xobj_matrix);
                carry.ctm_stack = vec![carry.ctm];
                parse_content_stream(&content, &xobj_fonts, carry, out, None, Some(xobj_id));
            }
        }
    }

    carry.ctm = saved_ctm;
    carry.ctm_stack = saved_ctm_stack;
}

fn decode_form_xobject(doc: &lopdf::Document, xobj_id: ObjectId) -> Option<Vec<u8>> {
    let xobj_obj = doc.get_object(xobj_id).ok()?;
    let xobj_stream = xobj_obj.as_stream().ok()?;
    let is_form = xobj_stream.dict.get(b"Subtype").ok()
        .and_then(|o| if let Object::Name(n) = o { Some(n.as_slice()) } else { None })
        == Some(b"Form");
    if !is_form {
        return None;
    }
    if xobj_stream.dict.get(b"Filter").is_ok() {
        let mut owned = xobj_stream.clone();
        if owned.decompress().is_ok() {
            Some(owned.content)
        } else if !xobj_stream.content.is_empty() {
            // Fallback: lopdf may have already decoded the stream during AES-256
            // decryption, leaving final bytes in content with Filter still present.
            Some(xobj_stream.content.clone())
        } else {
            None
        }
    } else {
        Some(xobj_stream.content.clone())
    }
}

fn xobject_fonts(
    doc: &lopdf::Document,
    page_id: ObjectId,
    xobj_id: ObjectId,
) -> HashMap<Vec<u8>, crate::extract::FontInfo> {
    // Page fonts serve as a fallback for XObjects that reference fonts defined on
    // the parent page (common in PScript5.dll/Distiller PDFs where the XObject has
    // its own /Resources dict but no /Font sub-entry).
    let page_fonts = collect_fonts(doc, page_id);

    let xobj_specific = doc.get_object(xobj_id)
        .ok()
        .and_then(|o| o.as_stream().ok())
        .and_then(|s| s.dict.get(b"Resources").ok())
        .and_then(|res_ref| resolve_dict(doc, res_ref))
        .map(|res_dict| collect_fonts_from_resources(doc, res_dict))
        .unwrap_or_default();

    if xobj_specific.is_empty() {
        page_fonts
    } else {
        // XObject-specific fonts take priority over page fonts on name collision.
        let mut merged = page_fonts;
        merged.extend(xobj_specific);
        merged
    }
}

fn xobject_matrix(doc: &lopdf::Document, xobj_id: ObjectId) -> [f32; 6] {
    doc.get_object(xobj_id)
        .ok()
        .and_then(|o| o.as_stream().ok())
        .map(|s| read_matrix(&s.dict))
        .unwrap_or(IDENTITY_CTM)
}

// ---------------------------------------------------------------------------
// Step 1: raw content stream bytes for a page
// ---------------------------------------------------------------------------

pub(crate) fn page_content_streams(doc: &lopdf::Document, page_id: ObjectId) -> Vec<Vec<u8>> {
    let Ok(page_obj) = doc.get_object(page_id) else {
        return vec![];
    };
    let Ok(page_dict) = page_obj.as_dict() else {
        return vec![];
    };
    let Ok(contents_obj) = page_dict.get(b"Contents") else {
        return vec![];
    };

    let ids: Vec<ObjectId> = match contents_obj {
        Object::Reference(id) => vec![*id],
        Object::Array(arr) => arr
            .iter()
            .filter_map(|o| {
                if let Object::Reference(id) = o {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect(),
        _ => return vec![],
    };

    let mut result = Vec::new();
    for id in ids {
        let Ok(stream_obj) = doc.get_object(id) else {
            continue;
        };
        let Ok(stream) = stream_obj.as_stream() else {
            continue;
        };
        let has_filter = stream.dict.get(b"Filter").is_ok();
        if has_filter {
            let mut owned = stream.clone();
            if owned.decompress().is_ok() {
                result.push(owned.content);
            } else if !stream.content.is_empty() {
                // Fallback: lopdf may have already decoded the stream during AES-256
                // decryption, leaving final bytes in content with Filter still present.
                result.push(stream.content.clone());
            }
        } else {
            result.push(stream.content.clone());
        }
    }
    result
}

/// Returns the `ObjectId`s of the content streams in the page `/Contents` array,
/// in order.  Used by `replace_text_fragments` to write back modified streams.
pub(crate) fn page_content_stream_ids(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> Vec<ObjectId> {
    let Ok(page_obj) = doc.get_object(page_id) else { return vec![] };
    let Ok(page_dict) = page_obj.as_dict() else { return vec![] };
    let Ok(contents_obj) = page_dict.get(b"Contents") else { return vec![] };
    match contents_obj {
        Object::Reference(id) => vec![*id],
        Object::Array(arr) => arr
            .iter()
            .filter_map(|o| if let Object::Reference(id) = o { Some(*id) } else { None })
            .collect(),
        _ => vec![],
    }
}

/// Like `page_content_streams` but also returns a warning for each stream that
/// could not be decompressed and fell back to raw content.
pub(crate) fn page_content_streams_verbose(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> (Vec<Vec<u8>>, Vec<ExtractionWarning>) {
    let Ok(page_obj) = doc.get_object(page_id) else {
        return (vec![], vec![]);
    };
    let Ok(page_dict) = page_obj.as_dict() else {
        return (vec![], vec![]);
    };
    let Ok(contents_obj) = page_dict.get(b"Contents") else {
        return (vec![], vec![]);
    };

    let ids: Vec<ObjectId> = match contents_obj {
        Object::Reference(id) => vec![*id],
        Object::Array(arr) => arr
            .iter()
            .filter_map(|o| if let Object::Reference(id) = o { Some(*id) } else { None })
            .collect(),
        _ => return (vec![], vec![]),
    };

    let mut result = Vec::new();
    let mut warnings = Vec::new();
    for id in ids {
        let Ok(stream_obj) = doc.get_object(id) else { continue };
        let Ok(stream) = stream_obj.as_stream() else { continue };
        let has_filter = stream.dict.get(b"Filter").is_ok();
        if has_filter {
            let mut owned = stream.clone();
            if owned.decompress().is_ok() {
                result.push(owned.content);
            } else if !stream.content.is_empty() {
                warnings.push(ExtractionWarning {
                    kind: WarningKind::StreamDecompressFailed,
                    stream_id: Some((id.0, id.1)),
                    message: format!(
                        "decompress() failed for content stream {id:?}; using raw content as fallback"
                    ),
                });
                result.push(stream.content.clone());
            }
        } else {
            result.push(stream.content.clone());
        }
    }
    (result, warnings)
}

/// Like `extract_text_runs_from_page` but also collects `ExtractionWarning`s for
/// streams that could not be decompressed.
pub(crate) fn extract_text_runs_from_page_verbose(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> Result<(Vec<TextFragment>, Vec<ExtractionWarning>)> {
    let (streams, mut warnings) = page_content_streams_verbose(doc, page_id);
    let fonts = collect_fonts(doc, page_id);

    let mut fragments = Vec::new();
    let mut carry = ParseCarryState::default();
    for (stream_idx, stream_bytes) in streams.iter().enumerate() {
        parse_content_stream(stream_bytes, &fonts, &mut carry, &mut fragments, Some(stream_idx), None);
    }
    extract_text_from_xobjects_verbose(doc, page_id, &mut carry, &mut fragments, 0, &mut warnings);
    Ok((fragments, warnings))
}

/// `extract_text_from_xobjects` variant that appends `ExtractionWarning`s for
/// XObjects that could not be decoded.
fn extract_text_from_xobjects_verbose(
    doc: &lopdf::Document,
    page_id: ObjectId,
    carry: &mut ParseCarryState,
    out: &mut Vec<TextFragment>,
    _depth: u8,
    warnings: &mut Vec<ExtractionWarning>,
) {
    let saved_ctm = carry.ctm;
    let saved_ctm_stack = carry.ctm_stack.clone();

    if !carry.do_ctm_map.is_empty() {
        let xobj_name_map = collect_inherited_xobject_name_map(doc, page_id);
        let do_ctm_map = std::mem::take(&mut carry.do_ctm_map);

        for (xobj_name, do_ctm) in &do_ctm_map {
            let Some(&xobj_id) = xobj_name_map.get(xobj_name.as_slice()) else { continue };
            match decode_form_xobject_verbose(doc, xobj_id) {
                Ok(content) => {
                    let xobj_fonts = xobject_fonts(doc, page_id, xobj_id);
                    let xobj_matrix = xobject_matrix(doc, xobj_id);
                    carry.ctm = multiply_ctm(*do_ctm, xobj_matrix);
                    carry.ctm_stack = vec![carry.ctm];
                    parse_content_stream(&content, &xobj_fonts, carry, out, None, Some(xobj_id));
                }
                Err(warn) => { warnings.push(warn); }
            }
        }
        carry.do_ctm_map = do_ctm_map;
    } else {
        let xobj_ids = collect_inherited_xobject_ids(doc, page_id);
        for xobj_id in xobj_ids {
            match decode_form_xobject_verbose(doc, xobj_id) {
                Ok(content) => {
                    let xobj_fonts = xobject_fonts(doc, page_id, xobj_id);
                    let xobj_matrix = xobject_matrix(doc, xobj_id);
                    carry.ctm = multiply_ctm(saved_ctm, xobj_matrix);
                    carry.ctm_stack = vec![carry.ctm];
                    parse_content_stream(&content, &xobj_fonts, carry, out, None, Some(xobj_id));
                }
                Err(warn) => { warnings.push(warn); }
            }
        }
    }

    carry.ctm = saved_ctm;
    carry.ctm_stack = saved_ctm_stack;
}

/// `decode_form_xobject` variant that returns an `ExtractionWarning` when the
/// XObject cannot be decoded at all (fallback also failed / content is empty).
fn decode_form_xobject_verbose(
    doc: &lopdf::Document,
    xobj_id: ObjectId,
) -> std::result::Result<Vec<u8>, ExtractionWarning> {
    match decode_form_xobject(doc, xobj_id) {
        Some(bytes) => Ok(bytes),
        None => Err(ExtractionWarning {
            kind: WarningKind::XObjectSkipped,
            stream_id: Some((xobj_id.0, xobj_id.1)),
            message: format!("Form XObject {xobj_id:?} could not be decoded"),
        }),
    }
}

// ---------------------------------------------------------------------------
// Step 2: font info from /Resources/Font
// ---------------------------------------------------------------------------

pub(crate) fn resolve_dict<'a>(
    doc: &'a lopdf::Document,
    obj: &'a Object,
) -> Option<&'a Dictionary> {
    match obj {
        Object::Dictionary(d) => Some(d),
        Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok(),
        _ => None,
    }
}

/// Parse PostScript font name into (base_font, is_bold, is_italic, font_family).
///
/// Strips subset prefixes like "ABCDEF+" before analysis.
/// Family is extracted as the portion before the first "-" or ",".
fn parse_font_attributes(raw: &str) -> (String, bool, bool, String) {
    let name = raw.split('+').next_back().unwrap_or(raw);
    let lower = name.to_lowercase();
    let is_bold = ["bold", "heavy", "black", "semibold", "demibold", "extrabold"]
        .iter()
        .any(|kw| lower.contains(kw));
    let is_italic = ["italic", "oblique", "slanted"].iter().any(|kw| lower.contains(kw));
    let family = name.split(['-', ',']).next().unwrap_or(name).to_string();
    (name.to_string(), is_bold, is_italic, family)
}

pub(crate) fn collect_fonts(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> HashMap<Vec<u8>, FontInfo> {
    collect_fonts_inner(doc, page_id).unwrap_or_default()
}

/// Collect fonts from a resources dictionary directly.
/// Used by both page-level and Form-XObject font collection.
pub(crate) fn collect_fonts_from_resources(
    doc: &lopdf::Document,
    resources_dict: &Dictionary,
) -> HashMap<Vec<u8>, FontInfo> {
    let mut fonts = HashMap::new();
    let Ok(font_obj) = resources_dict.get(b"Font") else {
        return fonts;
    };
    let Some(font_dict) = resolve_dict(doc, font_obj) else {
        return fonts;
    };
    collect_font_dict_entries(doc, font_dict, &mut fonts);
    fonts
}

fn collect_fonts_inner(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> Option<HashMap<Vec<u8>, FontInfo>> {
    // PDF spec §7.7.3: /Resources may be inherited from any ancestor /Pages node.
    // Walk up the /Parent chain until we find a node that carries /Resources.
    let mut current_id = page_id;
    loop {
        let obj = doc.get_object(current_id).ok()?;
        let dict = obj.as_dict().ok()?;
        if let Ok(resources_obj) = dict.get(b"Resources") {
            let resources_dict = resolve_dict(doc, resources_obj)?;
            return Some(collect_fonts_from_resources(doc, resources_dict));
        }
        // No /Resources on this node — climb to the parent Pages node.
        let parent_ref = dict.get(b"Parent").ok()?;
        let Object::Reference(parent_id) = parent_ref else {
            return None;
        };
        current_id = *parent_id;
    }
}

/// Walk up /Parent chain and return XObject IDs from the first
/// /Resources/XObject dict found (PDF spec §7.7.3 inheritance).
pub(crate) fn collect_inherited_xobject_ids(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> Vec<ObjectId> {
    let mut current_id = page_id;
    while let Ok(obj) = doc.get_object(current_id) {
        let Some(dict) = obj.as_dict().ok() else { break };
        if let Ok(res_obj) = dict.get(b"Resources") {
            let ids = resolve_dict(doc, res_obj)
                .and_then(|res_dict| {
                    res_dict.get(b"XObject").ok().and_then(|xobj_ref| resolve_dict(doc, xobj_ref))
                })
                .map(|xobj_dict| {
                    xobj_dict
                        .iter()
                        .filter_map(|(_, v)| {
                            if let Object::Reference(id) = v { Some(*id) } else { None }
                        })
                        .collect::<Vec<_>>()
                });
            if let Some(ids) = ids {
                return ids;
            }
            break; // /Resources found but no /XObject — stop climbing
        }
        let Ok(parent_ref) = dict.get(b"Parent") else { break };
        let Object::Reference(parent_id) = parent_ref else { break };
        current_id = *parent_id;
    }
    vec![]
}

/// Like `collect_inherited_xobject_ids` but returns a `name → ObjectId` map so that
/// `extract_text_from_xobjects` can look up XObjects by the name used in a `Do` operator.
fn collect_inherited_xobject_name_map(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> HashMap<Vec<u8>, ObjectId> {
    let mut current_id = page_id;
    while let Ok(obj) = doc.get_object(current_id) {
        let Some(dict) = obj.as_dict().ok() else { break };
        if let Ok(res_obj) = dict.get(b"Resources") {
            let map = resolve_dict(doc, res_obj)
                .and_then(|res_dict| {
                    res_dict.get(b"XObject").ok().and_then(|xobj_ref| resolve_dict(doc, xobj_ref))
                })
                .map(|xobj_dict| {
                    xobj_dict
                        .iter()
                        .filter_map(|(name, v)| {
                            if let Object::Reference(id) = v {
                                Some((name.clone(), *id))
                            } else {
                                None
                            }
                        })
                        .collect::<HashMap<Vec<u8>, ObjectId>>()
                });
            if let Some(m) = map {
                return m;
            }
            break;
        }
        let Ok(parent_ref) = dict.get(b"Parent") else { break };
        let Object::Reference(parent_id) = parent_ref else { break };
        current_id = *parent_id;
    }
    HashMap::new()
}

fn collect_font_dict_entries(
    doc: &lopdf::Document,
    font_dict: &Dictionary,
    fonts: &mut HashMap<Vec<u8>, FontInfo>,
) {
    for (name, font_ref) in font_dict.iter() {
        let Object::Reference(font_id) = font_ref else {
            continue;
        };
        let Ok(font_obj) = doc.get_object(*font_id) else {
            continue;
        };
        let Ok(fd) = font_obj.as_dict() else { continue };

        let subtype = fd.get(b"Subtype").ok().and_then(|o| {
            if let Object::Name(n) = o {
                Some(n.as_slice())
            } else {
                None
            }
        });

        let raw_base_font = fd
            .get(b"BaseFont")
            .ok()
            .and_then(|o| match o {
                Object::Name(n) => std::str::from_utf8(n).ok().map(|s| s.to_string()),
                _ => None,
            })
            .unwrap_or_default();
        let (base_font, is_bold, is_italic, font_family) = parse_font_attributes(&raw_base_font);

        let font_info = match subtype {
            Some(b"Type0") => match collect_type0_font(fd, doc, base_font, is_bold, is_italic, font_family) {
                Some(fi) => fi,
                None => continue,
            },
            Some(b"Type1") | Some(b"MMType1") | Some(b"TrueType") | Some(b"Type3") => {
                collect_simple_font(fd, doc, base_font, is_bold, is_italic, font_family)
            }
            _ => continue,
        };

        fonts.insert(name.clone(), font_info);
    }
}

fn collect_type0_font(
    fd: &Dictionary,
    doc: &lopdf::Document,
    base_font: String,
    is_bold: bool,
    is_italic: bool,
    font_family: String,
) -> Option<FontInfo> {
    let to_unicode = try_parse_to_unicode(fd, doc).unwrap_or_default();
    // When ToUnicode is absent and the encoding is Identity-H/V, fall back to treating
    // the 2-byte character code directly as a Unicode scalar (best-effort).
    let identity_fallback = to_unicode.is_empty() && is_identity_cmap(fd);

    let desc_obj = fd.get(b"DescendantFonts").ok()?;
    let Object::Array(desc_arr) = desc_obj else {
        return None;
    };
    let Some(Object::Reference(cid_id)) = desc_arr.first() else {
        return None;
    };
    let Ok(cid_obj) = doc.get_object(*cid_id) else {
        return None;
    };
    let Ok(cid_dict) = cid_obj.as_dict() else {
        return None;
    };

    let dw = cid_dict
        .get(b"DW")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .map(|n| n as u32)
        .unwrap_or(1000);

    let w_runs = cid_dict
        .get(b"W")
        .ok()
        .and_then(|o| {
            if let Object::Array(a) = o {
                Some(a.as_slice())
            } else {
                None
            }
        })
        .map(parse_w_array)
        .unwrap_or_default();

    Some(FontInfo {
        to_unicode,
        dw,
        w_runs,
        bytes_per_char: 2,
        identity_fallback,
        base_font,
        is_bold,
        is_italic,
        font_family,
    })
}

/// Returns true when the Type0 font's /Encoding is Identity-H or Identity-V (character code =
/// CID directly). No /Encoding entry is also treated as Identity-H per common practice.
fn is_identity_cmap(fd: &Dictionary) -> bool {
    match fd.get(b"Encoding").ok() {
        Some(Object::Name(n)) => matches!(n.as_slice(), b"Identity-H" | b"Identity-V"),
        None => true,
        _ => false,
    }
}

fn collect_simple_font(
    fd: &Dictionary,
    doc: &lopdf::Document,
    base_font: String,
    is_bold: bool,
    is_italic: bool,
    font_family: String,
) -> FontInfo {
    let to_unicode = if let Some(map) = try_parse_to_unicode(fd, doc) {
        map
    } else {
        build_encoding_map(fd, doc)
    };

    let (w_runs, dw) = collect_simple_font_widths(fd, doc);
    FontInfo {
        to_unicode,
        dw,
        w_runs,
        bytes_per_char: 1,
        identity_fallback: false,
        base_font,
        is_bold,
        is_italic,
        font_family,
    }
}

fn try_parse_to_unicode(fd: &Dictionary, doc: &lopdf::Document) -> Option<BTreeMap<u16, char>> {
    let to_uni_ref = fd.get(b"ToUnicode").ok()?;
    let Object::Reference(to_uni_id) = to_uni_ref else {
        return None;
    };
    let Ok(to_uni_obj) = doc.get_object(*to_uni_id) else {
        return None;
    };
    let Ok(stream) = to_uni_obj.as_stream() else {
        return None;
    };
    let cmap_bytes = if stream.dict.get(b"Filter").is_ok() {
        let mut owned = stream.clone();
        owned.decompress().ok()?;
        owned.content
    } else {
        stream.content.clone()
    };
    let map = parse_to_unicode_cmap(&cmap_bytes);
    if map.is_empty() { None } else { Some(map) }
}

fn collect_simple_font_widths(fd: &Dictionary, doc: &lopdf::Document) -> (Vec<WidthRun>, u32) {
    let dw = missing_width_from_descriptor(fd, doc);

    let first_char = match fd.get(b"FirstChar").ok().and_then(|o| o.as_i64().ok()) {
        Some(n) => n as u16,
        None => return (vec![], dw),
    };
    let widths_arr = match fd.get(b"Widths").ok() {
        Some(Object::Array(a)) => a,
        _ => return (vec![], dw),
    };
    let widths: Vec<u32> = widths_arr
        .iter()
        .filter_map(|o| o.as_i64().ok().map(|n| n as u32))
        .collect();
    if widths.is_empty() {
        return (vec![], dw);
    }
    (
        vec![WidthRun {
            start_gid: first_char,
            widths,
        }],
        dw,
    )
}

fn missing_width_from_descriptor(fd: &Dictionary, doc: &lopdf::Document) -> u32 {
    let desc = fd
        .get(b"FontDescriptor")
        .ok()
        .and_then(|o| resolve_dict(doc, o));
    desc.and_then(|d| d.get(b"MissingWidth").ok())
        .and_then(|o| o.as_i64().ok())
        .map(|n| n as u32)
        .unwrap_or(1000)
}

// ---------------------------------------------------------------------------
// Encoding resolution for simple fonts
// ---------------------------------------------------------------------------

fn build_encoding_map(fd: &Dictionary, doc: &lopdf::Document) -> BTreeMap<u16, char> {
    let enc_obj = match fd.get(b"Encoding").ok() {
        Some(o) => o,
        None => return encoding_table_to_btree(&STANDARD_ENCODING),
    };

    if let Object::Name(name) = enc_obj {
        return encoding_name_to_btree(name);
    }

    // Encoding dictionary (may be an indirect reference).
    let enc_dict = match resolve_dict(doc, enc_obj) {
        Some(d) => d,
        None => return encoding_table_to_btree(&STANDARD_ENCODING),
    };

    let base = enc_dict
        .get(b"BaseEncoding")
        .ok()
        .and_then(|o| {
            if let Object::Name(n) = o {
                Some(n.as_slice())
            } else {
                None
            }
        })
        .map(encoding_name_to_btree)
        .unwrap_or_else(|| encoding_table_to_btree(&STANDARD_ENCODING));

    apply_differences(enc_dict, base)
}

fn encoding_name_to_btree(name: &[u8]) -> BTreeMap<u16, char> {
    match name {
        b"WinAnsiEncoding" => encoding_table_to_btree(&WIN_ANSI_ENCODING),
        b"MacRomanEncoding" => encoding_table_to_btree(&MAC_ROMAN_ENCODING),
        b"StandardEncoding" => encoding_table_to_btree(&STANDARD_ENCODING),
        _ => encoding_table_to_btree(&STANDARD_ENCODING),
    }
}

fn encoding_table_to_btree(table: &[Option<char>; 256]) -> BTreeMap<u16, char> {
    table
        .iter()
        .enumerate()
        .filter_map(|(i, opt)| opt.map(|ch| (i as u16, ch)))
        .collect()
}

fn apply_differences(enc_dict: &Dictionary, mut map: BTreeMap<u16, char>) -> BTreeMap<u16, char> {
    let Ok(Object::Array(diffs)) = enc_dict.get(b"Differences") else {
        return map;
    };
    let mut current_code: u16 = 0;
    for obj in diffs {
        match obj {
            Object::Integer(n) => {
                current_code = *n as u16;
            }
            Object::Name(glyph_name) => {
                if let Some(ch) = glyph_name_to_char(glyph_name) {
                    map.insert(current_code, ch);
                }
                current_code = current_code.saturating_add(1);
            }
            _ => {}
        }
    }
    map
}

// ---------------------------------------------------------------------------
// Standard encoding tables  [Option<char>; 256]
// ---------------------------------------------------------------------------

#[rustfmt::skip]
const WIN_ANSI_ENCODING: [Option<char>; 256] = [
    // 0x00-0x1F: control (undefined)
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    // 0x20-0x2F
    Some(' '), Some('!'), Some('"'), Some('#'),
    Some('$'), Some('%'), Some('&'), Some('\''),
    Some('('), Some(')'), Some('*'), Some('+'),
    Some(','), Some('-'), Some('.'), Some('/'),
    // 0x30-0x3F
    Some('0'), Some('1'), Some('2'), Some('3'),
    Some('4'), Some('5'), Some('6'), Some('7'),
    Some('8'), Some('9'), Some(':'), Some(';'),
    Some('<'), Some('='), Some('>'), Some('?'),
    // 0x40-0x4F
    Some('@'), Some('A'), Some('B'), Some('C'),
    Some('D'), Some('E'), Some('F'), Some('G'),
    Some('H'), Some('I'), Some('J'), Some('K'),
    Some('L'), Some('M'), Some('N'), Some('O'),
    // 0x50-0x5F
    Some('P'), Some('Q'), Some('R'), Some('S'),
    Some('T'), Some('U'), Some('V'), Some('W'),
    Some('X'), Some('Y'), Some('Z'), Some('['),
    Some('\\'), Some(']'), Some('^'), Some('_'),
    // 0x60-0x6F
    Some('`'), Some('a'), Some('b'), Some('c'),
    Some('d'), Some('e'), Some('f'), Some('g'),
    Some('h'), Some('i'), Some('j'), Some('k'),
    Some('l'), Some('m'), Some('n'), Some('o'),
    // 0x70-0x7F
    Some('p'), Some('q'), Some('r'), Some('s'),
    Some('t'), Some('u'), Some('v'), Some('w'),
    Some('x'), Some('y'), Some('z'), Some('{'),
    Some('|'), Some('}'), Some('~'), None,          // 0x7F undefined
    // 0x80-0x8F  (Windows-1252 upper half)
    Some('€'), None,        Some('‚'), Some('ƒ'),
    Some('„'), Some('…'), Some('†'), Some('‡'),
    Some('ˆ'), Some('‰'), Some('Š'), Some('‹'),
    Some('Œ'), None,        Some('Ž'), None,
    // 0x90-0x9F
    None,        Some('\u{2018}'), Some('\u{2019}'), Some('\u{201C}'),
    Some('\u{201D}'), Some('•'), Some('–'), Some('—'),
    Some('˜'), Some('™'), Some('š'), Some('›'),
    Some('œ'), None,        Some('ž'), Some('Ÿ'),
    // 0xA0-0xAF  (Latin-1 Supplement)
    Some('\u{00A0}'), Some('¡'), Some('¢'), Some('£'),
    Some('¤'), Some('¥'), Some('¦'), Some('§'),
    Some('¨'), Some('©'), Some('ª'), Some('«'),
    Some('¬'), Some('-'),   Some('®'), Some('¯'),    // 0xAD = soft-hyphen → '-'
    // 0xB0-0xBF
    Some('°'), Some('±'), Some('²'), Some('³'),
    Some('´'), Some('µ'), Some('¶'), Some('·'),
    Some('¸'), Some('¹'), Some('º'), Some('»'),
    Some('¼'), Some('½'), Some('¾'), Some('¿'),
    // 0xC0-0xCF
    Some('À'), Some('Á'), Some('Â'), Some('Ã'),
    Some('Ä'), Some('Å'), Some('Æ'), Some('Ç'),
    Some('È'), Some('É'), Some('Ê'), Some('Ë'),
    Some('Ì'), Some('Í'), Some('Î'), Some('Ï'),
    // 0xD0-0xDF
    Some('Ð'), Some('Ñ'), Some('Ò'), Some('Ó'),
    Some('Ô'), Some('Õ'), Some('Ö'), Some('×'),
    Some('Ø'), Some('Ù'), Some('Ú'), Some('Û'),
    Some('Ü'), Some('Ý'), Some('Þ'), Some('ß'),
    // 0xE0-0xEF
    Some('à'), Some('á'), Some('â'), Some('ã'),
    Some('ä'), Some('å'), Some('æ'), Some('ç'),
    Some('è'), Some('é'), Some('ê'), Some('ë'),
    Some('ì'), Some('í'), Some('î'), Some('ï'),
    // 0xF0-0xFF
    Some('ð'), Some('ñ'), Some('ò'), Some('ó'),
    Some('ô'), Some('õ'), Some('ö'), Some('÷'),
    Some('ø'), Some('ù'), Some('ú'), Some('û'),
    Some('ü'), Some('ý'), Some('þ'), Some('ÿ'),
];

#[rustfmt::skip]
const MAC_ROMAN_ENCODING: [Option<char>; 256] = [
    // 0x00-0x1F
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    // 0x20-0x2F  (ASCII range)
    Some(' '), Some('!'), Some('"'), Some('#'),
    Some('$'), Some('%'), Some('&'), Some('\''),
    Some('('), Some(')'), Some('*'), Some('+'),
    Some(','), Some('-'), Some('.'), Some('/'),
    // 0x30-0x3F
    Some('0'), Some('1'), Some('2'), Some('3'),
    Some('4'), Some('5'), Some('6'), Some('7'),
    Some('8'), Some('9'), Some(':'), Some(';'),
    Some('<'), Some('='), Some('>'), Some('?'),
    // 0x40-0x4F
    Some('@'), Some('A'), Some('B'), Some('C'),
    Some('D'), Some('E'), Some('F'), Some('G'),
    Some('H'), Some('I'), Some('J'), Some('K'),
    Some('L'), Some('M'), Some('N'), Some('O'),
    // 0x50-0x5F
    Some('P'), Some('Q'), Some('R'), Some('S'),
    Some('T'), Some('U'), Some('V'), Some('W'),
    Some('X'), Some('Y'), Some('Z'), Some('['),
    Some('\\'), Some(']'), Some('^'), Some('_'),
    // 0x60-0x6F
    Some('`'), Some('a'), Some('b'), Some('c'),
    Some('d'), Some('e'), Some('f'), Some('g'),
    Some('h'), Some('i'), Some('j'), Some('k'),
    Some('l'), Some('m'), Some('n'), Some('o'),
    // 0x70-0x7F
    Some('p'), Some('q'), Some('r'), Some('s'),
    Some('t'), Some('u'), Some('v'), Some('w'),
    Some('x'), Some('y'), Some('z'), Some('{'),
    Some('|'), Some('}'), Some('~'), None,
    // 0x80-0x8F  (Mac Roman upper)
    Some('Ä'), Some('Å'), Some('Ç'), Some('É'),
    Some('Ñ'), Some('Ö'), Some('Ü'), Some('á'),
    Some('à'), Some('â'), Some('ä'), Some('ã'),
    Some('å'), Some('ç'), Some('é'), Some('è'),
    // 0x90-0x9F
    Some('ê'), Some('ë'), Some('í'), Some('ì'),
    Some('î'), Some('ï'), Some('ñ'), Some('ó'),
    Some('ò'), Some('ô'), Some('ö'), Some('õ'),
    Some('ú'), Some('ù'), Some('û'), Some('ü'),
    // 0xA0-0xAF
    Some('†'), Some('°'), Some('¢'), Some('£'),
    Some('§'), Some('•'), Some('¶'), Some('ß'),
    Some('®'), Some('©'), Some('™'), Some('´'),
    Some('¨'), Some('≠'), Some('Æ'), Some('Ø'),
    // 0xB0-0xBF
    Some('∞'), Some('±'), Some('≤'), Some('≥'),
    Some('¥'), Some('µ'), Some('∂'), Some('∑'),
    Some('∏'), Some('π'), Some('∫'), Some('ª'),
    Some('º'), Some('\u{2126}'), Some('æ'), Some('ø'), // Ω = U+2126
    // 0xC0-0xCF
    Some('¿'), Some('¡'), Some('¬'), Some('√'),
    Some('ƒ'), Some('≈'), Some('∆'), Some('«'),
    Some('»'), Some('…'), Some('\u{00A0}'), Some('À'), // 0xCA = NBSP
    Some('Ã'), Some('Õ'), Some('Œ'), Some('œ'),
    // 0xD0-0xDF
    Some('–'), Some('—'), Some('"'), Some('"'),
    Some('\u{2018}'), Some('\u{2019}'), Some('÷'), Some('\u{25CA}'), // lozenge
    Some('ÿ'), Some('Ÿ'), Some('⁄'), Some('¤'),   // 0xDB=currency(¤) per lopdf
    Some('‹'), Some('›'), Some('\u{FB01}'), Some('\u{FB02}'), // fi, fl
    // 0xE0-0xEF
    Some('‡'), Some('·'), Some('‚'), Some('„'),
    Some('‰'), Some('Â'), Some('Ê'), Some('Á'),
    Some('Ë'), Some('È'), Some('Í'), Some('Î'),
    Some('Ï'), Some('Ì'), Some('Ó'), Some('Ô'),
    // 0xF0-0xFF
    Some('\u{F8FF}'), Some('Ò'), Some('Ú'), Some('Û'), // 0xF0 = Apple logo (PUA)
    Some('Ù'), Some('ı'), Some('ˆ'), Some('˜'),
    Some('¯'), Some('˘'), Some('˙'), Some('˚'),
    Some('¸'), Some('˝'), Some('˛'), Some('ˇ'),
];

#[rustfmt::skip]
const STANDARD_ENCODING: [Option<char>; 256] = [
    // 0x00-0x1F
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    // 0x20-0x2F
    Some(' '), Some('!'), Some('"'), Some('#'),
    Some('$'), Some('%'), Some('&'), Some('\u{2019}'), // 0x27 = quoteright
    Some('('), Some(')'), Some('*'), Some('+'),
    Some(','), Some('-'), Some('.'), Some('/'),
    // 0x30-0x3F
    Some('0'), Some('1'), Some('2'), Some('3'),
    Some('4'), Some('5'), Some('6'), Some('7'),
    Some('8'), Some('9'), Some(':'), Some(';'),
    Some('<'), Some('='), Some('>'), Some('?'),
    // 0x40-0x4F
    Some('@'), Some('A'), Some('B'), Some('C'),
    Some('D'), Some('E'), Some('F'), Some('G'),
    Some('H'), Some('I'), Some('J'), Some('K'),
    Some('L'), Some('M'), Some('N'), Some('O'),
    // 0x50-0x5F
    Some('P'), Some('Q'), Some('R'), Some('S'),
    Some('T'), Some('U'), Some('V'), Some('W'),
    Some('X'), Some('Y'), Some('Z'), Some('['),
    Some('\\'), Some(']'), Some('^'), Some('_'),
    // 0x60-0x6F  (0x60 = quoteleft)
    Some('\u{2018}'), Some('a'), Some('b'), Some('c'),
    Some('d'), Some('e'), Some('f'), Some('g'),
    Some('h'), Some('i'), Some('j'), Some('k'),
    Some('l'), Some('m'), Some('n'), Some('o'),
    // 0x70-0x7F
    Some('p'), Some('q'), Some('r'), Some('s'),
    Some('t'), Some('u'), Some('v'), Some('w'),
    Some('x'), Some('y'), Some('z'), Some('{'),
    Some('|'), Some('}'), Some('~'), None,
    // 0x80-0xA0: undefined
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None,
    // 0xA1-0xAF
    Some('¡'), Some('¢'), Some('£'), Some('⁄'),  // 0xA4 = fraction U+2044
    Some('¥'), Some('ƒ'), Some('§'), Some('¤'),   // 0xA8 = currency U+00A4
    Some('\''), Some('"'), Some('«'), Some('‹'),
    Some('›'), Some('\u{FB01}'), Some('\u{FB02}'),  // fi, fl
    // 0xB0-0xBF
    None, Some('–'), Some('†'), Some('‡'),
    Some('·'), None, Some('¶'), Some('•'),
    Some('‚'), Some('„'), Some('"'), Some('»'),
    Some('…'), Some('‰'), None, Some('¿'),
    // 0xC0-0xCF
    None, Some('`'), Some('´'), Some('ˆ'),
    Some('˜'), Some('¯'), Some('˘'), Some('˙'),
    Some('¨'), None, Some('˚'), Some('¸'),
    None, Some('˝'), Some('˛'), Some('ˇ'),
    // 0xD0-0xDF
    Some('—'), None, None, None,
    None, None, None, None,
    None, None, None, None,
    None, None, None, None,
    // 0xE0-0xEF
    None, Some('Æ'), None, Some('ª'),
    None, None, None, None,
    Some('Ł'), Some('Ø'), Some('Œ'), Some('º'),
    None, None, None, None,
    // 0xF0-0xFF
    None, Some('æ'), None, None,
    None, Some('ı'), None, None,
    Some('ł'), Some('ø'), Some('œ'), Some('ß'),
    None, None, None, None,
];

// ---------------------------------------------------------------------------
// AGL subset: glyph name → char (binary-search via sorted table)
// ---------------------------------------------------------------------------

fn glyph_name_to_char(name: &[u8]) -> Option<char> {
    let s = std::str::from_utf8(name).ok()?;

    // First try AGL static table lookup.
    if let Ok(i) = AGL_TABLE.binary_search_by_key(&s, |&(n, _)| n) {
        return Some(AGL_TABLE[i].1);
    }

    // Fall back to uni<XXXX> / u<XXXX> pattern (AGL 2.0).
    let hex = s.strip_prefix("uni").or_else(|| s.strip_prefix('u'))?;

    // Guard: hex string length must be 1-8 chars (valid u32 in hex: 0x0 to 0xFFFFFFFF).
    // Longer strings are invalid; silently reject to avoid surprising behavior.
    if hex.is_empty() || hex.len() > 8 {
        return None;
    }

    let cp = u32::from_str_radix(hex, 16).ok()?;
    char::from_u32(cp)
}

/// Sorted by glyph name (required for binary_search_by_key).
static AGL_TABLE: &[(&str, char)] = &[
    // A
    ("A", 'A'),
    ("AE", 'Æ'),
    ("Aacute", 'Á'),
    ("Abreve", 'Ă'),
    ("Acircumflex", 'Â'),
    ("Adieresis", 'Ä'),
    ("Agrave", 'À'),
    ("Amacron", 'Ā'),
    ("Aogonek", 'Ą'),
    ("Aring", 'Å'),
    ("Atilde", 'Ã'),
    // B–D
    ("B", 'B'),
    ("C", 'C'),
    ("Cacute", 'Ć'),
    ("Ccaron", 'Č'),
    ("Ccedilla", 'Ç'),
    ("D", 'D'),
    ("Dcaron", 'Ď'),
    ("Dcroat", 'Đ'),
    ("Delta", '∆'),
    // E
    ("E", 'E'),
    ("Eacute", 'É'),
    ("Ecaron", 'Ě'),
    ("Ecircumflex", 'Ê'),
    ("Edieresis", 'Ë'),
    ("Egrave", 'È'),
    ("Emacron", 'Ē'),
    ("Eogonek", 'Ę'),
    ("Eth", 'Ð'),
    ("Euro", '€'),
    // F–H
    ("F", 'F'),
    ("G", 'G'),
    ("Gbreve", 'Ğ'),
    ("H", 'H'),
    // I–K
    ("I", 'I'),
    ("Iacute", 'Í'),
    ("Icircumflex", 'Î'),
    ("Idieresis", 'Ï'),
    ("Idotaccent", 'İ'),
    ("Igrave", 'Ì'),
    ("Imacron", 'Ī'),
    ("Iogonek", 'Į'),
    ("J", 'J'),
    ("K", 'K'),
    // L
    ("L", 'L'),
    ("Lacute", 'Ĺ'),
    ("Lcaron", 'Ľ'),
    ("Lcommaaccent", 'Ļ'),
    ("Lslash", 'Ł'),
    // M–N
    ("M", 'M'),
    ("N", 'N'),
    ("Nacute", 'Ń'),
    ("Ncaron", 'Ň'),
    ("Ncommaaccent", 'Ņ'),
    ("Ntilde", 'Ñ'),
    // O
    ("O", 'O'),
    ("OE", 'Œ'),
    ("Oacute", 'Ó'),
    ("Ocircumflex", 'Ô'),
    ("Odblacute", 'Ő'),
    ("Odieresis", 'Ö'),
    ("Ograve", 'Ò'),
    ("Omacron", 'Ō'),
    ("Omega", '\u{2126}'),
    ("Oslash", 'Ø'),
    ("Otilde", 'Õ'),
    // P–R
    ("P", 'P'),
    ("Q", 'Q'),
    ("R", 'R'),
    ("Racute", 'Ŕ'),
    ("Rcaron", 'Ř'),
    ("Rcommaaccent", 'Ŗ'),
    // S
    ("S", 'S'),
    ("Sacute", 'Ś'),
    ("Scaron", 'Š'),
    ("Scedilla", 'Ş'),
    ("Scommaaccent", 'Ș'),
    // T
    ("T", 'T'),
    ("Tcaron", 'Ť'),
    ("Tcedilla", 'Ţ'),
    ("Tcommaaccent", 'Ț'),
    ("Thorn", 'Þ'),
    // U
    ("U", 'U'),
    ("Uacute", 'Ú'),
    ("Ucircumflex", 'Û'),
    ("Udblacute", 'Ű'),
    ("Udieresis", 'Ü'),
    ("Ugrave", 'Ù'),
    ("Umacron", 'Ū'),
    ("Uogonek", 'Ų'),
    ("Uring", 'Ů'),
    ("V", 'V'),
    ("W", 'W'),
    ("X", 'X'),
    // Y–Z
    ("Y", 'Y'),
    ("Yacute", 'Ý'),
    ("Ydieresis", 'Ÿ'),
    ("Z", 'Z'),
    ("Zacute", 'Ź'),
    ("Zcaron", 'Ž'),
    ("Zdotaccent", 'Ż'),
    // a
    ("a", 'a'),
    ("aacute", 'á'),
    ("abreve", 'ă'),
    ("acircumflex", 'â'),
    ("adieresis", 'ä'),
    ("ae", 'æ'),
    ("agrave", 'à'),
    ("amacron", 'ā'),
    ("ampersand", '&'),
    ("aogonek", 'ą'),
    ("approxequal", '≈'),
    ("aring", 'å'),
    ("asciicircum", '^'),
    ("asciitilde", '~'),
    ("asterisk", '*'),
    ("at", '@'),
    ("atilde", 'ã'),
    // b–c
    ("b", 'b'),
    ("backslash", '\\'),
    ("bar", '|'),
    ("braceleft", '{'),
    ("braceright", '}'),
    ("bracketleft", '['),
    ("bracketright", ']'),
    ("breve", '˘'),
    ("brokenbar", '¦'),
    ("bullet", '•'),
    ("c", 'c'),
    ("cacute", 'ć'),
    ("caron", 'ˇ'),
    ("ccaron", 'č'),
    ("ccedilla", 'ç'),
    ("cedilla", '¸'),
    ("cent", '¢'),
    ("circumflex", 'ˆ'),
    ("colon", ':'),
    ("comma", ','),
    ("copyright", '©'),
    ("currency", '¤'),
    // d
    ("d", 'd'),
    ("dagger", '†'),
    ("daggerdbl", '‡'),
    ("dcaron", 'ď'),
    ("dcroat", 'đ'),
    ("degree", '°'),
    ("dieresis", '¨'),
    ("divide", '÷'),
    ("dollar", '$'),
    ("dotaccent", '˙'),
    ("dotlessi", 'ı'),
    // e
    ("e", 'e'),
    ("eacute", 'é'),
    ("ecaron", 'ě'),
    ("ecircumflex", 'ê'),
    ("edieresis", 'ë'),
    ("egrave", 'è'),
    ("eight", '8'),
    ("ellipsis", '…'),
    ("emacron", 'ē'),
    ("emdash", '—'),
    ("endash", '–'),
    ("eogonek", 'ę'),
    ("equal", '='),
    ("eth", 'ð'),
    ("euro", '€'),
    ("exclam", '!'),
    ("exclamdown", '¡'),
    // f
    ("f", 'f'),
    ("ff", '\u{FB00}'),
    ("ffi", '\u{FB03}'),
    ("ffl", '\u{FB04}'),
    ("fi", '\u{FB01}'),
    ("five", '5'),
    ("fl", '\u{FB02}'),
    ("florin", 'ƒ'),
    ("four", '4'),
    ("fraction", '⁄'),
    // g
    ("g", 'g'),
    ("gbreve", 'ğ'),
    ("germandbls", 'ß'),
    ("grave", '`'),
    ("greater", '>'),
    ("greaterequal", '≥'),
    ("guillemotleft", '«'),
    ("guillemotright", '»'),
    ("guilsinglleft", '‹'),
    ("guilsinglright", '›'),
    // h–i
    ("h", 'h'),
    ("hungarumlaut", '˝'),
    ("hyphen", '-'),
    ("i", 'i'),
    ("iacute", 'í'),
    ("icircumflex", 'î'),
    ("idieresis", 'ï'),
    ("idotaccent", 'ı'),
    ("igrave", 'ì'),
    ("imacron", 'ī'),
    ("infinity", '∞'),
    ("integral", '∫'),
    ("iogonek", 'į'),
    // j–k
    ("j", 'j'),
    ("k", 'k'),
    // l
    ("l", 'l'),
    ("lacute", 'ĺ'),
    ("lcaron", 'ľ'),
    ("lcommaaccent", 'ļ'),
    ("less", '<'),
    ("lessequal", '≤'),
    ("logicalnot", '¬'),
    ("lozenge", '◊'),
    ("lslash", 'ł'),
    // m–n
    ("m", 'm'),
    ("macron", '¯'),
    ("mu", 'µ'),
    ("multiply", '×'),
    ("n", 'n'),
    ("nacute", 'ń'),
    ("ncaron", 'ň'),
    ("ncommaaccent", 'ņ'),
    ("nine", '9'),
    ("notequal", '≠'),
    ("ntilde", 'ñ'),
    ("numbersign", '#'),
    // o
    ("o", 'o'),
    ("oacute", 'ó'),
    ("ocircumflex", 'ô'),
    ("odblacute", 'ő'),
    ("odieresis", 'ö'),
    ("oe", 'œ'),
    ("ogonek", '˛'),
    ("ograve", 'ò'),
    ("omacron", 'ō'),
    ("one", '1'),
    ("onehalf", '½'),
    ("onequarter", '¼'),
    ("onesuperior", '¹'),
    ("ordfeminine", 'ª'),
    ("ordmasculine", 'º'),
    ("oslash", 'ø'),
    ("otilde", 'õ'),
    // p–q
    ("p", 'p'),
    ("paragraph", '¶'),
    ("parenleft", '('),
    ("parenright", ')'),
    ("partialdiff", '∂'),
    ("percent", '%'),
    ("period", '.'),
    ("periodcentered", '·'),
    ("perthousand", '‰'),
    ("pi", 'π'),
    ("plus", '+'),
    ("plusminus", '±'),
    ("product", '∏'),
    ("q", 'q'),
    ("question", '?'),
    ("questiondown", '¿'),
    ("quotedbl", '"'),
    ("quotedblbase", '„'),
    ("quotedblleft", '"'),
    ("quotedblright", '"'),
    ("quoteleft", '\u{2018}'),
    ("quoteright", '\u{2019}'),
    ("quotesinglbase", '‚'),
    ("quotesingle", '\''),
    // r
    ("r", 'r'),
    ("racute", 'ŕ'),
    ("radical", '√'),
    ("rcaron", 'ř'),
    ("rcommaaccent", 'ŗ'),
    ("registered", '®'),
    ("ring", '˚'),
    // s
    ("s", 's'),
    ("sacute", 'ś'),
    ("scaron", 'š'),
    ("scedilla", 'ş'),
    ("scommaaccent", 'ș'),
    ("section", '§'),
    ("semicolon", ';'),
    ("seven", '7'),
    ("six", '6'),
    ("slash", '/'),
    ("space", ' '),
    ("sterling", '£'),
    ("summation", '∑'),
    // t
    ("t", 't'),
    ("tcaron", 'ť'),
    ("tcedilla", 'ţ'),
    ("tcommaaccent", 'ț'),
    ("thorn", 'þ'),
    ("three", '3'),
    ("threequarters", '¾'),
    ("threesuperior", '³'),
    ("tilde", '˜'),
    ("trademark", '™'),
    ("two", '2'),
    ("twosuperior", '²'),
    // u
    ("u", 'u'),
    ("uacute", 'ú'),
    ("ucircumflex", 'û'),
    ("udblacute", 'ű'),
    ("udieresis", 'ü'),
    ("ugrave", 'ù'),
    ("umacron", 'ū'),
    ("underscore", '_'),
    ("uogonek", 'ų'),
    ("uring", 'ů'),
    // v–x
    ("v", 'v'),
    ("w", 'w'),
    ("x", 'x'),
    // y–z
    ("y", 'y'),
    ("yacute", 'ý'),
    ("ydieresis", 'ÿ'),
    ("yen", '¥'),
    ("z", 'z'),
    ("zacute", 'ź'),
    ("zcaron", 'ž'),
    ("zdotaccent", 'ż'),
    ("zero", '0'),
];

// ---------------------------------------------------------------------------
// ToUnicode CMap parser — handles beginbfchar and beginbfrange
// ---------------------------------------------------------------------------

fn parse_to_unicode_cmap(bytes: &[u8]) -> BTreeMap<u16, char> {
    let mut map = BTreeMap::new();
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return map,
    };

    enum Section {
        None,
        BfChar,
        BfRange,
    }
    let mut section = Section::None;

    for line in text.lines() {
        let line = line.trim();
        if line.ends_with("beginbfchar") {
            section = Section::BfChar;
            continue;
        }
        if line == "endbfchar" {
            section = Section::None;
            continue;
        }
        if line.ends_with("beginbfrange") {
            section = Section::BfRange;
            continue;
        }
        if line == "endbfrange" {
            section = Section::None;
            continue;
        }
        match section {
            Section::BfChar => parse_bfchar_line(line, &mut map),
            Section::BfRange => parse_bfrange_line(line, &mut map),
            Section::None => {}
        }
    }
    map
}

fn parse_bfchar_line(line: &str, map: &mut BTreeMap<u16, char>) {
    let mut parts = line.split_ascii_whitespace();
    let gid_tok = match parts.next() {
        Some(s) => s,
        None => return,
    };
    let uni_tok = match parts.next() {
        Some(s) => s,
        None => return,
    };

    let gid_hex = gid_tok.trim_start_matches('<').trim_end_matches('>');
    let uni_hex = uni_tok.trim_start_matches('<').trim_end_matches('>');

    let Ok(gid) = u16::from_str_radix(gid_hex, 16) else {
        return;
    };

    let ch = hex_to_char(uni_hex);
    if let Some(ch) = ch {
        map.insert(gid, ch);
    }
}

fn parse_bfrange_line(line: &str, map: &mut BTreeMap<u16, char>) {
    // <lo> <hi> <dst>  or  <lo> <hi> [<u1> <u2> ...]
    // Use split_ascii_whitespace so tabs / multiple spaces between tokens are handled.
    let mut toks = line.split_ascii_whitespace();
    let lo_tok = match toks.next() {
        Some(s) => s,
        None => return,
    };
    let hi_tok = match toks.next() {
        Some(s) => s,
        None => return,
    };
    // Reconstruct rest from the original line starting at the third non-whitespace span.
    let rest = {
        let skip2 = line
            .trim_start()
            .trim_start_matches(|c: char| !c.is_ascii_whitespace()) // skip lo_tok
            .trim_start_matches(|c: char| c.is_ascii_whitespace()) // skip ws
            .trim_start_matches(|c: char| !c.is_ascii_whitespace()) // skip hi_tok
            .trim_start();
        if skip2.is_empty() {
            return;
        }
        skip2
    };

    let lo_hex = lo_tok.trim_start_matches('<').trim_end_matches('>');
    let hi_hex = hi_tok.trim_start_matches('<').trim_end_matches('>');
    let Ok(lo) = u16::from_str_radix(lo_hex, 16) else {
        return;
    };
    let Ok(hi) = u16::from_str_radix(hi_hex, 16) else {
        return;
    };
    if lo > hi {
        return;
    }

    if rest.starts_with('[') {
        // Explicit array form: [<u1> <u2> ...]
        let inner = rest.trim_start_matches('[').trim_end_matches(']');
        let mut code = lo;
        for tok in inner.split_whitespace() {
            if code > hi {
                break;
            }
            let hex = tok.trim_start_matches('<').trim_end_matches('>');
            if let Some(ch) = hex_to_char(hex) {
                map.insert(code, ch);
            }
            code = code.saturating_add(1);
        }
    } else {
        // Contiguous range: <dst_start>
        let dst_hex = rest.trim_start_matches('<').trim_end_matches('>');
        let Ok(dst_start) = u32::from_str_radix(dst_hex, 16) else {
            return;
        };
        for i in 0..=(hi as u32).saturating_sub(lo as u32) {
            let code = lo + i as u16;
            // Guard against adversarially crafted CMaps with dst_start near u32::MAX.
            let Some(cp) = dst_start.checked_add(i) else {
                break;
            };
            if let Some(ch) = char::from_u32(cp) {
                map.insert(code, ch);
            }
        }
    }
}

/// Decode a hex string from a CMap entry to a char.
/// Handles 2-byte (BMP) and 4-byte (surrogate pair) forms.
fn hex_to_char(hex: &str) -> Option<char> {
    match hex.len() {
        1 | 2 => {
            let cp = u32::from_str_radix(hex, 16).ok()?;
            char::from_u32(cp)
        }
        3 | 4 => {
            let cp = u32::from_str_radix(hex, 16).ok()?;
            char::from_u32(cp)
        }
        8 => {
            // UTF-16BE surrogate pair
            let hi = u16::from_str_radix(&hex[0..4], 16).ok()?;
            let lo = u16::from_str_radix(&hex[4..8], 16).ok()?;
            if (0xD800..=0xDBFF).contains(&hi) && (0xDC00..=0xDFFF).contains(&lo) {
                let cp = 0x10000u32 + ((hi as u32 - 0xD800) << 10) + (lo as u32 - 0xDC00);
                char::from_u32(cp)
            } else {
                // Treat as plain 32-bit codepoint
                let cp = u32::from_str_radix(hex, 16).ok()?;
                char::from_u32(cp)
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// /W array parser for CIDFont advance widths (unchanged)
// ---------------------------------------------------------------------------

fn parse_w_array(arr: &[Object]) -> Vec<WidthRun> {
    let mut runs = Vec::new();
    let mut i = 0;

    while i < arr.len() {
        let start_gid = match arr[i].as_i64() {
            Ok(n) => n as u16,
            Err(_) => {
                i += 1;
                continue;
            }
        };
        i += 1;
        if i >= arr.len() {
            break;
        }

        match &arr[i] {
            Object::Array(widths_arr) => {
                let widths: Vec<u32> = widths_arr
                    .iter()
                    .filter_map(|o| o.as_i64().ok().map(|n| n as u32))
                    .collect();
                runs.push(WidthRun { start_gid, widths });
                i += 1;
            }
            Object::Integer(_) | Object::Real(_) => {
                let end_gid = match arr[i].as_i64() {
                    Ok(n) => n as u16,
                    Err(_) => {
                        i += 1;
                        continue;
                    }
                };
                i += 1;
                if i >= arr.len() {
                    break;
                }
                let w = match arr[i].as_i64() {
                    Ok(n) => n as u32,
                    Err(_) => {
                        i += 1;
                        continue;
                    }
                };
                i += 1;
                let count = (end_gid as usize).saturating_sub(start_gid as usize) + 1;
                runs.push(WidthRun {
                    start_gid,
                    widths: vec![w; count],
                });
            }
            _ => {
                i += 1;
            }
        }
    }
    runs
}

// ---------------------------------------------------------------------------
// Step 3: Tokenizer
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Token {
    HexStr(Vec<u8>),
    LitStr(Vec<u8>),
    Name(Vec<u8>),
    Number(f32),
    Keyword(Vec<u8>),
    Array(Vec<Token>),
}

/// Tokenize a PDF content stream.  Returns `(token, byte_offset)` pairs where
/// `byte_offset` is the index of the first byte of that token in `input`.
/// Keyword offsets are used by `parse_content_stream` to populate
/// `TextFragment::source_op_start` / `source_op_end`.
fn tokenize(input: &[u8]) -> Vec<(Token, usize)> {
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < input.len() {
        let b = input[i];

        if is_pdf_whitespace(b) {
            i += 1;
            continue;
        }
        if b == b'%' {
            while i < input.len() && input[i] != b'\r' && input[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'<' {
            let tok_start = i;
            if i + 1 < input.len() && input[i + 1] == b'<' {
                // Dictionary literal — skip until >>
                i += 2;
                while i + 1 < input.len() && !(input[i] == b'>' && input[i + 1] == b'>') {
                    i += 1;
                }
                if i + 1 < input.len() {
                    i += 2;
                }
                continue;
            }
            // Hex string
            i += 1;
            let start = i;
            while i < input.len() && input[i] != b'>' {
                i += 1;
            }
            let hex = &input[start..i];
            if i < input.len() {
                i += 1;
            }
            tokens.push((Token::HexStr(decode_hex_bytes(hex)), tok_start));
            continue;
        }
        if b == b'/' {
            let tok_start = i;
            i += 1;
            let start = i;
            while i < input.len() && !is_pdf_whitespace(input[i]) && !is_pdf_delimiter(input[i]) {
                i += 1;
            }
            tokens.push((Token::Name(input[start..i].to_vec()), tok_start));
            continue;
        }
        if b == b'[' {
            let tok_start = i;
            i += 1;
            let (arr, consumed) = parse_array_tokens(&input[i..]);
            i += consumed;
            tokens.push((Token::Array(arr), tok_start));
            continue;
        }
        if b == b']' {
            i += 1;
            continue;
        }
        if b == b'(' {
            let tok_start = i;
            let (bytes, end_i) = parse_literal_string(input, i + 1);
            i = end_i;
            tokens.push((Token::LitStr(bytes), tok_start));
            continue;
        }

        // Number or keyword
        let start = i;
        while i < input.len() && !is_pdf_whitespace(input[i]) && !is_pdf_delimiter(input[i]) {
            i += 1;
        }
        let word = &input[start..i];
        if word.is_empty() {
            i += 1;
            continue;
        }
        if let Ok(s) = std::str::from_utf8(word)
            && let Ok(n) = s.parse::<f32>()
            && n.is_finite()
        {
            tokens.push((Token::Number(n), start));
            continue;
        }
        tokens.push((Token::Keyword(word.to_vec()), start));
    }

    tokens
}

fn parse_array_tokens(input: &[u8]) -> (Vec<Token>, usize) {
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < input.len() {
        let b = input[i];
        if is_pdf_whitespace(b) {
            i += 1;
            continue;
        }
        if b == b']' {
            i += 1;
            return (tokens, i);
        }
        if b == b'<' && (i + 1 >= input.len() || input[i + 1] != b'<') {
            i += 1;
            let start = i;
            while i < input.len() && input[i] != b'>' {
                i += 1;
            }
            let hex = &input[start..i];
            if i < input.len() {
                i += 1;
            }
            tokens.push(Token::HexStr(decode_hex_bytes(hex)));
            continue;
        }
        if b == b'(' {
            let (bytes, end_i) = parse_literal_string(input, i + 1);
            i = end_i;
            tokens.push(Token::LitStr(bytes));
            continue;
        }
        // Number or other
        let start = i;
        while i < input.len() && !is_pdf_whitespace(input[i]) && !is_pdf_delimiter(input[i]) {
            i += 1;
        }
        let word = &input[start..i];
        if word.is_empty() {
            i += 1;
            continue;
        }
        if let Ok(s) = std::str::from_utf8(word)
            && let Ok(n) = s.parse::<f32>()
        {
            tokens.push(Token::Number(n));
        }
        // Non-numeric token in array — skip
    }

    (tokens, i)
}

/// Parse a PDF literal string starting at `i` (the character after the opening `(`).
/// Returns (decoded_bytes, new_i) where new_i points past the closing `)`.
pub(crate) fn parse_literal_string(input: &[u8], mut i: usize) -> (Vec<u8>, usize) {
    let mut depth = 1i32;
    let mut out = Vec::new();

    while i < input.len() && depth > 0 {
        match input[i] {
            b'\\' => {
                i += 1;
                if i >= input.len() {
                    break;
                }
                match input[i] {
                    b'n' => {
                        out.push(b'\n');
                        i += 1;
                    }
                    b'r' => {
                        out.push(b'\r');
                        i += 1;
                    }
                    b't' => {
                        out.push(b'\t');
                        i += 1;
                    }
                    b'\\' => {
                        out.push(b'\\');
                        i += 1;
                    }
                    b'(' => {
                        out.push(b'(');
                        i += 1;
                    }
                    b')' => {
                        out.push(b')');
                        i += 1;
                    }
                    b'\r' => {
                        // Line continuation: \<CR> or \<CR><LF>
                        i += 1;
                        if i < input.len() && input[i] == b'\n' {
                            i += 1;
                        }
                    }
                    b'\n' => {
                        i += 1;
                    } // \<LF> line continuation
                    d @ b'0'..=b'7' => {
                        // Octal escape: 1–3 digits
                        let mut val = (d - b'0') as u16;
                        i += 1;
                        let mut count = 1;
                        while count < 3 && i < input.len() && (b'0'..=b'7').contains(&input[i]) {
                            val = val * 8 + (input[i] - b'0') as u16;
                            i += 1;
                            count += 1;
                        }
                        out.push((val & 0xFF) as u8);
                    }
                    _ => {
                        out.push(input[i]);
                        i += 1;
                    }
                }
            }
            b'(' => {
                depth += 1;
                out.push(b'(');
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth > 0 {
                    out.push(b')');
                }
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    (out, i)
}

pub(crate) fn decode_hex_bytes(hex: &[u8]) -> Vec<u8> {
    let cleaned: Vec<u8> = hex
        .iter()
        .filter(|&&b| !is_pdf_whitespace(b))
        .copied()
        .collect();
    let mut padded = cleaned;
    if !padded.len().is_multiple_of(2) {
        padded.push(b'0');
    }
    padded
        .chunks(2)
        .filter_map(|chunk| {
            let s = std::str::from_utf8(chunk).ok()?;
            u8::from_str_radix(s, 16).ok()
        })
        .collect()
}

pub(crate) fn is_pdf_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x0C | 0x00)
}

pub(crate) fn is_pdf_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

// ---------------------------------------------------------------------------
// Step 4: State machine over token stream
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// CTM (Current Transformation Matrix) helpers
// ---------------------------------------------------------------------------

const IDENTITY_CTM: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Compose two 2-D affine transforms: result = a × b.
/// Matrix layout: [a, b, c, d, e, f] represents the column-major form
/// | a  c  e |
/// | b  d  f |
/// | 0  0  1 |
fn multiply_ctm(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

/// Transform a point from local space to page space under a CTM.
fn apply_ctm(m: [f32; 6], x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// Uniform scale factor for lengths (width, font_size) under a CTM.
/// Uses the X-column norm sqrt(a² + b²).
fn ctm_scale(m: [f32; 6]) -> f32 {
    (m[0] * m[0] + m[1] * m[1]).sqrt()
}

/// Read a /Matrix array from an XObject or Form dict; returns identity if absent or malformed.
fn read_matrix(dict: &lopdf::Dictionary) -> [f32; 6] {
    dict.get(b"Matrix")
        .ok()
        .and_then(|o| o.as_array().ok())
        .and_then(|arr| {
            if arr.len() < 6 {
                return None;
            }
            let mut m = [0f32; 6];
            for (i, v) in arr[..6].iter().enumerate() {
                m[i] = v.as_float().ok()?;
            }
            Some(m)
        })
        .unwrap_or(IDENTITY_CTM)
}

// ---------------------------------------------------------------------------

/// Graphics and text state carried across multiple content streams on the same page.
///
/// Per the PDF spec, the graphics state (colour, render mode, CTM, etc.) persists
/// across streams when a page `/Contents` is an array of streams.  Text state also
/// persists: some generators (PScript5.dll/Distiller) split a single BT…ET block
/// across stream boundaries, so `in_bt`, the current font, and text position must
/// survive stream transitions too.
struct ParseCarryState {
    cur_color: [f32; 3],
    cur_render_mode: u8,
    /// CTM at the most recent `Do` invocation (used as fallback by XObject extraction).
    ctm: [f32; 6],
    /// Per-Do CTM map: each entry is `(xobj_name, ctm_at_do_time)` in stream order.
    /// `extract_text_from_xobjects` uses this so every XObject gets the CTM that was
    /// active at the specific `Do` that invoked it, not the last one in the stream.
    do_ctm_map: Vec<(Vec<u8>, [f32; 6])>,
    /// CTM stack shared across multiple content streams on the same page.
    /// Per the PDF spec, multiple streams in a Contents array share the same graphics
    /// state — so q/Q depth and cm transformations must persist across stream calls.
    ctm_stack: Vec<[f32; 6]>,
    /// Whether we are inside an open BT…ET block that was not closed before the
    /// stream ended.  Distiller/PScript5 PDFs occasionally split one logical BT block
    /// across several stream objects; carrying this flag lets following streams treat
    /// bare Tj/TJ operators as valid rather than silently dropping them.
    in_bt: bool,
    /// Current font name (set by `Tf`), carried so bare Tj in subsequent streams can
    /// still resolve a font when `in_bt` was inherited from a previous stream.
    font_name: Vec<u8>,
    /// Raw font size from the last `Tf` operator.
    tf_font_size: f32,
    /// Effective font size after Tm y-scale.
    font_size: f32,
    /// Y-axis scale from the last `Tm` matrix.
    tm_y_scale: f32,
    /// Current text X position.
    text_x: f32,
    /// Current text Y position.
    text_y: f32,
    /// X coordinate from the most recent `Tm` operator (column anchor).
    /// Updated only on `Tm`, never on `Td`/`TD` or after glyph advances.
    /// Carried across stream boundaries alongside `text_x/y`.
    tm_origin_x: f32,
    /// Y coordinate from the most recent `Tm` operator.
    tm_origin_y: f32,
    /// `true` once a `Tm` operator has been seen in the current BT block.
    /// Reset to `false` on `BT`.  When `false`, `tm_origin_x/y` are not exposed
    /// in `TextFragment::tm_origin_x` (both remain `None`).
    tm_origin_set: bool,
    /// X-scale from the most recent `Tm` matrix: √(a² + b²).
    /// Reset to 1.0 on `BT`.  Used to scale `Td` horizontal offsets and glyph widths.
    tm_x_scale: f32,
    /// Text line matrix (T_lm) x translation.  Updated by `Tm` and by `Td` per PDF spec.
    /// On `Td`, T_m is reset to T_lm_new; accumulated glyph advances are cleared.
    tm_lm_x: f32,
    /// Text line matrix (T_lm) y translation.  Paired with `tm_lm_x`.
    tm_lm_y: f32,
    /// Current text leading (set by `TL` and as side-effect of `TD`).
    /// Used by `T*` (≡ `0 -TL Td`).  Persists across BT/ET and content streams.
    text_leading: f32,
    /// Character spacing added after each glyph (set by `Tc`, default 0).
    char_spacing: f32,
    /// Word spacing added after each space glyph (set by `Tw`, default 0).
    word_spacing: f32,
}

impl Default for ParseCarryState {
    fn default() -> Self {
        Self {
            cur_color: [0.0, 0.0, 0.0],
            cur_render_mode: 0,
            ctm: IDENTITY_CTM,
            do_ctm_map: Vec::new(),
            ctm_stack: vec![IDENTITY_CTM],
            in_bt: false,
            font_name: Vec::new(),
            tf_font_size: 12.0,
            font_size: 12.0,
            tm_y_scale: 1.0,
            text_x: 0.0,
            text_y: 0.0,
            tm_origin_x: 0.0,
            tm_origin_y: 0.0,
            tm_origin_set: false,
            tm_x_scale: 1.0,
            tm_lm_x: 0.0,
            tm_lm_y: 0.0,
            text_leading: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
        }
    }
}

fn parse_content_stream(
    bytes: &[u8],
    fonts: &HashMap<Vec<u8>, FontInfo>,
    state: &mut ParseCarryState,
    out: &mut Vec<TextFragment>,
    stream_idx: Option<usize>,
    xobj_id: Option<(u32, u16)>,
) {
    let tokens = tokenize(bytes);
    let mut stack: Vec<(Token, usize)> = Vec::new();
    // Read text state from carry so that BT blocks split across stream boundaries
    // (a Distiller/PScript5 pattern) are handled correctly.
    let mut in_bt          = state.in_bt;
    let mut font_name      = state.font_name.clone();
    let mut tf_font_size   = state.tf_font_size;
    let mut font_size      = state.font_size;
    let mut tm_y_scale     = state.tm_y_scale;
    let mut tm_x_scale     = state.tm_x_scale;
    let mut tm_lm_x        = state.tm_lm_x;
    let mut tm_lm_y        = state.tm_lm_y;
    let mut x              = state.text_x;
    let mut y              = state.text_y;
    let mut tm_origin_set  = state.tm_origin_set;
    let mut text_leading   = state.text_leading;
    let mut char_spacing   = state.char_spacing;
    let mut word_spacing   = state.word_spacing;
    // CTM stack lives in state.ctm_stack so it persists across multiple content
    // streams on the same page (PDF spec: Contents array streams share graphics state).

    for (token, tok_pos) in tokens {
        match token {
            Token::Keyword(kw) => match kw.as_slice() {
                b"BT" => {
                    in_bt = true;
                    x = 0.0;
                    y = 0.0;
                    tm_origin_set = false;
                    tm_x_scale = 1.0;
                    tm_y_scale = 1.0;
                    tm_lm_x = 0.0;
                    tm_lm_y = 0.0;
                    stack.clear();
                }
                b"ET" => {
                    in_bt = false;
                    stack.clear();
                }
                b"TL" => {
                    if let Some((Token::Number(tl), _)) = stack.pop() {
                        text_leading = tl;
                    }
                    stack.clear();
                }
                b"Tc" => {
                    if let Some((Token::Number(v), _)) = stack.pop() {
                        char_spacing = v;
                    }
                    stack.clear();
                }
                b"Tw" => {
                    if let Some((Token::Number(v), _)) = stack.pop() {
                        word_spacing = v;
                    }
                    stack.clear();
                }
                b"Tf" if in_bt => {
                    let top = stack.pop();
                    let second = stack.pop();
                    if let (Some((Token::Number(size), _)), Some((Token::Name(name), _))) =
                        (top, second)
                    {
                        font_name = name;
                        tf_font_size = size;
                        // Per PDF spec, the text rendering matrix combines Tf size with
                        // the current Tm y-scale.  A Tf operator does not reset the Tm
                        // matrix, so the effective font size must stay tf × tm_y_scale.
                        font_size = size * tm_y_scale;
                    }
                    stack.clear();
                }
                b"Td" | b"TD" if in_bt => {
                    let top = stack.pop();
                    let second = stack.pop();
                    if let (Some((Token::Number(ty), _)), Some((Token::Number(tx), _))) =
                        (top, second)
                    {
                        // PDF spec: Td sets T_lm_new = [[1,0,0],[0,1,0],[tx,ty,1]] × T_lm
                        // and resets T_m = T_lm_new (clears intra-line glyph-advance drift).
                        // For axis-aligned Tm: new_lm = tx*tm_x_scale + lm_x, ty*tm_y_scale + lm_y.
                        // For rotated Tm the full a/b/c/d matrix is required; this is an
                        // approximation that is exact for the common axis-aligned case.
                        let new_lm_x = tx * tm_x_scale + tm_lm_x;
                        let new_lm_y = ty * tm_y_scale + tm_lm_y;
                        tm_lm_x = new_lm_x;
                        tm_lm_y = new_lm_y;
                        x = new_lm_x;
                        y = new_lm_y;
                        // TD also sets the text leading: TL = -ty (PDF spec §9.4.1).
                        if kw.as_slice() == b"TD" {
                            text_leading = -ty;
                        }
                    }
                    stack.clear();
                }
                b"T*" if in_bt => {
                    // T* ≡ `0 -TL Td` (PDF spec §9.4.1).
                    let new_lm_x = tm_lm_x;
                    let new_lm_y = -text_leading * tm_y_scale + tm_lm_y;
                    tm_lm_x = new_lm_x;
                    tm_lm_y = new_lm_y;
                    x = new_lm_x;
                    y = new_lm_y;
                    stack.clear();
                }
                b"Tm" if in_bt => {
                    // Tm: a b c d e f Tm (stack top = f)
                    let pop_f = stack.pop(); // f = y translation
                    let pop_e = stack.pop(); // e = x translation
                    let pop_d = stack.pop(); // d = y-axis component of scale/rotation
                    let pop_c = stack.pop(); // c = y-axis component of skew/rotation
                    let pop_b = stack.pop(); // b = x-axis vertical component
                    let pop_a = stack.pop(); // a = x-axis horizontal component
                    if let (Some((Token::Number(fy), _)), Some((Token::Number(ex), _))) =
                        (pop_f, pop_e)
                    {
                        x = ex;
                        y = fy;
                        // Record the Tm-set position as the BT-block column anchor.
                        // tm_origin_x is NOT updated by Td; it stays at the Tm value.
                        state.tm_origin_x = ex;
                        state.tm_origin_y = fy;
                        tm_origin_set = true;
                        // Also reset T_lm to the Tm translation (Td will update from here).
                        tm_lm_x = ex;
                        tm_lm_y = fy;
                    }
                    // Compute effective font size from the Tm y-scale:
                    // y_scale = sqrt(c² + d²) handles both scaling and rotation.
                    if let (Some((Token::Number(dv), _)), Some((Token::Number(cv), _))) =
                        (pop_d, pop_c)
                    {
                        let y_scale = (cv * cv + dv * dv).sqrt();
                        if y_scale > 0.0 {
                            font_size = tf_font_size * y_scale;
                            tm_y_scale = y_scale;
                        }
                    }
                    // Compute x-scale from the Tm a/b components: sqrt(a² + b²).
                    // For axis-aligned Tm (no rotation) this is the horizontal scale factor
                    // used to transform Td offsets and glyph advance widths into user space.
                    if let (Some((Token::Number(av), _)), Some((Token::Number(bv), _))) =
                        (pop_a, pop_b)
                    {
                        let x_scale = (av * av + bv * bv).sqrt();
                        if x_scale > 0.0 {
                            tm_x_scale = x_scale;
                            state.tm_x_scale = x_scale;
                        }
                    }
                    stack.clear();
                }
                b"Tr" => {
                    if let Some((Token::Number(mode), _)) = stack.pop() {
                        state.cur_render_mode = mode as u8;
                    }
                    stack.clear();
                }
                b"rg" => {
                    let b_val = stack.pop();
                    let g_val = stack.pop();
                    let r_val = stack.pop();
                    if let (
                        Some((Token::Number(bv), _)),
                        Some((Token::Number(gv), _)),
                        Some((Token::Number(rv), _)),
                    ) = (b_val, g_val, r_val)
                    {
                        state.cur_color = [rv, gv, bv];
                    }
                    stack.clear();
                }
                b"g" => {
                    if let Some((Token::Number(gray), _)) = stack.pop() {
                        state.cur_color = [gray, gray, gray];
                    }
                    stack.clear();
                }
                b"q" => {
                    state.ctm_stack.push(*state.ctm_stack.last().unwrap_or(&IDENTITY_CTM));
                    stack.clear();
                }
                b"Q" => {
                    if state.ctm_stack.len() > 1 {
                        state.ctm_stack.pop();
                    }
                    stack.clear();
                }
                b"Do" => {
                    let ctm = *state.ctm_stack.last().unwrap_or(&IDENTITY_CTM);
                    state.ctm = ctm;
                    // Record the XObject name (top of stack) paired with the CTM active
                    // at this invocation so extract_text_from_xobjects() can apply the
                    // correct per-Do CTM rather than the last one in the stream.
                    if let Some((Token::Name(name), _)) = stack.last() {
                        state.do_ctm_map.push((name.clone(), ctm));
                    }
                    stack.clear();
                }
                b"cm" => {
                    // Stack layout (bottom→top): a b c d e f  then  cm
                    let fv = stack.pop();
                    let ev = stack.pop();
                    let dv = stack.pop();
                    let cv = stack.pop();
                    let bv = stack.pop();
                    let av = stack.pop();
                    if let (
                        Some((Token::Number(f), _)),
                        Some((Token::Number(e), _)),
                        Some((Token::Number(d), _)),
                        Some((Token::Number(c), _)),
                        Some((Token::Number(b), _)),
                        Some((Token::Number(a), _)),
                    ) = (fv, ev, dv, cv, bv, av)
                    {
                        let mat = [a, b, c, d, e, f];
                        let top = state.ctm_stack.last_mut().unwrap();
                        *top = multiply_ctm(*top, mat);
                    }
                    stack.clear();
                }
                b"Tj" if in_bt => {
                    let op_start = Some(tok_pos);
                    let op_end   = Some(tok_pos + 2); // "Tj" is 2 bytes
                    let bytes_opt = match stack.pop() {
                        Some((Token::HexStr(b), _)) => Some(b),
                        Some((Token::LitStr(b), _)) => Some(b),
                        _ => None,
                    };
                    if let Some(char_bytes) = bytes_opt {
                        let ctm = *state.ctm_stack.last().unwrap_or(&IDENTITY_CTM);
                        let (px, py) = apply_ctm(ctm, x, y);
                        let scale = ctm_scale(ctm);
                        let (tm_ox, tm_oy) = if tm_origin_set {
                            let (ox, oy) = apply_ctm(ctm, state.tm_origin_x, state.tm_origin_y);
                            (Some(ox), Some(oy))
                        } else {
                            (None, None)
                        };
                        let tm_xs = if tm_origin_set { Some(tm_x_scale) } else { None };
                        let (tm_lm_ox, tm_lm_oy) = if tm_origin_set {
                            let (lx, ly) = apply_ctm(ctm, tm_lm_x, tm_lm_y);
                            (Some(lx), Some(ly))
                        } else {
                            (None, None)
                        };
                        // x_font_size uses the Tm x-scale for width; font_size (y-scale)
                        // is kept for height.  For uniform Tm they are equal.
                        let x_font_size = tf_font_size * tm_x_scale * scale;
                        if let Some(frag) = decode_chars_to_fragment(
                            &char_bytes,
                            &font_name,
                            font_size * scale,
                            x_font_size,
                            px,
                            py,
                            fonts,
                            state.cur_color,
                            state.cur_render_mode,
                            tf_font_size,
                            tm_y_scale,
                            stream_idx,
                            op_start,
                            op_end,
                            xobj_id,
                            tm_ox,
                            tm_oy,
                            tm_xs,
                            tm_lm_ox,
                            tm_lm_oy,
                        ) {
                            // frag.width is page-space (x-axis); reverse CTM scale to get
                            // local-space advance for the x cursor.
                            let local_advance =
                                if scale > 0.0 { frag.width / scale } else { frag.width };
                            // Apply Tc/Tw spacing (in unscaled text space → user space via tm_x_scale).
                            let n_chars = frag.text.chars().count() as f32;
                            let n_spaces = frag.text.chars().filter(|&c| c == ' ').count() as f32;
                            x += local_advance
                                + char_spacing * tm_x_scale * n_chars
                                + word_spacing * tm_x_scale * n_spaces;
                            out.push(frag);
                        }
                    }
                    stack.clear();
                }
                b"TJ" if in_bt => {
                    let op_start = Some(tok_pos);
                    let op_end   = Some(tok_pos + 2); // "TJ" is 2 bytes
                    if let Some((Token::Array(items), _)) = stack.pop() {
                        let ctm = *state.ctm_stack.last().unwrap_or(&IDENTITY_CTM);
                        let scale = ctm_scale(ctm);
                        let (tm_ox, tm_oy) = if tm_origin_set {
                            let (ox, oy) = apply_ctm(ctm, state.tm_origin_x, state.tm_origin_y);
                            (Some(ox), Some(oy))
                        } else {
                            (None, None)
                        };
                        let tm_xs = if tm_origin_set { Some(tm_x_scale) } else { None };
                        let (tm_lm_ox, tm_lm_oy) = if tm_origin_set {
                            let (lx, ly) = apply_ctm(ctm, tm_lm_x, tm_lm_y);
                            (Some(lx), Some(ly))
                        } else {
                            (None, None)
                        };
                        let x_font_size = tf_font_size * tm_x_scale * scale;
                        let mut cur_x = x; // local-space cursor
                        for item in items {
                            match item {
                                Token::HexStr(ref b) | Token::LitStr(ref b) => {
                                    let (px, py) = apply_ctm(ctm, cur_x, y);
                                    if let Some(frag) = decode_chars_to_fragment(
                                        b,
                                        &font_name,
                                        font_size * scale,
                                        x_font_size,
                                        px,
                                        py,
                                        fonts,
                                        state.cur_color,
                                        state.cur_render_mode,
                                        tf_font_size,
                                        tm_y_scale,
                                        stream_idx,
                                        op_start,
                                        op_end,
                                        xobj_id,
                                        tm_ox,
                                        tm_oy,
                                        tm_xs,
                                        tm_lm_ox,
                                        tm_lm_oy,
                                    ) {
                                        let local_advance = if scale > 0.0 {
                                            frag.width / scale
                                        } else {
                                            frag.width
                                        };
                                        let n_chars = frag.text.chars().count() as f32;
                                        let n_spaces =
                                            frag.text.chars().filter(|&c| c == ' ').count() as f32;
                                        cur_x += local_advance
                                            + char_spacing * tm_x_scale * n_chars
                                            + word_spacing * tm_x_scale * n_spaces;
                                        out.push(frag);
                                    }
                                }
                                Token::Number(kern) => {
                                    // Kern in TJ is in thousandths of a text-space unit;
                                    // multiply by tf_font_size × tm_x_scale to convert to
                                    // user space (horizontal axis).
                                    cur_x -= kern / 1000.0 * tf_font_size * tm_x_scale;
                                }
                                _ => {}
                            }
                        }
                        x = cur_x;
                    }
                    stack.clear();
                }
                _ => {
                    stack.clear();
                }
            },
            other => {
                stack.push((other, tok_pos));
            }
        }
    }

    // Write text state back so the next stream on this page inherits it.
    state.in_bt          = in_bt;
    state.font_name      = font_name;
    state.tf_font_size   = tf_font_size;
    state.font_size      = font_size;
    state.tm_y_scale     = tm_y_scale;
    state.tm_x_scale     = tm_x_scale;
    state.tm_lm_x        = tm_lm_x;
    state.tm_lm_y        = tm_lm_y;
    state.text_x         = x;
    state.text_y         = y;
    state.tm_origin_set  = tm_origin_set;
    state.text_leading   = text_leading;
    state.char_spacing   = char_spacing;
    state.word_spacing   = word_spacing;
}

#[allow(clippy::too_many_arguments)] // All args are logically required; a ctx struct would add ceremony
fn decode_chars_to_fragment(
    char_bytes: &[u8],
    font_name: &[u8],
    font_size: f32,
    x_font_size: f32,
    x: f32,
    y: f32,
    fonts: &HashMap<Vec<u8>, FontInfo>,
    color: [f32; 3],
    render_mode: u8,
    tf_font_size: f32,
    tm_y_scale: f32,
    source_stream: Option<usize>,
    source_op_start: Option<usize>,
    source_op_end: Option<usize>,
    source_xobject: Option<(u32, u16)>,
    tm_origin_x: Option<f32>,
    tm_origin_y: Option<f32>,
    tm_x_scale: Option<f32>,
    tm_lm_x: Option<f32>,
    tm_lm_y: Option<f32>,
) -> Option<TextFragment> {
    if char_bytes.is_empty() {
        return None;
    }
    let font_info = fonts.get(font_name)?;

    let mut text = String::new();
    let mut total_width = 0.0f32;

    match font_info.bytes_per_char {
        2 => {
            if !char_bytes.len().is_multiple_of(2) {
                return None;
            }
            for chunk in char_bytes.chunks(2) {
                let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
                let ch = font_info.to_unicode.get(&gid).copied().or_else(|| {
                    if font_info.identity_fallback {
                        char::from_u32(gid as u32)
                            .filter(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
                    } else {
                        None
                    }
                });
                let Some(ch) = ch else { continue };
                text.push(ch);
                let aw = font_info.advance_width(gid);
                total_width += aw as f32 / 1000.0 * x_font_size;
            }
        }
        _ => {
            for &b in char_bytes {
                let code = b as u16;
                let Some(&ch) = font_info.to_unicode.get(&code) else {
                    continue;
                };
                text.push(ch);
                let aw = font_info.advance_width(code);
                total_width += aw as f32 / 1000.0 * x_font_size;
            }
        }
    }

    if text.is_empty() {
        return None;
    }
    // Fix 5: zero-width fallback — some fonts have missing /W entries and dw=0,
    // which would make every fragment 0-width and break column detection.
    if total_width == 0.0 {
        total_width = text.chars().count() as f32 * x_font_size * 0.5;
    }
    let space_advance = font_info
        .to_unicode
        .iter()
        .find(|&(_gid, &ch)| ch == ' ')
        .map(|(&gid, _)| font_info.advance_width(gid) as f32 / 1000.0 * x_font_size)
        .unwrap_or(0.0);
    Some(TextFragment {
        text,
        x,
        y,
        width: total_width,
        height: font_size,
        font_size,
        font_name: String::from_utf8_lossy(font_name).into_owned(),
        color,
        invisible: render_mode == 3,
        is_bold: font_info.is_bold,
        is_italic: font_info.is_italic,
        font_family: font_info.font_family.clone(),
        base_font: font_info.base_font.clone(),
        space_advance,
        tf_font_size,
        tm_y_scale,
        source_stream,
        source_op_start,
        source_op_end,
        source_xobject,
        tm_origin_x,
        tm_origin_y,
        tm_x_scale,
        tm_lm_x,
        tm_lm_y,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;
