use lopdf::{Dictionary, Object, ObjectId, Stream};
use ttf_parser::Face;

use crate::{
    error::{Error, Result},
    font::FontHandle,
};

use super::helpers::{
    append_annotation_to_page, build_link_annot_base, build_markup_annot, check_finite,
    check_positive_size, parse_box_array, pdf_text_string, prepend_to_contents, read_page_box,
    set_page_box, wrap_paragraph,
};
use super::types::{
    Color, Document, FragmentReplaceFailureReason, FragmentReplaceOpts, PendingOp, PendingPage,
    PendingText, ReplaceOptions, TextRun,
};

/// Vertical alignment for [`PageHandle::add_text_box_aligned`].
pub enum VerticalAlign {
    /// Text starts at the top of the box (default).
    Top,
    /// Text block is centered vertically in the box.
    Center,
    /// Text block ends at the bottom of the box.
    Bottom,
}

/// A handle to a specific page for queuing text overlays.
///
/// Obtained via [`Document::page`]. All queued operations are written to the
/// PDF during [`Document::save`].
pub struct PageHandle<'doc> {
    pub(super) doc: &'doc mut Document,
    pub(super) page_id: ObjectId,
}

/// Count how many `Tj`/`TJ` operators in an object's stream have their
/// end-offset in `target_op_ends`, without modifying the stream.
fn count_ops_in_object(
    doc: &lopdf::Document,
    obj_id: lopdf::ObjectId,
    target_op_ends: &std::collections::HashSet<usize>,
) -> usize {
    let Ok(obj) = doc.get_object(obj_id) else {
        return 0;
    };
    let Ok(stream) = obj.as_stream() else {
        return 0;
    };
    let stream_bytes = if stream.dict.get(b"Filter").is_ok() {
        let mut owned = stream.clone();
        if owned.decompress().is_err() {
            return 0;
        }
        owned.content
    } else {
        stream.content.clone()
    };
    crate::replace::parse_ops(&stream_bytes)
        .iter()
        .filter(|op| {
            (op.keyword == b"Tj" || op.keyword == b"TJ") && target_op_ends.contains(&op.end)
        })
        .count()
}

/// Decompress an object's stream, suppress all `Tj`/`TJ` operators whose
/// `parse_ops` end-offset is in `target_op_ends` by replacing them with
/// `() Tj`, and write the rebuilt bytes back.  Returns the count of
/// operators suppressed.  A no-op (returns 0) if the object cannot be
/// found, is not a stream, or cannot be decompressed.
fn suppress_ops_in_object(
    doc: &mut lopdf::Document,
    obj_id: lopdf::ObjectId,
    target_op_ends: &std::collections::HashSet<usize>,
) -> usize {
    use lopdf::Object;

    let stream_bytes = {
        let Ok(obj) = doc.get_object(obj_id) else {
            return 0;
        };
        let Ok(stream) = obj.as_stream() else {
            return 0;
        };
        if stream.dict.get(b"Filter").is_ok() {
            let mut owned = stream.clone();
            if owned.decompress().is_err() {
                return 0;
            }
            owned.content
        } else {
            stream.content.clone()
        }
    };

    let ops = crate::replace::parse_ops(&stream_bytes);
    let mut new_bytes: Vec<u8> = Vec::with_capacity(stream_bytes.len() + 64);
    let mut prev_end = 0usize;
    let mut suppressed = 0usize;

    for op in &ops {
        if op.start > prev_end {
            new_bytes.extend_from_slice(&stream_bytes[prev_end..op.start]);
        }
        let is_target =
            (op.keyword == b"Tj" || op.keyword == b"TJ") && target_op_ends.contains(&op.end);
        if is_target {
            new_bytes.extend_from_slice(b"() Tj");
            suppressed += 1;
        } else {
            new_bytes.extend_from_slice(&stream_bytes[op.start..op.end]);
        }
        prev_end = op.end;
    }
    if prev_end < stream_bytes.len() {
        new_bytes.extend_from_slice(&stream_bytes[prev_end..]);
    }

    if suppressed > 0
        && let Ok(obj) = doc.get_object_mut(obj_id)
        && let Ok(stream) = obj.as_stream_mut()
    {
        stream.dict.remove(b"Filter");
        stream.dict.remove(b"DecodeParms");
        stream
            .dict
            .set("Length", Object::Integer(new_bytes.len() as i64));
        stream.content = new_bytes;
        stream.allows_compression = false;
    }
    suppressed
}

