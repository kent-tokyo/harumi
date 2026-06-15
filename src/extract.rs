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

    // Detect column zones (reuse existing X-gap algorithm).
    let col_zones = detect_text_columns(fragments, page_width);
    if col_zones.is_empty() {
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

    // Row-grouping threshold: half the first (topmost) fragment's font size, at
    // least 2 pt.  Using the topmost fragment's size avoids inflating the
    // threshold with large headings that appear later.
    let row_tol = {
        let first_fs = sorted.iter()
            .find(|f| f.font_size.is_finite() && f.font_size > 0.0)
            .map(|f| f.font_size)
            .unwrap_or(12.0);
        (first_fs * 0.5).max(2.0)
    };

    // Group fragments into rows by Y proximity.
    let mut rows: Vec<Vec<&TextFragment>> = Vec::new();
    for frag in &sorted {
        let in_current_row = rows.last().map(|r| {
            let row_y = r[0].y; // topmost y of this row
            (row_y - frag.y).abs() <= row_tol
        });
        if in_current_row == Some(true) {
            rows.last_mut().unwrap().push(frag);
        } else {
            rows.push(vec![frag]);
        }
    }

    // Helper: map an x coordinate to a column index.
    let col_for_x = |x: f32| -> usize {
        for (i, zone) in col_zones.iter().enumerate() {
            if x >= zone.x_start && x < zone.x_end {
                return i;
            }
        }
        // Outside all zones — assign to nearest zone.
        col_zones
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let da = (x - (a.x_start + a.x_end) * 0.5).abs();
                let db = (x - (b.x_start + b.x_end) * 0.5).abs();
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap_or(0)
    };

    // Collect fragments per (row, col) cell.
    let mut cell_map: std::collections::BTreeMap<(usize, usize), Vec<&TextFragment>> =
        std::collections::BTreeMap::new();
    for (row_idx, row_frags) in rows.iter().enumerate() {
        for frag in row_frags {
            let col_idx = col_for_x(frag.x);
            cell_map.entry((row_idx, col_idx)).or_default().push(frag);
        }
    }

    // Build TableCell for each occupied (row, col).
    cell_map
        .into_iter()
        .map(|((row, col), mut frags)| {
            // Within a cell, sort left-to-right.
            frags.sort_by(|a, b| {
                a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal)
            });
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
            TableCell {
                row,
                col,
                text,
                x,
                y,
                width: (right - x).max(0.0),
                height,
            }
        })
        .collect()
    // BTreeMap iteration is already sorted by (row, col), so no extra sort needed.
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
    for stream_bytes in &streams {
        parse_content_stream(stream_bytes, &fonts, &mut carry, &mut fragments);
    }
    // Also extract text from Form XObjects (headers, footers, watermarks).
    extract_text_from_xobjects(doc, page_id, &mut carry, &mut fragments, 0);
    Ok(fragments)
}

