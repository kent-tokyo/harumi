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
    /// X coordinate of the text baseline in PDF points (origin: bottom-left of visible page area).
    pub x: f32,
    /// Y coordinate of the text baseline in PDF points (origin: bottom-left of visible page area).
    pub y: f32,
    /// Estimated text width in PDF points, computed from the font's advance widths.
    pub width: f32,
    /// Approximate text height in PDF points (equals `font_size`, the full em height).
    ///
    /// The baseline is at `y`; the em square extends from approximately
    /// `y - descender_fraction * font_size` to `y + ascender_fraction * font_size`.
    pub height: f32,
    /// Counter-clockwise text-line orientation in degrees after page rotation normalization.
    ///
    /// `0.0` is ordinary horizontal text; `90.0` and `270.0` identify the common
    /// vertical-writing orientations.  This describes the text baseline direction,
    /// not the axis-aligned bounding-box orientation.
    pub rotation_degrees: f32,
    /// Font size in PDF points.
    pub font_size: f32,
    /// PDF resource name of the font at this position (e.g. `"HR0"`, `"F1"`).
    pub font_name: String,
    /// RGB fill color at this position, each component in `0.0..=1.0`.
    /// Defaults to black `[0.0, 0.0, 0.0]` when no color operator precedes the text.
    pub color: [f32; 3],
    /// Effective non-stroking opacity from the active `/ExtGState` (`/ca`).
    /// Defaults to `1.0` when no opacity graphics state is active.
    pub opacity: f32,
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
    /// A Type0/CIDFont had no usable `/ToUnicode` CMap. Text may have been
    /// decoded with the Identity-H/V best-effort fallback instead.
    MissingToUnicodeCMap,
    /// A font resource used a missing or unsupported `/Subtype` and was skipped.
    /// Text that references the font is therefore not represented in the result.
    UnsupportedFontSubtype,
    /// A Type0 font uses `/Identity-V`; extraction may recover text, but vertical
    /// writing metrics and full vertical reflow are not supported by this API.
    UnsupportedVerticalWriting,
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
    /// Full ToUnicode output, including mappings containing multiple UTF-16
    /// code units. `to_unicode` is retained for legacy single-char consumers.
    pub(crate) to_unicode_text: BTreeMap<u16, String>,
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
    /// True for Type0 fonts using the PDF vertical-writing encoding.
    pub(crate) vertical: bool,
    pub(crate) vertical_default: i32,
    pub(crate) vertical_runs: Vec<VerticalWidthRun>,
}

pub(crate) struct WidthRun {
    pub(crate) start_gid: u16,
    pub(crate) widths: Vec<u32>,
}

pub(crate) struct VerticalWidthRun {
    pub(crate) start_gid: u16,
    pub(crate) widths: Vec<i32>,
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

    pub(crate) fn vertical_advance(&self, gid: u16) -> i32 {
        for run in &self.vertical_runs {
            if gid >= run.start_gid {
                let idx = (gid - run.start_gid) as usize;
                if idx < run.widths.len() {
                    return run.widths[idx];
                }
            }
        }
        self.vertical_default
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
    Some([
        x_min,
        y_min,
        (x_max - x_min).max(0.0),
        (y_max - y_min).max(0.0),
    ])
}

/// A positioned rectangle for collision detection.
///
/// Coordinates follow the standard PDF convention: `[x, y, width, height]` in PDF points,
/// bottom-left origin.
#[non_exhaustive]
#[derive(Debug, Clone, Default)]
pub struct PlacedBox {
    /// `[x, y, width, height]` in PDF points.
    pub rect: [f32; 4],
}

impl PlacedBox {
    /// Construct a [`PlacedBox`] from a `[x, y, width, height]` rectangle in PDF points.
    pub fn new(rect: [f32; 4]) -> Self {
        Self { rect }
    }
}

/// A pair of overlapping [`PlacedBox`]es returned by [`detect_collisions`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct Collision {
    /// Index of the first box in the input slice.
    pub index_a: usize,
    /// Index of the second box in the input slice.
    pub index_b: usize,
    /// The intersection rectangle `[x, y, width, height]`.
    pub overlap_rect: [f32; 4],
    /// Pre-computed area of [`overlap_rect`](Collision::overlap_rect) in PDF points².
    ///
    /// Equals `overlap_rect[2] * overlap_rect[3]`.  Callers can use this directly
    /// without recomputing to decide collision severity.
    pub overlap_area: f32,
}

/// Detect pairwise axis-aligned bounding-box overlaps between `boxes`.
///
/// Returns one [`Collision`] entry for every pair `(i, j)` where `i < j` and
/// the two boxes intersect.  Adjacent boxes that only share an edge are **not**
/// considered overlapping (the intersection would have zero area).
///
/// # Example
///
/// ```rust
/// use harumi::{PlacedBox, detect_collisions};
///
/// let boxes = vec![
///     PlacedBox::new([0.0, 0.0, 100.0, 50.0]),
///     PlacedBox::new([80.0, 0.0, 100.0, 50.0]),  // overlaps first by 20 pt
///     PlacedBox::new([200.0, 0.0, 50.0, 50.0]),  // no overlap
/// ];
/// let collisions = detect_collisions(&boxes);
/// assert_eq!(collisions.len(), 1);
/// assert_eq!(collisions[0].index_a, 0);
/// assert_eq!(collisions[0].index_b, 1);
/// assert!(collisions[0].overlap_area > 0.0);
/// ```
pub fn detect_collisions(boxes: &[PlacedBox]) -> Vec<Collision> {
    let mut out = Vec::new();
    for (i, box_a) in boxes.iter().enumerate() {
        let [ax, ay, aw, ah] = box_a.rect;
        let ax2 = ax + aw;
        let ay2 = ay + ah;
        for (j, box_b) in boxes.iter().enumerate().skip(i + 1) {
            let [bx, by, bw, bh] = box_b.rect;
            let bx2 = bx + bw;
            let by2 = by + bh;
            let ox = ax.max(bx);
            let oy = ay.max(by);
            let ox2 = ax2.min(bx2);
            let oy2 = ay2.min(by2);
            if ox2 > ox && oy2 > oy {
                let ow = ox2 - ox;
                let oh = oy2 - oy;
                out.push(Collision {
                    index_a: i,
                    index_b: j,
                    overlap_rect: [ox, oy, ow, oh],
                    overlap_area: ow * oh,
                });
            }
        }
    }
    out
}

/// A ruling line or table/box border extracted from a page's vector graphics.
///
/// Coordinates use PDF bottom-left origin, in PDF points.
///
/// Obtain via [`extract_vector_rules`] (free function) or
/// [`crate::Document::extract_vector_rules`] (convenience method on a loaded document).
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct VectorRule {
    /// Start x coordinate.
    pub x1: f32,
    /// Start y coordinate (bottom-left origin).
    pub y1: f32,
    /// End x coordinate.
    pub x2: f32,
    /// End y coordinate.
    pub y2: f32,
    /// Effective stroke/fill width in PDF points.
    pub line_width: f32,
}

impl VectorRule {
    /// Construct a [`VectorRule`] from endpoint coordinates and a line width.
    pub fn new(x1: f32, y1: f32, x2: f32, y2: f32, line_width: f32) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            line_width,
        }
    }

    /// Returns `true` when the rule spans more in X than Y (approximately horizontal).
    pub fn is_horizontal(&self) -> bool {
        (self.x2 - self.x1).abs() >= (self.y2 - self.y1).abs()
    }

    /// Returns `true` when the rule spans more in Y than X (approximately vertical).
    pub fn is_vertical(&self) -> bool {
        (self.y2 - self.y1).abs() > (self.x2 - self.x1).abs()
    }

    /// Bounding box `[x, y, width, height]` in PDF points, expanded by half the line width.
    pub fn bbox(&self) -> [f32; 4] {
        let half = self.line_width / 2.0;
        let x = self.x1.min(self.x2) - half;
        let y = self.y1.min(self.y2) - half;
        let w = (self.x2 - self.x1).abs() + self.line_width;
        let h = (self.y2 - self.y1).abs() + self.line_width;
        [x, y, w, h]
    }
}

/// Extract ruling lines and table/box borders from a raw PDF content stream.
///
/// The function recognises:
/// - Stroked line segments (`m`/`l` + `S`/`s`/`B`/`b` operators)
/// - Thin filled rectangles (`re` + `f`/`F`/`B`/`b` where `min(w,h) ≤ 3 pt`) — these are
///   the most common form for table rules in SDS/GHS and other business PDFs.
///
/// The current transformation matrix (`cm`, `q`/`Q`) is tracked so that rules drawn in a
/// scaled or translated coordinate space are returned in page coordinates.
///
/// **Limitation**: Form XObject content is not descended into.  Rules painted exclusively
/// inside XObjects are not returned.
///
/// # Arguments
/// - `content` — raw (decompressed) page content stream bytes.
/// - `page_height` — height of the MediaBox in PDF points; used to preserve the
///   bottom-left coordinate origin.
pub fn extract_vector_rules(content: &[u8], page_height: f32) -> Vec<VectorRule> {
    let tokens = tokenize(content);
    let identity: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut ctm = identity;
    let mut ctm_stack: Vec<[f32; 6]> = Vec::new();

    // Graphics state parameters
    let mut line_width = 1.0_f32;
    let mut gs_stack: Vec<f32> = Vec::new(); // saved line_width values

    // Current path
    let mut path_points: Vec<(f32, f32)> = Vec::new();
    let mut path_rects: Vec<[f32; 4]> = Vec::new(); // rectangles from `re`

    let mut rules: Vec<VectorRule> = Vec::new();
    let _ = page_height; // reserved for future use (coordinates already in page space)

    let mut stack: Vec<Token> = Vec::new();

    let emit_stroked = |pts: &[(f32, f32)],
                        rects: &[[f32; 4]],
                        lw: f32,
                        ctm: [f32; 6],
                        rules: &mut Vec<VectorRule>| {
        // Emit line-segment pairs as rules
        for chunk in pts.windows(2) {
            let (ax, ay) = apply_ctm(ctm, chunk[0].0, chunk[0].1);
            let (bx, by) = apply_ctm(ctm, chunk[1].0, chunk[1].1);
            let effective_w = lw * ctm_scale(ctm);
            rules.push(VectorRule {
                x1: ax,
                y1: ay,
                x2: bx,
                y2: by,
                line_width: effective_w,
            });
        }
        // Stroked rectangles: emit all four edges if the rectangle is thin
        for &[rx, ry, rw, rh] in rects {
            let (ax, ay) = apply_ctm(ctm, rx, ry);
            let (bx, by) = apply_ctm(ctm, rx + rw, ry + rh);
            let effective_w = lw * ctm_scale(ctm);
            let w = (bx - ax).abs();
            let h = (by - ay).abs();
            if rw.min(rh).abs() <= 3.0 || w.min(h) <= 3.0 {
                // Thin rectangle — treat as a single rule
                rules.push(VectorRule {
                    x1: ax,
                    y1: ay,
                    x2: bx,
                    y2: by,
                    line_width: effective_w,
                });
            } else {
                // Thick rectangle — emit four border lines
                let (c1x, c1y) = apply_ctm(ctm, rx + rw, ry);
                let (c2x, c2y) = apply_ctm(ctm, rx, ry + rh);
                rules.push(VectorRule {
                    x1: ax,
                    y1: ay,
                    x2: c1x,
                    y2: c1y,
                    line_width: effective_w,
                });
                rules.push(VectorRule {
                    x1: c1x,
                    y1: c1y,
                    x2: bx,
                    y2: by,
                    line_width: effective_w,
                });
                rules.push(VectorRule {
                    x1: bx,
                    y1: by,
                    x2: c2x,
                    y2: c2y,
                    line_width: effective_w,
                });
                rules.push(VectorRule {
                    x1: c2x,
                    y1: c2y,
                    x2: ax,
                    y2: ay,
                    line_width: effective_w,
                });
            }
        }
    };

    let emit_filled = |rects: &[[f32; 4]], lw: f32, ctm: [f32; 6], rules: &mut Vec<VectorRule>| {
        for &[rx, ry, rw, rh] in rects {
            let (ax, ay) = apply_ctm(ctm, rx, ry);
            let (bx, by) = apply_ctm(ctm, rx + rw, ry + rh);
            let effective_w = lw * ctm_scale(ctm);
            let w = (bx - ax).abs();
            let h = (by - ay).abs();
            // Only emit filled rects when they are thin (≤ 3 pt in the narrow dimension)
            if rw.min(rh).abs() <= 3.0 || w.min(h) <= 3.0 {
                rules.push(VectorRule {
                    x1: ax,
                    y1: ay,
                    x2: bx,
                    y2: by,
                    line_width: effective_w,
                });
            }
        }
    };

    for (token, _) in tokens {
        match token {
            Token::Keyword(ref kw) => {
                match kw.as_slice() {
                    b"q" => {
                        ctm_stack.push(ctm);
                        gs_stack.push(line_width);
                    }
                    b"Q" => {
                        if let Some(saved) = ctm_stack.pop() {
                            ctm = saved;
                        }
                        if let Some(saved) = gs_stack.pop() {
                            line_width = saved;
                        }
                        stack.clear();
                        continue;
                    }
                    b"cm" => {
                        // Expect 6 numbers on stack
                        if stack.len() >= 6 {
                            let mut ns = [0.0f32; 6];
                            let len = stack.len();
                            for (i, slot) in ns.iter_mut().enumerate() {
                                if let Token::Number(v) = stack[len - 6 + i] {
                                    *slot = v;
                                }
                            }
                            ctm = multiply_ctm(ctm, ns);
                        }
                        stack.clear();
                        continue;
                    }
                    b"w" => {
                        if let Some(Token::Number(v)) = stack.last() {
                            line_width = *v;
                        }
                        stack.clear();
                        continue;
                    }
                    // Path construction operators
                    b"m" => {
                        if stack.len() >= 2 {
                            let len = stack.len();
                            if let (Token::Number(y), Token::Number(x)) =
                                (&stack[len - 1], &stack[len - 2])
                            {
                                path_points.push((*x, *y));
                            }
                        }
                        stack.clear();
                        continue;
                    }
                    b"l" => {
                        if stack.len() >= 2 {
                            let len = stack.len();
                            if let (Token::Number(y), Token::Number(x)) =
                                (&stack[len - 1], &stack[len - 2])
                            {
                                path_points.push((*x, *y));
                            }
                        }
                        stack.clear();
                        continue;
                    }
                    b"re" => {
                        if stack.len() >= 4 {
                            let len = stack.len();
                            if let (
                                Token::Number(h),
                                Token::Number(w),
                                Token::Number(y),
                                Token::Number(x),
                            ) = (
                                &stack[len - 1],
                                &stack[len - 2],
                                &stack[len - 3],
                                &stack[len - 4],
                            ) {
                                path_rects.push([*x, *y, *w, *h]);
                            }
                        }
                        stack.clear();
                        continue;
                    }
                    b"h" | b"v" | b"y" | b"c" => {
                        // Close/curve — just clear the operand stack; don't track curves as rules.
                        stack.clear();
                        continue;
                    }
                    // Path painting operators — stroke
                    b"S" | b"s" => {
                        let pts = path_points.clone();
                        let rects = path_rects.clone();
                        emit_stroked(&pts, &rects, line_width, ctm, &mut rules);
                        path_points.clear();
                        path_rects.clear();
                        stack.clear();
                        continue;
                    }
                    // Path painting operators — fill only
                    b"f" | b"F" | b"f*" => {
                        let rects = path_rects.clone();
                        emit_filled(&rects, line_width, ctm, &mut rules);
                        path_points.clear();
                        path_rects.clear();
                        stack.clear();
                        continue;
                    }
                    // Path painting operators — fill + stroke
                    b"B" | b"b" | b"B*" | b"b*" => {
                        let pts = path_points.clone();
                        let rects = path_rects.clone();
                        emit_stroked(&pts, &rects, line_width, ctm, &mut rules);
                        emit_filled(&rects, line_width, ctm, &mut rules);
                        path_points.clear();
                        path_rects.clear();
                        stack.clear();
                        continue;
                    }
                    // No-op path operators
                    b"n" => {
                        path_points.clear();
                        path_rects.clear();
                        stack.clear();
                        continue;
                    }
                    // Text block markers: skip everything inside BT..ET
                    b"BT" | b"ET" => {
                        stack.clear();
                        continue;
                    }
                    _ => {
                        stack.clear();
                        continue;
                    }
                }
            }
            other => stack.push(other),
        }
    }

    rules
}

