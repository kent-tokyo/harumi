use lopdf::ObjectId;

use crate::font::FontHandle;

/// A color value that can be either RGB or CMYK.
///
/// RGB is the default for most PDF operations and displays on screens.
/// CMYK is used primarily for print output.
///
/// # Example
/// ```
/// use harumi::Color;
///
/// let rgb = Color::Rgb([0.5, 0.2, 0.8]);
/// let cmyk = Color::Cmyk([0.0, 0.8, 0.2, 0.1]);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    /// RGB color: each component in `0.0..=1.0`.
    /// Emits PDF operators `rg` (fill) and `RG` (stroke).
    Rgb([f32; 3]),
    /// CMYK color: each component in `0.0..=1.0`.
    /// Emits PDF operators `k` (fill) and `K` (stroke).
    Cmyk([f32; 4]),
}

impl From<[f32; 3]> for Color {
    fn from(c: [f32; 3]) -> Self {
        Color::Rgb(c)
    }
}

impl From<[f32; 4]> for Color {
    fn from(c: [f32; 4]) -> Self {
        Color::Cmyk(c)
    }
}

/// A single text placement descriptor for use with [`PageHandle::add_invisible_text_runs`].
///
/// # Example
/// ```no_run
/// # use harumi::{Document, TextRun, Color};
/// # fn main() -> harumi::Result<()> {
/// # let mut doc = Document::from_bytes(&[])?;
/// # let font = doc.embed_font(&[])?;
/// doc.page(1)?.add_invisible_text_runs(&[
///     TextRun { text: "first line".into(), font, x: 72.0, y: 700.0, font_size: 12.0, render_mode: 3, color: Color::Rgb([0.0; 3]) },
///     TextRun { text: "second line".into(), font, x: 72.0, y: 685.0, font_size: 12.0, render_mode: 3, color: Color::Rgb([0.0; 3]) },
/// ])?;
/// # Ok(())
/// # }
/// ```
pub struct TextRun {
    /// The text to place.
    pub text: String,
    /// Font to use (obtained from [`Document::embed_font`]).
    pub font: FontHandle,
    /// X coordinate in PDF points (origin: bottom-left of page).
    pub x: f32,
    /// Y coordinate in PDF points (origin: bottom-left of page).
    pub y: f32,
    /// Font size in PDF points.
    pub font_size: f32,
    /// Fill color (RGB or CMYK). Only applied when `render_mode == 0`.
    pub color: Color,
    /// PDF text render mode. `0` = visible, `3` = invisible (OCR search layer).
    pub render_mode: u8,
}

/// A pending text placement, stored until `save()` finalizes the document.
#[allow(dead_code)] // `opacity` is read only under the `draw` feature
pub(super) struct PendingText {
    pub(super) font: FontHandle,
    pub(super) text: String,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) font_size: f32,
    pub(super) render_mode: u8,
    pub(super) color: Color,
    pub(super) opacity: f32,
    pub(super) rotation_degrees: f32,
    pub(super) bold: bool,
    pub(super) italic: bool,
}

/// A pending operation on a page (text or drawing primitive).
pub(super) enum PendingOp {
    Text(PendingText),
    Replace(crate::replace::TextReplaceOp),
    ReplacePreserve(crate::replace::TextReplacePreserveOp),
    ReplaceResubset(crate::replace::TextReplaceResubsetOp),
    #[cfg(feature = "draw")]
    Draw(crate::draw::DrawOp),
}

/// Per-page pending operations.
pub(super) struct PendingPage {
    pub(super) page_id: ObjectId,
    pub(super) ops: Vec<PendingOp>,
}

/// A document outline entry (bookmark) accumulated before save time.
pub(super) struct PendingBookmark {
    pub(super) title: String,
    pub(super) page: u32,
    /// PDF y coordinate (bottom-left origin) for the destination anchor.
    pub(super) y: f32,
    /// Outline hierarchy level: 0 = top-level (from add_bookmark), 1..6 = nested levels.
    pub(super) level: u8,
}