/// Recursively extract text from Form XObjects referenced in the page resources.
///
/// `depth` guards against infinite recursion (limit: 5 levels).  Coordinate
/// transformation via the XObject `/Matrix` is not applied; text coordinates
/// are emitted in the XObject's own coordinate system, which for most
/// header/footer XObjects matches the parent page coordinates.
fn extract_text_from_xobjects(
    doc: &lopdf::Document,
    page_id: ObjectId,
    carry: &mut ParseCarryState,
    out: &mut Vec<TextFragment>,
    depth: u8,
) {
    if depth > 5 {
        return;
    }
    // Walk up /Parent chain to find /Resources/XObject (PDF §7.7.3 inheritance).
    // Chrome/Skia PDFs commonly place /Resources on an ancestor /Pages node rather
    // than on each page dict, so a direct page_dict.get(b"Resources") would miss them.
    let xobj_ids = collect_inherited_xobject_ids(doc, page_id);

    for xobj_id in xobj_ids {
        let Ok(xobj_obj) = doc.get_object(xobj_id) else { continue };
        let Ok(xobj_stream) = xobj_obj.as_stream() else { continue };

        let is_form = xobj_stream.dict.get(b"Subtype").ok()
            .and_then(|o| if let Object::Name(n) = o { Some(n.as_slice()) } else { None })
            == Some(b"Form");
        if !is_form {
            continue;
        }

        let content = if xobj_stream.dict.get(b"Filter").is_ok() {
            let mut owned = xobj_stream.clone();
            if owned.decompress().is_err() {
                continue;
            }
            owned.content
        } else {
            xobj_stream.content.clone()
        };

        // Use the XObject's own resource fonts, falling back to page fonts.
        let xobj_fonts = xobj_stream.dict.get(b"Resources").ok()
            .and_then(|res_ref| resolve_dict(doc, res_ref))
            .map(|res_dict| collect_fonts_from_resources(doc, res_dict))
            .unwrap_or_else(|| collect_fonts(doc, page_id));

        parse_content_stream(&content, &xobj_fonts, carry, out);
    }
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
            }
        } else {
            result.push(stream.content.clone());
        }
    }
    result
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
            Some(b"Type1") | Some(b"MMType1") | Some(b"TrueType") => {
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

fn tokenize(input: &[u8]) -> Vec<Token> {
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
            tokens.push(Token::HexStr(decode_hex_bytes(hex)));
            continue;
        }
        if b == b'/' {
            i += 1;
            let start = i;
            while i < input.len() && !is_pdf_whitespace(input[i]) && !is_pdf_delimiter(input[i]) {
                i += 1;
            }
            tokens.push(Token::Name(input[start..i].to_vec()));
            continue;
        }
        if b == b'[' {
            i += 1;
            let (arr, consumed) = parse_array_tokens(&input[i..]);
            i += consumed;
            tokens.push(Token::Array(arr));
            continue;
        }
        if b == b']' {
            i += 1;
            continue;
        }
        if b == b'(' {
            let (bytes, end_i) = parse_literal_string(input, i + 1);
            i = end_i;
            tokens.push(Token::LitStr(bytes));
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
            tokens.push(Token::Number(n));
            continue;
        }
        tokens.push(Token::Keyword(word.to_vec()));
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

/// Graphics state carried across multiple content streams on the same page.
///
/// Per the PDF spec, the graphics state (colour, render mode, etc.) persists
/// across streams when a page `/Contents` is an array of streams.  This struct
/// captures the subset of state that affects text extraction.
struct ParseCarryState {
    cur_color: [f32; 3],
    cur_render_mode: u8,
}

impl Default for ParseCarryState {
    fn default() -> Self {
        Self { cur_color: [0.0, 0.0, 0.0], cur_render_mode: 0 }
    }
}

fn parse_content_stream(
    bytes: &[u8],
    fonts: &HashMap<Vec<u8>, FontInfo>,
    state: &mut ParseCarryState,
    out: &mut Vec<TextFragment>,
) {
    let tokens = tokenize(bytes);
    let mut stack: Vec<Token> = Vec::new();
    let mut in_bt = false;
    let mut font_name: Vec<u8> = Vec::new();
    let mut tf_font_size: f32 = 12.0; // raw size from Tf operator
    let mut font_size: f32 = 12.0;    // effective = tf_font_size × Tm y-scale
    let mut x: f32 = 0.0;
    let mut y: f32 = 0.0;

    for token in tokens {
        match token {
            Token::Keyword(kw) => match kw.as_slice() {
                b"BT" => {
                    in_bt = true;
                    x = 0.0;
                    y = 0.0;
                    stack.clear();
                }
                b"ET" => {
                    in_bt = false;
                    stack.clear();
                }
                b"Tf" if in_bt => {
                    let top = stack.pop();
                    let second = stack.pop();
                    if let (Some(Token::Number(size)), Some(Token::Name(name))) = (top, second) {
                        font_name = name;
                        tf_font_size = size;
                        font_size = size;
                    }
                    stack.clear();
                }
                b"Td" | b"TD" if in_bt => {
                    let top = stack.pop();
                    let second = stack.pop();
                    if let (Some(Token::Number(ty)), Some(Token::Number(tx))) = (top, second) {
                        x += tx;
                        y += ty;
                    }
                    stack.clear();
                }
                b"Tm" if in_bt => {
                    // Tm: a b c d e f Tm (stack top = f)
                    let pop_f = stack.pop(); // f = y translation
                    let pop_e = stack.pop(); // e = x translation
                    let pop_d = stack.pop(); // d = y-axis component of scale/rotation
                    let pop_c = stack.pop(); // c = y-axis component of skew/rotation
                    stack.pop(); // b
                    stack.pop(); // a
                    if let (Some(Token::Number(fy)), Some(Token::Number(ex))) = (pop_f, pop_e) {
                        x = ex;
                        y = fy;
                    }
                    // Compute effective font size from the Tm y-scale:
                    // y_scale = sqrt(c² + d²) handles both scaling and rotation.
                    if let (Some(Token::Number(dv)), Some(Token::Number(cv))) = (pop_d, pop_c) {
                        let y_scale = (cv * cv + dv * dv).sqrt();
                        if y_scale > 0.0 {
                            font_size = tf_font_size * y_scale;
                        }
                    }
                    stack.clear();
                }
                b"Tr" => {
                    if let Some(Token::Number(mode)) = stack.pop() {
                        state.cur_render_mode = mode as u8;
                    }
                    stack.clear();
                }
                b"rg" => {
                    let b_val = stack.pop();
                    let g_val = stack.pop();
                    let r_val = stack.pop();
                    if let (
                        Some(Token::Number(bv)),
                        Some(Token::Number(gv)),
                        Some(Token::Number(rv)),
                    ) = (b_val, g_val, r_val)
                    {
                        state.cur_color = [rv, gv, bv];
                    }
                    stack.clear();
                }
                b"g" => {
                    if let Some(Token::Number(gray)) = stack.pop() {
                        state.cur_color = [gray, gray, gray];
                    }
                    stack.clear();
                }
                b"Tj" if in_bt => {
                    let bytes_opt = match stack.pop() {
                        Some(Token::HexStr(b)) => Some(b),
                        Some(Token::LitStr(b)) => Some(b),
                        _ => None,
                    };
                    if let Some(char_bytes) = bytes_opt
                        && let Some(frag) = decode_chars_to_fragment(
                            &char_bytes,
                            &font_name,
                            font_size,
                            x,
                            y,
                            fonts,
                            state.cur_color,
                            state.cur_render_mode,
                        )
                    {
                        x += frag.width;
                        out.push(frag);
                    }
                    stack.clear();
                }
                b"TJ" if in_bt => {
                    if let Some(Token::Array(items)) = stack.pop() {
                        let mut cur_x = x;
                        for item in items {
                            match item {
                                Token::HexStr(ref b) | Token::LitStr(ref b) => {
                                    if let Some(frag) = decode_chars_to_fragment(
                                        b,
                                        &font_name,
                                        font_size,
                                        cur_x,
                                        y,
                                        fonts,
                                        state.cur_color,
                                        state.cur_render_mode,
                                    ) {
                                        cur_x += frag.width;
                                        out.push(frag);
                                    }
                                }
                                Token::Number(kern) => {
                                    cur_x -= kern / 1000.0 * font_size;
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
                stack.push(other);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)] // All args are logically required; a ctx struct would add ceremony
fn decode_chars_to_fragment(
    char_bytes: &[u8],
    font_name: &[u8],
    font_size: f32,
    x: f32,
    y: f32,
    fonts: &HashMap<Vec<u8>, FontInfo>,
    color: [f32; 3],
    render_mode: u8,
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
                total_width += aw as f32 / 1000.0 * font_size;
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
                total_width += aw as f32 / 1000.0 * font_size;
            }
        }
    }

    if text.is_empty() {
        return None;
    }
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
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Object;

    #[test]
    fn parse_to_unicode_cmap_basic() {
        let cmap = b"/CIDInit /ProcSet findresource begin\n\
                     12 dict begin\n\
                     begincmap\n\
                     1 beginbfchar\n\
                     <0001> <65E5>\n\
                     endbfchar\n\
                     endcmap\n\
                     end\nend\n";
        let map = parse_to_unicode_cmap(cmap);
        assert_eq!(map.get(&1u16), Some(&'日'));
    }

    #[test]
    fn parse_to_unicode_cmap_surrogate() {
        let cmap = b"1 beginbfchar\n<0001> <D840DC00>\nendbfchar\n";
        let map = parse_to_unicode_cmap(cmap);
        assert_eq!(map.get(&1u16), Some(&'\u{20000}'));
    }

    #[test]
    fn parse_bfrange_contiguous() {
        let cmap = b"1 beginbfrange\n<20> <7E> <0020>\nendbfrange\n";
        let map = parse_to_unicode_cmap(cmap);
        assert_eq!(map.get(&0x20), Some(&' '));
        assert_eq!(map.get(&0x41), Some(&'A'));
        assert_eq!(map.get(&0x7E), Some(&'~'));
    }

    #[test]
    fn parse_bfrange_explicit_array() {
        let cmap = b"1 beginbfrange\n<20> <21> [<0048> <0069>]\nendbfrange\n";
        let map = parse_to_unicode_cmap(cmap);
        assert_eq!(map.get(&0x20), Some(&'H'));
        assert_eq!(map.get(&0x21), Some(&'i'));
    }

    #[test]
    fn decode_hex_bytes_roundtrip() {
        let hex = b"00010002";
        let bytes = decode_hex_bytes(hex);
        assert_eq!(bytes, vec![0x00, 0x01, 0x00, 0x02]);
    }

    #[test]
    fn litstr_tokenizer_basic() {
        let stream = b"(Hello)";
        let tokens = tokenize(stream);
        assert!(matches!(&tokens[0], Token::LitStr(b) if b == b"Hello"));
    }

    #[test]
    fn litstr_escapes() {
        let stream = b"(He\\nllo\\041)"; // \n and \041 = '!'
        let tokens = tokenize(stream);
        match &tokens[0] {
            Token::LitStr(b) => {
                assert_eq!(b[0], b'H');
                assert_eq!(b[1], b'e');
                assert_eq!(b[2], b'\n');
                assert_eq!(b[3], b'l');
                assert_eq!(b[6], b'!');
            }
            _ => panic!("expected LitStr"),
        }
    }

    #[test]
    fn litstr_in_array() {
        let stream = b"[(Hel) -50 (lo)]";
        let tokens = tokenize(stream);
        if let Token::Array(items) = &tokens[0] {
            assert!(matches!(&items[0], Token::LitStr(b) if b == b"Hel"));
            assert!(matches!(&items[1], Token::Number(n) if (*n + 50.0).abs() < 0.1));
            assert!(matches!(&items[2], Token::LitStr(b) if b == b"lo"));
        } else {
            panic!("expected Array");
        }
    }

    #[test]
    fn tokenizer_smoke() {
        let stream = b"BT\n/F0 12 Tf\n100 200 Td\n<0001> Tj\nET\n";
        let tokens = tokenize(stream);
        let keywords: Vec<&[u8]> = tokens
            .iter()
            .filter_map(|t| {
                if let Token::Keyword(k) = t {
                    Some(k.as_slice())
                } else {
                    None
                }
            })
            .collect();
        assert!(keywords.contains(&b"BT".as_slice()));
        assert!(keywords.contains(&b"Tf".as_slice()));
        assert!(keywords.contains(&b"Td".as_slice()));
        assert!(keywords.contains(&b"Tj".as_slice()));
        assert!(keywords.contains(&b"ET".as_slice()));
    }

    #[test]
    fn parse_w_array_run_format() {
        let arr = vec![
            Object::Integer(0),
            Object::Array(vec![
                Object::Integer(500),
                Object::Integer(600),
                Object::Integer(700),
            ]),
        ];
        let runs = parse_w_array(&arr);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].start_gid, 0);
        assert_eq!(runs[0].widths, vec![500, 600, 700]);
    }

    #[test]
    fn font_info_advance_width_fallback() {
        let info = FontInfo {
            to_unicode: BTreeMap::new(),
            dw: 1000,
            w_runs: vec![WidthRun {
                start_gid: 5,
                widths: vec![600],
            }],
            bytes_per_char: 2,
            identity_fallback: false,
            base_font: String::new(),
            is_bold: false,
            is_italic: false,
            font_family: String::new(),
        };
        assert_eq!(info.advance_width(5), 600);
        assert_eq!(info.advance_width(0), 1000);
        assert_eq!(info.advance_width(99), 1000);
    }

    #[test]
    fn win_ansi_spot_checks() {
        assert_eq!(WIN_ANSI_ENCODING[0x20], Some(' '));
        assert_eq!(WIN_ANSI_ENCODING[0x41], Some('A'));
        assert_eq!(WIN_ANSI_ENCODING[0x80], Some('€'));
        assert_eq!(WIN_ANSI_ENCODING[0xE9], Some('é'));
        assert_eq!(WIN_ANSI_ENCODING[0x7F], None);
    }

    #[test]
    fn agl_table_sorted() {
        for i in 1..AGL_TABLE.len() {
            assert!(
                AGL_TABLE[i - 1].0 < AGL_TABLE[i].0,
                "AGL_TABLE not sorted at index {i}: {:?} >= {:?}",
                AGL_TABLE[i - 1].0,
                AGL_TABLE[i].0
            );
        }
    }

    #[test]
    fn glyph_name_lookup_spot_checks() {
        assert_eq!(glyph_name_to_char(b"space"), Some(' '));
        assert_eq!(glyph_name_to_char(b"eacute"), Some('é'));
        assert_eq!(glyph_name_to_char(b"euro"), Some('€'));
        assert_eq!(glyph_name_to_char(b"Euro"), Some('€'));
        assert_eq!(glyph_name_to_char(b"fi"), Some('\u{FB01}'));
        assert_eq!(glyph_name_to_char(b"nonexistent"), None);
    }

    #[test]
    fn encoding_table_to_btree_basic() {
        let map = encoding_table_to_btree(&WIN_ANSI_ENCODING);
        assert_eq!(map.get(&0x41), Some(&'A'));
        assert_eq!(map.get(&0x80), Some(&'€'));
        assert!(!map.contains_key(&0x7F)); // undefined slot not included
    }

    #[test]
    fn parse_font_attributes_cases() {
        // Plain family
        let (name, bold, italic, family) = parse_font_attributes("Helvetica");
        assert_eq!(name, "Helvetica");
        assert!(!bold);
        assert!(!italic);
        assert_eq!(family, "Helvetica");

        // Bold + subset prefix
        let (name, bold, italic, family) = parse_font_attributes("ABCDEF+Helvetica-Bold");
        assert_eq!(name, "Helvetica-Bold");
        assert!(bold);
        assert!(!italic);
        assert_eq!(family, "Helvetica");

        // BoldItalic
        let (name, bold, italic, family) = parse_font_attributes("TimesNewRoman-BoldItalic");
        assert_eq!(name, "TimesNewRoman-BoldItalic");
        assert!(bold);
        assert!(italic);
        assert_eq!(family, "TimesNewRoman");

        // Oblique style
        let (_name, bold, italic, _family) = parse_font_attributes("Arial-Oblique");
        assert!(!bold);
        assert!(italic);

        // Heavy weight
        let (_name, bold, _italic, _family) = parse_font_attributes("Futura-Heavy");
        assert!(bold);
    }

    #[test]
    fn detect_text_columns_single() {
        // Single-column page: all text on the left half
        let frags = vec![TextFragment {
            text: "Hello".into(),
            x: 50.0,
            y: 700.0,
            width: 100.0,
            height: 12.0,
            font_size: 12.0,
            font_name: "F1".into(),
            color: [0.0; 3],
            invisible: false,
            is_bold: false,
            is_italic: false,
            font_family: String::new(),
            base_font: String::new(),
        }];
        let zones = detect_text_columns(&frags, 595.0);
        assert_eq!(zones.len(), 1);

        // Empty input → empty
        assert!(detect_text_columns(&[], 595.0).is_empty());
    }

    #[test]
    fn detect_text_columns_two_columns() {
        // Two fragments with a 100pt gap between them
        let left = TextFragment {
            text: "Left".into(),
            x: 50.0,
            y: 700.0,
            width: 150.0,
            height: 12.0,
            font_size: 12.0,
            font_name: "F1".into(),
            color: [0.0; 3],
            invisible: false,
            is_bold: false,
            is_italic: false,
            font_family: String::new(),
            base_font: String::new(),
        };
        let right = TextFragment {
            text: "Right".into(),
            x: 350.0,
            y: 700.0,
            width: 150.0,
            height: 12.0,
            font_size: 12.0,
            font_name: "F1".into(),
            color: [0.0; 3],
            invisible: false,
            is_bold: false,
            is_italic: false,
            font_family: String::new(),
            base_font: String::new(),
        };
        let zones = detect_text_columns(&[left, right], 595.0);
        assert_eq!(zones.len(), 2, "expected two columns, got {:?}", zones);
        assert!(zones[0].x_start < zones[1].x_start);
    }

    fn make_frag(text: &str, x: f32, y: f32, w: f32, fs: f32) -> TextFragment {
        TextFragment {
            text: text.into(),
            x,
            y,
            width: w,
            height: fs,
            font_size: fs,
            font_name: "F1".into(),
            color: [0.0; 3],
            invisible: false,
            is_bold: false,
            is_italic: false,
            font_family: String::new(),
            base_font: String::new(),
        }
    }

    #[test]
    fn extract_table_cells_single_column() {
        // Three rows, one column.
        let frags = vec![
            make_frag("Header", 50.0, 700.0, 80.0, 12.0),
            make_frag("Row 1",  50.0, 680.0, 60.0, 12.0),
            make_frag("Row 2",  50.0, 660.0, 60.0, 12.0),
        ];
        let cells = extract_table_cells(&frags, 595.0, 842.0);
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].row, 0); assert_eq!(cells[0].col, 0);
        assert_eq!(cells[1].row, 1);
        assert_eq!(cells[2].row, 2);
        assert_eq!(cells[0].text, "Header");
    }

    #[test]
    fn extract_table_cells_two_columns() {
        // Two rows × two columns (100 pt gap between columns).
        let frags = vec![
            make_frag("A1", 50.0,  700.0, 80.0, 12.0),
            make_frag("B1", 300.0, 700.0, 80.0, 12.0),
            make_frag("A2", 50.0,  680.0, 80.0, 12.0),
            make_frag("B2", 300.0, 680.0, 80.0, 12.0),
        ];
        let cells = extract_table_cells(&frags, 595.0, 842.0);
        assert_eq!(cells.len(), 4);
        // Row 0, col 0 should be "A1"
        let a1 = cells.iter().find(|c| c.row == 0 && c.col == 0).unwrap();
        assert_eq!(a1.text, "A1");
        // Row 0, col 1 should be "B1"
        let b1 = cells.iter().find(|c| c.row == 0 && c.col == 1).unwrap();
        assert_eq!(b1.text, "B1");
    }

    #[test]
    fn extract_table_cells_merges_same_cell_fragments() {
        // Two fragments on the same line, same column → merged into one cell.
        let frags = vec![
            make_frag("Hello", 50.0,  700.0, 30.0, 12.0),
            make_frag("World", 85.0,  700.0, 30.0, 12.0),
        ];
        let cells = extract_table_cells(&frags, 595.0, 842.0);
        assert_eq!(cells.len(), 1);
        assert!(cells[0].text.contains("Hello"));
        assert!(cells[0].text.contains("World"));
    }

    #[test]
    fn extract_table_cells_empty_returns_empty() {
        assert!(extract_table_cells(&[], 595.0, 842.0).is_empty());
        assert!(extract_table_cells(&[], 0.0, 842.0).is_empty());
    }

    #[test]
    fn group_text_fragments_raw() {
        let frags = vec![
            make_frag("A", 50.0, 700.0, 20.0, 12.0),
            make_frag("B", 80.0, 700.0, 20.0, 12.0),
        ];
        let groups = group_text_fragments(&frags, GroupingStrategy::Raw);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn group_text_fragments_line() {
        let frags = vec![
            make_frag("A", 50.0,  700.0, 20.0, 12.0),
            make_frag("B", 80.0,  700.0, 20.0, 12.0), // same line
            make_frag("C", 50.0,  680.0, 20.0, 12.0), // new line (gap > 6pt)
        ];
        let groups = group_text_fragments(&frags, GroupingStrategy::Line);
        assert_eq!(groups.len(), 2, "expected 2 lines, got {}", groups.len());
        assert!(groups[0].text.contains('A') && groups[0].text.contains('B'));
    }

    #[test]
    fn group_text_fragments_paragraph() {
        // Three lines: first two close together (same paragraph), third far below.
        let frags = vec![
            make_frag("L1", 50.0, 700.0, 20.0, 12.0),
            make_frag("L2", 50.0, 686.0, 20.0, 12.0), // gap=14, < 1.5×12=18 → same paragraph
            make_frag("L3", 50.0, 630.0, 20.0, 12.0), // gap=56, > 18 → new paragraph
        ];
        let groups = group_text_fragments(&frags, GroupingStrategy::Paragraph);
        assert_eq!(groups.len(), 2, "expected 2 paragraphs, got {}", groups.len());
        assert!(groups[0].text.contains("L1") && groups[0].text.contains("L2"));
        assert!(groups[1].text.contains("L3"));
    }

    // Chrome/Skia PDFs put /Resources on a parent /Pages node rather than on each
    // page dict.  extract_text_from_xobjects() must walk up the /Parent chain to find
    // /Resources/XObject; without the fix it returns early and produces zero fragments.
    #[test]
    fn extract_xobjects_from_inherited_resources() {
        use lopdf::{Document, Stream};

        let mut doc = Document::new();

        // Type1 font with no explicit /Encoding → StandardEncoding fallback.
        // ASCII bytes H(72) e(101) l(108) l(108) o(111) map to "Hello".
        let mut font_d = Dictionary::new();
        font_d.set("Type", Object::Name(b"Font".to_vec()));
        font_d.set("Subtype", Object::Name(b"Type1".to_vec()));
        font_d.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
        let font_id = doc.add_object(Object::Dictionary(font_d));

        // Form XObject with its own /Resources/Font and a text content stream.
        let mut xobj_font_d = Dictionary::new();
        xobj_font_d.set("F1", Object::Reference(font_id));
        let mut xobj_res = Dictionary::new();
        xobj_res.set("Font", Object::Dictionary(xobj_font_d));
        let mut xobj_d = Dictionary::new();
        xobj_d.set("Type", Object::Name(b"XObject".to_vec()));
        xobj_d.set("Subtype", Object::Name(b"Form".to_vec()));
        xobj_d.set(
            "BBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(595),
                Object::Integer(842),
            ]),
        );
        xobj_d.set("Resources", Object::Dictionary(xobj_res));
        let xobj_id = doc.add_object(Object::Stream(Stream::new(
            xobj_d,
            b"BT /F1 12 Tf (Hello) Tj ET".to_vec(),
        )));

        // Minimal page content stream — text lives in the XObject, not here.
        let content_id = doc.add_object(Object::Stream(Stream::new(
            Dictionary::new(),
            b"q Q".to_vec(),
        )));

        // Page node with NO /Resources — Chrome/Skia style (inherits from Pages).
        let mut page_d = Dictionary::new();
        page_d.set("Type", Object::Name(b"Page".to_vec()));
        page_d.set(
            "MediaBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(595),
                Object::Integer(842),
            ]),
        );
        page_d.set("Contents", Object::Reference(content_id));
        let page_id = doc.add_object(Object::Dictionary(page_d));

        // Pages node: /Resources/XObject here (NOT on the page dict).
        let mut xobj_dict = Dictionary::new();
        xobj_dict.set("X1", Object::Reference(xobj_id));
        let mut pages_res = Dictionary::new();
        pages_res.set("XObject", Object::Dictionary(xobj_dict));
        let mut pages_d = Dictionary::new();
        pages_d.set("Type", Object::Name(b"Pages".to_vec()));
        pages_d.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages_d.set("Count", Object::Integer(1));
        pages_d.set("Resources", Object::Dictionary(pages_res));
        let pages_id = doc.add_object(Object::Dictionary(pages_d));

        // Wire up /Parent.
        if let Ok(obj) = doc.get_object_mut(page_id) {
            if let Ok(d) = obj.as_dict_mut() {
                d.set("Parent", Object::Reference(pages_id));
            }
        }

        // Catalog.
        let mut catalog = Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", Object::Reference(pages_id));
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
        let text: String = frags.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join("");
        assert!(
            !frags.is_empty(),
            "expected text from XObject with inherited /Resources, got none"
        );
        assert!(
            text.contains("Hello"),
            "expected 'Hello' in extracted text, got: {text:?}"
        );
    }

    // Validates the *real* Chrome/Skia decode path: Type0/CID font with Identity-H
    // encoding, ToUnicode CMap, and 2-byte hex glyph IDs (<XXXX> Tj), all inside a
    // Form XObject discovered via an inherited /Resources on the parent /Pages node.
    //
    // This is the path that matters for P1 (InPlace replace_text on Chrome/Skia PDFs).
    // The previous test used Type1/literal strings — a completely different decode branch.
    #[test]
    fn extract_cid_xobject_inherited_resources() {
        use lopdf::{Document, Stream};

        // GID→Unicode mapping used in this test:
        //   GID 0x0048 → 'H',  GID 0x0069 → 'i'
        // Content stream will be <00480069> Tj  (2 CIDs, 4 hex bytes).
        let cmap_bytes = b"/CIDInit /ProcSet findresource begin\n\
             12 dict begin\n\
             begincmap\n\
             /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> def\n\
             /CMapName /Adobe-Identity-H def\n\
             /CMapType 1 def\n\
             2 beginbfchar\n\
             <0048> <0048>\n\
             <0069> <0069>\n\
             endbfchar\n\
             endcmap\n\
             end end\n"
            .to_vec();

        let mut doc = Document::new();

        // ToUnicode CMap stream.
        let cmap_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), cmap_bytes)));

        // CIDFontType2 (descendant font).
        let mut cidfont_d = Dictionary::new();
        cidfont_d.set("Type", Object::Name(b"Font".to_vec()));
        cidfont_d.set("Subtype", Object::Name(b"CIDFontType2".to_vec()));
        cidfont_d.set("BaseFont", Object::Name(b"TestCIDFont".to_vec()));
        {
            let mut cidsys = Dictionary::new();
            cidsys.set("Registry", Object::String(b"Adobe".to_vec(), lopdf::StringFormat::Literal));
            cidsys.set("Ordering", Object::String(b"Identity".to_vec(), lopdf::StringFormat::Literal));
            cidsys.set("Supplement", Object::Integer(0));
            cidfont_d.set("CIDSystemInfo", Object::Dictionary(cidsys));
        }
        cidfont_d.set("DW", Object::Integer(1000));
        let cidfont_id = doc.add_object(Object::Dictionary(cidfont_d));

        // Type0 font dict.
        let mut font_d = Dictionary::new();
        font_d.set("Type", Object::Name(b"Font".to_vec()));
        font_d.set("Subtype", Object::Name(b"Type0".to_vec()));
        font_d.set("BaseFont", Object::Name(b"TestCIDFont".to_vec()));
        font_d.set("Encoding", Object::Name(b"Identity-H".to_vec()));
        font_d.set("DescendantFonts", Object::Array(vec![Object::Reference(cidfont_id)]));
        font_d.set("ToUnicode", Object::Reference(cmap_id));
        let font_id = doc.add_object(Object::Dictionary(font_d));

        // Form XObject: /Resources/Font has F1, content stream uses 2-byte CID hex.
        // <00480069> encodes GID 0x0048 ('H') and GID 0x0069 ('i').
        let mut xobj_font_d = Dictionary::new();
        xobj_font_d.set("F1", Object::Reference(font_id));
        let mut xobj_res = Dictionary::new();
        xobj_res.set("Font", Object::Dictionary(xobj_font_d));
        let mut xobj_d = Dictionary::new();
        xobj_d.set("Type", Object::Name(b"XObject".to_vec()));
        xobj_d.set("Subtype", Object::Name(b"Form".to_vec()));
        xobj_d.set(
            "BBox",
            Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(595), Object::Integer(842),
            ]),
        );
        xobj_d.set("Resources", Object::Dictionary(xobj_res));
        let xobj_id = doc.add_object(Object::Stream(Stream::new(
            xobj_d,
            b"BT /F1 12 Tf <00480069> Tj ET".to_vec(),
        )));

        // Minimal page content stream.
        let content_id = doc.add_object(Object::Stream(Stream::new(
            Dictionary::new(),
            b"q Q".to_vec(),
        )));

        // Page node with NO /Resources (inherits from Pages).
        let mut page_d = Dictionary::new();
        page_d.set("Type", Object::Name(b"Page".to_vec()));
        page_d.set(
            "MediaBox",
            Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(595), Object::Integer(842),
            ]),
        );
        page_d.set("Contents", Object::Reference(content_id));
        let page_id = doc.add_object(Object::Dictionary(page_d));

        // Pages node: /Resources/XObject here, NOT on page dict.
        let mut xobj_dict = Dictionary::new();
        xobj_dict.set("X1", Object::Reference(xobj_id));
        let mut pages_res = Dictionary::new();
        pages_res.set("XObject", Object::Dictionary(xobj_dict));
        let mut pages_d = Dictionary::new();
        pages_d.set("Type", Object::Name(b"Pages".to_vec()));
        pages_d.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages_d.set("Count", Object::Integer(1));
        pages_d.set("Resources", Object::Dictionary(pages_res));
        let pages_id = doc.add_object(Object::Dictionary(pages_d));

        if let Ok(obj) = doc.get_object_mut(page_id) {
            if let Ok(d) = obj.as_dict_mut() {
                d.set("Parent", Object::Reference(pages_id));
            }
        }

        let mut catalog = Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", Object::Reference(pages_id));
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
        let text: String = frags.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join("");
        assert!(
            !frags.is_empty(),
            "expected CID text from XObject with inherited /Resources, got none"
        );
        assert!(
            text.contains("Hi"),
            "expected 'Hi' from CID+hex decode, got: {text:?}"
        );
    }
}
