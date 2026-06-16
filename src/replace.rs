use std::collections::{BTreeMap, HashMap};

use crate::extract::{
    FontInfo, collect_fonts, collect_fonts_from_resources, collect_inherited_xobject_ids,
    decode_hex_bytes, is_pdf_delimiter, is_pdf_whitespace, page_content_streams,
    parse_literal_string, resolve_dict,
};
use crate::font::FontHandle;

pub(crate) struct TextReplaceOp {
    pub font: FontHandle,
    pub old_text: String,
    pub new_text: String,
}

pub(crate) struct ResolvedReplacement {
    pub old_text: String,
    pub new_text: String,
    pub new_pdf_font_name: Vec<u8>,
    pub char_to_gid: BTreeMap<char, u16>,
    pub gid_to_advance: BTreeMap<u16, u16>,
    pub units_per_em: u16,
}

pub(crate) struct TextReplacePreserveOp {
    pub old_text: String,
    pub new_text: String,
}

/// Parameters for text wrapping during resubset text replacement.
///
/// When populated, configures automatic line wrapping for replacement text
/// that exceeds the specified maximum width. Reserved for future use;
/// currently all wrap operations fall back to non-wrapped resubset.
#[allow(dead_code)] // Used in future wrap implementation
#[derive(Clone)]
pub(crate) struct WrapParams {
    pub font_bytes: Vec<u8>,
    pub line_height: f32,
    pub max_width: f32,
}