/// Check whether any text rectangle in `text_rects` overlaps a [`VectorRule`].
///
/// Returns a list of `(text_index, rule_index, severity)` tuples for each overlap found.
/// Only overlaps with positive area (excluding mere edge-touches) are reported.
pub fn detect_text_vs_rule_collisions(
    text_rects: &[[f32; 4]],
    rules: &[VectorRule],
) -> Vec<(usize, usize, crate::CollisionSeverity)> {
    let mut out = Vec::new();
    for (ti, &text_rect) in text_rects.iter().enumerate() {
        let text_area = rect_area(text_rect);
        if text_area <= 0.0 {
            continue;
        }
        for (ri, rule) in rules.iter().enumerate() {
            let rule_bbox = rule.bbox();
            if let Some(overlap) = rect_intersection(text_rect, rule_bbox) {
                let overlap_area = rect_area(overlap);
                if overlap_area > 0.0 {
                    let severity =
                        collision_severity(overlap_area, text_area, rect_area(rule_bbox));
                    out.push((ti, ri, severity));
                }
            }
        }
    }
    out
}

/// A minimal placement description for use with [`PageLayoutQuality::from_simple_placements`].
///
/// Downstream crates that compute their own text placements (without going through
/// [`crate::Document::plan_text_for_regions`]) can use this type to feed placements into
/// the layout quality assessment pipeline without needing to construct the
/// `#[non_exhaustive]` [`RegionFitPlan`] / [`LayoutRegion`] types directly.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct SimplePlacement {
    /// Caller-assigned identifier for this placement (echoed back in [`LayoutIssue::id`]).
    pub id: usize,
    /// Bounding box of the source glyphs: `[x, y, width, height]` in PDF points.
    pub source_rect: [f32; 4],
    /// Bounding box of the placed (translated) text: `[x, y, width, height]` in PDF points.
    pub placed_rect: [f32; 4],
    /// Effective font size used for the placed text, in PDF points.
    pub font_size: f32,
    /// Whether the text overflowed its available space.
    pub overflow: bool,
}

impl SimplePlacement {
    /// Construct a new [`SimplePlacement`].
    pub fn new(
        id: usize,
        source_rect: [f32; 4],
        placed_rect: [f32; 4],
        font_size: f32,
        overflow: bool,
    ) -> Self {
        Self {
            id,
            source_rect,
            placed_rect,
            font_size,
            overflow,
        }
    }
}

/// Structural relationship between two overlapping [`LayoutRegion`]s.
///
/// Returned as part of [`ClassifiedCollision`] by [`classify_collisions`].
///
/// Classification priority (highest wins):
/// 1. [`HeaderFooter`](CollisionKind::HeaderFooter) — either region has that role.
/// 2. [`SameRegion`](CollisionKind::SameRegion) — same `row` **and** same `col`.
/// 3. [`SameRow`](CollisionKind::SameRow) — same `row` index.
/// 4. [`AdjacentRow`](CollisionKind::AdjacentRow) — rows differ by exactly 1.
/// 5. [`SameColumn`](CollisionKind::SameColumn) — same `col`, different rows.
/// 6. [`Unknown`](CollisionKind::Unknown) — insufficient info or out-of-range index.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollisionKind {
    /// Both regions share the same `row` **and** the same `col` index —
    /// they map to the same layout cell.
    SameRegion,
    /// Both regions share the same `row` index but differ in `col`.
    SameRow,
    /// The `row` indices differ by exactly 1.
    AdjacentRow,
    /// Both regions share the same `col` index but differ in `row`.
    SameColumn,
    /// At least one region has [`LayoutRegionRole::HeaderFooter`].
    HeaderFooter,
    /// Insufficient row/column information or out-of-range collision index.
    Unknown,
}

/// How bad a collision is, based on how much of the smaller box it covers.
///
/// Returned as [`ClassifiedCollision::severity`].  Use this to decide whether to
/// ignore a collision, try font shrinking, or escalate to AI text shortening.
///
/// See also the standalone [`collision_severity`] function for callers who work
/// with raw [`PlacedBox`]es rather than [`LayoutRegion`]s.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollisionSeverity {
    /// Overlap covers less than 5 % of the smaller box — likely a rounding artifact
    /// or a deliberate structural touch (e.g. label/value adjacency).
    Minor,
    /// Overlap covers 5–20 % of the smaller box — visible but may be acceptable
    /// depending on the layout profile (e.g. an overflowing heading).
    Moderate,
    /// Overlap covers more than 20 % of the smaller box — significant enough to
    /// require shrinking, truncation, or AI text shortening.
    Major,
}

/// Compute a [`CollisionSeverity`] from raw box areas.
///
/// `overlap_area` is in PDF points² (from [`Collision::overlap_area`]).
/// `box_a_area` and `box_b_area` are the areas of the two overlapping boxes.
/// The severity is determined by the ratio of the overlap to the *smaller* box area.
///
/// Pass `0.0` for a box area when it is unknown; the function then falls back to
/// absolute thresholds (`< 50 pt²` = Minor, `< 400 pt²` = Moderate, else Major).
///
/// # Example
///
/// ```rust
/// use harumi::{collision_severity, CollisionSeverity};
///
/// // A 10×5 pt overlap in a 20×10 pt box (200 pt²) is 25 % → Major
/// assert_eq!(collision_severity(50.0, 200.0, 400.0), CollisionSeverity::Major);
/// // A 2×2 pt overlap in a 50×20 pt box (1000 pt²) is 0.4 % → Minor
/// assert_eq!(collision_severity(4.0, 1000.0, 2000.0), CollisionSeverity::Minor);
/// ```
pub fn collision_severity(
    overlap_area: f32,
    box_a_area: f32,
    box_b_area: f32,
) -> CollisionSeverity {
    let ref_area = box_a_area.min(box_b_area);
    if ref_area > 0.0 && ref_area.is_finite() {
        let ratio = overlap_area / ref_area;
        if ratio < 0.05 {
            CollisionSeverity::Minor
        } else if ratio < 0.20 {
            CollisionSeverity::Moderate
        } else {
            CollisionSeverity::Major
        }
    } else {
        // Fallback: absolute thresholds when box sizes are unknown
        if overlap_area < 50.0 {
            CollisionSeverity::Minor
        } else if overlap_area < 400.0 {
            CollisionSeverity::Moderate
        } else {
            CollisionSeverity::Major
        }
    }
}

/// A [`Collision`] annotated with the structural relationship between
/// the two overlapping [`LayoutRegion`]s.
///
/// Returned by [`classify_collisions`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ClassifiedCollision {
    /// The raw geometric collision (indices and overlap rect).
    pub collision: Collision,
    /// Structural classification of the overlap.
    pub kind: CollisionKind,
    /// Role of the region at `collision.index_a`, or `None` if out of bounds.
    pub region_a: Option<LayoutRegionRole>,
    /// Role of the region at `collision.index_b`, or `None` if out of bounds.
    pub region_b: Option<LayoutRegionRole>,
    /// How bad this collision is relative to the smaller of the two involved boxes.
    ///
    /// `Minor` collisions are often structural artefacts; `Major` ones need correction.
    /// Computed from `collision.overlap_area` and the `source_bbox` areas of the two regions.
    /// Falls back to absolute thresholds when a region index is out of bounds.
    pub severity: CollisionSeverity,
}

/// Annotate each [`Collision`] with a [`CollisionKind`] by comparing the
/// [`LayoutRegion`] metadata (row, col, role) at the collision indices.
///
/// `regions` must be the same slice that produced the [`PlacedBox`]es passed to
/// [`detect_collisions`] — `collision.index_a` and `index_b` are indices into it.
/// Out-of-range indices yield [`CollisionKind::Unknown`] with `region_a`/`region_b`
/// set to `None`.
///
/// # Example
///
/// ```rust
/// use harumi::{PlacedBox, detect_collisions, classify_collisions};
///
/// let boxes = vec![
///     PlacedBox::new([0.0, 0.0, 100.0, 50.0]),
///     PlacedBox::new([80.0, 0.0, 100.0, 50.0]),
/// ];
/// let collisions = detect_collisions(&boxes);
/// // With an empty regions slice, indices are out of range → Unknown
/// let classified = classify_collisions(&[], &collisions);
/// assert_eq!(classified.len(), 1);
/// use harumi::CollisionKind;
/// assert_eq!(classified[0].kind, CollisionKind::Unknown);
/// ```
pub fn classify_collisions(
    regions: &[LayoutRegion],
    collisions: &[Collision],
) -> Vec<ClassifiedCollision> {
    collisions
        .iter()
        .map(|c| {
            let ra = regions.get(c.index_a);
            let rb = regions.get(c.index_b);
            let area_a = ra
                .map(|r| r.source_bbox[2] * r.source_bbox[3])
                .unwrap_or(0.0);
            let area_b = rb
                .map(|r| r.source_bbox[2] * r.source_bbox[3])
                .unwrap_or(0.0);
            ClassifiedCollision {
                collision: c.clone(),
                kind: classify_collision_kind(ra, rb),
                region_a: ra.map(|r| r.role.clone()),
                region_b: rb.map(|r| r.role.clone()),
                severity: collision_severity(c.overlap_area, area_a, area_b),
            }
        })
        .collect()
}