impl<'doc> PageHandle<'doc> {
    /// Queues a single invisible text placement on this page.
    ///
    /// The text is rendered with PDF render mode 3 (`Tr 3`): it is not painted
    /// on screen but is fully selectable and searchable. This is the standard
    /// approach for OCR text layers.
    ///
    /// `position` is `[x, y]` in PDF points (origin: bottom-left of page).
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_bytes(&[])?;
    /// # let font = doc.embed_font(&[])?;
    /// doc.page(1)?.add_invisible_text("検索可能なテキスト", font, [72.0, 700.0], 12.0)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_invisible_text(
        &mut self,
        text: &str,
        font: FontHandle,
        position: [f32; 2],
        font_size: f32,
    ) -> Result<()> {
        check_finite(&[position[0], position[1], font_size], "add_invisible_text")?;
        self.push_text(PendingText {
            font,
            text: text.to_owned(),
            x: position[0],
            y: position[1],
            font_size,
            render_mode: 3,
            color: Color::Rgb([0.0; 3]),
            opacity: 1.0,
            rotation_degrees: 0.0,
            bold: false,
            italic: false,
            char_spacing: 0.0,
        });
        Ok(())
    }

    /// Queues a visible text placement with the given RGB color.
    ///
    /// The text is rendered with PDF render mode 0 (`Tr 0`): filled with the
    /// specified color. Use this for watermarks, stamps, or any annotation that
    /// should be visible in the PDF.
    ///
    /// `position` is `[x, y]` in PDF points (origin: bottom-left of page).
    /// `color` is `[r, g, b]` where each component is in `0.0..=1.0`.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_bytes(&[])?;
    /// # let font = doc.embed_font(&[])?;
    /// // Red "CONFIDENTIAL" stamp in the center of the page
    /// let (w, h) = doc.page(1)?.size()?;
    /// doc.page(1)?.add_text("CONFIDENTIAL", font, [w / 2.0 - 60.0, h / 2.0], 24.0, [0.8, 0.0, 0.0])?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_text(
        &mut self,
        text: &str,
        font: FontHandle,
        position: [f32; 2],
        font_size: f32,
        color: impl Into<Color>,
    ) -> Result<()> {
        let color = color.into();
        check_finite(&[position[0], position[1], font_size], "add_text")?;
        self.push_text(PendingText {
            font,
            text: text.to_owned(),
            x: position[0],
            y: position[1],
            font_size,
            render_mode: 0,
            color,
            opacity: 1.0,
            rotation_degrees: 0.0,
            bold: false,
            italic: false,
            char_spacing: 0.0,
        });
        Ok(())
    }

    /// Overlays visible text with optional bold/italic synthetic styling.
    ///
    /// Bold is simulated with PDF render mode 2 (fill+stroke).
    /// Italic is simulated with a 12° horizontal shear text matrix.
    /// Both may be combined. Color and font size work the same as [`add_text`].
    ///
    /// `position` is `[x, y]` in PDF points (origin: bottom-left of page).
    /// `color` is `[r, g, b]` where each component is in `0.0..=1.0`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_text_styled(
        &mut self,
        text: &str,
        font: FontHandle,
        position: [f32; 2],
        font_size: f32,
        color: impl Into<Color>,
        bold: bool,
        italic: bool,
    ) -> Result<()> {
        let color = color.into();
        check_finite(&[position[0], position[1], font_size], "add_text_styled")?;
        self.push_text(PendingText {
            font,
            text: text.to_owned(),
            x: position[0],
            y: position[1],
            font_size,
            render_mode: 0,
            color,
            opacity: 1.0,
            rotation_degrees: 0.0,
            bold,
            italic,
            char_spacing: 0.0,
        });
        Ok(())
    }

    /// Like [`add_text_styled`] but also sets the PDF `Tc` (character spacing) operator.
    ///
    /// `char_spacing` is in PDF text-space points. Negative values compress characters
    /// (useful when translated text is slightly wider than the original bounding box);
    /// positive values expand them. Practical range: approximately `−1.0` to `+2.0`.
    ///
    /// Formula for fitting text into a known width:
    /// `Tc = (target_width_pt − natural_width_pt) / char_count`
    #[allow(clippy::too_many_arguments)]
    pub fn add_text_styled_with_char_spacing(
        &mut self,
        text: &str,
        font: FontHandle,
        position: [f32; 2],
        font_size: f32,
        color: impl Into<Color>,
        bold: bool,
        italic: bool,
        char_spacing: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[position[0], position[1], font_size, char_spacing],
            "add_text_styled_with_char_spacing",
        )?;
        self.push_text(PendingText {
            font,
            text: text.to_owned(),
            x: position[0],
            y: position[1],
            font_size,
            render_mode: 0,
            color,
            opacity: 1.0,
            rotation_degrees: 0.0,
            bold,
            italic,
            char_spacing,
        });
        Ok(())
    }

    /// Queues multiple text placements in one call.
    ///
    /// All runs across the entire document are collected before subsetting,
    /// so each font is subsetted exactly once regardless of how many runs use it.
    pub fn add_invisible_text_runs(&mut self, runs: &[TextRun]) -> Result<()> {
        for run in runs {
            check_finite(&[run.x, run.y, run.font_size], "add_invisible_text_runs")?;
            self.push_text(PendingText {
                font: run.font,
                text: run.text.clone(),
                x: run.x,
                y: run.y,
                font_size: run.font_size,
                render_mode: run.render_mode,
                color: run.color,
                opacity: 1.0,
                rotation_degrees: 0.0,
                bold: false,
                italic: false,
                char_spacing: 0.0,
            });
        }
        Ok(())
    }

    /// Replaces all occurrences of `old_text` in this page's existing content streams
    /// with `new_text` rendered in `font`.
    ///
    /// Matching is per PDF text operator: `old_text` must exactly equal the decoded
    /// content of a single `Tj` operator or one string element within a `TJ` array.
    /// Text split across multiple operators is not matched.
    ///
    /// Width compensation is applied automatically via a `Td` operator so that
    /// subsequent text on the same line is not displaced.
    ///
    /// Returns the number of matches found (and queued for replacement). A return
    /// value of `0` means `old_text` was not found; no modification is queued.
    ///
    /// # Errors
    /// Returns [`Error::InvalidFont`] if `font` was not registered on this document,
    /// or [`Error::InvalidInput`] if called after [`save`](Document::save).
    pub fn replace_text(
        &mut self,
        old_text: &str,
        new_text: &str,
        font: FontHandle,
    ) -> Result<usize> {
        if self.doc.raw_fonts.get(font.0 as usize).is_none() {
            return Err(Error::InvalidFont(font.0));
        }
        let count =
            crate::replace::count_matches_in_page(&self.doc.inner, self.page_id, old_text, None)?;
        if count > 0 {
            self.push_op(PendingOp::Replace(crate::replace::TextReplaceOp {
                font,
                old_text: old_text.to_owned(),
                new_text: new_text.to_owned(),
                font_size_override: None,
                char_spacing: None,
            }));
        }
        Ok(count)
    }

    /// Like [`replace_text`](PageHandle::replace_text) but accepts [`ReplaceOptions`].
    ///
    /// The primary option is `normalize_whitespace`: when `true`, all whitespace is
    /// stripped from `old_text` before matching and replacement.  This is useful when
    /// `old_text` was assembled from [`TextFragment`](crate::TextFragment) values joined
    /// with spaces (e.g. `"T h e F r e e"` for a Chrome/Skia Type3 PDF), while the PDF
    /// itself stores the characters without any space glyph between them.
    ///
    /// Returns the number of matches found (and queued for replacement).
    ///
    /// # Errors
    /// Returns [`Error::InvalidFont`] if `font` was not registered on this document,
    /// or [`Error::InvalidInput`] if called after [`save`](Document::save).
    pub fn replace_text_opts(
        &mut self,
        old_text: &str,
        new_text: &str,
        font: FontHandle,
        opts: ReplaceOptions,
    ) -> Result<usize> {
        if self.doc.raw_fonts.get(font.0 as usize).is_none() {
            return Err(Error::InvalidFont(font.0));
        }
        let effective_old: String = if opts.normalize_whitespace {
            old_text.split_whitespace().collect()
        } else {
            old_text.to_owned()
        };
        let count = crate::replace::count_matches_in_page(
            &self.doc.inner,
            self.page_id,
            &effective_old,
            None,
        )?;
        if count > 0 {
            self.push_op(PendingOp::Replace(crate::replace::TextReplaceOp {
                font,
                old_text: effective_old,
                new_text: new_text.to_owned(),
                font_size_override: opts.font_size_override,
                char_spacing: opts.char_spacing_override,
            }));
        }
        Ok(count)
    }

    /// Returns a short diagnostic string explaining why `old_text` could not be matched
    /// in this page's content streams.  Intended for debug logging only.
    /// Possible return values: `"cross-Tf"`, `"vertical-Td-or-Tm"`, `"text-not-in-stream"`.
    pub fn diagnose_replace_failure(&self, old_text: &str) -> &'static str {
        crate::replace::diagnose_match_failure(&self.doc.inner, self.page_id, old_text)
    }

    /// Replaces all occurrences of `old_text` in this page's existing content streams
    /// with `new_text`, reusing the font already embedded in the PDF at that position.
    ///
    /// Unlike [`replace_text`](PageHandle::replace_text), no `FontHandle` is required —
    /// harumi reads the font reference from the preceding `Tf` operator in the stream.
    ///
    /// Returns the number of matches found (and queued for replacement). Glyph
    /// availability is validated eagerly: if any character in `new_text` is absent
    /// from the existing font's ToUnicode mapping (e.g. the font is subsetted),
    /// `Err(FontCharNotMapped)` is returned immediately so the caller can fall back
    /// to [`replace_text`](PageHandle::replace_text) with an explicit font.
    ///
    /// # Errors
    /// Returns [`Error::FontCharNotMapped`](crate::Error::FontCharNotMapped) if any
    /// character in `new_text` is not present in the font's ToUnicode mapping.
    pub fn replace_text_preserve_font(&mut self, old_text: &str, new_text: &str) -> Result<usize> {
        let count = crate::replace::count_matches_in_page(
            &self.doc.inner,
            self.page_id,
            old_text,
            Some(new_text),
        )?;
        if count > 0 {
            self.push_op(PendingOp::ReplacePreserve(
                crate::replace::TextReplacePreserveOp {
                    old_text: old_text.to_owned(),
                    new_text: new_text.to_owned(),
                },
            ));
        }
        Ok(count)
    }

    /// Replaces all occurrences of `old_text` in this page's existing content streams
    /// with `new_text`, expanding the font subset if necessary.
    ///
    /// Unlike [`replace_text_preserve_font`](PageHandle::replace_text_preserve_font),
    /// this method succeeds even when `new_text` contains characters absent from the
    /// current font subset.  The caller must supply the **original, unsubsetted** font
    /// bytes (`font_bytes`) so that harumi can rebuild the subset.
    ///
    /// After the new subset is embedded the GID numbering may change; harumi
    /// automatically re-encodes all content streams on every page that references
    /// the same font.
    ///
    /// # Limitations
    /// Only **CIDFontType2** fonts with `CIDToGIDMap /Identity` are supported
    /// (which is what harumi embeds).  Type1 / simple TrueType fonts will return
    /// [`Error::InvalidInput`].
    ///
    /// Returns the number of matches found (and queued for replacement).
    /// A return value of `0` means `old_text` was not found; no modification is queued.
    ///
    /// # Errors
    /// - [`Error::InvalidInput`] if called after [`save`](Document::save), or if the
    ///   font in the PDF is not a supported CIDFontType2.
    /// - [`Error::FontParse`] if `font_bytes` cannot be parsed as a TTF/OTF font.
    pub fn replace_text_resubset(
        &mut self,
        old_text: &str,
        new_text: &str,
        font_bytes: &[u8],
    ) -> Result<usize> {
        if self.doc.finalized {
            return Err(Error::InvalidInput(
                "replace_text_resubset called after save()".into(),
            ));
        }

        // SECURITY: Limit font byte size to prevent memory exhaustion.
        const MAX_FONT_SIZE: usize = 50 * 1024 * 1024; // 50 MB
        if font_bytes.len() > MAX_FONT_SIZE {
            return Err(Error::InvalidInput(format!(
                "font bytes exceed {} MB limit",
                MAX_FONT_SIZE / 1024 / 1024
            )));
        }

        // Validate font bytes eagerly.
        let face =
            ttf_parser::Face::parse(font_bytes, 0).map_err(|e| Error::FontParse(e.to_string()))?;
        if face.units_per_em() == 0 {
            return Err(Error::FontParse("font units_per_em is 0".into()));
        }
        let count =
            crate::replace::count_matches_in_page(&self.doc.inner, self.page_id, old_text, None)?;
        if count > 0 {
            self.push_op(PendingOp::ReplaceResubset(
                crate::replace::TextReplaceResubsetOp {
                    old_text: old_text.to_owned(),
                    new_text: new_text.to_owned(),
                    font_bytes: font_bytes.to_vec(),
                    wrap: None,
                },
            ));
        }
        Ok(count)
    }

    /// Like [`replace_text_resubset`], but wraps `new_text` to multiple lines if it exceeds the line width.
    ///
    /// # Parameters
    /// - `old_text`: Text to find
    /// - `new_text`: Replacement text (will be wrapped if too long)
    /// - `font_bytes`: TTF font bytes for subsetting and width calculation
    /// - `line_height`: Vertical spacing between wrapped lines (e.g., font_size * 1.2).
    ///   If 0.0, defaults to font_size * 1.2.
    ///
    /// # Limitations
    /// - Single-font replacements only (no font switching within wrapped text)
    /// - Estimates page width as ~450pt (A4 with standard margins); PDFs with custom widths may wrap differently
    /// - Does not handle multi-column layouts
    pub fn replace_text_resubset_with_wrap(
        &mut self,
        old_text: &str,
        new_text: &str,
        font_bytes: &[u8],
        line_height: f32,
    ) -> Result<usize> {
        if self.doc.finalized {
            return Err(Error::InvalidInput(
                "replace_text_resubset_with_wrap called after save()".into(),
            ));
        }

        // SECURITY: Limit font byte size to prevent memory exhaustion.
        const MAX_FONT_SIZE: usize = 50 * 1024 * 1024; // 50 MB
        if font_bytes.len() > MAX_FONT_SIZE {
            return Err(Error::InvalidInput(format!(
                "font bytes exceed {} MB limit",
                MAX_FONT_SIZE / 1024 / 1024
            )));
        }

        // Validate line_height: must be finite and non-negative.
        if !line_height.is_finite() || line_height < 0.0 {
            return Err(Error::InvalidInput(format!(
                "line_height must be finite and non-negative, got {}",
                line_height
            )));
        }

        let face =
            ttf_parser::Face::parse(font_bytes, 0).map_err(|e| Error::FontParse(e.to_string()))?;
        if face.units_per_em() == 0 {
            return Err(Error::FontParse("font units_per_em is 0".into()));
        }

        let count =
            crate::replace::count_matches_in_page(&self.doc.inner, self.page_id, old_text, None)?;

        if count > 0 {
            let max_width = self.page_width_for_wrap();
            // Ensure max_width is positive (safety check on MediaBox calculation).
            if max_width <= 0.0 {
                return Err(Error::InvalidInput(format!(
                    "page width for wrapping is non-positive ({}pt); cannot wrap",
                    max_width
                )));
            }
            let effective_lh = if line_height > 0.0 { line_height } else { 14.4 };

            self.push_op(PendingOp::ReplaceResubset(
                crate::replace::TextReplaceResubsetOp {
                    old_text: old_text.to_owned(),
                    new_text: new_text.to_owned(),
                    font_bytes: font_bytes.to_vec(),
                    wrap: Some(crate::replace::WrapParams {
                        font_bytes: font_bytes.to_vec(),
                        line_height: effective_lh,
                        max_width,
                    }),
                },
            ));
        }

        Ok(count)
    }

    /// Estimate page width for text wrapping from MediaBox.
    ///
    /// Returns MediaBox width minus 144pt (72pt margins × 2).
    /// Falls back to A4 default (451pt) if page dimensions are unavailable.
    fn page_width_for_wrap(&self) -> f32 {
        self.media_box()
            .map(|b| (b[2] - b[0]) - 144.0)
            .unwrap_or(451.0)
    }

    /// Replaces text, automatically falling back to font re-subsetting if the
    /// current font subset doesn't contain necessary characters.
    ///
    /// First attempts [`replace_text_preserve_font`](PageHandle::replace_text_preserve_font)
    /// using the existing embedded font. If that fails with [`FontCharNotMapped`](crate::Error::FontCharNotMapped),
    /// automatically retries with [`replace_text_resubset`](PageHandle::replace_text_resubset)
    /// using the provided fallback font bytes.
    ///
    /// This is a convenience method for multi-language replacements where the
    /// target characters may not be in the current subset. All other errors
    /// (e.g., text not found, invalid font) are propagated immediately.
    ///
    /// # Arguments
    /// * `old_text` — text to find
    /// * `new_text` — replacement text
    /// * `fallback_font` — original (unsubsetted) TTF bytes to use if the current font
    ///   lacks characters from `new_text`. For best results, provide the same font
    ///   that was used to create the page's existing text.
    ///
    /// # Returns
    /// The number of replacements found and queued (will be the same via either path).
    ///
    /// # Errors
    /// Errors from [`replace_text_resubset`](PageHandle::replace_text_resubset) if the
    /// fallback path is needed but encounters a font-format or other error.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_file("example.pdf")?;
    /// # let font_bytes = std::fs::read("font.ttf")?;
    /// let mut page = doc.page(1)?;
    /// let count = page.replace_text_with_fallback("Hello", "こんにちは", &font_bytes)?;
    /// println!("Replaced {} occurrences", count);
    /// # Ok(())
    /// # }
    /// ```
    pub fn replace_text_with_fallback(
        &mut self,
        old_text: &str,
        new_text: &str,
        fallback_font: &[u8],
    ) -> Result<usize> {
        match self.replace_text_preserve_font(old_text, new_text) {
            Ok(count) => Ok(count),
            Err(Error::FontCharNotMapped { .. }) => {
                self.replace_text_resubset(old_text, new_text, fallback_font)
            }
            Err(e) => Err(e),
        }
    }

    /// Scans the page for `old_text` and validates that all characters in `new_text`
    /// are present in the existing font's ToUnicode mapping — without modifying the document.
    ///
    /// Returns the number of occurrences of `old_text` found on this page.
    /// A return value of `0` means no replacement would occur.
    ///
    /// Use this to decide whether to call
    /// [`replace_text_preserve_font`](PageHandle::replace_text_preserve_font)
    /// (which would mutate the document) or fall back to
    /// [`replace_text`](PageHandle::replace_text) with an explicit font.
    ///
    /// # Errors
    /// Returns [`Error::FontCharNotMapped`](crate::Error::FontCharNotMapped) if any
    /// character in `new_text` is absent from the font's ToUnicode mapping.
    pub fn can_replace_text(&self, old_text: &str, new_text: &str) -> Result<usize> {
        crate::replace::count_matches_in_page(
            &self.doc.inner,
            self.page_id,
            old_text,
            Some(new_text),
        )
    }

    /// Suppress the `Tj`/`TJ` operators that produced `fragments` and place
    /// `new_text` at the position of the first fragment.
    ///
    /// Each fragment in `fragments` that carries `source_stream` / `source_op_start`
    /// (populated by [`extract_text_runs`](Document::extract_text_runs)) has its
    /// originating operator replaced with an empty-string `() Tj` so the original
    /// glyph is no longer rendered.  `new_text` is then queued as a new visible
    /// text run at the position of the first trackable fragment.
    ///
    /// Equivalent to [`replace_text_fragments_opts`] with
    /// [`FragmentReplaceOpts::default()`].
    ///
    /// **Offset stability warning:** This method rewrites the content stream in-place.
    /// Calling it multiple times on the same page with fragments from the same stream
    /// invalidates the `source_op_end` byte offsets of any fragments not yet processed.
    /// To suppress multiple logical lines safely in a single pass, use
    /// [`replace_text_fragments_batch`](PageHandle::replace_text_fragments_batch)
    /// instead.
    pub fn replace_text_fragments(
        &mut self,
        fragments: &[crate::extract::TextFragment],
        new_text: &str,
        font: FontHandle,
    ) -> Result<usize> {
        self.replace_text_fragments_opts(fragments, new_text, font, FragmentReplaceOpts::default())
    }

    /// Like [`replace_text_fragments`](PageHandle::replace_text_fragments) but with
    /// full control over how the replacement text is placed.
    ///
    /// Both page content streams (`source_stream`) and Form XObject streams
    /// (`source_xobject`) are handled — the source operator is rewritten to `() Tj`
    /// in whichever stream produced the fragment.
    ///
    /// When `opts.max_width` is set, the text is wrapped using the same algorithm
    /// as [`add_text_box`](PageHandle::add_text_box) and multiple
    /// `PendingOp::Text` entries are queued (one per line), positioned downward from
    /// the anchor fragment's coordinates.
    pub fn replace_text_fragments_opts(
        &mut self,
        fragments: &[crate::extract::TextFragment],
        new_text: &str,
        font: FontHandle,
        opts: FragmentReplaceOpts,
    ) -> Result<usize> {
        use std::collections::{HashMap, HashSet};

        if self.doc.raw_fonts.get(font.0 as usize).is_none() {
            return Err(Error::InvalidFont(font.0));
        }

        let mut total_suppressed = 0usize;

        // --- Page content stream suppression ---
        let mut by_stream: HashMap<usize, HashSet<usize>> = HashMap::new();
        for frag in fragments {
            if let (Some(sidx), Some(_), Some(op_end)) =
                (frag.source_stream, frag.source_op_start, frag.source_op_end)
            {
                by_stream.entry(sidx).or_default().insert(op_end);
            }
        }
        let stream_ids = crate::extract::page_content_stream_ids(&self.doc.inner, self.page_id);
        for (stream_idx, target_op_ends) in &by_stream {
            let Some(&stream_id) = stream_ids.get(*stream_idx) else {
                continue;
            };
            total_suppressed += if opts.dry_run {
                count_ops_in_object(&self.doc.inner, stream_id, target_op_ends)
            } else {
                suppress_ops_in_object(&mut self.doc.inner, stream_id, target_op_ends)
            };
        }

        // --- Form XObject stream suppression ---
        let mut by_xobj: HashMap<(u32, u16), HashSet<usize>> = HashMap::new();
        for frag in fragments {
            if let (Some(xobj_id), Some(_), Some(op_end)) = (
                frag.source_xobject,
                frag.source_op_start,
                frag.source_op_end,
            ) {
                by_xobj.entry(xobj_id).or_default().insert(op_end);
            }
        }
        for (xobj_id, target_op_ends) in &by_xobj {
            total_suppressed += if opts.dry_run {
                count_ops_in_object(&self.doc.inner, *xobj_id, target_op_ends)
            } else {
                suppress_ops_in_object(&mut self.doc.inner, *xobj_id, target_op_ends)
            };
        }

        // --- New text placement (skipped in dry-run mode) ---
        if !opts.dry_run && !new_text.is_empty() && total_suppressed > 0 {
            let anchor = fragments
                .iter()
                .find(|f| f.source_stream.is_some() || f.source_xobject.is_some())
                .or_else(|| fragments.first());
            if let Some(frag) = anchor {
                let fs_initial = opts.font_size.unwrap_or(frag.font_size).max(1.0);
                let ax = frag.x;
                let ay = frag.y + opts.y_offset;
                let color = opts.color.unwrap_or(Color::Rgb([0.0, 0.0, 0.0]));

                // Compute font size (shrink-to-fit) and line-wrap in one block so
                // the TTF face is parsed only once instead of twice.
                let (fs, lines): (f32, Vec<String>) = if let Some(max_w) = opts.max_width {
                    let font_bytes = &self.doc.raw_fonts[font.0 as usize].ttf_bytes;
                    let opt_face = ttf_parser::Face::parse(font_bytes, 0).ok();
                    let fs = if opts.shrink_to_fit {
                        let min_fs = opts.min_font_size.max(1.0);
                        let mut candidate = fs_initial;
                        loop {
                            let w = opt_face
                                .as_ref()
                                .map(|f| {
                                    super::helpers::text_width_with_face(new_text, f, candidate)
                                })
                                .unwrap_or(max_w);
                            if w <= max_w || candidate <= min_fs {
                                break;
                            }
                            candidate = (candidate * max_w / w).max(min_fs);
                        }
                        candidate
                    } else {
                        fs_initial
                    };
                    let lines = if let Some(face) = opt_face.as_ref() {
                        wrap_paragraph(new_text, face, fs, max_w)
                    } else {
                        vec![new_text.to_owned()]
                    };
                    (fs, lines)
                } else {
                    (fs_initial, vec![new_text.to_owned()])
                };

                let line_height = fs * 1.2;
                for (i, line) in lines.iter().enumerate() {
                    let ly = ay - i as f32 * line_height;
                    self.push_op(PendingOp::Text(PendingText {
                        font,
                        text: line.clone(),
                        x: ax,
                        y: ly,
                        font_size: fs,
                        render_mode: 0,
                        color,
                        opacity: 1.0,
                        rotation_degrees: 0.0,
                        bold: false,
                        italic: false,
                        char_spacing: 0.0,
                    }));
                }
            }
        }

        Ok(total_suppressed)
    }

    /// Replace text for multiple logical lines in a single content-stream pass.
    ///
    /// Unlike calling [`replace_text_fragments`] or [`replace_text_fragments_opts`]
    /// repeatedly, this method collects **all** suppression targets up-front and
    /// rewrites each content stream **exactly once**.  This prevents byte-offset
    /// shift: after one rewrite, subsequent entries that target the same stream would
    /// reference stale `source_op_end` values and silently miss their operators.
    ///
    /// `entries` is a slice of `(fragments, new_text)` pairs — each pair represents
    /// one logical line or table cell.  All entries share the same `font` and `opts`.
    ///
    /// ## Cross-stream behaviour
    ///
    /// Fragments may come from **different** source streams (different indices in the
    /// page `/Contents` array) or from different Form XObjects.  Each unique stream is
    /// rewritten exactly once regardless of how many entries reference it, so the
    /// single-pass guarantee holds per stream:
    ///
    /// - Entries whose fragments span multiple `source_stream` indices (e.g. a visual
    ///   line whose characters are split across two `/Contents` streams) are handled
    ///   correctly — both streams are suppressed in the same batch call.
    /// - If `opts.dry_run` is `true`, no streams are written; the return value is the
    ///   count of operators that *would* be suppressed (useful for pre-flight checks).
    ///
    /// Returns the total number of `Tj`/`TJ` operators suppressed (or that would be
    /// suppressed when `dry_run = true`) across all entries and streams.
    ///
    /// # Errors
    /// Returns [`Error::InvalidFont`] if `font` was not registered on this document.
    pub fn replace_text_fragments_batch(
        &mut self,
        entries: &[(&[crate::extract::TextFragment], &str)],
        font: FontHandle,
        opts: FragmentReplaceOpts,
    ) -> Result<usize> {
        use std::collections::{HashMap, HashSet};

        if self.doc.raw_fonts.get(font.0 as usize).is_none() {
            return Err(Error::InvalidFont(font.0));
        }

        // Pre-collect ALL targets across ALL entries before any stream is written.
        let mut by_stream: HashMap<usize, HashSet<usize>> = HashMap::new();
        let mut by_xobj: HashMap<(u32, u16), HashSet<usize>> = HashMap::new();

        for (fragments, _) in entries {
            for frag in *fragments {
                if let (Some(sidx), Some(_), Some(op_end)) =
                    (frag.source_stream, frag.source_op_start, frag.source_op_end)
                {
                    by_stream.entry(sidx).or_default().insert(op_end);
                }
                if let (Some(xobj_id), Some(_), Some(op_end)) = (
                    frag.source_xobject,
                    frag.source_op_start,
                    frag.source_op_end,
                ) {
                    by_xobj.entry(xobj_id).or_default().insert(op_end);
                }
            }
        }

        // Single pass per stream — no offset shift between entries.
        let mut total_suppressed = 0usize;
        let stream_ids = crate::extract::page_content_stream_ids(&self.doc.inner, self.page_id);

        for (stream_idx, target_op_ends) in &by_stream {
            let Some(&stream_id) = stream_ids.get(*stream_idx) else {
                continue;
            };
            total_suppressed += if opts.dry_run {
                count_ops_in_object(&self.doc.inner, stream_id, target_op_ends)
            } else {
                suppress_ops_in_object(&mut self.doc.inner, stream_id, target_op_ends)
            };
        }
        for (xobj_id, target_op_ends) in &by_xobj {
            total_suppressed += if opts.dry_run {
                count_ops_in_object(&self.doc.inner, *xobj_id, target_op_ends)
            } else {
                suppress_ops_in_object(&mut self.doc.inner, *xobj_id, target_op_ends)
            };
        }

        if opts.dry_run {
            return Ok(total_suppressed);
        }

        // Pre-compute text placements (immutable borrows only — kept in a Vec
        // so push_op's mutable borrow of self doesn't conflict).
        struct EntryPlacement {
            x: f32,
            lines: Vec<(f32, String)>, // (y, text) per line
            fs: f32,
            color: Color,
        }

        let placements: Vec<EntryPlacement> = {
            let font_bytes = &self.doc.raw_fonts[font.0 as usize].ttf_bytes;
            let face = ttf_parser::Face::parse(font_bytes, 0).ok();

            let mut result = Vec::new();
            for (fragments, new_text) in entries {
                if new_text.is_empty() {
                    continue;
                }
                let anchor = fragments
                    .iter()
                    .find(|f| f.source_stream.is_some() || f.source_xobject.is_some())
                    .or_else(|| fragments.first());
                let Some(frag) = anchor else { continue };

                let fs_initial = opts.font_size.unwrap_or(frag.font_size).max(1.0);
                let ay = frag.y + opts.y_offset;
                let color = opts.color.unwrap_or(Color::Rgb([0.0, 0.0, 0.0]));

                let fs = if opts.shrink_to_fit {
                    if let Some(max_w) = opts.max_width {
                        let min_fs = opts.min_font_size.max(1.0);
                        let mut candidate = fs_initial;
                        loop {
                            let w = face
                                .as_ref()
                                .map(|f| {
                                    super::helpers::text_width_with_face(new_text, f, candidate)
                                })
                                .unwrap_or(max_w);
                            if w <= max_w || candidate <= min_fs {
                                break;
                            }
                            candidate = (candidate * max_w / w).max(min_fs);
                        }
                        candidate
                    } else {
                        fs_initial
                    }
                } else {
                    fs_initial
                };

                let text_lines: Vec<String> =
                    if let (Some(max_w), Some(face)) = (opts.max_width, face.as_ref()) {
                        wrap_paragraph(new_text, face, fs, max_w)
                    } else {
                        vec![(*new_text).to_owned()]
                    };

                let line_height = fs * 1.2;
                let lines = text_lines
                    .into_iter()
                    .enumerate()
                    .map(|(i, l)| (ay - i as f32 * line_height, l))
                    .collect();

                result.push(EntryPlacement {
                    x: frag.x,
                    lines,
                    fs,
                    color,
                });
            }
            result
        }; // immutable borrows (font_bytes, face) released here

        // Mutable phase: queue PendingOp::Text for each placement.
        for p in placements {
            for (ly, line) in p.lines {
                self.push_op(PendingOp::Text(PendingText {
                    font,
                    text: line,
                    x: p.x,
                    y: ly,
                    font_size: p.fs,
                    render_mode: 0,
                    color: p.color,
                    opacity: 1.0,
                    rotation_degrees: 0.0,
                    bold: false,
                    italic: false,
                    char_spacing: 0.0,
                }));
            }
        }

        Ok(total_suppressed)
    }

    /// Batch replacement where each entry carries its own [`FragmentReplaceOpts`].
    pub fn replace_text_fragments_batch_opts(
        &mut self,
        entries: &[super::types::BatchEntry<'_>],
        font: crate::font::FontHandle,
    ) -> crate::error::Result<usize> {
        use super::types::Color;
        use std::collections::{HashMap, HashSet};

        if self.doc.raw_fonts.get(font.0 as usize).is_none() {
            return Err(crate::error::Error::InvalidFont(font.0));
        }

        let mut by_stream: HashMap<usize, HashSet<usize>> = HashMap::new();
        let mut by_xobj: HashMap<(u32, u16), HashSet<usize>> = HashMap::new();
        let any_dry_run = entries.iter().any(|e| e.opts.dry_run);

        for entry in entries {
            for frag in entry.fragments {
                if let (Some(sidx), Some(_), Some(op_end)) =
                    (frag.source_stream, frag.source_op_start, frag.source_op_end)
                {
                    by_stream.entry(sidx).or_default().insert(op_end);
                }
                if let (Some(xobj_id), Some(_), Some(op_end)) = (
                    frag.source_xobject,
                    frag.source_op_start,
                    frag.source_op_end,
                ) {
                    by_xobj.entry(xobj_id).or_default().insert(op_end);
                }
            }
        }

        let mut total_suppressed = 0usize;
        let stream_ids = crate::extract::page_content_stream_ids(&self.doc.inner, self.page_id);

        for (stream_idx, target_op_ends) in &by_stream {
            let Some(&stream_id) = stream_ids.get(*stream_idx) else {
                continue;
            };
            total_suppressed += if any_dry_run {
                count_ops_in_object(&self.doc.inner, stream_id, target_op_ends)
            } else {
                suppress_ops_in_object(&mut self.doc.inner, stream_id, target_op_ends)
            };
        }
        for (xobj_id, target_op_ends) in &by_xobj {
            total_suppressed += if any_dry_run {
                count_ops_in_object(&self.doc.inner, *xobj_id, target_op_ends)
            } else {
                suppress_ops_in_object(&mut self.doc.inner, *xobj_id, target_op_ends)
            };
        }

        if any_dry_run {
            return Ok(total_suppressed);
        }

        struct EntryPlacement {
            x: f32,
            lines: Vec<(f32, String)>,
            fs: f32,
            color: Color,
        }

        let placements: Vec<EntryPlacement> = {
            let font_bytes = &self.doc.raw_fonts[font.0 as usize].ttf_bytes;
            let face = ttf_parser::Face::parse(font_bytes, 0).ok();
            let mut result = Vec::new();
            for entry in entries {
                if entry.new_text.is_empty() {
                    continue;
                }
                let opts = &entry.opts;
                let anchor = entry
                    .fragments
                    .iter()
                    .find(|f| f.source_stream.is_some() || f.source_xobject.is_some())
                    .or_else(|| entry.fragments.first());
                let Some(frag) = anchor else { continue };
                let fs_initial = opts.font_size.unwrap_or(frag.font_size).max(1.0);
                let ay = frag.y + opts.y_offset;
                let color = opts.color.unwrap_or(Color::Rgb([0.0, 0.0, 0.0]));
                let fs = if opts.shrink_to_fit {
                    if let Some(max_w) = opts.max_width {
                        let min_fs = opts.min_font_size.max(1.0);
                        let mut candidate = fs_initial;
                        loop {
                            let w = face
                                .as_ref()
                                .map(|f| {
                                    super::helpers::text_width_with_face(
                                        entry.new_text,
                                        f,
                                        candidate,
                                    )
                                })
                                .unwrap_or(max_w);
                            if w <= max_w || candidate <= min_fs {
                                break;
                            }
                            candidate = (candidate * max_w / w).max(min_fs);
                        }
                        candidate
                    } else {
                        fs_initial
                    }
                } else {
                    fs_initial
                };
                let text_lines: Vec<String> =
                    if let (Some(max_w), Some(face)) = (opts.max_width, face.as_ref()) {
                        super::helpers::wrap_paragraph(entry.new_text, face, fs, max_w)
                    } else {
                        vec![entry.new_text.to_owned()]
                    };
                let line_height = fs * 1.2;
                let lines = text_lines
                    .into_iter()
                    .enumerate()
                    .map(|(i, l)| (ay - i as f32 * line_height, l))
                    .collect();
                result.push(EntryPlacement {
                    x: frag.x,
                    lines,
                    fs,
                    color,
                });
            }
            result
        };

        for p in placements {
            for (ly, line) in p.lines {
                self.push_op(PendingOp::Text(PendingText {
                    font,
                    text: line,
                    x: p.x,
                    y: ly,
                    font_size: p.fs,
                    render_mode: 0,
                    color: p.color,
                    opacity: 1.0,
                    rotation_degrees: 0.0,
                    bold: false,
                    italic: false,
                    char_spacing: 0.0,
                }));
            }
        }
        Ok(total_suppressed)
    }

    /// Suppress `fragments` and place `new_text` sized to fit within `bbox`.
    ///
    /// `bbox` = `[x, y, width, height]`; `width` is used as `max_width`.
    pub fn replace_fragments_fit_to_bbox(
        &mut self,
        fragments: &[crate::extract::TextFragment],
        new_text: &str,
        font: crate::font::FontHandle,
        bbox: [f32; 4],
        fit_opts: super::types::FitOptions,
    ) -> crate::error::Result<usize> {
        let opts = super::types::FragmentReplaceOpts {
            max_width: Some(bbox[2]),
            shrink_to_fit: fit_opts.shrink_to_fit,
            min_font_size: fit_opts.min_font_size,
            color: fit_opts.color,
            ..Default::default()
        };
        self.replace_text_fragments_opts(fragments, new_text, font, opts)
    }

    /// Suppress all `Tj`/`TJ` operators whose decoded text satisfies `predicate`,
    /// scanning the page content streams in memory without a save/reload cycle.
    ///
    /// This is the single-pass alternative to the two-step workaround:
    ///
    /// ```text
    /// // Old workaround (two encode/decode cycles):
    /// doc.save("tmp.pdf")?;
    /// let doc2 = Document::from_file("tmp.pdf")?;
    /// let frags = doc2.extract_text_runs(page)?;
    /// // … filter and suppress …
    ///
    /// // New API (single cycle, in memory):
    /// doc.page(n)?.suppress_text_where(|text| is_source_language(text))?;
    /// doc.save("out.pdf")?;
    /// ```
    ///
    /// The method re-extracts text directly from the current (possibly already
    /// rewritten) in-memory content streams, so it sees the same state as a
    /// save-and-reload would — without actually saving.  It requires no
    /// [`FontHandle`] because no new text is placed; operators are blanked with
    /// `() Tj`.
    ///
    /// Returns the count of suppressed operators.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_file("input.pdf")?;
    /// # let font = doc.embed_font(b"")?;
    /// // First pass: replace fragments with known source info.
    /// // let n = doc.page(1)?.replace_text_fragments_batch_opts(&entries, font)?;
    ///
    /// // Second pass (in memory, no save/reload): suppress any remaining Japanese.
    /// let suppressed = doc.page(1)?.suppress_text_where(|text| {
    ///     text.chars().any(|c| matches!(c, '\u{3000}'..='\u{9FFF}'))
    /// })?;
    /// doc.save("translated.pdf")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn suppress_text_where<F>(&mut self, predicate: F) -> crate::error::Result<usize>
    where
        F: Fn(&str) -> bool,
    {
        use std::collections::{HashMap, HashSet};

        // Re-extract from the current in-memory content streams.
        // This sees any suppressions already applied in this session.
        let frags = crate::extract::extract_text_runs_from_page(&self.doc.inner, self.page_id)?;

        // Collect suppression targets for fragments whose text matches.
        let mut by_stream: HashMap<usize, HashSet<usize>> = HashMap::new();
        let mut by_xobj: HashMap<(u32, u16), HashSet<usize>> = HashMap::new();

        for frag in frags.iter().filter(|f| predicate(&f.text)) {
            if let (Some(sidx), Some(_), Some(op_end)) =
                (frag.source_stream, frag.source_op_start, frag.source_op_end)
            {
                by_stream.entry(sidx).or_default().insert(op_end);
            }
            if let (Some(xobj_id), Some(_), Some(op_end)) = (
                frag.source_xobject,
                frag.source_op_start,
                frag.source_op_end,
            ) {
                by_xobj.entry(xobj_id).or_default().insert(op_end);
            }
        }

        if by_stream.is_empty() && by_xobj.is_empty() {
            return Ok(0);
        }

        // Suppress in a single pass per stream (stable byte offsets).
        let mut total = 0usize;
        let stream_ids = crate::extract::page_content_stream_ids(&self.doc.inner, self.page_id);

        for (stream_idx, target_op_ends) in &by_stream {
            let Some(&stream_id) = stream_ids.get(*stream_idx) else {
                continue;
            };
            total += suppress_ops_in_object(&mut self.doc.inner, stream_id, target_op_ends);
        }
        for (xobj_id, target_op_ends) in &by_xobj {
            total += suppress_ops_in_object(&mut self.doc.inner, *xobj_id, target_op_ends);
        }

        Ok(total)
    }

    /// Check whether a single [`TextFragment`](crate::TextFragment) can be
    /// suppressed by [`replace_text_fragments`](PageHandle::replace_text_fragments)
    /// without modifying the document.
    ///
    /// Returns `Ok(())` if the fragment's source operator is locatable in the
    /// current (possibly already-rewritten) content stream.
    /// Returns `Err(reason)` when suppression would silently fail, so callers can
    /// decide whether to fall back to overlay mode.
    ///
    /// This is a read-only inspection — the document is not changed.
    pub fn can_suppress_fragment(
        &self,
        fragment: &crate::extract::TextFragment,
    ) -> std::result::Result<(), FragmentReplaceFailureReason> {
        use FragmentReplaceFailureReason as R;

        let Some(op_end) = fragment.source_op_end else {
            return Err(R::NoSourceInfo);
        };

        // Locate the stream object.
        let obj_id: lopdf::ObjectId = if let Some(sidx) = fragment.source_stream {
            let stream_ids = crate::extract::page_content_stream_ids(&self.doc.inner, self.page_id);
            *stream_ids.get(sidx).ok_or(R::StreamIndexOutOfRange)?
        } else if let Some(xobj_id) = fragment.source_xobject {
            self.doc
                .inner
                .get_object(xobj_id)
                .map_err(|_| R::XObjectNotFound)?;
            xobj_id
        } else {
            return Err(R::NoSourceInfo);
        };

        // Decompress and scan for the operator.
        let stream_bytes = {
            let Ok(obj) = self.doc.inner.get_object(obj_id) else {
                return Err(R::XObjectNotFound);
            };
            let Ok(stream) = obj.as_stream() else {
                return Err(R::XObjectNotFound);
            };
            if stream.dict.get(b"Filter").is_ok() {
                let mut owned = stream.clone();
                owned.decompress().map_err(|_| R::DecompressFailed)?;
                owned.content
            } else {
                stream.content.clone()
            }
        };

        let found = crate::replace::parse_ops(&stream_bytes)
            .iter()
            .any(|op| op.end == op_end && (op.keyword == b"Tj" || op.keyword == b"TJ"));
        if found {
            Ok(())
        } else {
            Err(R::OperatorNotFound)
        }
    }

    /// Adds a clickable URL link annotation to this page.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    /// The annotation has no visible border; the clickable area is invisible in
    /// normal view but interactive in PDF viewers. The link is written into the
    /// PDF object graph immediately — it does not require a `save()` call to take
    /// effect, but it will be included in the saved output.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let mut doc = Document::from_file("report.pdf")?;
    /// // Clickable "website" label at the bottom of page 1
    /// doc.page(1)?.add_link_url([72.0, 40.0, 200.0, 20.0], "https://example.com")?;
    /// doc.save("report_linked.pdf")?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if `url` is empty or coordinates contain NaN/Infinity.
    ///
    /// # Security note
    /// The `url` string is written verbatim into the PDF `/URI` action. Do **not** pass
    /// user-supplied strings without validation: `javascript:`, `data:`, and `file://`
    /// URIs are accepted by the PDF spec but may be exploited by a malicious caller.
    pub fn add_link_url(&mut self, rect: [f32; 4], url: &str) -> Result<()> {
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "add_link_url")?;
        if url.is_empty() {
            return Err(Error::InvalidInput("url must not be empty".into()));
        }
        let mut action = Dictionary::new();
        action.set("Type", Object::Name(b"Action".to_vec()));
        action.set("S", Object::Name(b"URI".to_vec()));
        action.set(
            "URI",
            Object::String(url.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );

        let mut d = build_link_annot_base(rect);
        d.set("A", Object::Dictionary(action));
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    /// Adds an internal link annotation that navigates to a specific page.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    /// `target_page` is the 1-indexed destination page; clicking the annotation
    /// jumps to the top of that page.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let mut doc = Document::from_file("report.pdf")?;
    /// // Table-of-contents entry on page 1 that links to page 5
    /// doc.page(1)?.add_link_internal([72.0, 700.0, 300.0, 14.0], 5)?;
    /// doc.save("report_with_toc.pdf")?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::PageNotFound`] if `target_page` is out of range, or
    /// [`Error::InvalidInput`] if coordinates contain NaN/Infinity.
    pub fn add_link_internal(&mut self, rect: [f32; 4], target_page: u32) -> Result<()> {
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "add_link_internal")?;
        let page_ids = self.doc.inner.get_pages();
        let target_id = page_ids
            .get(&target_page)
            .copied()
            .ok_or(Error::PageNotFound(target_page))?;

        let dest = Object::Array(vec![
            Object::Reference(target_id),
            Object::Name(b"XYZ".to_vec()),
            Object::Null,
            Object::Null,
            Object::Null,
        ]);
        let mut d = build_link_annot_base(rect);
        d.set("Dest", dest);
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    // -----------------------------------------------------------------------
    // Markup annotations (Highlight / Underline / StrikeOut)
    // -----------------------------------------------------------------------

    /// Adds a highlight annotation over the given area.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points. `color` is an RGB
    /// triple in `0.0..=1.0`; a typical yellow highlight is `[1.0, 1.0, 0.0]`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if any coordinate is NaN/Infinity.
    pub fn add_highlight(&mut self, rect: [f32; 4], color: impl Into<Color>) -> Result<()> {
        let color = color.into();
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "add_highlight")?;
        check_positive_size(rect[2], rect[3], "add_highlight")?;
        let d = build_markup_annot(b"Highlight", rect, color);
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    /// Adds an underline annotation under the given area.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if any coordinate is NaN/Infinity.
    pub fn add_underline(&mut self, rect: [f32; 4], color: impl Into<Color>) -> Result<()> {
        let color = color.into();
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "add_underline")?;
        check_positive_size(rect[2], rect[3], "add_underline")?;
        let d = build_markup_annot(b"Underline", rect, color);
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    /// Adds a strikeout (strikethrough) annotation over the given area.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if any coordinate is NaN/Infinity.
    pub fn add_strikeout(&mut self, rect: [f32; 4], color: impl Into<Color>) -> Result<()> {
        let color = color.into();
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "add_strikeout")?;
        check_positive_size(rect[2], rect[3], "add_strikeout")?;
        let d = build_markup_annot(b"StrikeOut", rect, color);
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    /// Adds a squiggly (wavy underline) annotation under the given area.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if any coordinate is NaN/Infinity.
    pub fn add_squiggly(&mut self, rect: [f32; 4], color: impl Into<Color>) -> Result<()> {
        let color = color.into();
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "add_squiggly")?;
        check_positive_size(rect[2], rect[3], "add_squiggly")?;
        let d = build_markup_annot(b"Squiggly", rect, color);
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    /// Permanently blacks out the given area with a PDF Redact annotation.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    ///
    /// This adds a standard `/Redact` annotation with a solid black appearance stream.
    /// Note: this does NOT scrub underlying text or images from content streams —
    /// it renders a black box on screen and in print. To permanently remove
    /// underlying content, the document must be processed by a compliant
    /// PDF reader's "Apply Redactions" command.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if coordinates are NaN/Infinity or
    /// width/height are non-positive.
    pub fn redact(&mut self, rect: [f32; 4]) -> Result<()> {
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "redact")?;
        check_positive_size(rect[2], rect[3], "redact")?;

        let x1 = rect[0];
        let y1 = rect[1];
        let x2 = rect[0] + rect[2];
        let y2 = rect[1] + rect[3];
        let w = rect[2];
        let h = rect[3];

        // Build the appearance stream content: solid black rectangle fill.
        let ap_content = format!("q\n0 0 0 rg\n0 0 {:.4} {:.4} re\nf\nQ\n", w, h);

        // Build the appearance stream XObject dictionary.
        let mut ap_dict = Dictionary::new();
        ap_dict.set("Type", Object::Name(b"XObject".to_vec()));
        ap_dict.set("Subtype", Object::Name(b"Form".to_vec()));
        ap_dict.set(
            "BBox",
            Object::Array(vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(w),
                Object::Real(h),
            ]),
        );
        ap_dict.set("Resources", Object::Dictionary(Dictionary::new()));

        // Create the appearance stream object.
        let ap_stream = Stream::new(ap_dict, ap_content.into_bytes());
        let ap_id = self.doc.inner.add_object(Object::Stream(ap_stream));

        // Build the /Redact annotation dictionary.
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"Annot".to_vec()));
        d.set("Subtype", Object::Name(b"Redact".to_vec()));
        d.set(
            "Rect",
            Object::Array(vec![
                Object::Real(x1),
                Object::Real(y1),
                Object::Real(x2),
                Object::Real(y2),
            ]),
        );
        // Interior color: solid black (0, 0, 0 in RGB).
        d.set(
            "IC",
            Object::Array(vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(0.0),
            ]),
        );
        // Appearance stream.
        let mut ap = Dictionary::new();
        ap.set("N", Object::Reference(ap_id));
        d.set("AP", Object::Dictionary(ap));
        // No visible border.
        d.set(
            "Border",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(0),
            ]),
        );
        // Print flag.
        d.set("F", Object::Integer(4));

        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    /// Adds a text (sticky-note) annotation at the given point.
    ///
    /// `point` is `[x, y]` in PDF points (origin: bottom-left). The icon
    /// appears at the given position; viewers typically display a 20×20 pt icon.
    /// `contents` is the note body (Unicode, UTF-16BE encoded in the PDF).
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if any coordinate is NaN/Infinity.
    pub fn add_sticky_note(&mut self, point: [f32; 2], contents: &str) -> Result<()> {
        check_finite(&[point[0], point[1]], "add_sticky_note")?;
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"Annot".to_vec()));
        d.set("Subtype", Object::Name(b"Text".to_vec()));
        d.set(
            "Rect",
            Object::Array(vec![
                Object::Real(point[0]),
                Object::Real(point[1]),
                Object::Real(point[0] + 20.0),
                Object::Real(point[1] + 20.0),
            ]),
        );
        d.set("Contents", pdf_text_string(contents));
        d.set("Open", Object::Boolean(false));
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    // -----------------------------------------------------------------------

    /// Overlays multi-line visible text within a bounding box.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    /// Text wraps at word boundaries for Latin text, or at any character for CJK.
    /// Lines outside the box bounds are silently clipped.
    /// `line_height` sets the vertical distance between baselines; pass `0.0` to use
    /// `font_size * 1.2`.
    ///
    /// Equivalent to `add_text_box_aligned(..., VerticalAlign::Top)`.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_bytes(&[])?;
    /// # let font = doc.embed_font(&[])?;
    /// // Fill a 300pt-wide column with black text at 11pt, auto line-height
    /// doc.page(1)?.add_text_box(
    ///     "This is a long sentence that will wrap automatically.",
    ///     font,
    ///     [72.0, 400.0, 300.0, 200.0],
    ///     11.0,
    ///     [0.0, 0.0, 0.0],
    ///     0.0,
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::InvalidFont`] if `font` is not registered on this document,
    /// or [`Error::FontParse`] if the font bytes cannot be parsed.
    pub fn add_text_box(
        &mut self,
        text: &str,
        font: FontHandle,
        rect: [f32; 4],
        font_size: f32,
        color: impl Into<Color>,
        line_height: f32,
    ) -> Result<()> {
        let color = color.into();
        self.add_text_box_aligned(
            text,
            font,
            rect,
            font_size,
            color,
            line_height,
            VerticalAlign::Top,
        )
    }

    /// Overlays multi-line visible text within a bounding box with explicit vertical alignment.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    /// Text wraps at word boundaries for Latin text, or at any character for CJK.
    /// Lines outside the box bounds are silently clipped (top and bottom).
    /// `line_height` sets the vertical distance between baselines; pass `0.0` to use
    /// `font_size * 1.2`.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::{Document, VerticalAlign};
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_bytes(&[])?;
    /// # let font = doc.embed_font(&[])?;
    /// // Vertically center a label inside a 100pt-tall cell
    /// doc.page(1)?.add_text_box_aligned(
    ///     "Centered",
    ///     font,
    ///     [72.0, 350.0, 200.0, 100.0],
    ///     12.0,
    ///     [0.0, 0.0, 0.0],
    ///     0.0,
    ///     VerticalAlign::Center,
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::InvalidFont`] if `font` is not registered on this document,
    /// or [`Error::FontParse`] if the font bytes cannot be parsed.
    #[allow(clippy::too_many_arguments)]
    pub fn add_text_box_aligned(
        &mut self,
        text: &str,
        font: FontHandle,
        rect: [f32; 4],
        font_size: f32,
        color: impl Into<Color>,
        line_height: f32,
        align: VerticalAlign,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[rect[0], rect[1], rect[2], rect[3], font_size, line_height],
            "add_text_box_aligned",
        )?;
        if rect[2] <= 0.0 || rect[3] <= 0.0 {
            return Ok(());
        }

        let raw = self
            .doc
            .raw_fonts
            .get(font.0 as usize)
            .ok_or(Error::InvalidFont(font.0))?;
        let face = Face::parse(&raw.ttf_bytes, 0).map_err(|e| Error::FontParse(e.to_string()))?;

        let box_width = rect[2];
        let effective_lh = if line_height <= 0.0 {
            font_size * 1.2
        } else {
            line_height
        };

        let mut all_lines: Vec<String> = Vec::new();
        for paragraph in text.split('\n') {
            all_lines.extend(wrap_paragraph(paragraph, &face, font_size, box_width));
        }

        let n = all_lines.len() as f32;
        let start_y = match align {
            VerticalAlign::Top => rect[1] + rect[3] - font_size,
            VerticalAlign::Bottom => rect[1] + (n - 1.0) * effective_lh,
            VerticalAlign::Center => {
                rect[1] + rect[3] / 2.0 + ((n - 1.0) * effective_lh - font_size) / 2.0
            }
        };
        let top = rect[1] + rect[3];
        let bottom = rect[1];

        for (i, line) in all_lines.iter().enumerate() {
            let y = start_y - i as f32 * effective_lh;
            if y > top || y < bottom {
                continue;
            }
            self.push_text(PendingText {
                font,
                text: line.clone(),
                x: rect[0],
                y,
                font_size,
                render_mode: 0,
                color,
                opacity: 1.0,
                rotation_degrees: 0.0,
                bold: false,
                italic: false,
                char_spacing: 0.0,
            });
        }
        Ok(())
    }

    /// Returns the page dimensions in PDF points as `(width, height)`.
    ///
    /// Reads the `/MediaBox` entry directly from the page dictionary.
    /// Standard page sizes:
    ///
    /// | Format | Width (pt) | Height (pt) |
    /// |--------|-----------|------------|
    /// | A4     | 595       | 842        |
    /// | Letter | 612       | 792        |
    /// | A3     | 842       | 1190       |
    ///
    /// # Errors
    /// Returns [`Error::Pdf`] if the page has no `/MediaBox` entry (rare but
    /// possible for pages that inherit `/MediaBox` from a parent node).
    pub fn size(&self) -> Result<(f32, f32)> {
        // Walk up the page tree (max 32 hops) to find an inherited MediaBox.
        let mut current_id = self.page_id;
        for _ in 0..32 {
            let (media_box_opt, parent_opt) = {
                let obj = self.doc.inner.get_object(current_id)?;
                let dict = obj.as_dict()?;
                (
                    dict.get(b"MediaBox").ok().cloned(),
                    dict.get(b"Parent").ok().cloned(),
                )
            };
            if let Some(mb) = media_box_opt {
                let arr = mb.as_array()?;
                if arr.len() < 4 {
                    return Err(Error::Pdf(lopdf::Error::DictKey("MediaBox".to_string())));
                }
                let get = |i: usize| -> f32 {
                    match &arr[i] {
                        lopdf::Object::Integer(v) => *v as f32,
                        lopdf::Object::Real(v) => *v,
                        _ => 0.0,
                    }
                };
                return Ok((get(2) - get(0), get(3) - get(1)));
            }
            match parent_opt {
                Some(Object::Reference(id)) => current_id = id,
                _ => break,
            }
        }
        Err(Error::Pdf(lopdf::Error::DictKey("MediaBox".to_string())))
    }

    // -----------------------------------------------------------------------
    // Page boxes (CropBox, MediaBox, TrimBox, BleedBox, ArtBox)
    // -----------------------------------------------------------------------

    /// Returns the `/CropBox` of this page in `[x, y, width, height]` format (PDF points).
    ///
    /// Returns `None` when no `/CropBox` is set on this page (the visible area is
    /// then determined by the [`MediaBox`](Self::media_box)).
    pub fn crop_box(&self) -> Result<Option<[f32; 4]>> {
        read_page_box(&self.doc.inner, self.page_id, b"CropBox")
    }

    /// Sets the `/CropBox` of this page. The crop box clips the visible area.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if coordinates contain NaN/Infinity.
    pub fn set_crop_box(&mut self, rect: [f32; 4]) -> Result<()> {
        check_finite(&rect, "set_crop_box")?;
        set_page_box(&mut self.doc.inner, self.page_id, b"CropBox", rect)
    }

    /// Returns the `/MediaBox` of this page in `[x, y, width, height]` format (PDF points).
    ///
    /// Walks up the page tree to find an inherited value, like [`size`](Self::size) does.
    pub fn media_box(&self) -> Result<[f32; 4]> {
        let mut current_id = self.page_id;
        for _ in 0..32 {
            let (mb_opt, parent_opt) = {
                let dict = self.doc.inner.get_object(current_id)?.as_dict()?;
                (
                    dict.get(b"MediaBox").ok().cloned(),
                    dict.get(b"Parent").ok().cloned(),
                )
            };
            if let Some(mb) = mb_opt {
                return parse_box_array(&mb);
            }
            match parent_opt {
                Some(Object::Reference(id)) => current_id = id,
                _ => break,
            }
        }
        Err(Error::Pdf(lopdf::Error::DictKey("MediaBox".to_string())))
    }

    /// Overrides the `/MediaBox` of this page.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if coordinates contain NaN/Infinity.
    pub fn set_media_box(&mut self, rect: [f32; 4]) -> Result<()> {
        check_finite(&rect, "set_media_box")?;
        set_page_box(&mut self.doc.inner, self.page_id, b"MediaBox", rect)
    }

    /// Returns the `/TrimBox` of this page, or `None` if unset.
    pub fn trim_box(&self) -> Result<Option<[f32; 4]>> {
        read_page_box(&self.doc.inner, self.page_id, b"TrimBox")
    }

    /// Sets the `/TrimBox` of this page (intended print area after trimming).
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if coordinates contain NaN/Infinity.
    pub fn set_trim_box(&mut self, rect: [f32; 4]) -> Result<()> {
        check_finite(&rect, "set_trim_box")?;
        set_page_box(&mut self.doc.inner, self.page_id, b"TrimBox", rect)
    }

    /// Returns the `/BleedBox` of this page, or `None` if unset.
    pub fn bleed_box(&self) -> Result<Option<[f32; 4]>> {
        read_page_box(&self.doc.inner, self.page_id, b"BleedBox")
    }

    /// Sets the `/BleedBox` of this page (area for bleed in print production).
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if coordinates contain NaN/Infinity.
    pub fn set_bleed_box(&mut self, rect: [f32; 4]) -> Result<()> {
        check_finite(&rect, "set_bleed_box")?;
        set_page_box(&mut self.doc.inner, self.page_id, b"BleedBox", rect)
    }

    // -----------------------------------------------------------------------

    fn push_op(&mut self, op: PendingOp) {
        let page_id = self.page_id;
        match self.doc.pending.iter_mut().find(|p| p.page_id == page_id) {
            Some(p) => p.ops.push(op),
            None => self.doc.pending.push(PendingPage {
                page_id,
                ops: vec![op],
            }),
        }
    }

    fn push_text(&mut self, text_op: PendingText) {
        self.push_op(PendingOp::Text(text_op));
    }
}