/// Information about a file attached to a PDF.
///
/// Returned by [`Document::list_attachments`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct AttachmentInfo {
    /// The filename as stored in the PDF `/Filespec` `/F` entry.
    pub filename: String,
    /// Uncompressed size in bytes, from the embedded stream's `/Params /Size` entry.
    /// `0` if the size was not recorded.
    pub size: usize,
    /// MIME type from the embedded stream's `/Subtype` entry, if present.
    pub mime_type: Option<String>,
}

/// PDF /Info dictionary fields.
///
/// Used with [`Document::metadata`] and [`Document::set_metadata`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PdfMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
}

/// The type of a PDF form field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    /// Single-line or multiline text input (`/Tx`).
    Text,
    /// Checkbox or push-button (`/Btn`, non-radio).
    Checkbox,
    /// Radio button group (`/Btn` with radio flag set).
    Radio,
    /// Drop-down list or list box (`/Ch`).
    Choice,
    /// Digital signature field (`/Sig`).
    Signature,
    /// Unknown or unsupported field type.
    Unknown,
}

/// A PDF form field returned by [`Document::form_fields`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct FormField {
    /// The field name. For nested fields this is the dotted path,
    /// e.g. `"address.city"`.
    pub name: String,
    /// The kind of input widget.
    pub field_type: FieldType,
    /// The field's current string value.
    ///
    /// For text fields this is the entered text. For checkboxes/radio buttons
    /// this is the appearance state name (`"Yes"`, `"Off"`, etc.). For choice
    /// fields this is the selected option. Empty string when no value is set.
    pub value: String,
}

/// Options for creating a text field via [`Document::add_text_field`].
#[derive(Clone, Debug, Default)]
pub struct TextFieldOptions {
    /// Initial value of the field (`/V` entry). Default: empty string.
    pub default_value: String,
    /// Allow multiline input (`/Ff` bit 12, "Multiline" flag). Default: `false`.
    pub multiline: bool,
    /// Mark the field as read-only (`/Ff` bit 0, "ReadOnly" flag). Default: `false`.
    pub read_only: bool,
}

/// Options for [`PageHandle::replace_text_opts`].
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub struct ReplaceOptions {
    /// When `true`, all whitespace is stripped from `old_text` before matching.
    ///
    /// Use this when `old_text` was assembled by concatenating [`TextFragment`](crate::TextFragment)
    /// values with spaces (the default harumi-ai grouping strategy), but the underlying PDF
    /// stores each character in its own `BT`/`Tj`/`ET` block — a pattern used by
    /// Chrome/Skia-generated PDFs with Type3 fonts.  In that case `"T h e F r e e"` is
    /// normalised to `"TheFree"` before the match, allowing cross-BT replacement to succeed.
    pub normalize_whitespace: bool,
}

/// Placement options for [`PageHandle::replace_text_fragments_opts`].
///
/// Controls how the replacement text is sized, wrapped, and positioned.
/// Construct with `FragmentReplaceOpts::default()` and override specific fields.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct FragmentReplaceOpts {
    /// Font size for the placed text.  Defaults to the first fragment's `font_size`
    /// when `None`.
    pub font_size: Option<f32>,
    /// Maximum line width in PDF points.  When set, long text is wrapped to fit
    /// within this width using the same algorithm as `add_text_box`.
    pub max_width: Option<f32>,
    /// Y-axis adjustment applied to the anchor fragment's `y` coordinate.
    /// Positive shifts text up; negative shifts down.  Default `0.0`.
    pub y_offset: f32,
    /// Text color.  Defaults to black `[0.0, 0.0, 0.0]` when `None`.
    pub color: Option<Color>,
    /// When `true` and `max_width` is set, reduce the font size proportionally
    /// until the replacement text fits on a single line within `max_width`.
    /// The font size is never reduced below `min_font_size`.
    /// If the text still does not fit at `min_font_size`, it is rendered at
    /// that size without truncation.  Default `false`.
    pub shrink_to_fit: bool,
    /// Minimum font size (PDF points) used when `shrink_to_fit` is `true`.
    /// Ignored when `shrink_to_fit` is `false`.  Default `4.0`.
    pub min_font_size: f32,
    /// When `true`, perform a dry run: count how many `Tj`/`TJ` operators
    /// *would* be suppressed without actually writing any content stream.
    /// New text is **not** queued as a `PendingOp`.  Default `false`.
    ///
    /// Useful for pre-flight checks — call with `dry_run: true` first to
    /// confirm the operators are reachable, then make the real call (or fall
    /// back to overlay mode if the count is 0).
    pub dry_run: bool,
}