/// Text replacement operation using font subsetting with optional line wrapping.
///
/// Replaces `old_text` with `new_text`, expanding the PDF's embedded font
/// subset to include new characters. The `wrap` field reserves space for
/// future multi-line text wrapping support.
pub(crate) struct TextReplaceResubsetOp {
    pub old_text: String,
    pub new_text: String,
    pub font_bytes: Vec<u8>,
    pub wrap: Option<WrapParams>,
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Scans the existing content streams for `old_text` and returns the match count.
///
/// Matches text accumulated across consecutive Tj/TJ operators within the same
/// font context (cross-operator matching). When `new_text` is `Some`, each match
/// is also validated against the font's ToUnicode map.
pub(crate) fn count_matches_in_page(
    doc: &lopdf::Document,
    page_id: lopdf::ObjectId,
    old_text: &str,
    new_text: Option<&str>,
) -> crate::Result<usize> {
    if old_text.is_empty() {
        return Ok(0);
    }
    let existing_fonts = collect_fonts(doc, page_id);
    let streams = page_content_streams(doc, page_id);
    let mut total = 0usize;

    let pattern_chars: Vec<char> = old_text.chars().collect();
    let plen = pattern_chars.len();

    total += count_matches_in_raw_streams(
        &streams.iter().map(|v| v.as_slice()).collect::<Vec<_>>(),
        &existing_fonts,
        old_text,
        new_text,
        &pattern_chars,
        plen,
    )?;

    // Also scan Form XObject streams — Chrome/Skia PDFs place all text in XObjects
    // referenced from inherited /Resources rather than in the page content streams.
    total += count_matches_in_inherited_xobjects(doc, page_id, old_text, new_text, &pattern_chars, plen)?;

    Ok(total)
}

/// Count pattern matches inside Form XObjects discovered via inherited resources.
fn count_matches_in_inherited_xobjects(
    doc: &lopdf::Document,
    page_id: lopdf::ObjectId,
    old_text: &str,
    new_text: Option<&str>,
    pattern_chars: &[char],
    plen: usize,
) -> crate::Result<usize> {
    use lopdf::Object;
    let mut total = 0usize;

    for xobj_id in collect_inherited_xobject_ids(doc, page_id) {
        let Ok(xobj_obj) = doc.get_object(xobj_id) else { continue };
        let Ok(xobj_stream) = xobj_obj.as_stream() else { continue };
        let is_form = xobj_stream
            .dict
            .get(b"Subtype")
            .ok()
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
        let xobj_fonts = xobj_stream
            .dict
            .get(b"Resources")
            .ok()
            .and_then(|res_ref| resolve_dict(doc, res_ref))
            .map(|res_dict| collect_fonts_from_resources(doc, res_dict))
            .unwrap_or_default();
        if xobj_fonts.is_empty() {
            continue;
        }
        total += count_matches_in_raw_streams(
            &[content.as_slice()],
            &xobj_fonts,
            old_text,
            new_text,
            pattern_chars,
            plen,
        )?;
    }

    Ok(total)
}

/// Count pattern matches in a slice of raw content stream bytes using the given font map.
fn count_matches_in_raw_streams(
    streams: &[&[u8]],
    fonts: &HashMap<Vec<u8>, FontInfo>,
    old_text: &str,
    new_text: Option<&str>,
    pattern_chars: &[char],
    plen: usize,
) -> crate::Result<usize> {
    let mut total = 0usize;

    for &bytes in streams {
        let ops = parse_ops(bytes);
        let segments = collect_char_segments(&ops, fonts);

        for seg in &segments {
            let text: String = seg.chars.iter().map(|e| e.ch).collect();

            // SECURITY: Prevent ReDoS (algorithmic complexity attack).
            // Naive substring search is O(n*m); limit to prevent excessive CPU.
            const MAX_SEARCH_COMPLEXITY: usize = 100_000_000;
            let search_complexity = text.len().saturating_mul(old_text.len());
            if search_complexity > MAX_SEARCH_COMPLEXITY {
                return Err(crate::Error::InvalidInput(format!(
                    "text search complexity too high ({}*{} > {}); text or pattern too long",
                    text.len(),
                    old_text.len(),
                    MAX_SEARCH_COMPLEXITY
                )));
            }

            let mut pos = 0usize;
            while let Some(byte_idx) = text[pos..].find(old_text) {
                if let Some(new) = new_text {
                    validate_chars_in_font(new, &seg.font_name, fonts)?;
                }
                total += 1;
                pos += byte_idx + old_text.len();
            }
        }

        // Cross-Tf pass: count matches that span a font boundary.
        // Same-font matches are already counted above; the `first_font != last_font`
        // guard prevents double-counting.
        if plen > 0 {
            let cross_segs = collect_cross_tf_segments(&ops, fonts);
            for seg in &cross_segs {
                let text_chars: Vec<char> = seg.chars.iter().map(|e| e.ch).collect();
                let mut pos = 0usize;
                while pos + plen <= text_chars.len() {
                    if text_chars[pos..pos + plen] == pattern_chars[..] {
                        let char_end = pos + plen;
                        let first_font = &seg.chars[pos].font_name;
                        let last_font = &seg.chars[char_end - 1].font_name;
                        if first_font != last_font {
                            total += 1;
                        }
                        pos = char_end;
                    } else {
                        pos += 1;
                    }
                }
            }
        }
    }

    Ok(total)
}

fn validate_chars_in_font(
    text: &str,
    font_name: &[u8],
    existing_fonts: &std::collections::HashMap<Vec<u8>, FontInfo>,
) -> crate::Result<()> {
    let Some(fi) = existing_fonts.get(font_name) else {
        return Ok(());
    };
    let char_to_gid: std::collections::HashMap<char, u16> =
        fi.to_unicode.iter().map(|(&gid, &ch)| (ch, gid)).collect();
    let font_name_str = String::from_utf8_lossy(font_name).into_owned();
    for ch in text.chars() {
        let gid = match char_to_gid.get(&ch) {
            Some(&g) => g,
            None => {
                return Err(crate::Error::FontCharNotMapped {
                    ch,
                    font_name: font_name_str,
                });
            }
        };
        // Simple fonts use 1-byte encoding; gid must fit in u8
        if fi.bytes_per_char == 1 && gid > 255 {
            return Err(crate::Error::FontCharNotMapped {
                ch,
                font_name: font_name_str,
            });
        }
    }
    Ok(())
}

/// Rewrite all content streams for a page, applying the given replacements.
/// Returns `(rewritten_bytes, fonts_used)` where `fonts_used` is the set of
/// `new_pdf_font_name` values that were actually emitted into the rewritten stream.
pub(crate) fn rewrite_page_streams(
    doc: &lopdf::Document,
    page_id: lopdf::ObjectId,
    resolved: &[ResolvedReplacement],
) -> (Vec<u8>, std::collections::HashSet<Vec<u8>>) {
    let existing_fonts = collect_fonts(doc, page_id);
    let streams = page_content_streams(doc, page_id);
    let mut out = Vec::new();
    let mut fonts_used = std::collections::HashSet::new();
    for bytes in &streams {
        let (rewritten, used) = rewrite_content_stream(bytes, resolved, &existing_fonts);
        out.extend_from_slice(&rewritten);
        if !out.ends_with(b"\n") {
            out.push(b'\n');
        }
        fonts_used.extend(used);
    }
    (out, fonts_used)
}

/// Rewrite Form XObject content streams referenced via the page's inherited /Resources.
///
/// Chrome/Skia PDFs place all text inside Form XObjects (invoked by `Do` from the main
/// page stream) rather than in the page stream itself.  This function discovers those
/// XObjects, rewrites each one's content stream with the given replacements, and returns
/// `(xobj_id, new_content, fonts_used_in_that_xobj)` for every XObject that was modified.
///
/// The caller is responsible for:
/// - Writing `new_content` back into the XObject stream object.
/// - Registering any fonts in `fonts_used_in_that_xobj` inside the XObject's own
///   `/Resources/Font` dictionary (not the page's resources).
pub(crate) fn rewrite_form_xobject_streams(
    doc: &lopdf::Document,
    page_id: lopdf::ObjectId,
    resolved: &[ResolvedReplacement],
) -> Vec<(lopdf::ObjectId, Vec<u8>, std::collections::HashSet<Vec<u8>>)> {
    use lopdf::Object;

    let xobj_ids = collect_inherited_xobject_ids(doc, page_id);
    let mut results = Vec::new();

    for xobj_id in xobj_ids {
        let Ok(xobj_obj) = doc.get_object(xobj_id) else { continue };
        let Ok(xobj_stream) = xobj_obj.as_stream() else { continue };

        let is_form = xobj_stream
            .dict
            .get(b"Subtype")
            .ok()
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

        let xobj_fonts = xobj_stream
            .dict
            .get(b"Resources")
            .ok()
            .and_then(|res_ref| resolve_dict(doc, res_ref))
            .map(|res_dict| collect_fonts_from_resources(doc, res_dict))
            .unwrap_or_default();

        if xobj_fonts.is_empty() {
            continue;
        }

        let (new_content, fonts_used) = rewrite_content_stream(&content, resolved, &xobj_fonts);
        if !fonts_used.is_empty() {
            results.push((xobj_id, new_content, fonts_used));
        }
    }

    results
}

/// Rewrite all content streams for a page using the font already embedded in the PDF.
/// Returns the rewritten bytes, or `Err` if a character in any `new_text` is absent from
/// the font's ToUnicode mapping.
/// Emit a single Tj operator with optional width compensation via Td.
///
/// Helper to reduce duplication in Tj/Td emission patterns.
/// Emits: `<hex_str> Tj` followed by `delta 0 Td` if delta is significant.
fn emit_tj_with_width_delta(out: &mut Vec<u8>, text_hex: &[u8], width_delta: f32) {
    out.extend_from_slice(&encode_str_hex(text_hex));
    out.extend_from_slice(b" Tj\n");
    // SECURITY: Clamp width_delta to prevent NaN/Infinity in PDF output.
    let clamped_delta = width_delta.clamp(-1_000_000.0, 1_000_000.0);
    if clamped_delta.is_finite() && clamped_delta.abs() > 0.01 {
        push_number(out, clamped_delta);
        out.extend_from_slice(b" 0 Td\n");
    }
}

pub(crate) fn rewrite_page_streams_preserve_font(
    doc: &lopdf::Document,
    page_id: lopdf::ObjectId,
    replacements: &[TextReplacePreserveOp],
    wrap_params_by_old_text: Option<&HashMap<String, crate::replace::WrapParams>>,
) -> crate::Result<Vec<u8>> {
    let existing_fonts = collect_fonts(doc, page_id);
    let streams = page_content_streams(doc, page_id);
    let mut out = Vec::new();
    for bytes in &streams {
        let rewritten = rewrite_stream_preserve_font(
            bytes,
            replacements,
            &existing_fonts,
            wrap_params_by_old_text,
        )?;
        out.extend_from_slice(&rewritten);
        if !out.ends_with(b"\n") {
            out.push(b'\n');
        }
    }
    Ok(out)
}

fn rewrite_stream_preserve_font(
    bytes: &[u8],
    replacements: &[TextReplacePreserveOp],
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
    wrap_params_by_old_text: Option<&HashMap<String, crate::replace::WrapParams>>,
) -> crate::Result<Vec<u8>> {
    if replacements.is_empty() {
        return Ok(bytes.to_vec());
    }

    let ops = parse_ops(bytes);
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut last_copied = 0usize;

    // Pre-compute cross-op matches.
    let cross_op = find_cross_op_matches_preserve(&ops, replacements, existing_fonts);
    let mut op_role: HashMap<usize, (usize, u8)> = HashMap::new();
    for (co_idx, co) in cross_op.iter().enumerate() {
        op_role.insert(co.first_op, (co_idx, 0));
        for mid in co.first_op + 1..co.last_op {
            op_role.insert(mid, (co_idx, 1));
        }
        op_role.insert(co.last_op, (co_idx, 2));
    }

    let mut in_bt = false;
    let mut cur_font: Vec<u8> = Vec::new();
    let mut cur_size: f32 = 12.0;

    for (op_idx, op) in ops.iter().enumerate() {
        match op.keyword.as_slice() {
            b"BT" => {
                in_bt = true;
                cur_font.clear();
            }
            b"ET" => {
                in_bt = false;
            }
            b"Tf" if in_bt => {
                if let (Some(Operand::Name(name)), Some(Operand::Num(size))) =
                    (op.operands.first(), op.operands.get(1))
                {
                    cur_font = name.clone();
                    cur_size = *size;
                }
            }
            // Suppress horizontal Td/TD ops that fall inside a cross-op match region.
            b"Td" | b"TD" if in_bt => {
                if let Some(&(_co_idx, _role)) = op_role.get(&op_idx) {
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    last_copied = op.end;
                }
            }
            b"Tj" if in_bt => {
                // Cross-op match.
                if let Some(&(co_idx, role)) = op_role.get(&op_idx) {
                    let co = &cross_op[co_idx];
                    let r = &replacements[co.replacement_idx];
                    let fi = match existing_fonts.get(&co.font_name) {
                        Some(fi) => fi,
                        None => {
                            last_copied = op.end;
                            continue;
                        }
                    };
                    let char_to_gid: HashMap<char, u16> =
                        fi.to_unicode.iter().map(|(&gid, &ch)| (ch, gid)).collect();
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    match role {
                        0 => {
                            if !co.prefix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.prefix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            let new_bytes =
                                encode_chars_as_bytes(&r.new_text, &char_to_gid, fi.bytes_per_char);
                            out.extend_from_slice(&encode_str_hex(&new_bytes));
                            out.extend_from_slice(b" Tj\n");
                        }
                        1 => { /* middle: skip */ }
                        _ => {
                            if !co.suffix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.suffix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            let new_bytes =
                                encode_chars_as_bytes(&r.new_text, &char_to_gid, fi.bytes_per_char);
                            let orig_w = co.orig_width * co.font_size;
                            let new_w =
                                orig_width(&new_bytes, &co.font_name, co.font_size, existing_fonts);
                            let delta = orig_w - new_w;
                            if delta.abs() > 0.01 {
                                push_number(&mut out, delta);
                                out.extend_from_slice(b" 0 Td\n");
                            }
                        }
                    }
                    last_copied = op.end;
                    continue;
                }
                // Single-op match.
                let str_bytes = match op.operands.first() {
                    Some(Operand::Str(b)) => b.clone(),
                    _ => continue,
                };
                let decoded = decode_str(&str_bytes, &cur_font, existing_fonts);
                if let Some(r) = replacements.iter().find(|r| r.old_text == decoded) {
                    let fi = match existing_fonts.get(&cur_font) {
                        Some(fi) => fi,
                        None => continue,
                    };
                    let char_to_gid: HashMap<char, u16> =
                        fi.to_unicode.iter().map(|(&gid, &ch)| (ch, gid)).collect();
                    let font_name_str = String::from_utf8_lossy(&cur_font).into_owned();
                    for ch in r.new_text.chars() {
                        if !char_to_gid.contains_key(&ch) {
                            return Err(crate::Error::FontCharNotMapped {
                                ch,
                                font_name: font_name_str.clone(),
                            });
                        }
                    }
                    out.extend_from_slice(&bytes[last_copied..op.start]);

                    // Check if wrapping is requested for this replacement.
                    if let Some(wp) = wrap_params_by_old_text.and_then(|m| m.get(&r.old_text)) {
                        // Parse TTF font and wrap the replacement text.
                        let face = match ttf_parser::Face::parse(&wp.font_bytes, 0) {
                            Ok(f) => f,
                            Err(_) => {
                                // If font parsing fails, fall back to single-line replacement.
                                let new_bytes = encode_chars_as_bytes(
                                    &r.new_text,
                                    &char_to_gid,
                                    fi.bytes_per_char,
                                );
                                let orig_w =
                                    orig_width(&str_bytes, &cur_font, cur_size, existing_fonts);
                                let new_w =
                                    orig_width(&new_bytes, &cur_font, cur_size, existing_fonts);
                                let delta = orig_w - new_w;
                                emit_tj_with_width_delta(&mut out, &new_bytes, delta);
                                last_copied = op.end;
                                continue;
                            }
                        };

                        let lines =
                            crate::wrap_paragraph(&r.new_text, &face, cur_size, wp.max_width);
                        let orig_w = orig_width(&str_bytes, &cur_font, cur_size, existing_fonts);

                        // Emit each line as a separate Tj operator with Td between them.
                        for (i, line) in lines.iter().enumerate() {
                            let line_bytes =
                                encode_chars_as_bytes(line, &char_to_gid, fi.bytes_per_char);
                            out.extend_from_slice(&encode_str_hex(&line_bytes));
                            out.extend_from_slice(b" Tj\n");

                            // Move down after each line except the last.
                            if i < lines.len() - 1 {
                                out.extend_from_slice(b"0 ");
                                push_number(&mut out, -wp.line_height);
                                out.extend_from_slice(b" Td\n");
                            }
                        }

                        // Apply width compensation only to the final line.
                        // DESIGN: The horizontal space consumed by wrapped text is determined by the
                        // last line's width (lines prior to the last are on separate rows and don't
                        // affect the text cursor position for content following this replacement).
                        // We compare the original (unwrapped) text width against the final line width
                        // and emit a Td offset to adjust horizontal position after the replacement.
                        // This ensures that text following the replacement is positioned correctly
                        // relative to where the original single-line text would have ended.
                        if let Some(last_line) = lines.last() {
                            let last_bytes =
                                encode_chars_as_bytes(last_line, &char_to_gid, fi.bytes_per_char);
                            let last_w =
                                orig_width(&last_bytes, &cur_font, cur_size, existing_fonts);
                            let delta = orig_w - last_w;
                            if delta.abs() > 0.01 {
                                push_number(&mut out, delta);
                                out.extend_from_slice(b" 0 Td\n");
                            }
                        }
                    } else {
                        // Single-line replacement (no wrapping).
                        let new_bytes =
                            encode_chars_as_bytes(&r.new_text, &char_to_gid, fi.bytes_per_char);
                        let orig_w = orig_width(&str_bytes, &cur_font, cur_size, existing_fonts);
                        let new_w = orig_width(&new_bytes, &cur_font, cur_size, existing_fonts);
                        let delta = orig_w - new_w;
                        emit_tj_with_width_delta(&mut out, &new_bytes, delta);
                    }

                    last_copied = op.end;
                }
            }
            b"TJ" if in_bt => {
                if let Some(&(co_idx, role)) = op_role.get(&op_idx) {
                    let co = &cross_op[co_idx];
                    let r = &replacements[co.replacement_idx];
                    let fi = match existing_fonts.get(&co.font_name) {
                        Some(fi) => fi,
                        None => {
                            last_copied = op.end;
                            continue;
                        }
                    };
                    let char_to_gid: HashMap<char, u16> =
                        fi.to_unicode.iter().map(|(&gid, &ch)| (ch, gid)).collect();
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    match role {
                        0 => {
                            if !co.prefix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.prefix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            let new_bytes =
                                encode_chars_as_bytes(&r.new_text, &char_to_gid, fi.bytes_per_char);
                            out.extend_from_slice(&encode_str_hex(&new_bytes));
                            out.extend_from_slice(b" Tj\n");
                        }
                        1 => {}
                        _ => {
                            if !co.suffix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.suffix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            let new_bytes =
                                encode_chars_as_bytes(&r.new_text, &char_to_gid, fi.bytes_per_char);
                            let orig_w = co.orig_width * co.font_size;
                            let new_w =
                                orig_width(&new_bytes, &co.font_name, co.font_size, existing_fonts);
                            let delta = orig_w - new_w;
                            if delta.abs() > 0.01 {
                                push_number(&mut out, delta);
                                out.extend_from_slice(b" 0 Td\n");
                            }
                        }
                    }
                    last_copied = op.end;
                    continue;
                }
                // Single-op TJ match.
                let arr = match op.operands.first() {
                    Some(Operand::Array(a)) => a.clone(),
                    _ => continue,
                };
                let any_match = arr.iter().any(|elem| {
                    if let ArrElem::Str(b) = elem {
                        let decoded = decode_str(b, &cur_font, existing_fonts);
                        replacements.iter().any(|r| r.old_text == decoded)
                    } else {
                        false
                    }
                });
                if any_match {
                    let fi = match existing_fonts.get(&cur_font) {
                        Some(fi) => fi,
                        None => continue,
                    };
                    let char_to_gid: HashMap<char, u16> =
                        fi.to_unicode.iter().map(|(&gid, &ch)| (ch, gid)).collect();
                    let font_name_str = String::from_utf8_lossy(&cur_font).into_owned();
                    for elem in &arr {
                        if let ArrElem::Str(b) = elem {
                            let decoded = decode_str(b, &cur_font, existing_fonts);
                            if let Some(r) = replacements.iter().find(|r| r.old_text == decoded) {
                                for ch in r.new_text.chars() {
                                    if !char_to_gid.contains_key(&ch) {
                                        return Err(crate::Error::FontCharNotMapped {
                                            ch,
                                            font_name: font_name_str.clone(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    let fragment = emit_tj_array_preserve(
                        &arr,
                        replacements,
                        &cur_font,
                        cur_size,
                        existing_fonts,
                        fi,
                        &char_to_gid,
                    );
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    out.extend_from_slice(&fragment);
                    last_copied = op.end;
                }
            }
            _ => {}
        }
    }

    out.extend_from_slice(&bytes[last_copied..]);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Stream rewriter
// ---------------------------------------------------------------------------

/// Returns `(rewritten_bytes, fonts_used)`.
/// `fonts_used` contains the `new_pdf_font_name` values of replacements that were actually applied.
/// Supports replacements that span multiple consecutive Tj/TJ operators (cross-operator matching).
pub(crate) fn rewrite_content_stream(
    bytes: &[u8],
    replacements: &[ResolvedReplacement],
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
) -> (Vec<u8>, std::collections::HashSet<Vec<u8>>) {
    if replacements.is_empty() {
        return (bytes.to_vec(), std::collections::HashSet::new());
    }

    let ops = parse_ops(bytes);
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len() + bytes.len() / 4);
    let mut last_copied = 0usize;
    let mut fonts_used: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();

    // Pre-compute cross-op matches (first_op != last_op).
    let cross_op = find_cross_op_matches(&ops, replacements, existing_fonts);
    // Map each op_idx to (cross_op index, role: 0=first 1=middle 2=last).
    let mut op_role: HashMap<usize, (usize, u8)> = HashMap::new();
    for (co_idx, co) in cross_op.iter().enumerate() {
        op_role.insert(co.first_op, (co_idx, 0));
        for mid in co.first_op + 1..co.last_op {
            op_role.insert(mid, (co_idx, 1));
        }
        op_role.insert(co.last_op, (co_idx, 2));
    }

    // Pre-compute Tz scale for each cross-op match.
    // Tz compresses/expands the replacement text so it occupies the same horizontal
    // space as the original.  Only applied when 70% ≤ scale ≤ 130% to avoid
    // illegible compression; outside that range we fall back to Td compensation.
    let co_tz_scale: Vec<Option<f32>> = cross_op
        .iter()
        .map(|co| {
            let r = &replacements[co.replacement_idx];
            let orig_w = co.orig_width * co.font_size;
            let new_w = new_width(r, co.font_size);
            if new_w > 0.0 {
                let s = orig_w / new_w * 100.0;
                if (70.0..=130.0).contains(&s) { Some(s) } else { None }
            } else {
                None
            }
        })
        .collect();

    let mut in_bt = false;
    let mut cur_font: Vec<u8> = Vec::new();
    let mut cur_size: f32 = 12.0;

    for (op_idx, op) in ops.iter().enumerate() {
        match op.keyword.as_slice() {
            b"BT" => {
                in_bt = true;
                cur_font.clear();
            }
            b"ET" => {
                in_bt = false;
            }
            b"Tf" if in_bt => {
                if let (Some(Operand::Name(name)), Some(Operand::Num(size))) =
                    (op.operands.first(), op.operands.get(1))
                {
                    cur_font = name.clone();
                    cur_size = *size;
                }
                // Suppress intermediate Tf ops inside a cross-Tf match region.
                if let Some(&(_co_idx, role)) = op_role.get(&op_idx)
                    && role == 1
                {
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    last_copied = op.end;
                }
            }
            // Suppress horizontal Td/TD ops that fall inside a cross-op match region.
            // These are per-character advance operators common in Japanese PDFs.
            b"Td" | b"TD" if in_bt => {
                if let Some(&(_co_idx, _role)) = op_role.get(&op_idx) {
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    last_copied = op.end;
                }
            }
            b"Tj" if in_bt => {
                // Cross-op match handling takes priority.
                if let Some(&(co_idx, role)) = op_role.get(&op_idx) {
                    let co = &cross_op[co_idx];
                    let r = &replacements[co.replacement_idx];
                    let tz = co_tz_scale[co_idx];
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    match role {
                        0 => {
                            // First op: emit prefix + replacement (no suffix/comp yet).
                            if !co.prefix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.prefix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            emit_cross_op_replacement(&mut out, r, co, tz);
                            fonts_used.insert(r.new_pdf_font_name.clone());
                        }
                        1 => { /* middle: skip */ }
                        _ => {
                            // Last op: emit suffix + width compensation (Td only if no Tz).
                            if !co.suffix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.suffix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            if tz.is_none() {
                                let orig_w = co.orig_width * co.font_size;
                                let new_w = new_width(r, co.font_size);
                                let delta = orig_w - new_w;
                                if delta.abs() > 0.01 {
                                    push_number(&mut out, delta);
                                    out.extend_from_slice(b" 0 Td\n");
                                }
                            }
                        }
                    }
                    last_copied = op.end;
                    continue;
                }
                // Single-op match (existing logic).
                let str_bytes = match op.operands.first() {
                    Some(Operand::Str(b)) => b,
                    _ => continue,
                };
                let decoded = decode_str(str_bytes, &cur_font, existing_fonts);
                if let Some(r) = find_replacement(&decoded, replacements) {
                    let orig_w = orig_width(str_bytes, &cur_font, cur_size, existing_fonts);
                    let new_w = new_width(r, cur_size);
                    let delta = orig_w - new_w;
                    let fragment = emit_replacement(r, &cur_font, cur_size, delta);
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    out.extend_from_slice(&fragment);
                    last_copied = op.end;
                    fonts_used.insert(r.new_pdf_font_name.clone());
                }
            }
            b"TJ" if in_bt => {
                if let Some(&(co_idx, role)) = op_role.get(&op_idx) {
                    let co = &cross_op[co_idx];
                    let r = &replacements[co.replacement_idx];
                    let tz = co_tz_scale[co_idx];
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    match role {
                        0 => {
                            if !co.prefix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.prefix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            emit_cross_op_replacement(&mut out, r, co, tz);
                            fonts_used.insert(r.new_pdf_font_name.clone());
                        }
                        1 => { /* middle: skip */ }
                        _ => {
                            if !co.suffix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.suffix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            if tz.is_none() {
                                let orig_w = co.orig_width * co.font_size;
                                let new_w = new_width(r, co.font_size);
                                let delta = orig_w - new_w;
                                if delta.abs() > 0.01 {
                                    push_number(&mut out, delta);
                                    out.extend_from_slice(b" 0 Td\n");
                                }
                            }
                        }
                    }
                    last_copied = op.end;
                    continue;
                }
                // Single-op TJ match.
                let arr = match op.operands.first() {
                    Some(Operand::Array(a)) => a,
                    _ => continue,
                };
                let any_match = arr.iter().any(|elem| {
                    if let ArrElem::Str(b) = elem {
                        let decoded = decode_str(b, &cur_font, existing_fonts);
                        find_replacement(&decoded, replacements).is_some()
                    } else {
                        false
                    }
                });
                if any_match {
                    let (fragment, used) = emit_tj_array(
                        &arr.clone(),
                        replacements,
                        &cur_font,
                        cur_size,
                        existing_fonts,
                    );
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    out.extend_from_slice(&fragment);
                    last_copied = op.end;
                    fonts_used.extend(used);
                }
            }
            _ => {}
        }
    }

    out.extend_from_slice(&bytes[last_copied..]);
    (out, fonts_used)
}

/// Emit the font-switch + replacement-text part of a cross-operator replacement.
/// (Does NOT emit prefix, suffix, or width compensation — those are handled by the caller.)
///
/// If `tz_scale` is `Some(s)`, emits `s Tz` before the text and `100 Tz` after so the
/// replacement occupies exactly the same horizontal space as the original.  The caller
/// must then skip the normal `Td` width-compensation step.
fn emit_cross_op_replacement(
    out: &mut Vec<u8>,
    r: &ResolvedReplacement,
    co: &CrossOpMatch,
    tz_scale: Option<f32>,
) {
    if let Some(scale) = tz_scale {
        push_number(out, scale);
        out.extend_from_slice(b" Tz\n");
    }
    out.push(b'/');
    out.extend_from_slice(&r.new_pdf_font_name);
    out.push(b' ');
    push_number(out, co.font_size);
    out.extend_from_slice(b" Tf\n");
    out.extend_from_slice(&gids_hex(r));
    out.extend_from_slice(b" Tj\n");
    out.push(b'/');
    out.extend_from_slice(&co.font_name);
    out.push(b' ');
    push_number(out, co.font_size);
    out.extend_from_slice(b" Tf\n");
    if tz_scale.is_some() {
        out.extend_from_slice(b"100 Tz\n");
    }
}

// ---------------------------------------------------------------------------
// Replacement emission
// ---------------------------------------------------------------------------

fn emit_replacement(
    r: &ResolvedReplacement,
    orig_font_name: &[u8],
    font_size: f32,
    width_delta: f32,
) -> Vec<u8> {
    let mut out = Vec::new();
    // Switch to replacement font
    out.push(b'/');
    out.extend_from_slice(&r.new_pdf_font_name);
    out.push(b' ');
    push_number(&mut out, font_size);
    out.extend_from_slice(b" Tf\n");
    // Replacement text
    out.extend_from_slice(&gids_hex(r));
    out.extend_from_slice(b" Tj\n");
    // Restore original font
    out.push(b'/');
    out.extend_from_slice(orig_font_name);
    out.push(b' ');
    push_number(&mut out, font_size);
    out.extend_from_slice(b" Tf\n");
    // Width compensation
    if width_delta.abs() > 0.01 {
        push_number(&mut out, width_delta);
        out.extend_from_slice(b" 0 Td\n");
    }
    out
}

/// Rewrite a TJ array, replacing matching string elements.
/// Returns `(rewritten_bytes, fonts_used)`.
fn emit_tj_array(
    arr: &[ArrElem],
    replacements: &[ResolvedReplacement],
    orig_font_name: &[u8],
    font_size: f32,
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
) -> (Vec<u8>, std::collections::HashSet<Vec<u8>>) {
    let mut out = Vec::new();
    let mut fonts_used: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();
    // pending_kern: accumulated position delta (in PDF points) to emit as Td before the next string
    let mut pending_kern: f32 = 0.0;

    for elem in arr {
        match elem {
            ArrElem::Num(k) => {
                // TJ kern: positive k → shift left (tighten); Td delta = -k/1000 * fs
                pending_kern += -k / 1000.0 * font_size;
            }
            ArrElem::Str(bytes) => {
                let decoded = decode_str(bytes, orig_font_name, existing_fonts);

                // Emit accumulated kerning as Td before this string
                if pending_kern.abs() > 0.01 {
                    push_number(&mut out, pending_kern);
                    out.extend_from_slice(b" 0 Td\n");
                    pending_kern = 0.0;
                }

                if let Some(r) = find_replacement(&decoded, replacements) {
                    let orig_w = orig_width(bytes, orig_font_name, font_size, existing_fonts);
                    let new_w = new_width(r, font_size);
                    // Font switch + replacement
                    out.push(b'/');
                    out.extend_from_slice(&r.new_pdf_font_name);
                    out.push(b' ');
                    push_number(&mut out, font_size);
                    out.extend_from_slice(b" Tf\n");
                    out.extend_from_slice(&gids_hex(r));
                    out.extend_from_slice(b" Tj\n");
                    // Restore original font
                    out.push(b'/');
                    out.extend_from_slice(orig_font_name);
                    out.push(b' ');
                    push_number(&mut out, font_size);
                    out.extend_from_slice(b" Tf\n");
                    // Accumulate width delta for next Td
                    pending_kern = orig_w - new_w;
                    fonts_used.insert(r.new_pdf_font_name.clone());
                } else {
                    // Re-encode original bytes as hex Tj
                    out.extend_from_slice(&encode_str_hex(bytes));
                    out.extend_from_slice(b" Tj\n");
                }
            }
        }
    }

    // Emit any trailing kern
    if pending_kern.abs() > 0.01 {
        push_number(&mut out, pending_kern);
        out.extend_from_slice(b" 0 Td\n");
    }

    (out, fonts_used)
}

// ---------------------------------------------------------------------------
// Width helpers
// ---------------------------------------------------------------------------

fn orig_width(
    bytes: &[u8],
    font_name: &[u8],
    font_size: f32,
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
) -> f32 {
    let Some(fi) = existing_fonts.get(font_name) else {
        return 0.0;
    };
    let mut total = 0.0f32;
    if fi.bytes_per_char == 2 && bytes.len().is_multiple_of(2) {
        for chunk in bytes.chunks(2) {
            let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
            total += fi.advance_width(gid) as f32 / 1000.0 * font_size;
        }
    } else {
        for &b in bytes {
            total += fi.advance_width(b as u16) as f32 / 1000.0 * font_size;
        }
    }
    total
}

fn new_width(r: &ResolvedReplacement, font_size: f32) -> f32 {
    let upm = r.units_per_em as f32;
    r.new_text
        .chars()
        .map(|ch| {
            let gid = *r.char_to_gid.get(&ch).unwrap_or(&0);
            *r.gid_to_advance.get(&gid).unwrap_or(&1000) as f32 * font_size / upm
        })
        .sum()
}

// ---------------------------------------------------------------------------
// Encode helpers
// ---------------------------------------------------------------------------

fn decode_str(
    bytes: &[u8],
    font_name: &[u8],
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
) -> String {
    let Some(fi) = existing_fonts.get(font_name) else {
        return String::new();
    };
    let mut text = String::new();
    if fi.bytes_per_char == 2 {
        if bytes.len().is_multiple_of(2) {
            for chunk in bytes.chunks(2) {
                let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
                if let Some(&ch) = fi.to_unicode.get(&gid) {
                    text.push(ch);
                }
            }
        }
    } else {
        for &b in bytes {
            if let Some(&ch) = fi.to_unicode.get(&(b as u16)) {
                text.push(ch);
            }
        }
    }
    text
}

fn find_replacement<'a>(
    text: &str,
    replacements: &'a [ResolvedReplacement],
) -> Option<&'a ResolvedReplacement> {
    if text.is_empty() {
        return None;
    }
    replacements.iter().find(|r| r.old_text == text)
}

fn gids_hex(r: &ResolvedReplacement) -> Vec<u8> {
    let mut out = vec![b'<'];
    for ch in r.new_text.chars() {
        let gid = *r.char_to_gid.get(&ch).unwrap_or(&0);
        let [hi, lo] = gid.to_be_bytes();
        out.push(hex_nibble(hi >> 4));
        out.push(hex_nibble(hi & 0xF));
        out.push(hex_nibble(lo >> 4));
        out.push(hex_nibble(lo & 0xF));
    }
    out.push(b'>');
    out
}

pub(crate) fn encode_str_hex(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![b'<'];
    for &b in bytes {
        out.push(hex_nibble(b >> 4));
        out.push(hex_nibble(b & 0xF));
    }
    out.push(b'>');
    out
}

fn hex_nibble(n: u8) -> u8 {
    b"0123456789ABCDEF"[n as usize]
}

fn encode_chars_as_bytes(
    text: &str,
    char_to_gid: &HashMap<char, u16>,
    bytes_per_char: u8,
) -> Vec<u8> {
    let mut out = Vec::new();
    for ch in text.chars() {
        let gid = char_to_gid.get(&ch).copied().unwrap_or(0);
        if bytes_per_char == 2 {
            out.extend_from_slice(&gid.to_be_bytes());
        } else {
            out.push(gid as u8);
        }
    }
    out
}

fn emit_tj_array_preserve(
    arr: &[ArrElem],
    replacements: &[TextReplacePreserveOp],
    font_name: &[u8],
    font_size: f32,
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
    fi: &FontInfo,
    char_to_gid: &HashMap<char, u16>,
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut pending_kern: f32 = 0.0;

    for elem in arr {
        match elem {
            ArrElem::Num(k) => {
                pending_kern += -k / 1000.0 * font_size;
            }
            ArrElem::Str(bytes) => {
                let decoded = decode_str(bytes, font_name, existing_fonts);
                if pending_kern.abs() > 0.01 {
                    push_number(&mut out, pending_kern);
                    out.extend_from_slice(b" 0 Td\n");
                    pending_kern = 0.0;
                }
                if let Some(r) = replacements.iter().find(|r| r.old_text == decoded) {
                    let new_bytes =
                        encode_chars_as_bytes(&r.new_text, char_to_gid, fi.bytes_per_char);
                    let orig_w = orig_width(bytes, font_name, font_size, existing_fonts);
                    let new_w = orig_width(&new_bytes, font_name, font_size, existing_fonts);
                    out.extend_from_slice(&encode_str_hex(&new_bytes));
                    out.extend_from_slice(b" Tj\n");
                    pending_kern += orig_w - new_w;
                } else {
                    out.extend_from_slice(&encode_str_hex(bytes));
                    out.extend_from_slice(b" Tj\n");
                }
            }
        }
    }

    if pending_kern.abs() > 0.01 {
        push_number(&mut out, pending_kern);
        out.extend_from_slice(b" 0 Td\n");
    }

    out
}

pub(crate) fn push_number(out: &mut Vec<u8>, v: f32) {
    let v = if v.is_finite() { v } else { 0.0 };
    if v.fract() == 0.0 && v.abs() < 1e9 {
        let s = format!("{}", v as i64);
        out.extend_from_slice(s.as_bytes());
    } else {
        let s = format!("{:.4}", v);
        let s = s.trim_end_matches('0');
        let s = if s.ends_with('.') {
            format!("{}0", s)
        } else {
            s.to_string()
        };
        out.extend_from_slice(s.as_bytes());
    }
}

// ---------------------------------------------------------------------------
// Cross-operator text accumulation and matching
// ---------------------------------------------------------------------------

/// A single decoded character with its location in the content stream.
pub(crate) struct CharEntry {
    pub ch: char,
    pub op_idx: usize,
    pub raw_bytes: Vec<u8>,
    pub font_name: Vec<u8>,
}

/// A text segment within a BT/ET block sharing the same font and size.
pub(crate) struct CharSegment {
    pub chars: Vec<CharEntry>,
    pub font_name: Vec<u8>,
    pub font_size: f32,
}

/// A replacement match (with new font) that spans multiple consecutive Tj/TJ operators.
/// A replacement match that spans multiple consecutive Tj/TJ operators.
/// Used by both the font-switching and font-preserving replacement paths.
struct CrossOpMatch {
    replacement_idx: usize,
    first_op: usize,
    last_op: usize,
    /// Raw bytes of chars in `first_op` that come before the match start.
    prefix_raw: Vec<u8>,
    /// Raw bytes of chars in `last_op` that come after the match end.
    suffix_raw: Vec<u8>,
    /// Sum of `advance_width(gid) / 1000` for each matched char (unscaled).
    orig_width: f32,
    font_size: f32,
    font_name: Vec<u8>,
}

/// Append decoded characters from `str_bytes` (a Tj/TJ string operand) into `char_buf`.
fn push_chars_from_bytes(
    char_buf: &mut Vec<CharEntry>,
    str_bytes: &[u8],
    op_idx: usize,
    font_name: &[u8],
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
) {
    let fi = match existing_fonts.get(font_name) {
        Some(fi) => fi,
        None => return,
    };
    if fi.bytes_per_char == 2 {
        if str_bytes.len().is_multiple_of(2) {
            for chunk in str_bytes.chunks(2) {
                let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
                if let Some(&ch) = fi.to_unicode.get(&gid) {
                    char_buf.push(CharEntry {
                        ch,
                        op_idx,
                        raw_bytes: chunk.to_vec(),
                        font_name: font_name.to_vec(),
                    });
                }
            }
        }
    } else {
        for &b in str_bytes {
            if let Some(&ch) = fi.to_unicode.get(&(b as u16)) {
                char_buf.push(CharEntry {
                    ch,
                    op_idx,
                    raw_bytes: vec![b],
                    font_name: font_name.to_vec(),
                });
            }
        }
    }
}

/// Split the content stream into per-Tf character segments within each BT/ET block.
/// The buffer resets on each BT and on every Tf operator.
pub(crate) fn collect_char_segments(
    ops: &[Op],
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
) -> Vec<CharSegment> {
    let mut segments: Vec<CharSegment> = Vec::new();
    let mut in_bt = false;
    let mut cur_font: Vec<u8> = Vec::new();
    let mut cur_size: f32 = 12.0;
    let mut cur_chars: Vec<CharEntry> = Vec::new();

    for (op_idx, op) in ops.iter().enumerate() {
        match op.keyword.as_slice() {
            b"BT" => {
                in_bt = true;
                cur_chars.clear();
                cur_font.clear();
            }
            b"ET" => {
                if in_bt && !cur_chars.is_empty() {
                    segments.push(CharSegment {
                        chars: std::mem::take(&mut cur_chars),
                        font_name: cur_font.clone(),
                        font_size: cur_size,
                    });
                }
                in_bt = false;
            }
            b"Tf" if in_bt => {
                if let (Some(Operand::Name(name)), Some(Operand::Num(size))) =
                    (op.operands.first(), op.operands.get(1))
                {
                    if !cur_chars.is_empty() {
                        segments.push(CharSegment {
                            chars: std::mem::take(&mut cur_chars),
                            font_name: cur_font.clone(),
                            font_size: cur_size,
                        });
                    }
                    cur_font = name.clone();
                    cur_size = *size;
                }
            }
            b"Tj" if in_bt => {
                if let Some(Operand::Str(str_bytes)) = op.operands.first() {
                    push_chars_from_bytes(
                        &mut cur_chars,
                        str_bytes,
                        op_idx,
                        &cur_font,
                        existing_fonts,
                    );
                }
            }
            b"TJ" if in_bt => {
                if let Some(Operand::Array(arr)) = op.operands.first() {
                    for elem in arr {
                        if let ArrElem::Str(b) = elem {
                            push_chars_from_bytes(
                                &mut cur_chars,
                                b,
                                op_idx,
                                &cur_font,
                                existing_fonts,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
    segments
}

/// Collect chars across Tf boundaries within each BT/ET block.
/// Unlike `collect_char_segments`, this function does NOT reset on `Tf` —
/// it merges all chars in a visual line into one segment regardless of font switches.
/// Breaks on: ET, Tm (absolute position reset), vertical Td/TD (|ty| >= 0.01).
/// Segment `font_name` = last active font (used for restore after replacement).
/// Segment `font_size` = first Tf's size in the segment (used for replacement instruction).
fn collect_cross_tf_segments(
    ops: &[Op],
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
) -> Vec<CharSegment> {
    let mut segments: Vec<CharSegment> = Vec::new();
    let mut in_bt = false;
    let mut cur_font: Vec<u8> = Vec::new();
    let mut cur_size: f32 = 12.0;
    let mut seg_first_size: f32 = 12.0;
    let mut cur_chars: Vec<CharEntry> = Vec::new();

    macro_rules! flush_seg {
        () => {
            if !cur_chars.is_empty() {
                segments.push(CharSegment {
                    chars: std::mem::take(&mut cur_chars),
                    font_name: cur_font.clone(),
                    font_size: seg_first_size,
                });
            }
        };
    }

    for (op_idx, op) in ops.iter().enumerate() {
        match op.keyword.as_slice() {
            b"BT" => {
                flush_seg!();
                in_bt = true;
                cur_chars.clear();
                cur_font.clear();
                seg_first_size = 12.0;
            }
            b"ET" => {
                flush_seg!();
                in_bt = false;
            }
            b"Tf" if in_bt => {
                if let (Some(Operand::Name(name)), Some(Operand::Num(size))) =
                    (op.operands.first(), op.operands.get(1))
                {
                    if cur_chars.is_empty() {
                        seg_first_size = *size;
                    }
                    cur_font = name.clone();
                    cur_size = *size;
                }
            }
            b"Tm" if in_bt => {
                flush_seg!();
                cur_font.clear();
                seg_first_size = 12.0;
            }
            b"Td" | b"TD" if in_bt => {
                let is_vertical = match (op.operands.first(), op.operands.get(1)) {
                    (Some(Operand::Num(_)), Some(Operand::Num(ty))) => ty.abs() >= 0.01,
                    _ => false,
                };
                if is_vertical {
                    flush_seg!();
                    seg_first_size = cur_size;
                }
            }
            b"Tj" if in_bt => {
                if let Some(Operand::Str(str_bytes)) = op.operands.first() {
                    push_chars_from_bytes(
                        &mut cur_chars,
                        str_bytes,
                        op_idx,
                        &cur_font,
                        existing_fonts,
                    );
                }
            }
            b"TJ" if in_bt => {
                if let Some(Operand::Array(arr)) = op.operands.first() {
                    for elem in arr {
                        if let ArrElem::Str(b) = elem {
                            push_chars_from_bytes(
                                &mut cur_chars,
                                b,
                                op_idx,
                                &cur_font,
                                existing_fonts,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
    segments
}

/// Find all matches of each replacement that span multiple Tj/TJ operators
/// (single-op matches are handled by the existing per-op logic).
/// Core loop shared by `find_cross_op_matches` and `find_cross_op_matches_preserve`.
/// `old_texts` is a parallel slice of the old-text patterns, one per replacement entry.
fn find_cross_op_matches_inner(
    ops: &[Op],
    old_texts: &[&str],
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
) -> Vec<CrossOpMatch> {
    let mut result = Vec::new();
    let segments = collect_char_segments(ops, existing_fonts);

    for seg in &segments {
        let text_chars: Vec<char> = seg.chars.iter().map(|e| e.ch).collect();
        let fi = match existing_fonts.get(&seg.font_name) {
            Some(fi) => fi,
            None => continue,
        };

        for (r_idx, &old_text) in old_texts.iter().enumerate() {
            let pattern_chars: Vec<char> = old_text.chars().collect();
            let plen = pattern_chars.len();
            if plen == 0 {
                continue;
            }

            let mut pos = 0usize;
            while pos + plen <= text_chars.len() {
                if text_chars[pos..pos + plen] == pattern_chars[..] {
                    let char_end = pos + plen;
                    let first_op = seg.chars[pos].op_idx;
                    let last_op = seg.chars[char_end - 1].op_idx;

                    if first_op != last_op {
                        // Accept cross-op matches where all intermediate ops are Tj/TJ or
                        // horizontal-only Td/TD (ty == 0). Horizontal Td ops are common in
                        // per-character CJK PDFs; they must not shift the baseline.
                        let all_text = ops[first_op + 1..last_op]
                            .iter()
                            .all(|o| match o.keyword.as_slice() {
                                b"Tj" | b"TJ" => true,
                                b"Td" | b"TD" => match (
                                    o.operands.first(),
                                    o.operands.get(1),
                                ) {
                                    (Some(Operand::Num(_)), Some(Operand::Num(ty))) => {
                                        ty.abs() < 0.01
                                    }
                                    _ => false,
                                },
                                _ => false,
                            });

                        if all_text {
                            let prefix_raw: Vec<u8> = seg.chars[..pos]
                                .iter()
                                .filter(|e| e.op_idx == first_op)
                                .flat_map(|e| e.raw_bytes.iter().copied())
                                .collect();

                            let suffix_raw: Vec<u8> = seg.chars[char_end..]
                                .iter()
                                .filter(|e| e.op_idx == last_op)
                                .flat_map(|e| e.raw_bytes.iter().copied())
                                .collect();

                            let orig_width: f32 = seg.chars[pos..char_end]
                                .iter()
                                .map(|e| {
                                    let gid = if fi.bytes_per_char == 2 && e.raw_bytes.len() == 2 {
                                        u16::from_be_bytes([e.raw_bytes[0], e.raw_bytes[1]])
                                    } else if !e.raw_bytes.is_empty() {
                                        e.raw_bytes[0] as u16
                                    } else {
                                        0
                                    };
                                    fi.advance_width(gid) as f32 / 1000.0
                                })
                                .sum();

                            result.push(CrossOpMatch {
                                replacement_idx: r_idx,
                                first_op,
                                last_op,
                                prefix_raw,
                                suffix_raw,
                                orig_width,
                                font_size: seg.font_size,
                                font_name: seg.font_name.clone(),
                            });
                        }
                    }
                    pos = char_end;
                } else {
                    pos += 1;
                }
            }
        }
    }
    result
}

/// Find cross-Tf matches: like `find_cross_op_matches_inner` but uses merged cross-Tf segments
/// and allows Tf operators in the intermediate-op whitelist.
/// Only emits matches where the op range spans at least one Tf (same-font cases are
/// already handled by `find_cross_op_matches_inner` and must not be double-counted).
fn find_cross_tf_matches_inner(
    ops: &[Op],
    old_texts: &[&str],
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
) -> Vec<CrossOpMatch> {
    let mut result = Vec::new();
    let segments = collect_cross_tf_segments(ops, existing_fonts);

    for seg in &segments {
        let text_chars: Vec<char> = seg.chars.iter().map(|e| e.ch).collect();
        // Use segment font as fallback for width calculation when per-char font is missing.
        let fi_fallback = existing_fonts.get(&seg.font_name);

        for (r_idx, &old_text) in old_texts.iter().enumerate() {
            let pattern_chars: Vec<char> = old_text.chars().collect();
            let plen = pattern_chars.len();
            if plen == 0 {
                continue;
            }

            let mut pos = 0usize;
            while pos + plen <= text_chars.len() {
                if text_chars[pos..pos + plen] == pattern_chars[..] {
                    let char_end = pos + plen;
                    let first_op = seg.chars[pos].op_idx;
                    let last_op = seg.chars[char_end - 1].op_idx;

                    if first_op != last_op {
                        // Only claim matches that genuinely cross a Tf boundary.
                        // Same-font cross-op matches are handled by find_cross_op_matches_inner.
                        let has_tf = ops[first_op..=last_op]
                            .iter()
                            .any(|o| o.keyword == b"Tf");
                        if !has_tf {
                            pos = char_end;
                            continue;
                        }

                        let all_text = ops[first_op + 1..last_op]
                            .iter()
                            .all(|o| match o.keyword.as_slice() {
                                b"Tj" | b"TJ" => true,
                                b"Tf" => true,
                                b"Td" | b"TD" => match (
                                    o.operands.first(),
                                    o.operands.get(1),
                                ) {
                                    (Some(Operand::Num(_)), Some(Operand::Num(ty))) => {
                                        ty.abs() < 0.01
                                    }
                                    _ => false,
                                },
                                _ => false,
                            });

                        if all_text {
                            let prefix_raw: Vec<u8> = seg.chars[..pos]
                                .iter()
                                .filter(|e| e.op_idx == first_op)
                                .flat_map(|e| e.raw_bytes.iter().copied())
                                .collect();

                            let suffix_raw: Vec<u8> = seg.chars[char_end..]
                                .iter()
                                .filter(|e| e.op_idx == last_op)
                                .flat_map(|e| e.raw_bytes.iter().copied())
                                .collect();

                            // Use per-char font for accurate width; fall back to segment font.
                            let orig_width: f32 = seg.chars[pos..char_end]
                                .iter()
                                .map(|e| {
                                    let fi = existing_fonts
                                        .get(&e.font_name)
                                        .or(fi_fallback)
                                        .expect("font must exist");
                                    let gid =
                                        if fi.bytes_per_char == 2 && e.raw_bytes.len() == 2 {
                                            u16::from_be_bytes([e.raw_bytes[0], e.raw_bytes[1]])
                                        } else if !e.raw_bytes.is_empty() {
                                            e.raw_bytes[0] as u16
                                        } else {
                                            0
                                        };
                                    fi.advance_width(gid) as f32 / 1000.0
                                })
                                .sum();

                            // Restore to the font active at the last matched character.
                            let restore_font = seg.chars[char_end - 1].font_name.clone();

                            result.push(CrossOpMatch {
                                replacement_idx: r_idx,
                                first_op,
                                last_op,
                                prefix_raw,
                                suffix_raw,
                                orig_width,
                                font_size: seg.font_size,
                                font_name: restore_font,
                            });
                        }
                    }
                    pos = char_end;
                } else {
                    pos += 1;
                }
            }
        }
    }
    result
}

fn find_cross_op_matches(
    ops: &[Op],
    replacements: &[ResolvedReplacement],
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
) -> Vec<CrossOpMatch> {
    let old_texts: Vec<&str> = replacements.iter().map(|r| r.old_text.as_str()).collect();
    let mut result = find_cross_op_matches_inner(ops, &old_texts, existing_fonts);
    result.extend(find_cross_tf_matches_inner(ops, &old_texts, existing_fonts));
    result
}

fn find_cross_op_matches_preserve(
    ops: &[Op],
    replacements: &[TextReplacePreserveOp],
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
) -> Vec<CrossOpMatch> {
    let old_texts: Vec<&str> = replacements.iter().map(|r| r.old_text.as_str()).collect();
    find_cross_op_matches_inner(ops, &old_texts, existing_fonts)
}

// ---------------------------------------------------------------------------
// Byte-offset-aware PDF operation parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) enum ArrElem {
    Str(Vec<u8>),
    Num(f32),
}

#[derive(Debug)]
pub(crate) enum Operand {
    Str(Vec<u8>),
    Num(f32),
    Name(Vec<u8>),
    Array(Vec<ArrElem>),
}

pub(crate) struct Op {
    pub start: usize,
    pub end: usize,
    pub keyword: Vec<u8>,
    pub operands: Vec<Operand>,
}

pub(crate) fn parse_ops(bytes: &[u8]) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut i = 0usize;
    let mut operands: Vec<Operand> = Vec::new();
    let mut op_start: Option<usize> = None;

    while i < bytes.len() {
        let b = bytes[i];

        if is_pdf_whitespace(b) {
            i += 1;
            continue;
        }
        if b == b'%' {
            while i < bytes.len() && bytes[i] != b'\r' && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        let token_start = i;
        if op_start.is_none() {
            op_start = Some(i);
        }

        match b {
            b'<' if i + 1 < bytes.len() && bytes[i + 1] == b'<' => {
                // Dictionary — skip to matching >>
                i += 2;
                let mut depth = 1i32;
                while i + 1 < bytes.len() {
                    if bytes[i] == b'<' && bytes[i + 1] == b'<' {
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'>' && bytes[i + 1] == b'>' {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            b'<' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'>' {
                    i += 1;
                }
                let hex = &bytes[start..i];
                if i < bytes.len() {
                    i += 1;
                }
                operands.push(Operand::Str(decode_hex_bytes(hex)));
            }
            b'/' => {
                i += 1;
                let start = i;
                while i < bytes.len() && !is_pdf_whitespace(bytes[i]) && !is_pdf_delimiter(bytes[i])
                {
                    i += 1;
                }
                operands.push(Operand::Name(bytes[start..i].to_vec()));
            }
            b'[' => {
                i += 1;
                let (arr, consumed) = parse_tj_array(&bytes[i..]);
                i += consumed;
                operands.push(Operand::Array(arr));
            }
            b']' => {
                i += 1;
            }
            b'(' => {
                let (s, end_i) = parse_literal_string(bytes, i + 1);
                i = end_i;
                operands.push(Operand::Str(s));
            }
            _ => {
                let start = i;
                while i < bytes.len() && !is_pdf_whitespace(bytes[i]) && !is_pdf_delimiter(bytes[i])
                {
                    i += 1;
                }
                let word = &bytes[start..i];
                if word.is_empty() {
                    i += 1;
                    continue;
                }
                if let Ok(s) = std::str::from_utf8(word)
                    && let Ok(n) = s.parse::<f32>()
                    && n.is_finite()
                {
                    operands.push(Operand::Num(n));
                } else {
                    ops.push(Op {
                        start: op_start.unwrap_or(token_start),
                        end: i,
                        keyword: word.to_vec(),
                        operands: std::mem::take(&mut operands),
                    });
                    op_start = None;
                }
            }
        }
    }

    ops
}

fn parse_tj_array(bytes: &[u8]) -> (Vec<ArrElem>, usize) {
    let mut elems = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if is_pdf_whitespace(b) {
            i += 1;
            continue;
        }
        if b == b']' {
            i += 1;
            return (elems, i);
        }
        match b {
            b'<' if !(i + 1 < bytes.len() && bytes[i + 1] == b'<') => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'>' {
                    i += 1;
                }
                let hex = &bytes[start..i];
                if i < bytes.len() {
                    i += 1;
                }
                elems.push(ArrElem::Str(decode_hex_bytes(hex)));
            }
            b'(' => {
                let (s, end_i) = parse_literal_string(bytes, i + 1);
                i = end_i;
                elems.push(ArrElem::Str(s));
            }
            _ => {
                let start = i;
                while i < bytes.len() && !is_pdf_whitespace(bytes[i]) && !is_pdf_delimiter(bytes[i])
                {
                    i += 1;
                }
                let word = &bytes[start..i];
                if word.is_empty() {
                    i += 1;
                    continue;
                }
                if let Ok(s) = std::str::from_utf8(word)
                    && let Ok(n) = s.parse::<f32>()
                {
                    elems.push(ArrElem::Num(n));
                }
                // Non-numeric token in TJ array: skip
            }
        }
    }

    (elems, i)
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

/// Classify why `old_text` was not matched in any content stream of the page.
/// Used by harumi-ai for debug logging in the InPlace fallback branch.
pub(crate) fn diagnose_match_failure(
    doc: &lopdf::Document,
    page_id: lopdf::ObjectId,
    old_text: &str,
) -> &'static str {
    let existing_fonts = collect_fonts(doc, page_id);
    for bytes in page_content_streams(doc, page_id).iter() {
        let ops = parse_ops(bytes);
        // Merged view (cross-Tf): text found here means Req1 did not fire.
        for seg in collect_cross_tf_segments(&ops, &existing_fonts) {
            let text: String = seg.chars.iter().map(|e| e.ch).collect();
            if text.contains(old_text) {
                return "cross-Tf";
            }
        }
        // Same-font view: text found but blocked by vertical-Td or Tm.
        for seg in collect_char_segments(&ops, &existing_fonts) {
            let text: String = seg.chars.iter().map(|e| e.ch).collect();
            if text.contains(old_text) {
                return "vertical-Td-or-Tm";
            }
        }
        // Cross-BT view: Chrome/Skia Type3 fonts emit one character per BT/Tj/ET
        // block, so segments never hold more than one char. Concatenate all decoded
        // chars (tracking the active font across BT boundaries) and check for
        // the target string.
        {
            let mut cur_font: Vec<u8> = Vec::new();
            let mut cross_bt_chars: Vec<CharEntry> = Vec::new();
            for (op_idx, op) in ops.iter().enumerate() {
                match op.keyword.as_slice() {
                    b"Tf" => {
                        if let Some(Operand::Name(name)) = op.operands.first() {
                            cur_font = name.clone();
                        }
                    }
                    b"Tj" => {
                        if let Some(Operand::Str(b)) = op.operands.first() {
                            push_chars_from_bytes(
                                &mut cross_bt_chars,
                                b,
                                op_idx,
                                &cur_font,
                                &existing_fonts,
                            );
                        }
                    }
                    b"TJ" => {
                        if let Some(Operand::Array(arr)) = op.operands.first() {
                            for elem in arr {
                                if let ArrElem::Str(b) = elem {
                                    push_chars_from_bytes(
                                        &mut cross_bt_chars,
                                        b,
                                        op_idx,
                                        &cur_font,
                                        &existing_fonts,
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            let cross_bt: String = cross_bt_chars.iter().map(|e| e.ch).collect();
            if cross_bt.contains(old_text) {
                return "type3-char-per-tj";
            }
        }
    }
    "text-not-in-stream"
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_font(gid_to_char: &[(u16, char)], bytes_per_char: u8) -> FontInfo {
        let to_unicode: BTreeMap<u16, char> = gid_to_char.iter().copied().collect();
        FontInfo {
            to_unicode,
            dw: 1000,
            w_runs: vec![],
            bytes_per_char,
            identity_fallback: false,
            base_font: String::new(),
            is_bold: false,
            is_italic: false,
            font_family: String::new(),
        }
    }

    #[test]
    fn decode_str_cid() {
        let fi = make_font(&[(0x0041, 'A'), (0x0042, 'B')], 2);
        let mut fonts: HashMap<Vec<u8>, FontInfo> = HashMap::new();
        fonts.insert(b"F0".to_vec(), fi);
        let bytes = &[0x00u8, 0x41, 0x00, 0x42];
        assert_eq!(decode_str(bytes, b"F0", &fonts), "AB");
    }

    #[test]
    fn push_number_integer() {
        let mut out = Vec::new();
        push_number(&mut out, 12.0);
        assert_eq!(out, b"12");
    }

    #[test]
    fn push_number_float() {
        let mut out = Vec::new();
        push_number(&mut out, 1.5);
        assert_eq!(std::str::from_utf8(&out).unwrap(), "1.5");
    }

    #[test]
    fn gids_hex_basic() {
        let r = ResolvedReplacement {
            old_text: "X".into(),
            new_text: "A".into(),
            new_pdf_font_name: b"F1".to_vec(),
            char_to_gid: [('A', 0x0041u16)].into_iter().collect(),
            gid_to_advance: [(0x0041u16, 500u16)].into_iter().collect(),
            units_per_em: 1000,
        };
        let hex = gids_hex(&r);
        assert_eq!(hex, b"<0041>");
    }

    #[test]
    fn rewrite_tj_no_match() {
        // Stream with a Tj that doesn't match any replacement → verbatim copy
        let stream = b"BT\n/F0 12 Tf\n<0041> Tj\nET\n";
        let fonts: HashMap<Vec<u8>, FontInfo> = HashMap::new();
        let replacements: Vec<ResolvedReplacement> = vec![ResolvedReplacement {
            old_text: "B".into(),
            new_text: "X".into(),
            new_pdf_font_name: b"F1".to_vec(),
            char_to_gid: BTreeMap::new(),
            gid_to_advance: BTreeMap::new(),
            units_per_em: 1000,
        }];
        let (result, fonts_used) = rewrite_content_stream(stream, &replacements, &fonts);
        assert_eq!(result, stream);
        assert!(fonts_used.is_empty());
    }

    #[test]
    fn parse_ops_basic() {
        let stream = b"BT\n/F0 12 Tf\n<0041> Tj\nET\n";
        let ops = parse_ops(stream);
        let keywords: Vec<&[u8]> = ops.iter().map(|o| o.keyword.as_slice()).collect();
        assert!(keywords.contains(&b"BT".as_slice()));
        assert!(keywords.contains(&b"Tf".as_slice()));
        assert!(keywords.contains(&b"Tj".as_slice()));
        assert!(keywords.contains(&b"ET".as_slice()));
    }

    // ── per-char Td+Tj matching (⑤-b) ──────────────────────────────────────

    /// A PDF where each character has its own Tj with a horizontal Td between
    /// each pair — common in Japanese per-character PDFs.
    /// Verify that the cross-op matcher finds the match across Td operators.
    #[test]
    fn per_char_td_match_found() {
        // Simulate: (A) Tj  12 0 Td  (B) Tj  12 0 Td  (C) Tj
        // GIDs: A=0x0041, B=0x0042, C=0x0043  (2-byte CID encoding)
        let stream = b"BT\n/F0 12 Tf\n<0041> Tj\n12 0 Td\n<0042> Tj\n12 0 Td\n<0043> Tj\nET\n";
        let fi = make_font(&[(0x0041, 'A'), (0x0042, 'B'), (0x0043, 'C')], 2);
        let mut fonts: HashMap<Vec<u8>, FontInfo> = HashMap::new();
        fonts.insert(b"F0".to_vec(), fi);

        let ops = parse_ops(stream);
        let segs = collect_char_segments(&ops, &fonts);
        // All three characters should be in one segment.
        assert_eq!(segs.len(), 1);
        let text: String = segs[0].chars.iter().map(|e| e.ch).collect();
        assert_eq!(text, "ABC");

        // The cross-op matcher should find "ABC" spanning the Td operators.
        let r = TextReplacePreserveOp { old_text: "ABC".into(), new_text: "ABC".into() };
        let matches = find_cross_op_matches_preserve(&ops, &[r], &fonts);
        assert_eq!(matches.len(), 1, "expected 1 cross-op match across Td operators");
        let m = &matches[0];
        // first_op and last_op should differ (it's a cross-op match).
        assert_ne!(m.first_op, m.last_op);
    }

    /// Vertical Td (ty ≠ 0) must NOT be allowed in cross-op matches.
    #[test]
    fn vertical_td_blocks_cross_op_match() {
        // (A) Tj  0 -14 Td  (B) Tj — vertical Td means new line, not part of same match.
        let stream = b"BT\n/F0 12 Tf\n<0041> Tj\n0 -14 Td\n<0042> Tj\nET\n";
        let fi = make_font(&[(0x0041, 'A'), (0x0042, 'B')], 2);
        let mut fonts: HashMap<Vec<u8>, FontInfo> = HashMap::new();
        fonts.insert(b"F0".to_vec(), fi);

        let ops = parse_ops(stream);
        let r = TextReplacePreserveOp { old_text: "AB".into(), new_text: "AB".into() };
        let matches = find_cross_op_matches_preserve(&ops, &[r], &fonts);
        assert_eq!(matches.len(), 0, "vertical Td should block cross-op matching");
    }

    /// When a per-char stream is rewritten, the intermediate Td ops must be
    /// suppressed and the output must not contain them.
    ///
    /// The intermediate Td uses a distinctive `7 0` offset so we can
    /// distinguish it from any width-compensation Td emitted after the match.
    #[test]
    fn per_char_td_rewrite_suppresses_intermediate_tds() {
        // CID stream: (日)Tj  7 0 Td  (本)Tj — replace "日本" → "AB"
        // GIDs: 日=0x65E5, 本=0x672C; A=0x0041, B=0x0042
        // Use a distinctive "7 0 Td" for the intermediate so we can tell it apart
        // from any width-compensation Td the rewriter might emit later.
        let stream = b"BT\n/F0 12 Tf\n<65E5> Tj\n7 0 Td\n<672C> Tj\nET\n";
        let fi_orig = make_font(&[(0x65E5, '日'), (0x672C, '本')], 2);
        let mut fonts: HashMap<Vec<u8>, FontInfo> = HashMap::new();
        fonts.insert(b"F0".to_vec(), fi_orig);

        let r = ResolvedReplacement {
            old_text: "日本".into(),
            new_text: "AB".into(),
            new_pdf_font_name: b"HR0".to_vec(),
            char_to_gid: [('A', 0x0041u16), ('B', 0x0042u16)].into_iter().collect(),
            gid_to_advance: [(0x0041u16, 500u16), (0x0042u16, 500u16)].into_iter().collect(),
            units_per_em: 1000,
        };

        let (out, _) = rewrite_content_stream(stream, &[r], &fonts);
        let out_str = String::from_utf8_lossy(&out);

        // The distinctive "7 0 Td" (intermediate) must NOT appear in output.
        assert!(!out_str.contains("7 0 Td"), "intermediate Td should be suppressed: {out_str}");
        // The replacement font should be used.
        assert!(out_str.contains("HR0"), "replacement font HR0 should appear: {out_str}");
        // The replacement GIDs should appear.
        assert!(
            out_str.contains("0041") && out_str.contains("0042"),
            "replacement GIDs should appear: {out_str}"
        );
    }
}