// ---------------------------------------------------------------------------
// Page content transform: scale_page_content, resize_page_with_content
// ---------------------------------------------------------------------------
impl<'doc> PageHandle<'doc> {
    /// Scales all existing content on this page by inserting a `cm` (Concatenate Matrix)
    /// operator as a new leading content stream.
    ///
    /// `scale_x` and `scale_y` are multipliers applied to the X and Y axes respectively.
    /// Use equal values for uniform scaling (e.g. `1.414` to scale A4 content to A3).
    ///
    /// This operates on already-written content streams only. Pending text/draw operations
    /// that have not yet been flushed (i.e. `save()` has not been called) are not affected
    /// by this call — they are written after `cm` is applied and will also be scaled.
    ///
    /// **Annotations** (links, highlights, form fields) have their own coordinates and
    /// are **not** scaled by this call.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if called after [`save`](Document::save), or if
    /// either scale value is non-positive or non-finite.
    pub fn scale_page_content(&mut self, scale_x: f32, scale_y: f32) -> Result<()> {
        if self.doc.finalized {
            return Err(Error::InvalidInput(
                "scale_page_content after save() is not supported".into(),
            ));
        }
        check_finite(&[scale_x, scale_y], "scale_page_content")?;
        if scale_x <= 0.0 || scale_y <= 0.0 {
            return Err(Error::InvalidInput("scale values must be positive".into()));
        }
        let cm_bytes = format!("{scale_x:.4} 0 0 {scale_y:.4} 0 0 cm\n").into_bytes();
        let cm_stream = Stream::new(Dictionary::new(), cm_bytes);
        let cm_id = self.doc.inner.add_object(Object::Stream(cm_stream));
        prepend_to_contents(&mut self.doc.inner, self.page_id, cm_id)
    }