fn classify_collision_kind(ra: Option<&LayoutRegion>, rb: Option<&LayoutRegion>) -> CollisionKind {
    use CollisionKind::*;

    if matches!(ra.map(|r| &r.role), Some(LayoutRegionRole::HeaderFooter))
        || matches!(rb.map(|r| &r.role), Some(LayoutRegionRole::HeaderFooter))
    {
        return HeaderFooter;
    }

    let row_a = ra.and_then(|r| r.row);
    let row_b = rb.and_then(|r| r.row);
    let col_a = ra.and_then(|r| r.col);
    let col_b = rb.and_then(|r| r.col);

    if let (Some(ra_row), Some(rb_row), Some(ra_col), Some(rb_col)) = (row_a, row_b, col_a, col_b)
        && ra_row == rb_row
        && ra_col == rb_col
    {
        return SameRegion;
    }

    if let (Some(ra_row), Some(rb_row)) = (row_a, row_b)
        && ra_row == rb_row
    {
        return SameRow;
    }

    if let (Some(ra_row), Some(rb_row)) = (row_a, row_b)
        && ra_row.abs_diff(rb_row) == 1
    {
        return AdjacentRow;
    }

    if let (Some(ra_col), Some(rb_col)) = (col_a, col_b)
        && ra_col == rb_col
    {
        return SameColumn;
    }

    Unknown
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
        return vec![ColumnZone {
            x_start: 0.0,
            x_end: page_width,
        }];
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
                    (lm - *a)
                        .abs()
                        .partial_cmp(&(lm - *b).abs())
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
        let in_current_row = rows.last().map(|r| (r[0].y - frag.y).abs() <= row_tol);
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
            let height = frags
                .iter()
                .map(|f| f.height.max(0.0))
                .fold(0.0f32, f32::max);
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
// Image bbox extraction
// ---------------------------------------------------------------------------

/// Collect XObject resource names that are Image XObjects (Subtype == /Image).
fn collect_image_xobject_names(doc: &lopdf::Document, page_id: ObjectId) -> Vec<Vec<u8>> {
    use lopdf::Object;
    (|| -> Option<Vec<Vec<u8>>> {
        let page_obj = doc.get_object(page_id).ok()?;
        let page_dict = page_obj.as_dict().ok()?;
        let resources_obj = page_dict.get(b"Resources").ok()?;
        let resources_dict = doc.dereference(resources_obj).ok()?.1.as_dict().ok()?;
        let xobject_obj = resources_dict.get(b"XObject").ok()?;
        let xobject_dict = doc.dereference(xobject_obj).ok()?.1.as_dict().ok()?;

        let names: Vec<Vec<u8>> = xobject_dict
            .iter()
            .filter_map(|(name, value)| {
                let xobj_id = match value {
                    Object::Reference(id) => *id,
                    _ => return None,
                };
                let xobj = doc.get_object(xobj_id).ok()?;
                let xobj_dict = xobj
                    .as_stream()
                    .ok()
                    .map(|s| &s.dict)
                    .or_else(|| xobj.as_dict().ok())?;
                let subtype = xobj_dict.get(b"Subtype").ok()?;
                if subtype == &Object::Name(b"Image".to_vec()) {
                    Some(name.to_vec())
                } else {
                    None
                }
            })
            .collect();
        Some(names)
    })()
    .unwrap_or_default()
}

/// Returns `[x, y, width, height]` in PDF points for each Image XObject on the page.
///
/// Uses the CTM at each `Do` invocation to compute placement.  Rotated and
/// sheared images are represented by the axis-aligned bounding box of their
/// transformed unit square, which is conservative for collision detection.
pub(crate) fn extract_image_bboxes_from_page(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> Vec<[f32; 4]> {
    let image_names = collect_image_xobject_names(doc, page_id);
    if image_names.is_empty() {
        return vec![];
    }

    let streams = page_content_streams(doc, page_id);
    let identity: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut ctm = identity;
    let mut ctm_stack: Vec<[f32; 6]> = Vec::new();
    let mut bboxes: Vec<[f32; 4]> = Vec::new();

    for stream_bytes in &streams {
        let tokens = tokenize(stream_bytes);
        let mut i = 0;
        while i < tokens.len() {
            if let Token::Keyword(kw) = &tokens[i].0 {
                match kw.as_slice() {
                    b"q" => {
                        ctm_stack.push(ctm);
                    }
                    b"Q" => {
                        if let Some(saved) = ctm_stack.pop() {
                            ctm = saved;
                        }
                    }
                    b"cm" if i >= 6 => {
                        // Expect 6 Number tokens immediately before `cm`.
                        let ns: Vec<f32> = tokens[i - 6..i]
                            .iter()
                            .filter_map(|(t, _)| {
                                if let Token::Number(n) = t {
                                    Some(*n)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if ns.len() == 6 {
                            ctm = multiply_ctm(ctm, [ns[0], ns[1], ns[2], ns[3], ns[4], ns[5]]);
                        }
                    }
                    b"Do" => {
                        // Operand is the Name token immediately before `Do`.
                        if i > 0
                            && let Token::Name(name) = &tokens[i - 1].0
                            && image_names.contains(name)
                            && let Some(bbox) = transformed_unit_square_bbox(ctm)
                        {
                            bboxes.push(bbox);
                        }
                    }
                    _ => {}
                }
            }
            i += 1;
        }
    }

    bboxes
}

/// Return the axis-aligned bounding box of a PDF unit square after applying a
/// six-parameter affine transform `[a b c d e f]`.
fn transformed_unit_square_bbox(ctm: [f32; 6]) -> Option<[f32; 4]> {
    let [a, b, c, d, e, f] = ctm;
    if !ctm.iter().all(|value| value.is_finite()) {
        return None;
    }
    let points = [
        (e, f),
        (a + e, b + f),
        (c + e, d + f),
        (a + c + e, b + d + f),
    ];
    let min_x = points
        .iter()
        .map(|point| point.0)
        .fold(f32::INFINITY, f32::min);
    let max_x = points
        .iter()
        .map(|point| point.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = points
        .iter()
        .map(|point| point.1)
        .fold(f32::INFINITY, f32::min);
    let max_y = points
        .iter()
        .map(|point| point.1)
        .fold(f32::NEG_INFINITY, f32::max);
    let width = max_x - min_x;
    let height = max_y - min_y;
    if width > f32::EPSILON && height > f32::EPSILON {
        Some([min_x, min_y, width, height])
    } else {
        None
    }
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
    let extgstates = collect_extgstates(doc, page_id);

    let mut fragments = Vec::new();
    // Carry graphics state (colour, render-mode) across streams on the same page.
    let mut carry = ParseCarryState::default();
    for (stream_idx, stream_bytes) in streams.iter().enumerate() {
        parse_content_stream(
            stream_bytes,
            &fonts,
            &extgstates,
            &mut carry,
            &mut fragments,
            Some(stream_idx),
            None,
        );
    }
    // Also extract text from Form XObjects (headers, footers, watermarks).
    extract_text_from_xobjects(doc, page_id, &mut carry, &mut fragments, 0);
    normalize_page_coordinates(doc, page_id, &mut fragments);
    Ok(fragments)
}

#[derive(Clone, Copy)]
struct PageGeometry {
    crop_box: [f32; 4],
    user_unit: f32,
    rotate: i32,
}

/// Resolve the page-space geometry that viewers use for layout. Content streams
/// are authored in default user space; extraction exposes page-local points so
/// CropBox origins, physical UserUnit scaling, and page rotation do not leak
/// into layout-region inference.
fn page_geometry(doc: &lopdf::Document, page_id: ObjectId) -> PageGeometry {
    let mut current_id = page_id;
    let mut media_box = None;
    let mut crop_box = None;
    let mut user_unit = None;
    let mut rotate = None;

    for _ in 0..32 {
        let Ok(dict) = doc
            .get_object(current_id)
            .and_then(|object| object.as_dict())
        else {
            break;
        };
        if media_box.is_none() {
            media_box = dict
                .get(b"MediaBox")
                .ok()
                .and_then(|object| page_box_values(doc, object));
        }
        if crop_box.is_none() {
            crop_box = dict
                .get(b"CropBox")
                .ok()
                .and_then(|object| page_box_values(doc, object));
        }
        if user_unit.is_none() {
            user_unit = dict
                .get(b"UserUnit")
                .ok()
                .and_then(|object| resolve_object(doc, object))
                .and_then(object_number);
        }
        if rotate.is_none() {
            rotate = dict
                .get(b"Rotate")
                .ok()
                .and_then(|object| resolve_object(doc, object))
                .and_then(object_number)
                .map(|value| value.round() as i32);
        }
        let Some(Object::Reference(parent_id)) = dict.get(b"Parent").ok() else {
            break;
        };
        current_id = *parent_id;
    }

    let media = media_box.unwrap_or([0.0, 0.0, 595.0, 842.0]);
    PageGeometry {
        crop_box: crop_box.unwrap_or(media),
        user_unit: user_unit
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(1.0),
        rotate: rotate.unwrap_or(0),
    }
}

fn page_box_values(doc: &lopdf::Document, object: &Object) -> Option<[f32; 4]> {
    let object = resolve_object(doc, object)?;
    let array = object.as_array().ok()?;
    if array.len() < 4 {
        return None;
    }
    let mut values = [0.0; 4];
    for (index, value) in values.iter_mut().enumerate() {
        *value = object_number(&array[index])?;
    }
    values
        .iter()
        .all(|value| value.is_finite())
        .then_some(values)
}

fn object_number(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) => Some(*value),
        _ => None,
    }
}

fn normalize_page_coordinates(
    doc: &lopdf::Document,
    page_id: ObjectId,
    fragments: &mut [TextFragment],
) {
    let geometry = page_geometry(doc, page_id);
    for fragment in fragments {
        let (x, y) = transform_page_point(geometry, fragment.x, fragment.y);
        let (width, height) = transform_page_extent(geometry, fragment.width, fragment.height);
        fragment.x = x;
        fragment.y = y;
        fragment.width = width;
        fragment.height = height;
        fragment.rotation_degrees =
            normalize_rotation(fragment.rotation_degrees + geometry.rotate as f32);
        fragment.font_size *= geometry.user_unit;
        fragment.space_advance *= geometry.user_unit;
        fragment.tf_font_size *= geometry.user_unit;
        if let (Some(x), Some(y)) = (fragment.tm_origin_x, fragment.tm_origin_y) {
            let (x, y) = transform_page_point(geometry, x, y);
            fragment.tm_origin_x = Some(x);
            fragment.tm_origin_y = Some(y);
        }
        if let (Some(x), Some(y)) = (fragment.tm_lm_x, fragment.tm_lm_y) {
            let (x, y) = transform_page_point(geometry, x, y);
            fragment.tm_lm_x = Some(x);
            fragment.tm_lm_y = Some(y);
        }
    }
}

fn normalize_rotation(degrees: f32) -> f32 {
    let mut normalized = degrees.rem_euclid(360.0);
    if normalized >= 360.0 - 0.0001 {
        normalized = 0.0;
    }
    normalized
}

fn transform_page_point(geometry: PageGeometry, x: f32, y: f32) -> (f32, f32) {
    let [x0, y0, x1, y1] = geometry.crop_box;
    let local_x = (x - x0) * geometry.user_unit;
    let local_y = (y - y0) * geometry.user_unit;
    let width = (x1 - x0) * geometry.user_unit;
    let height = (y1 - y0) * geometry.user_unit;
    match geometry.rotate.rem_euclid(360) {
        90 => (local_y, width - local_x),
        180 => (width - local_x, height - local_y),
        270 => (height - local_y, local_x),
        _ => (local_x, local_y),
    }
}

fn transform_page_extent(geometry: PageGeometry, width: f32, height: f32) -> (f32, f32) {
    let width = width.abs() * geometry.user_unit;
    let height = height.abs() * geometry.user_unit;
    if geometry.rotate.rem_euclid(360) % 180 == 90 {
        (height, width)
    } else {
        (width, height)
    }
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
            let Some(&xobj_id) = xobj_name_map.get(xobj_name.as_slice()) else {
                continue;
            };
            if let Some(content) = decode_form_xobject(doc, xobj_id) {
                let xobj_fonts = xobject_fonts(doc, page_id, xobj_id);
                let xobj_extgstates = xobject_extgstates(doc, page_id, xobj_id);
                let xobj_matrix = xobject_matrix(doc, xobj_id);
                carry.ctm = multiply_ctm(*do_ctm, xobj_matrix);
                carry.ctm_stack = vec![carry.ctm];
                parse_content_stream(
                    &content,
                    &xobj_fonts,
                    &xobj_extgstates,
                    carry,
                    out,
                    None,
                    Some(xobj_id),
                );
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
                let xobj_extgstates = xobject_extgstates(doc, page_id, xobj_id);
                let xobj_matrix = xobject_matrix(doc, xobj_id);
                carry.ctm = multiply_ctm(saved_ctm, xobj_matrix);
                carry.ctm_stack = vec![carry.ctm];
                parse_content_stream(
                    &content,
                    &xobj_fonts,
                    &xobj_extgstates,
                    carry,
                    out,
                    None,
                    Some(xobj_id),
                );
            }
        }
    }

    carry.ctm = saved_ctm;
    carry.ctm_stack = saved_ctm_stack;
}

fn decode_form_xobject(doc: &lopdf::Document, xobj_id: ObjectId) -> Option<Vec<u8>> {
    let xobj_obj = doc.get_object(xobj_id).ok()?;
    let xobj_stream = xobj_obj.as_stream().ok()?;
    let is_form = xobj_stream.dict.get(b"Subtype").ok().and_then(|o| {
        if let Object::Name(n) = o {
            Some(n.as_slice())
        } else {
            None
        }
    }) == Some(b"Form");
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

    let xobj_specific = doc
        .get_object(xobj_id)
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

/// Resolve ExtGState resources for a Form XObject, with inherited page-level
/// entries as fallback. A private XObject entry wins on name collisions.
fn xobject_extgstates(
    doc: &lopdf::Document,
    page_id: ObjectId,
    xobj_id: ObjectId,
) -> HashMap<Vec<u8>, f32> {
    let mut result = collect_extgstates(doc, page_id);
    let Some(resources_dict) = doc
        .get_object(xobj_id)
        .ok()
        .and_then(|object| object.as_stream().ok())
        .and_then(|stream| stream.dict.get(b"Resources").ok())
        .and_then(|resources| resolve_dict(doc, resources))
    else {
        return result;
    };
    result.extend(collect_extgstates_from_resources(doc, resources_dict));
    result
}

fn xobject_fonts_verbose(
    doc: &lopdf::Document,
    page_id: ObjectId,
    xobj_id: ObjectId,
    warnings: &mut Vec<ExtractionWarning>,
) -> HashMap<Vec<u8>, crate::extract::FontInfo> {
    let page_fonts = collect_fonts(doc, page_id);
    let xobj_specific = doc
        .get_object(xobj_id)
        .ok()
        .and_then(|o| o.as_stream().ok())
        .and_then(|s| s.dict.get(b"Resources").ok())
        .and_then(|res_ref| resolve_dict(doc, res_ref))
        .map(|res_dict| collect_fonts_from_resources_verbose(doc, res_dict, warnings))
        .unwrap_or_default();

    if xobj_specific.is_empty() {
        page_fonts
    } else {
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
pub(crate) fn page_content_stream_ids(doc: &lopdf::Document, page_id: ObjectId) -> Vec<ObjectId> {
    let Ok(page_obj) = doc.get_object(page_id) else {
        return vec![];
    };
    let Ok(page_dict) = page_obj.as_dict() else {
        return vec![];
    };
    let Ok(contents_obj) = page_dict.get(b"Contents") else {
        return vec![];
    };
    match contents_obj {
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
            .filter_map(|o| {
                if let Object::Reference(id) = o {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect(),
        _ => return (vec![], vec![]),
    };

    let mut result = Vec::new();
    let mut warnings = Vec::new();
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
    let fonts = collect_fonts_verbose(doc, page_id, &mut warnings);
    let extgstates = collect_extgstates(doc, page_id);

    let mut fragments = Vec::new();
    let mut carry = ParseCarryState::default();
    for (stream_idx, stream_bytes) in streams.iter().enumerate() {
        parse_content_stream(
            stream_bytes,
            &fonts,
            &extgstates,
            &mut carry,
            &mut fragments,
            Some(stream_idx),
            None,
        );
    }
    extract_text_from_xobjects_verbose(doc, page_id, &mut carry, &mut fragments, 0, &mut warnings);
    normalize_page_coordinates(doc, page_id, &mut fragments);
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
            let Some(&xobj_id) = xobj_name_map.get(xobj_name.as_slice()) else {
                continue;
            };
            match decode_form_xobject_verbose(doc, xobj_id) {
                Ok(content) => {
                    let xobj_fonts = xobject_fonts_verbose(doc, page_id, xobj_id, warnings);
                    let xobj_extgstates = xobject_extgstates(doc, page_id, xobj_id);
                    let xobj_matrix = xobject_matrix(doc, xobj_id);
                    carry.ctm = multiply_ctm(*do_ctm, xobj_matrix);
                    carry.ctm_stack = vec![carry.ctm];
                    parse_content_stream(
                        &content,
                        &xobj_fonts,
                        &xobj_extgstates,
                        carry,
                        out,
                        None,
                        Some(xobj_id),
                    );
                }
                Err(warn) => {
                    warnings.push(warn);
                }
            }
        }
        carry.do_ctm_map = do_ctm_map;
    } else {
        let xobj_ids = collect_inherited_xobject_ids(doc, page_id);
        for xobj_id in xobj_ids {
            match decode_form_xobject_verbose(doc, xobj_id) {
                Ok(content) => {
                    let xobj_fonts = xobject_fonts_verbose(doc, page_id, xobj_id, warnings);
                    let xobj_extgstates = xobject_extgstates(doc, page_id, xobj_id);
                    let xobj_matrix = xobject_matrix(doc, xobj_id);
                    carry.ctm = multiply_ctm(saved_ctm, xobj_matrix);
                    carry.ctm_stack = vec![carry.ctm];
                    parse_content_stream(
                        &content,
                        &xobj_fonts,
                        &xobj_extgstates,
                        carry,
                        out,
                        None,
                        Some(xobj_id),
                    );
                }
                Err(warn) => {
                    warnings.push(warn);
                }
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
    let is_bold = [
        "bold",
        "heavy",
        "black",
        "semibold",
        "demibold",
        "extrabold",
    ]
    .iter()
    .any(|kw| lower.contains(kw));
    let is_italic = ["italic", "oblique", "slanted"]
        .iter()
        .any(|kw| lower.contains(kw));
    let family = name.split(['-', ',']).next().unwrap_or(name).to_string();
    (name.to_string(), is_bold, is_italic, family)
}

pub(crate) fn collect_fonts(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> HashMap<Vec<u8>, FontInfo> {
    collect_fonts_inner(doc, page_id).unwrap_or_default()
}

/// Collect non-stroking opacity values from the inherited page `/ExtGState`
/// resources. Values outside the PDF alpha range are ignored defensively.
fn collect_extgstates(doc: &lopdf::Document, page_id: ObjectId) -> HashMap<Vec<u8>, f32> {
    let mut current_id = page_id;
    loop {
        let Ok(obj) = doc.get_object(current_id) else {
            return HashMap::new();
        };
        let Ok(dict) = obj.as_dict() else {
            return HashMap::new();
        };
        if let Ok(resources_obj) = dict.get(b"Resources") {
            let Some(resources_dict) = resolve_dict(doc, resources_obj) else {
                return HashMap::new();
            };
            return collect_extgstates_from_resources(doc, resources_dict);
        }
        let Ok(parent_ref) = dict.get(b"Parent") else {
            return HashMap::new();
        };
        let Object::Reference(parent_id) = parent_ref else {
            return HashMap::new();
        };
        current_id = *parent_id;
    }
}

fn collect_extgstates_from_resources(
    doc: &lopdf::Document,
    resources_dict: &Dictionary,
) -> HashMap<Vec<u8>, f32> {
    let mut result = HashMap::new();
    let Ok(ext_obj) = resources_dict.get(b"ExtGState") else {
        return result;
    };
    let Some(ext_dict) = resolve_dict(doc, ext_obj) else {
        return result;
    };
    for (name, value) in ext_dict.iter() {
        let Some(gs) = resolve_dict(doc, value) else {
            continue;
        };
        let Some(opacity) = gs
            .get(b"ca")
            .ok()
            .and_then(object_number)
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        else {
            continue;
        };
        result.insert(name.to_vec(), opacity);
    }
    result
}

fn collect_fonts_verbose(
    doc: &lopdf::Document,
    page_id: ObjectId,
    warnings: &mut Vec<ExtractionWarning>,
) -> HashMap<Vec<u8>, FontInfo> {
    let mut current_id = page_id;
    loop {
        let Ok(obj) = doc.get_object(current_id) else {
            return HashMap::new();
        };
        let Ok(dict) = obj.as_dict() else {
            return HashMap::new();
        };
        if let Ok(resources_obj) = dict.get(b"Resources") {
            let Some(resources_dict) = resolve_dict(doc, resources_obj) else {
                return HashMap::new();
            };
            return collect_fonts_from_resources_verbose(doc, resources_dict, warnings);
        }
        let Ok(parent_ref) = dict.get(b"Parent") else {
            return HashMap::new();
        };
        let Object::Reference(parent_id) = parent_ref else {
            return HashMap::new();
        };
        current_id = *parent_id;
    }
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

fn collect_fonts_from_resources_verbose(
    doc: &lopdf::Document,
    resources_dict: &Dictionary,
    warnings: &mut Vec<ExtractionWarning>,
) -> HashMap<Vec<u8>, FontInfo> {
    let mut fonts = HashMap::new();
    let Ok(font_obj) = resources_dict.get(b"Font") else {
        return fonts;
    };
    let Some(font_dict) = resolve_dict(doc, font_obj) else {
        return fonts;
    };
    collect_font_dict_entries_verbose(doc, font_dict, &mut fonts, warnings);
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
        let Some(dict) = obj.as_dict().ok() else {
            break;
        };
        if let Ok(res_obj) = dict.get(b"Resources") {
            let ids = resolve_dict(doc, res_obj)
                .and_then(|res_dict| {
                    res_dict
                        .get(b"XObject")
                        .ok()
                        .and_then(|xobj_ref| resolve_dict(doc, xobj_ref))
                })
                .map(|xobj_dict| {
                    xobj_dict
                        .iter()
                        .filter_map(|(_, v)| {
                            if let Object::Reference(id) = v {
                                Some(*id)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                });
            if let Some(ids) = ids {
                return ids;
            }
            break; // /Resources found but no /XObject — stop climbing
        }
        let Ok(parent_ref) = dict.get(b"Parent") else {
            break;
        };
        let Object::Reference(parent_id) = parent_ref else {
            break;
        };
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
        let Some(dict) = obj.as_dict().ok() else {
            break;
        };
        if let Ok(res_obj) = dict.get(b"Resources") {
            let map = resolve_dict(doc, res_obj)
                .and_then(|res_dict| {
                    res_dict
                        .get(b"XObject")
                        .ok()
                        .and_then(|xobj_ref| resolve_dict(doc, xobj_ref))
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
        let Ok(parent_ref) = dict.get(b"Parent") else {
            break;
        };
        let Object::Reference(parent_id) = parent_ref else {
            break;
        };
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
            Some(b"Type0") => {
                match collect_type0_font(fd, doc, base_font, is_bold, is_italic, font_family) {
                    Some(fi) => fi,
                    None => continue,
                }
            }
            Some(b"Type1") | Some(b"MMType1") | Some(b"TrueType") | Some(b"Type3") => {
                collect_simple_font(fd, doc, base_font, is_bold, is_italic, font_family)
            }
            _ => continue,
        };

        fonts.insert(name.clone(), font_info);
    }
}

fn collect_font_dict_entries_verbose(
    doc: &lopdf::Document,
    font_dict: &Dictionary,
    fonts: &mut HashMap<Vec<u8>, FontInfo>,
    warnings: &mut Vec<ExtractionWarning>,
) {
    for (name, font_ref) in font_dict.iter() {
        let font_dict = match font_ref {
            Object::Reference(font_id) => doc
                .get_object(*font_id)
                .ok()
                .and_then(|object| object.as_dict().ok()),
            _ => None,
        };
        let subtype = font_dict
            .and_then(|dict| dict.get(b"Subtype").ok())
            .and_then(|object| match object {
                Object::Name(value) => Some(String::from_utf8_lossy(value).into_owned()),
                _ => None,
            });
        let supported = subtype.as_deref().is_some_and(|value| {
            matches!(value, "Type0" | "Type1" | "MMType1" | "TrueType" | "Type3")
        });
        if !supported {
            let detail = subtype
                .as_deref()
                .map_or_else(|| "missing or malformed".to_owned(), str::to_owned);
            warnings.push(ExtractionWarning {
                kind: WarningKind::UnsupportedFontSubtype,
                stream_id: None,
                message: format!(
                    "font /{} has unsupported {} /Subtype; referenced text was skipped",
                    String::from_utf8_lossy(name),
                    detail
                ),
            });
        }
        let identity_v = font_dict.is_some_and(|dict| {
            dict.get(b"Encoding").ok() == Some(&Object::Name(b"Identity-V".to_vec()))
        });
        if subtype.as_deref() == Some("Type0") && identity_v {
            warnings.push(ExtractionWarning {
                kind: WarningKind::UnsupportedVerticalWriting,
                stream_id: None,
                message: format!(
                    "font /{} uses /Identity-V; vertical writing metrics and reflow remain best-effort",
                    String::from_utf8_lossy(name)
                ),
            });
        }
    }
    collect_font_dict_entries(doc, font_dict, fonts);
    for (name, font_info) in fonts.iter() {
        if font_info.identity_fallback {
            warnings.push(ExtractionWarning {
                kind: WarningKind::MissingToUnicodeCMap,
                stream_id: None,
                message: format!(
                    "font /{} has no usable /ToUnicode CMap; Identity fallback was used",
                    String::from_utf8_lossy(name)
                ),
            });
        }
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
    let to_unicode_text = try_parse_to_unicode_text(fd, doc).unwrap_or_else(|| {
        to_unicode
            .iter()
            .map(|(&gid, &ch)| (gid, ch.to_string()))
            .collect()
    });
    // When ToUnicode is absent and the encoding is Identity-H/V, fall back to treating
    // the 2-byte character code directly as a Unicode scalar (best-effort).
    let identity_fallback = to_unicode.is_empty() && is_identity_cmap(fd);

    let desc_obj = resolve_object(doc, fd.get(b"DescendantFonts").ok()?)?;
    let Object::Array(desc_arr) = desc_obj else {
        return None;
    };
    let cid_obj = resolve_object(doc, desc_arr.first()?)?;
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
    let vertical = matches!(
        fd.get(b"Encoding").ok(),
        Some(Object::Name(name)) if name.as_slice() == b"Identity-V"
    );
    let vertical_default = cid_dict
        .get(b"DW2")
        .ok()
        .and_then(|object| resolve_object(doc, object))
        .and_then(|object| object.as_array().ok())
        .and_then(|array| array.first())
        .and_then(object_number)
        .map(|value| -value.round() as i32)
        .unwrap_or(-880);
    let vertical_runs = cid_dict
        .get(b"W2")
        .ok()
        .and_then(|object| resolve_object(doc, object))
        .and_then(|object| object.as_array().ok())
        .map(|array| parse_w2_array(array))
        .unwrap_or_default();

    Some(FontInfo {
        to_unicode,
        to_unicode_text,
        dw,
        w_runs,
        bytes_per_char: 2,
        identity_fallback,
        base_font,
        is_bold,
        is_italic,
        font_family,
        vertical,
        vertical_default,
        vertical_runs,
    })
}

/// Resolves the indirect-object form accepted for PDF dictionary values while
/// preserving direct values. Several producers serialize Type0 font arrays as
/// an indirect object even though direct arrays are also valid.
fn resolve_object<'a>(doc: &'a lopdf::Document, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => doc.get_object(*id).ok(),
        _ => Some(object),
    }
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
    let to_unicode_text = try_parse_to_unicode_text(fd, doc).unwrap_or_else(|| {
        to_unicode
            .iter()
            .map(|(&gid, &ch)| (gid, ch.to_string()))
            .collect()
    });

    let (w_runs, dw) = collect_simple_font_widths(fd, doc);
    FontInfo {
        to_unicode,
        to_unicode_text,
        dw,
        w_runs,
        bytes_per_char: 1,
        identity_fallback: false,
        base_font,
        is_bold,
        is_italic,
        font_family,
        vertical: false,
        vertical_default: -880,
        vertical_runs: Vec::new(),
    }
}

fn try_parse_to_unicode_text(
    fd: &Dictionary,
    doc: &lopdf::Document,
) -> Option<BTreeMap<u16, String>> {
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
    let map = parse_to_unicode_cmap_text(&cmap_bytes);
    if map.is_empty() { None } else { Some(map) }
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

fn parse_to_unicode_cmap_text(bytes: &[u8]) -> BTreeMap<u16, String> {
    let mut map = BTreeMap::new();
    let Ok(text) = std::str::from_utf8(bytes) else {
        return map;
    };
    let mut section = None;
    for line in text.lines().map(str::trim) {
        if line.ends_with("beginbfchar") {
            section = Some("char");
        } else if line == "endbfchar" {
            section = None;
        } else if line.ends_with("beginbfrange") {
            section = Some("range");
        } else if line == "endbfrange" {
            section = None;
        } else if section == Some("char") {
            let mut parts = line.split_ascii_whitespace();
            let Some(gid_hex) = parts.next() else {
                continue;
            };
            let Some(dst_hex) = parts.next() else {
                continue;
            };
            let gid_hex = gid_hex.trim_matches(['<', '>']);
            let dst_hex = dst_hex.trim_matches(['<', '>']);
            let Ok(gid) = u16::from_str_radix(gid_hex, 16) else {
                continue;
            };
            if let Some(value) = hex_to_unicode_string(dst_hex) {
                map.insert(gid, value);
            }
        } else if section == Some("range") {
            let mut parts = line.split_ascii_whitespace();
            let (Some(lo_hex), Some(hi_hex), Some(dst)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let Ok(lo) = u16::from_str_radix(lo_hex.trim_matches(['<', '>']), 16) else {
                continue;
            };
            let Ok(hi) = u16::from_str_radix(hi_hex.trim_matches(['<', '>']), 16) else {
                continue;
            };
            if dst == "[" {
                let values = line
                    .split(['<', '>', '[', ']', ' ', '\t'])
                    .filter(|part| !part.is_empty())
                    .skip(2);
                for (offset, value) in values.enumerate() {
                    let Some(decoded) = hex_to_unicode_string(value) else {
                        continue;
                    };
                    let Some(code) = lo.checked_add(offset as u16) else {
                        break;
                    };
                    if code > hi {
                        break;
                    }
                    map.insert(code, decoded);
                }
            } else {
                let dst_hex = dst.trim_matches(['<', '>']);
                let Ok(start) = u32::from_str_radix(dst_hex, 16) else {
                    continue;
                };
                for offset in 0..=u32::from(hi.saturating_sub(lo)) {
                    let Some(value) = start.checked_add(offset) else {
                        break;
                    };
                    let Some(decoded) = char::from_u32(value).map(|ch| ch.to_string()) else {
                        continue;
                    };
                    map.insert(lo + offset as u16, decoded);
                }
            }
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

fn hex_to_unicode_string(hex: &str) -> Option<String> {
    if hex.is_empty() || !hex.len().is_multiple_of(4) {
        return hex_to_char(hex).map(|ch| ch.to_string());
    }
    let units = hex
        .as_bytes()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| u16::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf16(&units).ok()
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

/// Parse the vertical CID metrics in `/W2`. Each array entry is a triple
/// `(w1y, v1x, v1y)`; only the vertical displacement is needed for extraction.
fn parse_w2_array(arr: &[Object]) -> Vec<VerticalWidthRun> {
    let mut runs = Vec::new();
    let mut i = 0;
    while i < arr.len() {
        let Ok(start) = arr[i].as_i64() else {
            i += 1;
            continue;
        };
        let start_gid = start as u16;
        i += 1;
        if i >= arr.len() {
            break;
        }
        match &arr[i] {
            Object::Array(values) => {
                let widths = values
                    .chunks(3)
                    .filter_map(|triple| triple.first().and_then(|value| value.as_i64().ok()))
                    .map(|value| value as i32)
                    .collect::<Vec<_>>();
                runs.push(VerticalWidthRun { start_gid, widths });
                i += 1;
            }
            Object::Integer(_) | Object::Real(_) => {
                let Ok(end) = arr[i].as_i64() else {
                    i += 1;
                    continue;
                };
                i += 1;
                if i + 2 >= arr.len() {
                    break;
                }
                let Some(w1y) = arr[i].as_i64().ok() else {
                    i += 1;
                    continue;
                };
                i += 3;
                let count = (end as usize).saturating_sub(start_gid as usize) + 1;
                runs.push(VerticalWidthRun {
                    start_gid,
                    widths: vec![w1y as i32; count],
                });
            }
            _ => i += 1,
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

/// Expand PDF shorthand text-showing operators into the existing text path.
/// `\'` is `T*` + `Tj`; `\"` sets `Tw`/`Tc`, then performs `T*` + `Tj`.
fn expand_quote_operators(tokens: Vec<(Token, usize)>) -> Vec<(Token, usize)> {
    let mut expanded = Vec::with_capacity(tokens.len());
    for (token, position) in tokens {
        let Token::Keyword(keyword) = &token else {
            expanded.push((token, position));
            continue;
        };
        if keyword == b"'" {
            if let Some((string, string_position)) = expanded.pop() {
                expanded.push((Token::Keyword(b"T*".to_vec()), position));
                expanded.push((string, string_position));
                expanded.push((Token::Keyword(b"Tj".to_vec()), position));
            } else {
                expanded.push((token, position));
            }
        } else if keyword == b"\"" {
            let Some((string, string_position)) = expanded.pop() else {
                expanded.push((token, position));
                continue;
            };
            let Some((character_spacing, character_position)) = expanded.pop() else {
                expanded.push((string, string_position));
                expanded.push((token, position));
                continue;
            };
            let Some((word_spacing, word_position)) = expanded.pop() else {
                expanded.push((character_spacing, character_position));
                expanded.push((string, string_position));
                expanded.push((token, position));
                continue;
            };
            expanded.push((word_spacing, word_position));
            expanded.push((Token::Keyword(b"Tw".to_vec()), position));
            expanded.push((character_spacing, character_position));
            expanded.push((Token::Keyword(b"Tc".to_vec()), position));
            expanded.push((Token::Keyword(b"T*".to_vec()), position));
            expanded.push((string, string_position));
            expanded.push((Token::Keyword(b"Tj".to_vec()), position));
        } else {
            expanded.push((token, position));
        }
    }
    expanded
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
    cur_opacity: f32,
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
    /// Non-stroking opacity stack paired with `q`/`Q` graphics-state saves.
    opacity_stack: Vec<f32>,
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
    /// Linear part of the most recent text matrix: `[a, b, c, d]`.
    tm_matrix: [f32; 4],
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
    /// Horizontal text scaling as a multiplier (set by `Tz`, default 1.0).
    horizontal_scale: f32,
    /// Text rise in text space points (set by `Ts`, default 0).
    text_rise: f32,
}

impl Default for ParseCarryState {
    fn default() -> Self {
        Self {
            cur_color: [0.0, 0.0, 0.0],
            cur_opacity: 1.0,
            cur_render_mode: 0,
            ctm: IDENTITY_CTM,
            do_ctm_map: Vec::new(),
            ctm_stack: vec![IDENTITY_CTM],
            opacity_stack: vec![1.0],
            in_bt: false,
            font_name: Vec::new(),
            tf_font_size: 12.0,
            font_size: 12.0,
            tm_y_scale: 1.0,
            tm_matrix: [1.0, 0.0, 0.0, 1.0],
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
            horizontal_scale: 1.0,
            text_rise: 0.0,
        }
    }
}

fn parse_content_stream(
    bytes: &[u8],
    fonts: &HashMap<Vec<u8>, FontInfo>,
    extgstates: &HashMap<Vec<u8>, f32>,
    state: &mut ParseCarryState,
    out: &mut Vec<TextFragment>,
    stream_idx: Option<usize>,
    xobj_id: Option<(u32, u16)>,
) {
    let tokens = expand_quote_operators(tokenize(bytes));
    let mut stack: Vec<(Token, usize)> = Vec::new();
    // Read text state from carry so that BT blocks split across stream boundaries
    // (a Distiller/PScript5 pattern) are handled correctly.
    let mut in_bt = state.in_bt;
    let mut font_name = state.font_name.clone();
    let mut tf_font_size = state.tf_font_size;
    let mut font_size = state.font_size;
    let mut tm_y_scale = state.tm_y_scale;
    let mut tm_matrix = state.tm_matrix;
    let mut tm_x_scale = state.tm_x_scale;
    let mut tm_lm_x = state.tm_lm_x;
    let mut tm_lm_y = state.tm_lm_y;
    let mut x = state.text_x;
    let mut y = state.text_y;
    let mut tm_origin_set = state.tm_origin_set;
    let mut text_leading = state.text_leading;
    let mut char_spacing = state.char_spacing;
    let mut word_spacing = state.word_spacing;
    let mut horizontal_scale = state.horizontal_scale;
    let mut text_rise = state.text_rise;
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
                    tm_matrix = [1.0, 0.0, 0.0, 1.0];
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
                b"Tz" => {
                    if let Some((Token::Number(v), _)) = stack.pop()
                        && v.is_finite()
                        && v > 0.0
                    {
                        horizontal_scale = v / 100.0;
                    }
                    stack.clear();
                }
                b"Ts" => {
                    if let Some((Token::Number(v), _)) = stack.pop()
                        && v.is_finite()
                    {
                        text_rise = v;
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
                    if let (
                        Some((Token::Number(av), _)),
                        Some((Token::Number(bv), _)),
                        Some((Token::Number(cv), _)),
                        Some((Token::Number(dv), _)),
                    ) = (
                        pop_a.as_ref(),
                        pop_b.as_ref(),
                        pop_c.as_ref(),
                        pop_d.as_ref(),
                    ) {
                        tm_matrix = [*av, *bv, *cv, *dv];
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
                    state
                        .ctm_stack
                        .push(*state.ctm_stack.last().unwrap_or(&IDENTITY_CTM));
                    state.opacity_stack.push(state.cur_opacity);
                    stack.clear();
                }
                b"Q" => {
                    if state.ctm_stack.len() > 1 {
                        state.ctm_stack.pop();
                    }
                    if let Some(opacity) = state.opacity_stack.pop() {
                        state.cur_opacity = opacity;
                    }
                    stack.clear();
                }
                b"gs" => {
                    if let Some((Token::Name(name), _)) = stack.pop()
                        && let Some(opacity) = extgstates.get(&name)
                    {
                        state.cur_opacity = *opacity;
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
                    let op_end = Some(tok_pos + 2); // "Tj" is 2 bytes
                    let bytes_opt = match stack.pop() {
                        Some((Token::HexStr(b), _)) => Some(b),
                        Some((Token::LitStr(b), _)) => Some(b),
                        _ => None,
                    };
                    if let Some(char_bytes) = bytes_opt {
                        let ctm = *state.ctm_stack.last().unwrap_or(&IDENTITY_CTM);
                        let (px, py) = apply_ctm(ctm, x, y + text_rise);
                        let scale = ctm_scale(ctm);
                        let (tm_ox, tm_oy) = if tm_origin_set {
                            let (ox, oy) = apply_ctm(ctm, state.tm_origin_x, state.tm_origin_y);
                            (Some(ox), Some(oy))
                        } else {
                            (None, None)
                        };
                        let tm_xs = if tm_origin_set {
                            Some(tm_x_scale)
                        } else {
                            None
                        };
                        let (tm_lm_ox, tm_lm_oy) = if tm_origin_set {
                            let (lx, ly) = apply_ctm(ctm, tm_lm_x, tm_lm_y);
                            (Some(lx), Some(ly))
                        } else {
                            (None, None)
                        };
                        // x_font_size uses the Tm x-scale for width; font_size (y-scale)
                        // is kept for height.  For uniform Tm they are equal.
                        let x_font_size = tf_font_size * tm_x_scale * horizontal_scale * scale;
                        if let Some(frag) = decode_chars_to_fragment(
                            &char_bytes,
                            &font_name,
                            font_size * scale,
                            x_font_size,
                            px,
                            py,
                            ctm,
                            tm_matrix,
                            horizontal_scale,
                            fonts,
                            state.cur_color,
                            state.cur_opacity,
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
                            let local_advance = if scale > 0.0 {
                                frag.width / scale
                            } else {
                                frag.width
                            };
                            // Apply Tc/Tw spacing (in unscaled text space → user space via tm_x_scale).
                            let n_chars = frag.text.chars().count() as f32;
                            let n_spaces = frag.text.chars().filter(|&c| c == ' ').count() as f32;
                            x += local_advance
                                + char_spacing * tm_x_scale * horizontal_scale * n_chars
                                + word_spacing * tm_x_scale * horizontal_scale * n_spaces;
                            out.push(frag);
                        }
                    }
                    stack.clear();
                }
                b"TJ" if in_bt => {
                    let op_start = Some(tok_pos);
                    let op_end = Some(tok_pos + 2); // "TJ" is 2 bytes
                    if let Some((Token::Array(items), _)) = stack.pop() {
                        let ctm = *state.ctm_stack.last().unwrap_or(&IDENTITY_CTM);
                        let scale = ctm_scale(ctm);
                        let (tm_ox, tm_oy) = if tm_origin_set {
                            let (ox, oy) = apply_ctm(ctm, state.tm_origin_x, state.tm_origin_y);
                            (Some(ox), Some(oy))
                        } else {
                            (None, None)
                        };
                        let tm_xs = if tm_origin_set {
                            Some(tm_x_scale)
                        } else {
                            None
                        };
                        let (tm_lm_ox, tm_lm_oy) = if tm_origin_set {
                            let (lx, ly) = apply_ctm(ctm, tm_lm_x, tm_lm_y);
                            (Some(lx), Some(ly))
                        } else {
                            (None, None)
                        };
                        let x_font_size = tf_font_size * tm_x_scale * horizontal_scale * scale;
                        let mut cur_x = x; // local-space cursor
                        for item in items {
                            match item {
                                Token::HexStr(ref b) | Token::LitStr(ref b) => {
                                    let (px, py) = apply_ctm(ctm, cur_x, y + text_rise);
                                    if let Some(frag) = decode_chars_to_fragment(
                                        b,
                                        &font_name,
                                        font_size * scale,
                                        x_font_size,
                                        px,
                                        py,
                                        ctm,
                                        tm_matrix,
                                        horizontal_scale,
                                        fonts,
                                        state.cur_color,
                                        state.cur_opacity,
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
                                            + char_spacing
                                                * tm_x_scale
                                                * horizontal_scale
                                                * n_chars
                                            + word_spacing
                                                * tm_x_scale
                                                * horizontal_scale
                                                * n_spaces;
                                        out.push(frag);
                                    }
                                }
                                Token::Number(kern) => {
                                    // Kern in TJ is in thousandths of a text-space unit;
                                    // multiply by tf_font_size × tm_x_scale to convert to
                                    // user space (horizontal axis).
                                    cur_x -= kern / 1000.0
                                        * tf_font_size
                                        * tm_x_scale
                                        * horizontal_scale;
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
    state.in_bt = in_bt;
    state.font_name = font_name;
    state.tf_font_size = tf_font_size;
    state.font_size = font_size;
    state.tm_y_scale = tm_y_scale;
    state.tm_matrix = tm_matrix;
    state.tm_x_scale = tm_x_scale;
    state.tm_lm_x = tm_lm_x;
    state.tm_lm_y = tm_lm_y;
    state.text_x = x;
    state.text_y = y;
    state.tm_origin_set = tm_origin_set;
    state.text_leading = text_leading;
    state.char_spacing = char_spacing;
    state.word_spacing = word_spacing;
    state.horizontal_scale = horizontal_scale;
    state.text_rise = text_rise;
}

#[allow(clippy::too_many_arguments)] // All args are logically required; a ctx struct would add ceremony
fn decode_chars_to_fragment(
    char_bytes: &[u8],
    font_name: &[u8],
    font_size: f32,
    x_font_size: f32,
    x: f32,
    y: f32,
    ctm: [f32; 6],
    tm_matrix: [f32; 4],
    horizontal_scale: f32,
    fonts: &HashMap<Vec<u8>, FontInfo>,
    color: [f32; 3],
    opacity: f32,
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
    let mut total_advance = 0.0f32;
    let mut total_vertical_advance = 0.0f32;

    match font_info.bytes_per_char {
        2 => {
            if !char_bytes.len().is_multiple_of(2) {
                return None;
            }
            for chunk in char_bytes.chunks(2) {
                let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
                let mapped = font_info
                    .to_unicode_text
                    .get(&gid)
                    .cloned()
                    .or_else(|| font_info.to_unicode.get(&gid).map(|ch| ch.to_string()))
                    .or_else(|| {
                        if font_info.identity_fallback {
                            char::from_u32(gid as u32)
                                .filter(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
                                .map(|c| c.to_string())
                        } else {
                            None
                        }
                    });
                let Some(mapped) = mapped else { continue };
                text.push_str(&mapped);
                let aw = font_info.advance_width(gid);
                total_advance += aw as f32 / 1000.0;
                total_vertical_advance += font_info.vertical_advance(gid) as f32 / 1000.0;
            }
        }
        _ => {
            for &b in char_bytes {
                let code = b as u16;
                let mapped = font_info
                    .to_unicode_text
                    .get(&code)
                    .cloned()
                    .or_else(|| font_info.to_unicode.get(&code).map(|ch| ch.to_string()));
                let Some(mapped) = mapped else {
                    continue;
                };
                text.push_str(&mapped);
                let aw = font_info.advance_width(code);
                total_advance += aw as f32 / 1000.0;
            }
        }
    }

    if text.is_empty() {
        return None;
    }
    // Fix 5: zero-width fallback — some fonts have missing /W entries and dw=0,
    // which would make every fragment 0-width and break column detection.
    if total_advance == 0.0 {
        total_advance = text.chars().count() as f32 * 0.5;
    }
    let (bbox_x, bbox_y, bbox_width, bbox_height, rotation_degrees) = if font_info.vertical {
        if total_vertical_advance == 0.0 {
            total_vertical_advance = text.chars().count() as f32 * -0.88;
        }
        let (x, y, width, height, rotation) =
            vertical_text_bbox(x, y, total_vertical_advance, tf_font_size, ctm, tm_matrix);
        (x, y, width, height, rotation)
    } else {
        let (x, y, width, height) = text_bbox(
            x,
            y,
            total_advance,
            tf_font_size,
            horizontal_scale,
            ctm,
            tm_matrix,
        );
        (x, y, width, height, text_rotation_degrees(ctm, tm_matrix))
    };
    let space_advance = font_info
        .to_unicode_text
        .iter()
        .find(|&(&gid, text)| text == " " || font_info.to_unicode.get(&gid) == Some(&' '))
        .map(|(&gid, _)| font_info.advance_width(gid) as f32 / 1000.0 * x_font_size)
        .unwrap_or(0.0);
    Some(TextFragment {
        text,
        x: bbox_x,
        y: bbox_y,
        width: bbox_width,
        height: bbox_height,
        rotation_degrees,
        font_size,
        font_name: String::from_utf8_lossy(font_name).into_owned(),
        color,
        opacity,
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

fn text_rotation_degrees(ctm: [f32; 6], tm_matrix: [f32; 4]) -> f32 {
    let [a, b, _, _] = tm_matrix;
    let x = ctm[0] * a + ctm[2] * b;
    let y = ctm[1] * a + ctm[3] * b;
    if x.abs() < 0.0001 && y.abs() < 0.0001 {
        0.0
    } else {
        normalize_rotation(y.atan2(x).to_degrees())
    }
}

/// Compute a conservative bbox for Identity-V text using the PDF vertical
/// displacement from `/DW2` or `/W2`.
fn vertical_text_bbox(
    x: f32,
    y: f32,
    advance: f32,
    tf_font_size: f32,
    ctm: [f32; 6],
    tm_matrix: [f32; 4],
) -> (f32, f32, f32, f32, f32) {
    let [a, b, c, d] = tm_matrix;
    let glyph_axis = (
        (ctm[0] * a + ctm[2] * b) * tf_font_size,
        (ctm[1] * a + ctm[3] * b) * tf_font_size,
    );
    let vertical_axis = (
        (ctm[0] * c + ctm[2] * d) * advance,
        (ctm[1] * c + ctm[3] * d) * advance,
    );
    let corners = [
        (x, y),
        (x + glyph_axis.0, y + glyph_axis.1),
        (x + vertical_axis.0, y + vertical_axis.1),
        (
            x + glyph_axis.0 + vertical_axis.0,
            y + glyph_axis.1 + vertical_axis.1,
        ),
    ];
    let min_x = corners
        .iter()
        .map(|point| point.0)
        .fold(f32::INFINITY, f32::min);
    let max_x = corners
        .iter()
        .map(|point| point.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = corners
        .iter()
        .map(|point| point.1)
        .fold(f32::INFINITY, f32::min);
    let max_y = corners
        .iter()
        .map(|point| point.1)
        .fold(f32::NEG_INFINITY, f32::max);
    let rotation = normalize_rotation(vertical_axis.1.atan2(vertical_axis.0).to_degrees());
    (min_x, min_y, max_x - min_x, max_y - min_y, rotation)
}

/// Convert the text-space rectangle into an axis-aligned page-space bbox.
/// The four corners are transformed by the full CTM × Tm linear part, so
/// rotated and sheared text is represented by its enclosing rectangle.
fn text_bbox(
    x: f32,
    y: f32,
    advance: f32,
    tf_font_size: f32,
    horizontal_scale: f32,
    ctm: [f32; 6],
    tm_matrix: [f32; 4],
) -> (f32, f32, f32, f32) {
    let [a, b, c, d] = tm_matrix;
    let horizontal = (
        (ctm[0] * a + ctm[2] * b) * tf_font_size * horizontal_scale * advance,
        (ctm[1] * a + ctm[3] * b) * tf_font_size * horizontal_scale * advance,
    );
    let vertical = (
        (ctm[0] * c + ctm[2] * d) * tf_font_size,
        (ctm[1] * c + ctm[3] * d) * tf_font_size,
    );
    if horizontal.1.abs() < 0.0001 && vertical.0.abs() < 0.0001 {
        return (x, y, horizontal.0.abs(), vertical.1.abs());
    }
    let origin = (x, y);
    let corners = [
        origin,
        (origin.0 + horizontal.0, origin.1 + horizontal.1),
        (origin.0 + vertical.0, origin.1 + vertical.1),
        (
            origin.0 + horizontal.0 + vertical.0,
            origin.1 + horizontal.1 + vertical.1,
        ),
    ];
    let min_x = corners
        .iter()
        .map(|point| point.0)
        .fold(f32::INFINITY, f32::min);
    let max_x = corners
        .iter()
        .map(|point| point.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = corners
        .iter()
        .map(|point| point.1)
        .fold(f32::INFINITY, f32::min);
    let max_y = corners
        .iter()
        .map(|point| point.1)
        .fold(f32::NEG_INFINITY, f32::max);
    (min_x, min_y, max_x - min_x, max_y - min_y)
}

// ---------------------------------------------------------------------------
// Layout region planning
// ---------------------------------------------------------------------------

/// Classifies the structural role of a [`LayoutRegion`].
///
/// `#[non_exhaustive]` — future variants may be added without a semver break.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutRegionKind {
    /// A heading at the given level (1 = largest, following the same font-size
    /// thresholds as [`crate::TextChunk`]).
    Heading(u8),
    /// A free-standing paragraph (single-column, non-tabular text block).
    Paragraph,
    /// A cell inside a detected table or form grid.
    TableCell,
    /// Could not be classified with available signals.
    Unknown,
}

/// A detected layout region on a page, with both source-text bounds and the
/// inferred available rectangle for replacement text.
///
/// Obtain via [`extract_layout_regions`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct LayoutRegion {
    /// Structural classification.
    pub kind: LayoutRegionKind,
    /// Functional role in a translation/editing workflow.
    pub role: LayoutRegionRole,
    /// 0-based row index within the detected table/grid (`None` for headings/paragraphs).
    pub row: Option<usize>,
    /// 0-based column index (`None` for headings/paragraphs).
    pub col: Option<usize>,
    /// Concatenated text of all source fragments.
    pub text: String,
    /// Bounding box of the *source* glyphs: `[x, y, width, height]` in PDF points.
    pub source_bbox: [f32; 4],
    /// Inferred *available* area for replacement text: `[x, y, width, height]`.
    ///
    /// Width extends to the start of the next column (or the page edge), not just
    /// to the end of the source glyphs — this is the key difference from `source_bbox`.
    /// Height spans from the current row's ascender down to the next row's ascender
    /// (or a generous estimate for the last row).
    pub usable_rect: [f32; 4],
    /// All source fragments (carry `source_op_*` fields for suppression).
    pub fragments: Vec<TextFragment>,
}

/// Options for [`extract_layout_regions`].
///
/// Construct with `LayoutRegionOptions::default()` and override fields as needed.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct LayoutRegionOptions {
    /// Infer `usable_rect` height from the distance to the adjacent row.
    /// Default `true`.
    pub infer_row_heights: bool,
    /// Infer `usable_rect` width from the gap to the next column (or page edge).
    /// When `false`, `usable_rect.width` falls back to `source_bbox.width`.
    /// Default `true`.
    pub infer_column_widths: bool,
    /// Padding in PDF points subtracted from the inferred usable dimensions.
    /// Default `2.0`.
    pub margin: f32,
}

impl Default for LayoutRegionOptions {
    fn default() -> Self {
        Self {
            infer_row_heights: true,
            infer_column_widths: true,
            margin: 2.0,
        }
    }
}

/// Combines a [`LayoutRegion`] with the [`crate::FitResult`] for its planned
/// replacement text and any [`Collision`]s against neighbouring regions.
///
/// Returned by [`crate::Document::plan_text_for_regions`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct RegionFitPlan {
    /// The layout region being filled.
    pub region: LayoutRegion,
    /// How the replacement text lays out inside `region.usable_rect`.
    pub fit: crate::document::FitResult,
    /// Classified collisions between this region's `fit.used_rect` and other regions
    /// in the same planning batch.  Each entry carries the raw geometric [`Collision`]
    /// plus a [`CollisionKind`] and the roles of the two colliding regions.
    pub collisions: Vec<ClassifiedCollision>,
}

/// Type of layout problem found while checking planned replacement text.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutIssueKind {
    /// The planned text does not fit inside the target rectangle.
    TextOverflow,
    /// Two planned text rectangles overlap.
    TextCollision,
    /// Planned text intersects an image on the page.
    ImageOverlap,
    /// The planned text rectangle moved too far from the source glyph bounds.
    BboxDrift,
    /// The text was shrunk but remains acceptable.
    AcceptedShrink,
    /// Planned text rectangle overlaps a ruling line or table border.
    TextVsTableBorder,
    /// Planned text overflows its detected table cell boundary.
    TableCellSpillover,
    /// Text appears clipped by the page or clip-box boundary.
    ClippedText,
    /// Same-row label and value pair have a significant baseline difference.
    BaselineMismatch,
    /// Font size is a significant outlier compared to column neighbors.
    FontSizeOutlier,
}

/// Severity of a layout issue.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LayoutIssueSeverity {
    /// Low-risk issue, usually acceptable for best-effort output.
    Minor,
    /// Visible issue that should be considered for repair.
    Moderate,
    /// Significant issue that should be repaired or escalated.
    Major,
}

impl From<CollisionSeverity> for LayoutIssueSeverity {
    fn from(value: CollisionSeverity) -> Self {
        match value {
            CollisionSeverity::Minor => Self::Minor,
            CollisionSeverity::Moderate => Self::Moderate,
            CollisionSeverity::Major => Self::Major,
        }
    }
}

/// One concrete layout issue in a page-level quality report.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct LayoutIssue {
    /// 1-based page number.
    pub page: u32,
    /// Region index within the planning batch.
    pub id: usize,
    /// Problem category.
    pub kind: LayoutIssueKind,
    /// Problem severity.
    pub severity: LayoutIssueSeverity,
    /// Primary issue rectangle, when available.
    pub rect: Option<[f32; 4]>,
    /// Source glyph bounding box for the affected region, when available.
    pub source_rect: Option<[f32; 4]>,
    /// Planned text rectangle for the affected region, when available.
    pub placed_rect: Option<[f32; 4]>,
    /// Overlap area in PDF points², when relevant.
    pub overlap_area: Option<f32>,
    /// Short human-readable diagnostic.
    pub message: String,
}

/// Page-level aggregate quality summary derived from a batch of [`RegionFitPlan`]s.
///
/// Build one with [`PageFitSummary::from_plans`] after calling
/// [`Document::plan_text_for_regions`] or
/// [`Document::plan_text_for_regions_with_policy`].  Use the summary to decide
/// whether a translated page meets quality gates before writing the final PDF.
///
/// # Example
///
/// ```rust,no_run
/// # use harumi::{Document, PageFitSummary};
/// # fn main() -> harumi::Result<()> {
/// # let doc = Document::from_bytes(&[])?;
/// # let plans = vec![];
/// let summary = PageFitSummary::from_plans(&plans);
/// if summary.collision_count > 0 || summary.overflow_count > 0 {
///     // inspect plans and adjust placement
/// }
/// # Ok(())
/// # }
/// ```
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct PageFitSummary {
    /// Number of regions where [`FitResult::overflow`] is `true`.
    pub overflow_count: usize,
    /// Number of unique classified collisions across all regions in the batch.
    ///
    /// Each `(index_a, index_b)` pair is counted once, even if it appears in
    /// multiple `RegionFitPlan.collisions` lists.
    pub collision_count: usize,
    /// Number of regions where [`FitResult::status`] is [`PlacementStatus::Shrunk`]
    /// or [`PlacementStatus::ShrunkToMin`].
    pub shrunk_count: usize,
    /// The largest [`Collision::overlap_area`] across all unique collisions, or `0.0`
    /// if there are none.
    pub worst_overlap_area: f32,
    /// The [`Collision::overlap_rect`] of the worst (largest-area) collision,
    /// or `None` if there are no collisions.
    pub worst_overlap_rect: Option<[f32; 4]>,
}

impl PageFitSummary {
    /// Compute a summary from a slice of [`RegionFitPlan`]s returned by a planning call.
    pub fn from_plans(plans: &[RegionFitPlan]) -> Self {
        use crate::document::PlacementStatus;
        use std::collections::HashSet;

        let overflow_count = plans.iter().filter(|p| p.fit.overflow()).count();
        let shrunk_count = plans
            .iter()
            .filter(|p| {
                matches!(
                    p.fit.status,
                    PlacementStatus::Shrunk | PlacementStatus::ShrunkToMin
                )
            })
            .count();

        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        let mut worst_area = 0.0_f32;
        let mut worst_rect: Option<[f32; 4]> = None;

        for plan in plans {
            for cc in &plan.collisions {
                let key = (cc.collision.index_a, cc.collision.index_b);
                if seen.insert(key) && cc.collision.overlap_area > worst_area {
                    worst_area = cc.collision.overlap_area;
                    worst_rect = Some(cc.collision.overlap_rect);
                }
            }
        }

        Self {
            overflow_count,
            collision_count: seen.len(),
            shrunk_count,
            worst_overlap_area: worst_area,
            worst_overlap_rect: worst_rect,
        }
    }
}

/// Page-level layout quality report for translated or replacement text.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct PageLayoutQuality {
    /// 1-based page number.
    pub page_num: u32,
    /// Backward-compatible aggregate fitting and collision summary.
    pub summary: PageFitSummary,
    /// Detailed issues suitable for debug overlays or AI repair prompts.
    pub issues: Vec<LayoutIssue>,
    /// Number of text overflow issues.
    pub overflow_count: usize,
    /// Number of unique text collision issues.
    pub collision_count: usize,
    /// Number of text-vs-image overlap issues.
    pub image_overlap_count: usize,
    /// Number of source-vs-placed drift issues.
    pub bbox_drift_count: usize,
    /// Worst issue severity on the page, or `None` when no issues were found.
    pub worst_severity: Option<LayoutIssueSeverity>,
}

impl PageLayoutQuality {
    /// Build a quality report from region fit plans and optional image bounding boxes.
    ///
    /// `image_bboxes` are `[x, y, width, height]` rectangles in PDF points, matching
    /// [`crate::Document::page_image_bboxes`].
    pub fn from_plans(page_num: u32, plans: &[RegionFitPlan], image_bboxes: &[[f32; 4]]) -> Self {
        use crate::document::PlacementStatus;
        use std::collections::HashSet;

        let summary = PageFitSummary::from_plans(plans);
        let mut issues = Vec::new();

        for (idx, plan) in plans.iter().enumerate() {
            if plan.fit.overflow()
                || matches!(
                    plan.fit.status,
                    PlacementStatus::Overflow
                        | PlacementStatus::Truncated
                        | PlacementStatus::ShrunkToMin
                )
            {
                let severity = if matches!(
                    plan.fit.status,
                    PlacementStatus::Overflow | PlacementStatus::Truncated
                ) || plan.fit.overflow_vertical
                {
                    LayoutIssueSeverity::Major
                } else {
                    LayoutIssueSeverity::Moderate
                };
                issues.push(LayoutIssue {
                    page: page_num,
                    id: idx,
                    kind: LayoutIssueKind::TextOverflow,
                    severity,
                    rect: Some(plan.fit.used_rect),
                    source_rect: Some(plan.region.source_bbox),
                    placed_rect: Some(plan.fit.used_rect),
                    overlap_area: None,
                    message: "planned text overflows its target rectangle".to_owned(),
                });
            } else if matches!(plan.fit.status, PlacementStatus::Shrunk) {
                issues.push(LayoutIssue {
                    page: page_num,
                    id: idx,
                    kind: LayoutIssueKind::AcceptedShrink,
                    severity: LayoutIssueSeverity::Minor,
                    rect: Some(plan.fit.used_rect),
                    source_rect: Some(plan.region.source_bbox),
                    placed_rect: Some(plan.fit.used_rect),
                    overlap_area: None,
                    message: "text was shrunk to fit".to_owned(),
                });
            }

            let drift = bbox_drift_ratio(plan.region.source_bbox, plan.fit.used_rect);
            if drift > 0.35 {
                let severity = if drift > 1.0 {
                    LayoutIssueSeverity::Major
                } else {
                    LayoutIssueSeverity::Moderate
                };
                issues.push(LayoutIssue {
                    page: page_num,
                    id: idx,
                    kind: LayoutIssueKind::BboxDrift,
                    severity,
                    rect: Some(plan.fit.used_rect),
                    source_rect: Some(plan.region.source_bbox),
                    placed_rect: Some(plan.fit.used_rect),
                    overlap_area: None,
                    message: "planned text moved away from its source bounds".to_owned(),
                });
            }

            for image_rect in image_bboxes {
                if let Some(overlap) = rect_intersection(plan.fit.used_rect, *image_rect) {
                    let overlap_area = rect_area(overlap);
                    let placed_area = rect_area(plan.fit.used_rect);
                    let severity =
                        collision_severity(overlap_area, placed_area, rect_area(*image_rect))
                            .into();
                    issues.push(LayoutIssue {
                        page: page_num,
                        id: idx,
                        kind: LayoutIssueKind::ImageOverlap,
                        severity,
                        rect: Some(overlap),
                        source_rect: Some(plan.region.source_bbox),
                        placed_rect: Some(plan.fit.used_rect),
                        overlap_area: Some(overlap_area),
                        message: "planned text overlaps an image".to_owned(),
                    });
                }
            }
        }

        let mut seen_collisions = HashSet::new();
        for plan in plans {
            for cc in &plan.collisions {
                let key = (cc.collision.index_a, cc.collision.index_b);
                if seen_collisions.insert(key) {
                    issues.push(LayoutIssue {
                        page: page_num,
                        id: cc.collision.index_a,
                        kind: LayoutIssueKind::TextCollision,
                        severity: cc.severity.clone().into(),
                        rect: Some(cc.collision.overlap_rect),
                        source_rect: plans
                            .get(cc.collision.index_a)
                            .map(|p| p.region.source_bbox),
                        placed_rect: plans.get(cc.collision.index_a).map(|p| p.fit.used_rect),
                        overlap_area: Some(cc.collision.overlap_area),
                        message: format!(
                            "planned text collides with region {}",
                            cc.collision.index_b
                        ),
                    });
                }
            }
        }

        let overflow_count = issues
            .iter()
            .filter(|i| i.kind == LayoutIssueKind::TextOverflow)
            .count();
        let collision_count = issues
            .iter()
            .filter(|i| i.kind == LayoutIssueKind::TextCollision)
            .count();
        let image_overlap_count = issues
            .iter()
            .filter(|i| i.kind == LayoutIssueKind::ImageOverlap)
            .count();
        let bbox_drift_count = issues
            .iter()
            .filter(|i| i.kind == LayoutIssueKind::BboxDrift)
            .count();
        let worst_severity = issues.iter().map(|i| i.severity.clone()).max();

        Self {
            page_num,
            summary,
            issues,
            overflow_count,
            collision_count,
            image_overlap_count,
            bbox_drift_count,
            worst_severity,
        }
    }

    /// Like [`from_plans`](Self::from_plans) but also checks placements against
    /// `rules` (ruling lines / table borders) extracted from the page.
    ///
    /// Issues of kind [`LayoutIssueKind::TextVsTableBorder`] are appended for any
    /// placement rectangle that overlaps a [`VectorRule`].
    pub fn from_plans_with_rules(
        page_num: u32,
        plans: &[RegionFitPlan],
        image_bboxes: &[[f32; 4]],
        rules: &[VectorRule],
    ) -> Self {
        let mut quality = Self::from_plans(page_num, plans, image_bboxes);
        let placed_rects: Vec<[f32; 4]> = plans.iter().map(|p| p.fit.used_rect).collect();
        let rule_hits = detect_text_vs_rule_collisions(&placed_rects, rules);
        for (text_idx, _rule_idx, severity) in rule_hits {
            quality.issues.push(LayoutIssue {
                page: page_num,
                id: text_idx,
                kind: LayoutIssueKind::TextVsTableBorder,
                severity: severity.into(),
                rect: plans.get(text_idx).map(|p| p.fit.used_rect),
                source_rect: plans.get(text_idx).map(|p| p.region.source_bbox),
                placed_rect: plans.get(text_idx).map(|p| p.fit.used_rect),
                overlap_area: None,
                message: "planned text overlaps a ruling line or table border".to_owned(),
            });
        }
        if quality
            .worst_severity
            .as_ref()
            .map(|s| s < &LayoutIssueSeverity::Major)
            .unwrap_or(true)
        {
            quality.worst_severity = quality.issues.iter().map(|i| i.severity.clone()).max();
        }
        quality
    }

    /// Build a quality report from externally computed [`SimplePlacement`]s.
    ///
    /// This is the entry point for downstream crates (such as `harumi-ai`) that compute
    /// their own text placements without going through [`crate::Document::plan_text_for_regions`].
    /// Because [`RegionFitPlan`] and [`LayoutRegion`] are `#[non_exhaustive]`, they cannot be
    /// constructed with struct literal syntax; this method accepts the lightweight
    /// [`SimplePlacement`] type instead.
    ///
    /// Checks performed:
    /// - Text/text collision between all `placements`.
    /// - Text/image overlap against `image_bboxes`.
    /// - Text/rule overlap against `rules` (pass `&[]` to skip).
    /// - Overflow flag from each placement.
    pub fn from_simple_placements(
        page_num: u32,
        placements: &[SimplePlacement],
        image_bboxes: &[[f32; 4]],
        rules: &[VectorRule],
    ) -> Self {
        let mut issues = Vec::new();

        let placed_boxes: Vec<PlacedBox> = placements
            .iter()
            .map(|p| PlacedBox::new(p.placed_rect))
            .collect();
        let placed_rects: Vec<[f32; 4]> = placements.iter().map(|p| p.placed_rect).collect();

        // Overflow
        let mut overflow_count = 0usize;
        for p in placements {
            if p.overflow {
                overflow_count += 1;
                issues.push(LayoutIssue {
                    page: page_num,
                    id: p.id,
                    kind: LayoutIssueKind::TextOverflow,
                    severity: LayoutIssueSeverity::Major,
                    rect: Some(p.placed_rect),
                    source_rect: Some(p.source_rect),
                    placed_rect: Some(p.placed_rect),
                    overlap_area: None,
                    message: "placed text overflows its target rectangle".to_owned(),
                });
            }
        }

        // Text/text collisions
        let raw_collisions = detect_collisions(&placed_boxes);
        let mut collision_count = 0usize;
        for col in &raw_collisions {
            let area_a = rect_area(placements[col.index_a].placed_rect);
            let area_b = rect_area(placements[col.index_b].placed_rect);
            let severity: LayoutIssueSeverity =
                collision_severity(col.overlap_area, area_a, area_b).into();
            collision_count += 1;
            issues.push(LayoutIssue {
                page: page_num,
                id: col.index_a,
                kind: LayoutIssueKind::TextCollision,
                severity,
                rect: Some(col.overlap_rect),
                source_rect: placements.get(col.index_a).map(|p| p.source_rect),
                placed_rect: placements.get(col.index_a).map(|p| p.placed_rect),
                overlap_area: Some(col.overlap_area),
                message: format!("placed text collides with placement {}", col.index_b),
            });
        }

        // Text/image overlaps
        let mut image_overlap_count = 0usize;
        for (i, p) in placements.iter().enumerate() {
            for img_rect in image_bboxes {
                if let Some(overlap) = rect_intersection(p.placed_rect, *img_rect) {
                    let overlap_area = rect_area(overlap);
                    let placed_area = rect_area(p.placed_rect);
                    let severity: LayoutIssueSeverity =
                        collision_severity(overlap_area, placed_area, rect_area(*img_rect)).into();
                    image_overlap_count += 1;
                    issues.push(LayoutIssue {
                        page: page_num,
                        id: i,
                        kind: LayoutIssueKind::ImageOverlap,
                        severity,
                        rect: Some(overlap),
                        source_rect: Some(p.source_rect),
                        placed_rect: Some(p.placed_rect),
                        overlap_area: Some(overlap_area),
                        message: "placed text overlaps an image".to_owned(),
                    });
                }
            }
        }

        // Text/rule overlaps
        let rule_hits = detect_text_vs_rule_collisions(&placed_rects, rules);
        for (ti, _ri, severity) in rule_hits {
            issues.push(LayoutIssue {
                page: page_num,
                id: placements.get(ti).map(|p| p.id).unwrap_or(ti),
                kind: LayoutIssueKind::TextVsTableBorder,
                severity: severity.into(),
                rect: placements.get(ti).map(|p| p.placed_rect),
                source_rect: placements.get(ti).map(|p| p.source_rect),
                placed_rect: placements.get(ti).map(|p| p.placed_rect),
                overlap_area: None,
                message: "placed text overlaps a ruling line or table border".to_owned(),
            });
        }

        let worst_severity = issues.iter().map(|i| i.severity.clone()).max();

        let summary = {
            let worst_overlap_area = raw_collisions
                .iter()
                .map(|c| c.overlap_area)
                .fold(0.0_f32, f32::max);
            let worst_overlap_rect = raw_collisions
                .iter()
                .max_by(|a, b| {
                    a.overlap_area
                        .partial_cmp(&b.overlap_area)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|c| c.overlap_rect);
            PageFitSummary {
                overflow_count,
                collision_count,
                shrunk_count: 0,
                worst_overlap_area,
                worst_overlap_rect,
            }
        };

        Self {
            page_num,
            summary,
            issues,
            overflow_count,
            collision_count,
            image_overlap_count,
            bbox_drift_count: 0,
            worst_severity,
        }
    }
}

fn rect_area(rect: [f32; 4]) -> f32 {
    if rect.iter().all(|v| v.is_finite()) {
        rect[2].max(0.0) * rect[3].max(0.0)
    } else {
        0.0
    }
}

fn rect_intersection(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    if !a.iter().chain(b.iter()).all(|v| v.is_finite()) {
        return None;
    }
    let ax2 = a[0] + a[2];
    let ay2 = a[1] + a[3];
    let bx2 = b[0] + b[2];
    let by2 = b[1] + b[3];
    let x = a[0].max(b[0]);
    let y = a[1].max(b[1]);
    let x2 = ax2.min(bx2);
    let y2 = ay2.min(by2);
    if x2 > x && y2 > y {
        Some([x, y, x2 - x, y2 - y])
    } else {
        None
    }
}

fn bbox_drift_ratio(source: [f32; 4], placed: [f32; 4]) -> f32 {
    if !source.iter().chain(placed.iter()).all(|v| v.is_finite()) {
        return 0.0;
    }
    let source_diag = (source[2].powi(2) + source[3].powi(2)).sqrt().max(1.0);
    let source_cx = source[0] + source[2] / 2.0;
    let source_cy = source[1] + source[3] / 2.0;
    let placed_cx = placed[0] + placed[2] / 2.0;
    let placed_cy = placed[1] + placed[3] / 2.0;
    let dist = ((placed_cx - source_cx).powi(2) + (placed_cy - source_cy).powi(2)).sqrt();
    dist / source_diag
}

/// A matched label/value region pair extracted from a form or table layout.
///
/// Returned by [`extract_label_value_pairs`].  Each entry groups one
/// [`LayoutRegionRole::LeftLabel`] region with all [`LayoutRegionRole::RightValue`]
/// siblings that share the same `row` index.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct LabelValuePair {
    /// The left-column label region (`role == LeftLabel`).
    pub label: LayoutRegion,
    /// Right-column value regions on the same row (`role == RightValue`).
    /// Ordered by column index (left to right).  Typically one entry, but dense
    /// forms occasionally split a single row into multiple value columns.
    pub values: Vec<LayoutRegion>,
}

/// Pair [`LayoutRegionRole::LeftLabel`] regions with their same-row
/// [`LayoutRegionRole::RightValue`] siblings.
///
/// Pass the slice returned by [`extract_layout_regions`].  Regions without a
/// row index, or lone `RightValue` entries with no matching `LeftLabel`, are
/// silently skipped.
///
/// # Example
///
/// ```rust
/// use harumi::{LayoutRegionOptions, TextFragment, extract_layout_regions, extract_label_value_pairs};
///
/// // (Assuming frags extracted from a form PDF)
/// # let frags: Vec<TextFragment> = vec![];
/// let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());
/// let pairs = extract_label_value_pairs(&regions);
/// for pair in &pairs {
///     println!("Label: {}  Value: {}", pair.label.text,
///              pair.values.first().map(|v| v.text.as_str()).unwrap_or("—"));
/// }
/// ```
pub fn extract_label_value_pairs(regions: &[LayoutRegion]) -> Vec<LabelValuePair> {
    use std::collections::BTreeMap;

    // Collect labels and values keyed by row index
    let mut labels: BTreeMap<usize, LayoutRegion> = BTreeMap::new();
    let mut values: BTreeMap<usize, Vec<LayoutRegion>> = BTreeMap::new();

    for region in regions {
        let Some(row) = region.row else { continue };
        match region.role {
            LayoutRegionRole::LeftLabel => {
                labels.insert(row, region.clone());
            }
            LayoutRegionRole::RightValue => {
                values.entry(row).or_default().push(region.clone());
            }
            _ => {}
        }
    }

    // Sort each row's values by column index
    for vals in values.values_mut() {
        vals.sort_by_key(|r| r.col.unwrap_or(usize::MAX));
    }

    // Pair labels with their row's values; rows without a label are already absent
    labels
        .into_iter()
        .map(|(row, label)| {
            let vals = values.remove(&row).unwrap_or_default();
            LabelValuePair {
                label,
                values: vals,
            }
        })
        .collect()
}

/// Functional role of a [`LayoutRegion`] in a translation or editing workflow.
///
/// Assigned by [`extract_layout_regions`] based on column position, row siblings,
/// and proximity to the page edge.  Use [`RegionTextFitOptions::for_role`] to get
/// sensible default fitting policies for each role.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutRegionRole {
    /// Column-0 cell that has a sibling in a higher column on the same row.
    /// Typically a form label; default policy preserves source baseline and width.
    LeftLabel,
    /// Column ≥ 1 cell that has a col-0 sibling on the same row.
    /// Typically a form value; default policy clamps width to the column zone.
    RightValue,
    /// Single-column or `Paragraph`-kind region.
    ParagraphBody,
    /// Region whose `kind` is [`LayoutRegionKind::Heading`].
    SectionHeading,
    /// Region whose source bbox is within 8 % of the page top or bottom.
    HeaderFooter,
    /// Could not be assigned a more specific role.
    Unknown,
}

/// How to anchor replacement text vertically within a layout region.
///
/// Used in [`RegionTextFitOptions`] / [`crate::Document::plan_text_for_regions_with_policy`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaselinePolicy {
    /// Fit into the source glyph bounding box — preserves the original row baseline.
    /// This is the safest choice for dense fixed-layout forms.
    PreserveSourceBaseline,
    /// Fit into the full `usable_rect` height, top-aligned (v1.10 behaviour).
    TopAlignToRegion,
    /// Centre the source-height slot within `usable_rect`.
    CenterInRegion,
}

/// How to determine the available width for replacement text.
///
/// Used in [`RegionTextFitOptions`] / [`crate::Document::plan_text_for_regions_with_policy`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidthPolicy {
    /// Keep `source_bbox.width` — no expansion beyond the source glyphs.
    SourceLineWidth,
    /// Expand to the full `usable_rect.width` (column gap to the next zone or page edge).
    RegionUsableWidth,
    /// Synonym for [`RegionUsableWidth`](WidthPolicy::RegionUsableWidth); alias for clarity.
    ClampToColumn,
    /// Extend to just before the nearest same-row sibling at a higher column index.
    /// Requires a sibling scan at plan time (O(n) per region).
    ClampBeforeNextRegion,
}

/// Per-region fitting policy for [`crate::Document::plan_text_for_regions_with_policy`].
///
/// Construct with `RegionTextFitOptions::default()` or
/// [`RegionTextFitOptions::for_role`] and override fields as needed.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct RegionTextFitOptions {
    /// Vertical placement strategy. Default [`BaselinePolicy::PreserveSourceBaseline`].
    pub baseline: BaselinePolicy,
    /// Horizontal width strategy. Default [`WidthPolicy::SourceLineWidth`].
    pub width: WidthPolicy,
    /// Minimum font size in PDF points. Default `6.0`.
    pub min_font_size: f32,
    /// Maximum number of wrapped lines. `None` = unlimited. Default `None`.
    pub max_lines: Option<usize>,
    /// When `true`, keep the source `x` coordinate; when `false`, align to the column
    /// zone's `x_start`. Default `true`.
    pub preserve_source_x: bool,
}

impl Default for RegionTextFitOptions {
    fn default() -> Self {
        Self {
            baseline: BaselinePolicy::PreserveSourceBaseline,
            width: WidthPolicy::SourceLineWidth,
            min_font_size: 6.0,
            max_lines: None,
            preserve_source_x: true,
        }
    }
}

impl RegionTextFitOptions {
    /// Return sensible defaults for a given [`LayoutRegionRole`].
    ///
    /// Pass `&[]` as the `options` slice to
    /// [`Document::plan_text_for_regions_with_policy`](crate::Document::plan_text_for_regions_with_policy)
    /// to use these role-based defaults automatically for every region.
    pub fn for_role(role: &LayoutRegionRole) -> Self {
        match role {
            LayoutRegionRole::RightValue | LayoutRegionRole::SectionHeading => Self {
                baseline: BaselinePolicy::PreserveSourceBaseline,
                width: WidthPolicy::ClampToColumn,
                preserve_source_x: true,
                ..Self::default()
            },
            LayoutRegionRole::ParagraphBody => Self {
                baseline: BaselinePolicy::TopAlignToRegion,
                width: WidthPolicy::RegionUsableWidth,
                preserve_source_x: false,
                ..Self::default()
            },
            // LeftLabel / HeaderFooter / Unknown → safest defaults (preserve source baseline + width)
            _ => Self::default(),
        }
    }
}

/// Detect layout regions on a page, inferring the usable area for each cell.
///
/// Unlike [`extract_table_cells`], every region carries a `usable_rect` that
/// extends the width to the start of the next column (or the page edge) rather
/// than only to the end of the source glyphs.  This lets downstream translation
/// code call [`crate::Document::fit_text_to_box`] with the full available space
/// instead of fighting the source-text bounding box.
///
/// # Arguments
///
/// * `fragments` — output of [`crate::Document::extract_text_runs`], ideally
///   pre-filtered to the page's visible text.
/// * `page_width` / `page_height` — from [`crate::PageHandle::size`].
/// * `options` — inference knobs; `LayoutRegionOptions::default()` is a good start.
///
/// # Returns
///
/// Regions in reading order (top-to-bottom, left-to-right within each row).
/// Returns an empty `Vec` when `fragments` is empty or `page_width ≤ 0`.
pub fn extract_layout_regions(
    fragments: &[TextFragment],
    page_width: f32,
    page_height: f32,
    options: LayoutRegionOptions,
) -> Vec<LayoutRegion> {
    if fragments.is_empty() || page_width <= 0.0 {
        return vec![];
    }

    // ---- 1. Visible, sorted fragments -----------------------------------------
    let visible: Vec<TextFragment> = fragments
        .iter()
        .filter(|f| !f.invisible && !f.text.trim().is_empty() && f.font_size.is_finite())
        .cloned()
        .collect();
    if visible.is_empty() {
        return vec![];
    }

    // ---- 2. Column detection + usable widths ----------------------------------
    let zones = detect_text_columns(&visible, page_width);
    let col_usable_widths: Vec<f32> = zones
        .iter()
        .enumerate()
        .map(|(i, z)| {
            let right = if i + 1 < zones.len() {
                zones[i + 1].x_start
            } else {
                page_width
            };
            (right - z.x_start - options.margin).max(1.0)
        })
        .collect();

    // ---- 3. Table cell detection ----------------------------------------------
    let cells = extract_table_cells(&visible, page_width, page_height);
    if cells.is_empty() {
        return vec![];
    }

    // ---- 4. Median font size for heading classification ----------------------
    let mut font_sizes: Vec<f32> = visible
        .iter()
        .map(|f| f.font_size)
        .filter(|&fs| (4.0_f32..=48.0).contains(&fs))
        .collect();
    font_sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_fs = if font_sizes.is_empty() {
        10.0_f32
    } else {
        font_sizes[font_sizes.len() / 2]
    };

    // ---- 5. Row-top map (row_idx → max ascender y in that row) ---------------
    let mut row_top_map: std::collections::BTreeMap<usize, f32> = std::collections::BTreeMap::new();
    for cell in &cells {
        let top = cell
            .fragments
            .iter()
            .filter(|f| f.font_size.is_finite())
            .map(|f| f.y + f.font_size * 0.75)
            .fold(f32::NEG_INFINITY, f32::max);
        if top.is_finite() {
            let entry = row_top_map.entry(cell.row).or_insert(top);
            if top > *entry {
                *entry = top;
            }
        }
    }

    // ---- 5b. Row → column-set map for role detection -------------------------
    let mut row_cols: std::collections::BTreeMap<usize, std::collections::BTreeSet<usize>> =
        std::collections::BTreeMap::new();
    for cell in &cells {
        row_cols.entry(cell.row).or_default().insert(cell.col);
    }

    // ---- 6. Build LayoutRegion per cell --------------------------------------
    let mut regions: Vec<LayoutRegion> = Vec::with_capacity(cells.len());

    for cell in cells {
        let source_bbox = text_fragment_bounds(&cell.fragments).unwrap_or(cell.bbox());

        // --- Horizontal (usable_x, usable_w) ---
        let (usable_x, usable_w) =
            if options.infer_column_widths && cell.col < col_usable_widths.len() {
                (zones[cell.col].x_start, col_usable_widths[cell.col])
            } else {
                (source_bbox[0], source_bbox[2])
            };

        // --- Vertical (usable_y, usable_h) ---
        let (usable_y, usable_h) = if options.infer_row_heights {
            let current_top = row_top_map
                .get(&cell.row)
                .copied()
                .filter(|v| v.is_finite())
                .unwrap_or(source_bbox[1] + source_bbox[3]);
            // Use checked_add to avoid usize overflow when cell.row == usize::MAX.
            let next_top = cell
                .row
                .checked_add(1)
                .and_then(|r| row_top_map.get(&r))
                .copied();
            if let Some(next_top) = next_top {
                let h = (current_top - next_top).max(source_bbox[3]);
                (next_top, h)
            } else {
                // Last row: estimate height = 1.5× source height, floor below source
                let h = (source_bbox[3] * 1.5).max(source_bbox[3]);
                let y = current_top - h;
                (y.max(options.margin), h)
            }
        } else {
            (source_bbox[1], source_bbox[3])
        };

        // --- Kind classification ---
        let avg_fs = {
            let sizes: Vec<f32> = cell
                .fragments
                .iter()
                .map(|f| f.font_size)
                .filter(|fs| fs.is_finite() && *fs > 0.0)
                .collect();
            if sizes.is_empty() {
                median_fs
            } else {
                sizes.iter().sum::<f32>() / sizes.len() as f32
            }
        };
        let ratio = if median_fs > 0.0 {
            avg_fs / median_fs
        } else {
            1.0
        };
        let is_bold = cell.fragments.iter().any(|f| f.is_bold);
        let kind = if ratio >= 1.8 || (ratio >= 1.5 && is_bold) {
            LayoutRegionKind::Heading(1)
        } else if ratio >= 1.5 {
            LayoutRegionKind::Heading(2)
        } else if ratio >= 1.3 {
            LayoutRegionKind::Heading(3)
        } else if ratio >= 1.15 || (ratio >= 1.05 && is_bold) {
            LayoutRegionKind::Heading(4)
        } else if zones.len() <= 1 && cell.col == 0 {
            // Single column without tabular siblings → paragraph
            LayoutRegionKind::Paragraph
        } else {
            LayoutRegionKind::TableCell
        };

        // --- Role classification ---
        let role = match &kind {
            LayoutRegionKind::Heading(_) => LayoutRegionRole::SectionHeading,
            _ => {
                let top_y = source_bbox[1] + source_bbox[3];
                let bot_y = source_bbox[1];
                if page_height > 0.0
                    && top_y.is_finite()
                    && bot_y.is_finite()
                    && (top_y > page_height * 0.92 || bot_y < page_height * 0.08)
                {
                    LayoutRegionRole::HeaderFooter
                } else if let Some(cols) = row_cols.get(&cell.row) {
                    let has_higher = cols.iter().any(|&c| c > cell.col);
                    let has_lower = cols.iter().any(|&c| c < cell.col);
                    if cell.col == 0 && has_higher {
                        LayoutRegionRole::LeftLabel
                    } else if cell.col > 0 && has_lower {
                        LayoutRegionRole::RightValue
                    } else if matches!(kind, LayoutRegionKind::Paragraph) {
                        LayoutRegionRole::ParagraphBody
                    } else {
                        LayoutRegionRole::Unknown
                    }
                } else if matches!(kind, LayoutRegionKind::Paragraph) {
                    LayoutRegionRole::ParagraphBody
                } else {
                    LayoutRegionRole::Unknown
                }
            }
        };

        regions.push(LayoutRegion {
            kind,
            role,
            row: Some(cell.row),
            col: Some(cell.col),
            text: cell.text,
            source_bbox,
            usable_rect: [usable_x, usable_y, usable_w, usable_h],
            fragments: cell.fragments,
        });
    }

    // Sort by (row asc, col asc) — stable reading order
    regions.sort_by_key(|r| (r.row.unwrap_or(usize::MAX), r.col.unwrap_or(usize::MAX)));
    regions
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;
