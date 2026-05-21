use std::collections::{BTreeMap, HashMap};

use crate::extract::{
    FontInfo, collect_fonts, decode_hex_bytes, is_pdf_delimiter, is_pdf_whitespace,
    page_content_streams, parse_literal_string,
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

    for bytes in &streams {
        let ops = parse_ops(bytes);
        let segments = collect_char_segments(&ops, &existing_fonts);

        for seg in &segments {
            let text: String = seg.chars.iter().map(|e| e.ch).collect();
            let mut pos = 0usize;
            while let Some(byte_idx) = text[pos..].find(old_text) {
                if let Some(new) = new_text {
                    validate_chars_in_font(new, &seg.font_name, &existing_fonts)?;
                }
                total += 1;
                pos += byte_idx + old_text.len();
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
    let Some(fi) = existing_fonts.get(font_name) else { return Ok(()) };
    let char_to_gid: std::collections::HashMap<char, u16> =
        fi.to_unicode.iter().map(|(&gid, &ch)| (ch, gid)).collect();
    let font_name_str = String::from_utf8_lossy(font_name).into_owned();
    for ch in text.chars() {
        let gid = match char_to_gid.get(&ch) {
            Some(&g) => g,
            None => return Err(crate::Error::FontCharNotMapped { ch, font_name: font_name_str }),
        };
        // Simple fonts use 1-byte encoding; gid must fit in u8
        if fi.bytes_per_char == 1 && gid > 255 {
            return Err(crate::Error::FontCharNotMapped { ch, font_name: font_name_str });
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

/// Rewrite all content streams for a page using the font already embedded in the PDF.
/// Returns the rewritten bytes, or `Err` if a character in any `new_text` is absent from
/// the font's ToUnicode mapping.
pub(crate) fn rewrite_page_streams_preserve_font(
    doc: &lopdf::Document,
    page_id: lopdf::ObjectId,
    replacements: &[TextReplacePreserveOp],
) -> crate::Result<Vec<u8>> {
    let existing_fonts = collect_fonts(doc, page_id);
    let streams = page_content_streams(doc, page_id);
    let mut out = Vec::new();
    for bytes in &streams {
        let rewritten = rewrite_stream_preserve_font(bytes, replacements, &existing_fonts)?;
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
            b"Tj" if in_bt => {
                // Cross-op match.
                if let Some(&(co_idx, role)) = op_role.get(&op_idx) {
                    let co = &cross_op[co_idx];
                    let r = &replacements[co.replacement_idx];
                    let fi = match existing_fonts.get(&co.font_name) {
                        Some(fi) => fi,
                        None => { last_copied = op.end; continue; }
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
                            let new_bytes = encode_chars_as_bytes(&r.new_text, &char_to_gid, fi.bytes_per_char);
                            out.extend_from_slice(&encode_str_hex(&new_bytes));
                            out.extend_from_slice(b" Tj\n");
                        }
                        1 => { /* middle: skip */ }
                        _ => {
                            if !co.suffix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.suffix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            let new_bytes = encode_chars_as_bytes(&r.new_text, &char_to_gid, fi.bytes_per_char);
                            let orig_w = co.orig_width * co.font_size;
                            let new_w = orig_width(&new_bytes, &co.font_name, co.font_size, existing_fonts);
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
                    let new_bytes =
                        encode_chars_as_bytes(&r.new_text, &char_to_gid, fi.bytes_per_char);
                    let orig_w = orig_width(&str_bytes, &cur_font, cur_size, existing_fonts);
                    let new_w = orig_width(&new_bytes, &cur_font, cur_size, existing_fonts);
                    let delta = orig_w - new_w;
                    let mut fragment = Vec::new();
                    fragment.extend_from_slice(&encode_str_hex(&new_bytes));
                    fragment.extend_from_slice(b" Tj\n");
                    if delta.abs() > 0.01 {
                        push_number(&mut fragment, delta);
                        fragment.extend_from_slice(b" 0 Td\n");
                    }
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    out.extend_from_slice(&fragment);
                    last_copied = op.end;
                }
            }
            b"TJ" if in_bt => {
                if let Some(&(co_idx, role)) = op_role.get(&op_idx) {
                    let co = &cross_op[co_idx];
                    let r = &replacements[co.replacement_idx];
                    let fi = match existing_fonts.get(&co.font_name) {
                        Some(fi) => fi,
                        None => { last_copied = op.end; continue; }
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
                            let new_bytes = encode_chars_as_bytes(&r.new_text, &char_to_gid, fi.bytes_per_char);
                            out.extend_from_slice(&encode_str_hex(&new_bytes));
                            out.extend_from_slice(b" Tj\n");
                        }
                        1 => {}
                        _ => {
                            if !co.suffix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.suffix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            let new_bytes = encode_chars_as_bytes(&r.new_text, &char_to_gid, fi.bytes_per_char);
                            let orig_w = co.orig_width * co.font_size;
                            let new_w = orig_width(&new_bytes, &co.font_name, co.font_size, existing_fonts);
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
                            if let Some(r) =
                                replacements.iter().find(|r| r.old_text == decoded)
                            {
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
            b"Tj" if in_bt => {
                // Cross-op match handling takes priority.
                if let Some(&(co_idx, role)) = op_role.get(&op_idx) {
                    let co = &cross_op[co_idx];
                    let r = &replacements[co.replacement_idx];
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    match role {
                        0 => {
                            // First op: emit prefix + replacement (no suffix/comp yet).
                            if !co.prefix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.prefix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            emit_cross_op_replacement(&mut out, r, co);
                            fonts_used.insert(r.new_pdf_font_name.clone());
                        }
                        1 => { /* middle: skip */ }
                        _ => {
                            // Last op: emit suffix + width compensation.
                            if !co.suffix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.suffix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            let orig_w = co.orig_width * co.font_size;
                            let new_w = new_width(r, co.font_size);
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
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    match role {
                        0 => {
                            if !co.prefix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.prefix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            emit_cross_op_replacement(&mut out, r, co);
                            fonts_used.insert(r.new_pdf_font_name.clone());
                        }
                        1 => { /* middle: skip */ }
                        _ => {
                            if !co.suffix_raw.is_empty() {
                                out.extend_from_slice(&encode_str_hex(&co.suffix_raw));
                                out.extend_from_slice(b" Tj\n");
                            }
                            let orig_w = co.orig_width * co.font_size;
                            let new_w = new_width(r, co.font_size);
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
                    let (fragment, used) =
                        emit_tj_array(&arr.clone(), replacements, &cur_font, cur_size, existing_fonts);
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
fn emit_cross_op_replacement(out: &mut Vec<u8>, r: &ResolvedReplacement, co: &CrossOpMatch) {
    out.push(b'/');
    out.extend_from_slice(&r.new_pdf_font_name);
    out.push(b' ');
    push_number(out, co.font_size);
    out.extend_from_slice(b" Tf\n");
    out.extend_from_slice(&gids_hex(r));
    out.extend_from_slice(b" Tj\n");
    out.push(b'/');
    out.extend_from_slice(&co.orig_font_name);
    out.push(b' ');
    push_number(out, co.font_size);
    out.extend_from_slice(b" Tf\n");
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
    let Some(fi) = existing_fonts.get(font_name) else { return 0.0 };
    let mut total = 0.0f32;
    if fi.bytes_per_char == 2 && bytes.len() % 2 == 0 {
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
    let Some(fi) = existing_fonts.get(font_name) else { return String::new() };
    let mut text = String::new();
    if fi.bytes_per_char == 2 {
        if bytes.len() % 2 == 0 {
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

fn encode_str_hex(bytes: &[u8]) -> Vec<u8> {
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

fn push_number(out: &mut Vec<u8>, v: f32) {
    let v = if v.is_finite() { v } else { 0.0 };
    if v.fract() == 0.0 && v.abs() < 1e9 {
        let s = format!("{}", v as i64);
        out.extend_from_slice(s.as_bytes());
    } else {
        let s = format!("{:.4}", v);
        let s = s.trim_end_matches('0');
        let s = if s.ends_with('.') { format!("{}0", s) } else { s.to_string() };
        out.extend_from_slice(s.as_bytes());
    }
}

// ---------------------------------------------------------------------------
// Cross-operator text accumulation and matching
// ---------------------------------------------------------------------------

/// A single decoded character with its location in the content stream.
struct CharEntry {
    ch: char,
    op_idx: usize,
    raw_bytes: Vec<u8>,
}

/// A text segment within a BT/ET block sharing the same font and size.
struct CharSegment {
    chars: Vec<CharEntry>,
    font_name: Vec<u8>,
    font_size: f32,
}

/// A replacement match (with new font) that spans multiple consecutive Tj/TJ operators.
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
    orig_font_name: Vec<u8>,
}

/// A font-preserving replacement match that spans multiple consecutive Tj/TJ operators.
struct CrossOpMatchPreserve {
    replacement_idx: usize,
    first_op: usize,
    last_op: usize,
    prefix_raw: Vec<u8>,
    suffix_raw: Vec<u8>,
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
        if str_bytes.len() % 2 == 0 {
            for chunk in str_bytes.chunks(2) {
                let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
                if let Some(&ch) = fi.to_unicode.get(&gid) {
                    char_buf.push(CharEntry { ch, op_idx, raw_bytes: chunk.to_vec() });
                }
            }
        }
    } else {
        for &b in str_bytes {
            if let Some(&ch) = fi.to_unicode.get(&(b as u16)) {
                char_buf.push(CharEntry { ch, op_idx, raw_bytes: vec![b] });
            }
        }
    }
}

/// Split the content stream into per-Tf character segments within each BT/ET block.
/// The buffer resets on each BT and on every Tf operator.
fn collect_char_segments(
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
                    push_chars_from_bytes(&mut cur_chars, str_bytes, op_idx, &cur_font, existing_fonts);
                }
            }
            b"TJ" if in_bt => {
                if let Some(Operand::Array(arr)) = op.operands.first() {
                    for elem in arr {
                        if let ArrElem::Str(b) = elem {
                            push_chars_from_bytes(&mut cur_chars, b, op_idx, &cur_font, existing_fonts);
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
fn find_cross_op_matches(
    ops: &[Op],
    replacements: &[ResolvedReplacement],
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

        for (r_idx, r) in replacements.iter().enumerate() {
            let pattern_chars: Vec<char> = r.old_text.chars().collect();
            let plen = pattern_chars.len();
            if plen == 0 { continue; }

            let mut pos = 0usize;
            while pos + plen <= text_chars.len() {
                if text_chars[pos..pos + plen] == pattern_chars[..] {
                    let char_end = pos + plen;
                    let first_op = seg.chars[pos].op_idx;
                    let last_op = seg.chars[char_end - 1].op_idx;

                    if first_op != last_op {
                        // Only accept cross-op matches where all intermediate ops are Tj/TJ.
                        // This prevents matching across positional operators (Td, Tm, etc.).
                        let all_text = ops[first_op + 1..last_op]
                            .iter()
                            .all(|o| matches!(o.keyword.as_slice(), b"Tj" | b"TJ"));

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

                            let orig_width: f32 = seg.chars[pos..char_end].iter().map(|e| {
                                let gid = if fi.bytes_per_char == 2 && e.raw_bytes.len() == 2 {
                                    u16::from_be_bytes([e.raw_bytes[0], e.raw_bytes[1]])
                                } else if !e.raw_bytes.is_empty() {
                                    e.raw_bytes[0] as u16
                                } else { 0 };
                                fi.advance_width(gid) as f32 / 1000.0
                            }).sum();

                            result.push(CrossOpMatch {
                                replacement_idx: r_idx,
                                first_op,
                                last_op,
                                prefix_raw,
                                suffix_raw,
                                orig_width,
                                font_size: seg.font_size,
                                orig_font_name: seg.font_name.clone(),
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

/// Like `find_cross_op_matches` but for font-preserving replacements.
fn find_cross_op_matches_preserve(
    ops: &[Op],
    replacements: &[TextReplacePreserveOp],
    existing_fonts: &HashMap<Vec<u8>, FontInfo>,
) -> Vec<CrossOpMatchPreserve> {
    let mut result = Vec::new();
    let segments = collect_char_segments(ops, existing_fonts);

    for seg in &segments {
        let text_chars: Vec<char> = seg.chars.iter().map(|e| e.ch).collect();
        let fi = match existing_fonts.get(&seg.font_name) {
            Some(fi) => fi,
            None => continue,
        };

        for (r_idx, r) in replacements.iter().enumerate() {
            let pattern_chars: Vec<char> = r.old_text.chars().collect();
            let plen = pattern_chars.len();
            if plen == 0 { continue; }

            let mut pos = 0usize;
            while pos + plen <= text_chars.len() {
                if text_chars[pos..pos + plen] == pattern_chars[..] {
                    let char_end = pos + plen;
                    let first_op = seg.chars[pos].op_idx;
                    let last_op = seg.chars[char_end - 1].op_idx;

                    if first_op != last_op {
                        let all_text = ops[first_op + 1..last_op]
                            .iter()
                            .all(|o| matches!(o.keyword.as_slice(), b"Tj" | b"TJ"));

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

                            let orig_width: f32 = seg.chars[pos..char_end].iter().map(|e| {
                                let gid = if fi.bytes_per_char == 2 && e.raw_bytes.len() == 2 {
                                    u16::from_be_bytes([e.raw_bytes[0], e.raw_bytes[1]])
                                } else if !e.raw_bytes.is_empty() {
                                    e.raw_bytes[0] as u16
                                } else { 0 };
                                fi.advance_width(gid) as f32 / 1000.0
                            }).sum();

                            result.push(CrossOpMatchPreserve {
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

// ---------------------------------------------------------------------------
// Byte-offset-aware PDF operation parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum ArrElem {
    Str(Vec<u8>),
    Num(f32),
}

#[derive(Debug)]
enum Operand {
    Str(Vec<u8>),
    Num(f32),
    Name(Vec<u8>),
    Array(Vec<ArrElem>),
}

struct Op {
    start: usize,
    end: usize,
    keyword: Vec<u8>,
    operands: Vec<Operand>,
}

fn parse_ops(bytes: &[u8]) -> Vec<Op> {
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
                while i < bytes.len()
                    && !is_pdf_whitespace(bytes[i])
                    && !is_pdf_delimiter(bytes[i])
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
                while i < bytes.len()
                    && !is_pdf_whitespace(bytes[i])
                    && !is_pdf_delimiter(bytes[i])
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
                while i < bytes.len()
                    && !is_pdf_whitespace(bytes[i])
                    && !is_pdf_delimiter(bytes[i])
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
}