    /// Resizes the page and scales all existing content to fit the new dimensions.
    ///
    /// This is a convenience wrapper that:
    /// 1. Reads the current page size via [`size()`](Self::size).
    /// 2. Calls [`scale_page_content`](Self::scale_page_content) with
    ///    `scale_x = new_width / current_width` and `scale_y = new_height / current_height`.
    /// 3. Updates the MediaBox to the new dimensions via [`set_media_box`](Self::set_media_box).
    ///
    /// The CropBox (if any) is removed so the new MediaBox defines the visible area.
    ///
    /// **Annotations** are **not** repositioned.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if called after [`save`](Document::save) or if the
    /// new dimensions are non-positive or non-finite.
    pub fn resize_page_with_content(&mut self, new_width: f32, new_height: f32) -> Result<()> {
        if self.doc.finalized {
            return Err(Error::InvalidInput(
                "resize_page_with_content after save() is not supported".into(),
            ));
        }
        check_finite(&[new_width, new_height], "resize_page_with_content")?;
        check_positive_size(new_width, new_height, "resize_page_with_content")?;
        let (cur_w, cur_h) = self.size()?;
        if cur_w <= 0.0 || cur_h <= 0.0 {
            return Err(Error::InvalidInput(
                "current page size is zero or negative".into(),
            ));
        }
        let scale_x = new_width / cur_w;
        let scale_y = new_height / cur_h;
        self.scale_page_content(scale_x, scale_y)?;
        // Remove CropBox so the new MediaBox controls the visible area.
        self.doc
            .inner
            .get_object_mut(self.page_id)?
            .as_dict_mut()?
            .remove(b"CropBox");
        self.set_media_box([0.0, 0.0, new_width, new_height])
    }
}