impl Default for FragmentReplaceOpts {
    fn default() -> Self {
        Self {
            font_size: None,
            max_width: None,
            y_offset: 0.0,
            color: None,
            shrink_to_fit: false,
            min_font_size: 4.0,
            dry_run: false,
        }
    }
}

/// One entry for [`PageHandle::replace_text_fragments_batch_opts`].
///
/// Carries its own [`FragmentReplaceOpts`], enabling per-cell font size,
/// max width, shrink-to-fit, and colour in a single batch call.
pub struct BatchEntry<'a> {
    /// Fragments to suppress.
    pub fragments: &'a [crate::extract::TextFragment],
    /// Replacement text to place at the anchor fragment's position.
    pub new_text: &'a str,
    /// Per-entry placement options.
    pub opts: FragmentReplaceOpts,
}

/// Options for [`PageHandle::replace_fragments_fit_to_bbox`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct FitOptions {
    /// Scale the font down until the text fits within the cell width. Default `true`.
    pub shrink_to_fit: bool,
    /// Minimum font size (pt) when `shrink_to_fit` is active. Default `6.0`.
    pub min_font_size: f32,
    /// Override text colour. `None` → black.
    pub color: Option<Color>,
}

impl Default for FitOptions {
    fn default() -> Self {
        Self { shrink_to_fit: true, min_font_size: 6.0, color: None }
    }
}

/// Options for [`Document::fit_text_to_box`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct BoxFitOptions {
    /// Minimum font size (PDF points) when shrinking. Default `6.0`.
    pub min_font_size: f32,
    /// Maximum number of lines to keep. `None` = unlimited. Default `None`.
    pub max_lines: Option<usize>,
    /// Whether to wrap text at word/character boundaries. Default `true`.
    pub wrap: bool,
    /// Policy for handling text that overflows the rectangle. Default [`OverflowPolicy::WrapThenShrink`].
    pub overflow: OverflowPolicy,
}

impl Default for BoxFitOptions {
    fn default() -> Self {
        Self {
            min_font_size: 6.0,
            max_lines: None,
            wrap: true,
            overflow: OverflowPolicy::WrapThenShrink,
        }
    }
}

/// How [`Document::fit_text_to_box`] handles text that does not fit.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverflowPolicy {
    /// Shrink the font size (no wrap) until the single-line text fits within the rectangle width.
    Shrink,
    /// Wrap first; if total wrapped height still exceeds the rectangle, shrink the font and re-wrap.
    WrapThenShrink,
    /// Wrap, then drop lines that exceed `max_lines` or the rectangle height.
    Truncate,
    /// Return the layout as-is and report overflow via [`FitResult::overflow_horizontal`] /
    /// [`FitResult::overflow_vertical`], without modifying text or font size.
    Report,
}