// ---------------------------------------------------------------------------
// draw feature: DebugOverlayOptions, add_rect, add_fit_debug_overlay, …
// ---------------------------------------------------------------------------

/// Display options for [`PageHandle::add_fit_debug_overlay`].
///
/// Each color is `Some([r, g, b])` in `0.0..=1.0`.  Setting a field to `None`
/// skips that overlay layer entirely.
#[cfg(feature = "draw")]
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct DebugOverlayOptions {
    /// Stroke color for the source text bounding box (`region.source_bbox`).
    /// Default: blue `[0.2, 0.6, 1.0]`.
    pub source_box_color: Option<Color>,
    /// Stroke color for the planned placement box (`fit.used_rect`).
    /// Default: green `[0.1, 0.8, 0.1]`.
    pub placed_box_color: Option<Color>,
    /// Stroke color for collision overlap rectangles.
    /// Default: orange `[1.0, 0.55, 0.0]`.
    pub collision_box_color: Option<Color>,
    /// Stroke color for text overflow rectangles.
    /// Default: red `[1.0, 0.1, 0.1]`.
    pub overflow_box_color: Option<Color>,
    /// Stroke color for text/image overlap rectangles.
    /// Default: purple `[0.6, 0.2, 0.9]`.
    pub image_overlap_box_color: Option<Color>,
    /// Stroke color for accepted shrink rectangles.
    /// Default: blue `[0.2, 0.4, 1.0]`.
    pub accepted_shrink_box_color: Option<Color>,
    /// Stroke color for source/placed bbox drift rectangles.
    /// Default: orange `[1.0, 0.55, 0.0]`.
    pub bbox_drift_box_color: Option<Color>,
    /// Stroke line width in PDF points.  Default: `0.5`.
    pub line_width: f32,
}

#[cfg(feature = "draw")]
impl Default for DebugOverlayOptions {
    fn default() -> Self {
        Self {
            source_box_color: Some(Color::Rgb([0.2, 0.6, 1.0])),
            placed_box_color: Some(Color::Rgb([0.1, 0.8, 0.1])),
            collision_box_color: Some(Color::Rgb([1.0, 0.55, 0.0])),
            overflow_box_color: Some(Color::Rgb([1.0, 0.1, 0.1])),
            image_overlap_box_color: Some(Color::Rgb([0.6, 0.2, 0.9])),
            accepted_shrink_box_color: Some(Color::Rgb([0.2, 0.4, 1.0])),
            bbox_drift_box_color: Some(Color::Rgb([1.0, 0.55, 0.0])),
            line_width: 0.5,
        }
    }
}

#[cfg(feature = "draw")]
impl<'doc> PageHandle<'doc> {
    /// Overlays a filled rectangle on this page.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `opacity` is in `0.0` (fully transparent) to `1.0` (fully opaque).
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_bytes(&[])?;
    /// // Semi-transparent yellow highlight band, 14pt tall
    /// doc.page(1)?.add_rect([72.0, 690.0, 300.0, 14.0], [1.0, 1.0, 0.0], 0.4)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_rect(
        &mut self,
        rect: [f32; 4],
        color: impl Into<Color>,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(&[rect[0], rect[1], rect[2], rect[3], opacity], "add_rect")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Rect {
            rect,
            color,
            opacity,
        }));
        Ok(())
    }

    /// Overlays a stroked rectangle border (no fill) on this page.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `line_width` is the stroke width in PDF points.
    /// `opacity` is in `0.0..=1.0`.
    pub fn add_rect_stroke(
        &mut self,
        rect: [f32; 4],
        color: impl Into<Color>,
        line_width: f32,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[rect[0], rect[1], rect[2], rect[3], line_width, opacity],
            "add_rect_stroke",
        )?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::RectStroke {
            rect,
            color,
            line_width,
            opacity,
        }));
        Ok(())
    }

    /// Overlays a closed polygon on this page.
    ///
    /// `points` is a slice of `[x, y]` vertices in PDF points (origin: bottom-left).
    /// At least 2 points are required; fewer produce no output.
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `opacity` is in `0.0..=1.0`.
    /// `filled = true` fills the polygon. `stroke_width > 0` strokes the outline.
    /// Both can be active simultaneously (`B` operator).
    pub fn add_polygon(
        &mut self,
        points: &[[f32; 2]],
        color: impl Into<Color>,
        opacity: f32,
        filled: bool,
        stroke_width: f32,
    ) -> Result<()> {
        let color = color.into();
        {
            let coords: Vec<f32> = points.iter().flat_map(|p| p.iter().copied()).collect();
            check_finite(&coords, "add_polygon points")?;
        }
        check_finite(&[opacity, stroke_width], "add_polygon")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Polygon {
            points: points.to_vec(),
            color,
            opacity,
            filled,
            stroke_width,
        }));
        Ok(())
    }

    /// Overlays a stroked line segment on this page.
    ///
    /// `from` and `to` are endpoints in PDF points (origin: bottom-left).
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `line_width` is the stroke width in PDF points.
    /// `opacity` is in `0.0..=1.0`.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_bytes(&[])?;
    /// // Horizontal black rule at y=600, 1pt wide
    /// doc.page(1)?.add_line([72.0, 600.0], [520.0, 600.0], [0.0, 0.0, 0.0], 1.0, 1.0)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_line(
        &mut self,
        from: [f32; 2],
        to: [f32; 2],
        color: impl Into<Color>,
        line_width: f32,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[from[0], from[1], to[0], to[1], line_width, opacity],
            "add_line",
        )?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Line {
            from,
            to,
            color,
            width: line_width,
            opacity,
        }));
        Ok(())
    }

    /// Overlays a stroked open polyline (multi-segment path) on this page.
    ///
    /// `points` is a slice of `[x, y]` vertices in PDF points (origin: bottom-left).
    /// At least 2 points are required; fewer produce no output.
    /// Unlike [`add_polygon`](PageHandle::add_polygon), the path is left open (not closed).
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `line_width` is the stroke width in PDF points.
    /// `opacity` is in `0.0..=1.0`.
    pub fn add_polyline(
        &mut self,
        points: &[[f32; 2]],
        color: impl Into<Color>,
        line_width: f32,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        if points.len() < 2 {
            return Ok(());
        }
        {
            let coords: Vec<f32> = points.iter().flat_map(|p| p.iter().copied()).collect();
            check_finite(&coords, "add_polyline points")?;
        }
        check_finite(&[line_width, opacity], "add_polyline")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Polyline {
            points: points.to_vec(),
            color,
            width: line_width,
            opacity,
        }));
        Ok(())
    }

    /// Overlays an ellipse (or circle) on this page.
    ///
    /// `rect` is `[x, y, width, height]` — the bounding box of the ellipse in PDF points
    /// (origin: bottom-left). For a circle, set `width == height`.
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `opacity` is in `0.0` (transparent) to `1.0` (opaque).
    /// `filled = true` fills the ellipse. `stroke_width > 0` strokes the outline.
    /// Both can be active simultaneously (`B` operator).
    pub fn add_ellipse(
        &mut self,
        rect: [f32; 4],
        color: impl Into<Color>,
        opacity: f32,
        filled: bool,
        stroke_width: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[rect[0], rect[1], rect[2], rect[3], opacity, stroke_width],
            "add_ellipse",
        )?;
        if rect[2] <= 0.0 || rect[3] <= 0.0 {
            return Err(Error::InvalidInput(
                "add_ellipse: width and height must be positive".into(),
            ));
        }
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Ellipse {
            rect,
            color,
            opacity,
            filled,
            stroke_width,
        }));
        Ok(())
    }

    /// Overlays an open or closed path on this page.
    ///
    /// `points` is a slice of `[x, y]` vertices in PDF points (origin: bottom-left).
    /// At least 2 points are required; fewer produce no output.
    /// `closed = true` closes the path (`h`); `closed = false` leaves it open.
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `filled = true` fills the interior. `stroke_width > 0` strokes the outline.
    /// Both can be active simultaneously (`B` operator).
    /// `opacity` is in `0.0..=1.0`.
    pub fn add_path(
        &mut self,
        points: &[[f32; 2]],
        closed: bool,
        color: impl Into<Color>,
        filled: bool,
        stroke_width: f32,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        if points.len() < 2 {
            return Ok(());
        }
        {
            let coords: Vec<f32> = points.iter().flat_map(|p| p.iter().copied()).collect();
            check_finite(&coords, "add_path points")?;
        }
        check_finite(&[stroke_width, opacity], "add_path")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Path {
            points: points.to_vec(),
            closed,
            color,
            opacity,
            filled,
            stroke_width,
        }));
        Ok(())
    }

    /// Overlays visible text with opacity on this page.
    ///
    /// Like [`add_text`](PageHandle::add_text) but applies a uniform fill opacity
    /// via an ExtGState (`/ca`). `opacity` is in `0.0` (transparent) to `1.0` (opaque).
    pub fn add_text_with_opacity(
        &mut self,
        text: &str,
        font: FontHandle,
        position: [f32; 2],
        font_size: f32,
        color: impl Into<Color>,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[position[0], position[1], font_size, opacity],
            "add_text_with_opacity",
        )?;
        self.push_text(PendingText {
            font,
            text: text.to_owned(),
            x: position[0],
            y: position[1],
            font_size,
            render_mode: 0,
            color,
            opacity,
            rotation_degrees: 0.0,
            bold: false,
            italic: false,
            char_spacing: 0.0,
        });
        Ok(())
    }

    /// Overlays visible text with rotation and opacity on this page.
    ///
    /// `rotation_degrees` rotates the text counter-clockwise around `position`.
    /// Use `0.0` for horizontal text. Internally emits a PDF `Tm` text matrix when
    /// `rotation_degrees != 0.0`, enabling arbitrary angles including CJK watermarks.
    ///
    /// `opacity` is in `0.0` (transparent) to `1.0` (opaque).
    #[allow(clippy::too_many_arguments)]
    pub fn add_text_with_rotation(
        &mut self,
        text: &str,
        font: FontHandle,
        position: [f32; 2],
        font_size: f32,
        color: impl Into<Color>,
        opacity: f32,
        rotation_degrees: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[
                position[0],
                position[1],
                font_size,
                opacity,
                rotation_degrees,
            ],
            "add_text_with_rotation",
        )?;
        self.push_text(PendingText {
            font,
            text: text.to_owned(),
            x: position[0],
            y: position[1],
            font_size,
            render_mode: 0,
            color,
            opacity,
            rotation_degrees,
            bold: false,
            italic: false,
            char_spacing: 0.0,
        });
        Ok(())
    }

    /// Overlays multi-line visible text in a bounding box with opacity.
    ///
    /// Like [`add_text_box`](PageHandle::add_text_box) but applies a uniform fill opacity.
    /// `opacity` is in `0.0` (transparent) to `1.0` (opaque).
    #[allow(clippy::too_many_arguments)]
    pub fn add_text_box_with_opacity(
        &mut self,
        text: &str,
        font: FontHandle,
        rect: [f32; 4],
        font_size: f32,
        color: impl Into<Color>,
        line_height: f32,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[
                rect[0],
                rect[1],
                rect[2],
                rect[3],
                font_size,
                line_height,
                opacity,
            ],
            "add_text_box_with_opacity",
        )?;
        if rect[2] <= 0.0 || rect[3] <= 0.0 {
            return Ok(());
        }

        let raw = self
            .doc
            .raw_fonts
            .get(font.0 as usize)
            .ok_or(Error::InvalidFont(font.0))?;
        let face = Face::parse(&raw.ttf_bytes, 0).map_err(|e| Error::FontParse(e.to_string()))?;

        let box_width = rect[2];
        let effective_lh = if line_height <= 0.0 {
            font_size * 1.2
        } else {
            line_height
        };

        let mut all_lines: Vec<String> = Vec::new();
        for paragraph in text.split('\n') {
            all_lines.extend(wrap_paragraph(paragraph, &face, font_size, box_width));
        }

        let start_y = rect[1] + rect[3] - font_size;
        let top = rect[1] + rect[3];
        let bottom = rect[1];

        for (i, line) in all_lines.iter().enumerate() {
            let y = start_y - i as f32 * effective_lh;
            if y > top || y < bottom {
                continue;
            }
            self.push_text(PendingText {
                font,
                text: line.clone(),
                x: rect[0],
                y,
                font_size,
                render_mode: 0,
                color,
                opacity,
                rotation_degrees: 0.0,
                bold: false,
                italic: false,
                char_spacing: 0.0,
            });
        }
        Ok(())
    }

    /// Draws debug overlay rectangles on this page showing source boxes,
    /// placement boxes, and collision rectangles from a set of [`RegionFitPlan`]s.
    ///
    /// Useful for visually inspecting layout quality after a planning pass —
    /// e.g. load the original PDF, call `plan_text_for_regions_with_policy`, then
    /// call this method on the same page to produce an annotated copy.
    ///
    /// The overlay respects `opts.source_box_color`, `opts.placed_box_color`, and
    /// `opts.collision_box_color`.  Set any of them to `None` to skip that layer.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::{Document, DebugOverlayOptions};
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_bytes(&[])?;
    /// # let font = doc.embed_font(&[])?;
    /// # let regions = vec![];
    /// # let replacements = vec![];
    /// let plans = doc.plan_text_for_regions_with_policy(&regions, &replacements, font, &[])?;
    /// doc.page(1)?.add_fit_debug_overlay(&plans, DebugOverlayOptions::default())?;
    /// doc.save("debug.pdf")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_fit_debug_overlay(
        &mut self,
        fit_plans: &[crate::RegionFitPlan],
        opts: DebugOverlayOptions,
    ) -> Result<()> {
        let lw = opts.line_width;
        if !lw.is_finite() {
            return Err(crate::Error::InvalidInput(
                "DebugOverlayOptions::line_width must be finite".into(),
            ));
        }

        // Guard: only draw a rect if all four coordinates are finite and w/h are positive.
        let rect_ok = |r: [f32; 4]| r.iter().all(|v| v.is_finite()) && r[2] > 0.0 && r[3] > 0.0;

        for plan in fit_plans {
            if let Some(color) = opts.source_box_color {
                let r = plan.region.source_bbox;
                if rect_ok(r) {
                    self.add_rect_stroke(r, color, lw, 1.0)?;
                }
            }
            if let Some(color) = opts.placed_box_color {
                let r = plan.fit.used_rect;
                if rect_ok(r) {
                    self.add_rect_stroke(r, color, lw, 1.0)?;
                }
            }
        }

        // Draw collision rects — deduplicate by (index_a, index_b) across plans.
        if let Some(color) = opts.collision_box_color {
            let mut seen = std::collections::HashSet::new();
            for plan in fit_plans {
                for cc in &plan.collisions {
                    let key = (cc.collision.index_a, cc.collision.index_b);
                    if seen.insert(key) {
                        let r = cc.collision.overlap_rect;
                        if rect_ok(r) {
                            self.add_rect_stroke(r, color, lw, 1.0)?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Draws debug overlay rectangles from a page-level layout quality report.
    ///
    /// Issue colors default to: overflow red, collision orange, image overlap
    /// purple, accepted shrink blue, and bbox drift orange.  Set the corresponding
    /// [`DebugOverlayOptions`] field to `None` to skip an issue layer.
    pub fn add_layout_quality_debug_overlay(
        &mut self,
        quality: &crate::PageLayoutQuality,
        opts: DebugOverlayOptions,
    ) -> Result<()> {
        let lw = opts.line_width;
        if !lw.is_finite() {
            return Err(crate::Error::InvalidInput(
                "DebugOverlayOptions::line_width must be finite".into(),
            ));
        }

        let rect_ok = |r: [f32; 4]| r.iter().all(|v| v.is_finite()) && r[2] > 0.0 && r[3] > 0.0;

        for issue in &quality.issues {
            let color = match issue.kind {
                crate::LayoutIssueKind::TextOverflow => opts.overflow_box_color,
                crate::LayoutIssueKind::TextCollision => opts.collision_box_color,
                crate::LayoutIssueKind::ImageOverlap => opts.image_overlap_box_color,
                crate::LayoutIssueKind::BboxDrift => opts.bbox_drift_box_color,
                crate::LayoutIssueKind::AcceptedShrink => opts.accepted_shrink_box_color,
                // New variants: reuse overflow color for border/cell issues, collision for
                // baseline/size outliers. Fall through to None for any future variants.
                crate::LayoutIssueKind::TextVsTableBorder
                | crate::LayoutIssueKind::TableCellSpillover
                | crate::LayoutIssueKind::ClippedText => opts.overflow_box_color,
                crate::LayoutIssueKind::BaselineMismatch
                | crate::LayoutIssueKind::FontSizeOutlier => opts.collision_box_color,
            };
            if let (Some(rect), Some(color)) = (issue.rect, color)
                && rect_ok(rect)
            {
                self.add_rect_stroke(rect, color, lw, 1.0)?;
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// image feature: add_image, add_image_with_opacity
// ---------------------------------------------------------------------------
#[cfg(feature = "image")]
impl<'doc> PageHandle<'doc> {
    /// Overlays a raster image (JPEG or PNG) on this page at full opacity.
    ///
    /// `image_bytes` is the raw file content (JPEG or PNG).
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    ///
    /// PNG images with an alpha channel are composited against a white
    /// background. True transparency (PDF SMask) is planned for v0.3.
    pub fn add_image(&mut self, image_bytes: &[u8], rect: [f32; 4]) -> Result<()> {
        self.add_image_with_opacity(image_bytes, rect, 1.0)
    }

    /// Overlays a raster image with the given opacity.
    ///
    /// `opacity` is in `0.0` (fully transparent) to `1.0` (fully opaque).
    pub fn add_image_with_opacity(
        &mut self,
        image_bytes: &[u8],
        rect: [f32; 4],
        opacity: f32,
    ) -> Result<()> {
        check_finite(
            &[rect[0], rect[1], rect[2], rect[3], opacity],
            "add_image_with_opacity",
        )?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Image {
            bytes: image_bytes.to_vec(),
            rect,
            opacity,
        }));
        Ok(())
    }
}