/// Outcome of a [`Document::fit_text_to_box`] placement.
///
/// Returned as [`FitResult::status`].  Callers can use this to decide whether
/// to accept the placement, try a different policy, or fall back to a shorter text.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementStatus {
    /// Text fit inside the rectangle without any adjustments.
    Ok,
    /// Font size was reduced from the requested value but remains above
    /// [`BoxFitOptions::min_font_size`].
    Shrunk,
    /// Font size was reduced all the way to [`BoxFitOptions::min_font_size`] and
    /// the text may still overflow.
    ShrunkToMin,
    /// Text overflows the rectangle (only possible with [`OverflowPolicy::Report`]).
    Overflow,
    /// One or more lines were dropped (only possible with [`OverflowPolicy::Truncate`]).
    Truncated,
}

/// Result of [`Document::fit_text_to_box`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct FitResult {
    /// Wrapped (and possibly truncated or re-wrapped after shrinking) lines.
    pub lines: Vec<String>,
    /// Font size actually used (may be smaller than the requested size after shrinking).
    pub font_size: f32,
    /// Bounding box actually occupied by the text: `[x, y, width, height]` in PDF points.
    ///
    /// The rectangle is top-aligned within the requested rect (matching `add_text_box` placement).
    /// `width` is the widest line actually rendered, capped at the requested rect width.
    pub used_rect: [f32; 4],
    /// `true` if any line is wider than the requested rectangle width.
    pub overflow_horizontal: bool,
    /// `true` if the total text height exceeds the requested rectangle height.
    pub overflow_vertical: bool,
    /// High-level outcome of the fitting operation.
    ///
    /// Use this to decide whether to accept the placement, retry with different options,
    /// or fall back to a shorter text.  See [`PlacementStatus`] for the full decision tree.
    pub status: PlacementStatus,
}

impl FitResult {
    /// `true` if either horizontal or vertical overflow occurred.
    pub fn overflow(&self) -> bool {
        self.overflow_horizontal || self.overflow_vertical
    }
}

/// Reason why a [`TextFragment`](crate::TextFragment) cannot be suppressed
/// by [`PageHandle::replace_text_fragments`].
///
/// Returned by [`PageHandle::can_suppress_fragment`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FragmentReplaceFailureReason {
    /// Neither `source_stream` nor `source_xobject` is set — source tracking
    /// was unavailable when this fragment was extracted (e.g. came from a
    /// deeply nested Form XObject or an old PDF structure).
    NoSourceInfo,
    /// `source_stream` index is out of range for this page's `/Contents` array.
    StreamIndexOutOfRange,
    /// The XObject identified by `source_xobject` could not be found in the document.
    XObjectNotFound,
    /// No `Tj`/`TJ` operator with `op.end == source_op_end` was found in the
    /// stream.  The stream may have already been rewritten by a previous call.
    OperatorNotFound,
    /// The stream could not be decompressed.
    DecompressFailed,
}

/// Raw font data stored before subsetting.
pub(super) struct RawFont {
    pub(super) ttf_bytes: Vec<u8>,
}

/// An existing PDF document that can be annotated with text overlays.
///
/// Load a document with [`Document::from_file`] or [`Document::from_bytes`],
/// add text with [`page`](Document::page), then write the result with
/// [`save`](Document::save).
///
/// # Deferred subsetting
///
/// [`embed_font`](Document::embed_font) is cheap — it only stores the raw TTF
/// bytes. At [`save`](Document::save) time, harumi collects all characters
/// used across every page and subsets each font exactly once.
pub struct Document {
    pub(crate) inner: lopdf::Document,
    pub(super) raw_fonts: Vec<RawFont>,
    pub(super) pending: Vec<PendingPage>,
    pub(super) pending_bookmarks: Vec<PendingBookmark>,
    /// Set to true after the first successful `finalize()`. Prevents silent corruption
    /// when new ops are queued after a `save()` call (font subsets would mismatch).
    pub(super) finalized: bool,
    /// Pending encryption parameters. Applied just before write at save time.
    pub(super) pending_encryption: Option<(String, String, EncAlgorithm)>,
}

/// Internal encryption algorithm selector for `pending_encryption`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EncAlgorithm {
    Rc4_128,
    Aes256,
}
