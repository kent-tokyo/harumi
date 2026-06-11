use std::collections::{BTreeMap, HashMap, HashSet};

use lopdf::{Dictionary, Object, ObjectId, Stream};

use crate::error::{Error, Result};
use crate::extract::{FontInfo, collect_fonts, page_content_streams};
use crate::font::{cmap, embed::build_widths_array, subset::subset_font};
use crate::replace::{ArrElem, Operand, collect_char_segments, encode_str_hex, parse_ops};

// ---------------------------------------------------------------------------
// Object-graph navigation
// ---------------------------------------------------------------------------

/// IDs of objects forming a CIDFontType2 font in a PDF.
pub(crate) struct CidFontIds {
    pub cid_id: ObjectId,
    /// FontFile2 (TTF) or FontFile3 (CFF/OTF) stream.
    pub font_file_id: ObjectId,
    pub to_unicode_id: ObjectId,
}

/// Resolves the CID font object IDs for `font_name` on the given page.
///
/// Returns `Err(InvalidInput)` if:
/// - the font is not a Type0 / CIDFontType2 with Identity CIDToGIDMap
/// - any required PDF object is missing or malformed
pub(crate) fn resolve_cid_font_ids(
    doc: &lopdf::Document,
    page_id: ObjectId,
    font_name: &[u8],
) -> Result<CidFontIds> {
    let page_dict = doc.get_object(page_id)?.as_dict()?;
    let resources_obj = page_dict
        .get(b"Resources")
        .map_err(|_| Error::InvalidInput("page has no /Resources".into()))?;
    let res_dict = crate::extract::resolve_dict(doc, resources_obj)
        .ok_or_else(|| Error::InvalidInput("cannot resolve /Resources".into()))?;
    let font_obj = res_dict
        .get(b"Font")
        .map_err(|_| Error::InvalidInput("page has no /Resources/Font".into()))?;
    let font_dict = crate::extract::resolve_dict(doc, font_obj)
        .ok_or_else(|| Error::InvalidInput("cannot resolve /Font dict".into()))?;

    let font_ref = font_dict.get(font_name).map_err(|_| {
        Error::InvalidInput(format!(
            "font '{}' not found in /Resources/Font",
            String::from_utf8_lossy(font_name)
        ))
    })?;
    let Object::Reference(type0_oid) = font_ref else {
        return Err(Error::InvalidInput("font entry is not a Reference".into()));
    };
    let type0_dict = doc.get_object(*type0_oid)?.as_dict()?;

    // Verify Type0
    let subtype = type0_dict.get(b"Subtype").ok().and_then(|o| {
        if let Object::Name(n) = o {
            Some(n.as_slice())
        } else {
            None
        }
    });
    if subtype != Some(b"Type0") {
        return Err(Error::InvalidInput(
            "replace_text_resubset only supports CIDFontType2 (Type0) fonts; \
             Type1/TrueType simple fonts are not supported"
                .into(),
        ));
    }

    // ToUnicode
    let to_unicode_id = match type0_dict.get(b"ToUnicode") {
        Ok(Object::Reference(id)) => *id,
        _ => {
            return Err(Error::InvalidInput(
                "Type0 font has no /ToUnicode stream".into(),
            ));
        }
    };

    // DescendantFonts[0] → CIDFont
    let desc_obj = type0_dict
        .get(b"DescendantFonts")
        .map_err(|_| Error::InvalidInput("Type0 font missing /DescendantFonts".into()))?;
    let Object::Array(desc_arr) = desc_obj else {
        return Err(Error::InvalidInput(
            "/DescendantFonts is not an Array".into(),
        ));
    };
    let Some(Object::Reference(cid_oid)) = desc_arr.first() else {
        return Err(Error::InvalidInput(
            "/DescendantFonts[0] is not a Reference".into(),
        ));
    };
    let cid_dict = doc.get_object(*cid_oid)?.as_dict()?;

    // Verify CIDToGIDMap is Identity
    let cgm = cid_dict.get(b"CIDToGIDMap").ok().and_then(|o| {
        if let Object::Name(n) = o {
            Some(n.as_slice())
        } else {
            None
        }
    });
    if cgm != Some(b"Identity") {
        return Err(Error::InvalidInput(
            "replace_text_resubset only supports CIDToGIDMap=Identity".into(),
        ));
    }

    // FontDescriptor
    let desc_ref = cid_dict
        .get(b"FontDescriptor")
        .map_err(|_| Error::InvalidInput("CIDFont missing /FontDescriptor".into()))?;
    let Object::Reference(descriptor_oid) = desc_ref else {
        return Err(Error::InvalidInput(
            "/FontDescriptor is not a Reference".into(),
        ));
    };
    let descriptor = doc.get_object(*descriptor_oid)?.as_dict()?;

    // FontFile2 (TTF) or FontFile3 (OTF/CFF)
    let font_file_id = if let Ok(Object::Reference(id)) = descriptor.get(b"FontFile2") {
        *id
    } else if let Ok(Object::Reference(id)) = descriptor.get(b"FontFile3") {
        *id
    } else {
        return Err(Error::InvalidInput(
            "FontDescriptor has no FontFile2 or FontFile3".into(),
        ));
    };

    Ok(CidFontIds {
        cid_id: *cid_oid,
        font_file_id,
        to_unicode_id,
    })
}

// ---------------------------------------------------------------------------
// Font update
// ---------------------------------------------------------------------------

/// Replace the subsetted font bytes and regenerate /W and ToUnicode in the PDF.
pub(crate) fn update_cid_font(
    doc: &mut lopdf::Document,
    ids: &CidFontIds,
    new_bytes: Vec<u8>,
    new_gid_to_char: &BTreeMap<u16, char>,
    new_gid_to_advance: &BTreeMap<u16, u16>,
    units_per_em: u16,
) -> Result<()> {
    // 1. Replace font file stream content.
    let ff_stream = doc.get_object_mut(ids.font_file_id)?.as_stream_mut()?;
    ff_stream.content = new_bytes.clone();
    ff_stream
        .dict
        .set("Length1", Object::Integer(new_bytes.len() as i64));
    // Remove any compression filter so content is raw.
    ff_stream.dict.remove(b"Filter");
    ff_stream.dict.remove(b"DecodeParms");

    // 2. Rebuild /W widths array in CIDFont.
    let w_array = build_widths_array(new_gid_to_advance, units_per_em);
    let cid_dict = doc.get_object_mut(ids.cid_id)?.as_dict_mut()?;
    cid_dict.set("W", Object::Array(w_array));

    // 3. Rebuild ToUnicode CMap stream.
    let cmap_bytes = cmap::generate_to_unicode(new_gid_to_char);
    let tu_stream = doc.get_object_mut(ids.to_unicode_id)?.as_stream_mut()?;
    tu_stream.content = cmap_bytes.clone();
    tu_stream.dict.remove(b"Filter");
    tu_stream.dict.remove(b"DecodeParms");
    tu_stream
        .dict
        .set("Length", Object::Integer(cmap_bytes.len() as i64));

    Ok(())
}

// ---------------------------------------------------------------------------
// Char collection
// ---------------------------------------------------------------------------

/// Collect all chars used by `font_name` in a set of content stream bytes.
/// Returns `(old_gid_to_char, char_set)` where `old_gid_to_char` is the
/// current GID→char mapping from the FontInfo and `char_set` is the set of
/// Unicode characters decoded.
pub(crate) fn collect_chars_for_font(
    streams: &[Vec<u8>],
    font_name: &[u8],
    fonts: &HashMap<Vec<u8>, FontInfo>,
) -> (BTreeMap<u16, char>, HashSet<char>) {
    let old_gid_to_char = fonts
        .get(font_name)
        .map(|fi| fi.to_unicode.clone())
        .unwrap_or_default();

    let mut chars: HashSet<char> = HashSet::new();
    for bytes in streams {
        let ops = parse_ops(bytes);
        let segs = collect_char_segments(&ops, fonts);
        for seg in &segs {
            if seg.font_name == font_name {
                chars.extend(seg.chars.iter().map(|e| e.ch));
            }
        }
    }
    (old_gid_to_char, chars)
}

/// Return the distinct font resource names used at positions where `text`
/// appears in the page's content streams.
pub(crate) fn find_fonts_for_text(
    doc: &lopdf::Document,
    page_id: ObjectId,
    text: &str,
) -> Vec<Vec<u8>> {
    let fonts = collect_fonts(doc, page_id);
    let streams = page_content_streams(doc, page_id);
    let mut result: Vec<Vec<u8>> = Vec::new();
    for bytes in &streams {
        let ops = parse_ops(bytes);
        let segs = collect_char_segments(&ops, &fonts);
        for seg in &segs {
            let seg_text: String = seg.chars.iter().map(|e| e.ch).collect();
            if seg_text.contains(text) && !result.contains(&seg.font_name) {
                result.push(seg.font_name.clone());
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// GID re-encoding
// ---------------------------------------------------------------------------

/// Re-encode a content stream, remapping GIDs for `target_font` from the old
/// subset GID space to the new one.  Text *content* is unchanged; only the
/// numeric GID values inside hex strings change.
///
/// Other fonts' text operators are copied verbatim.
pub(crate) fn reencode_stream_for_font(
    bytes: &[u8],
    target_font: &[u8],
    old_gid_to_char: &BTreeMap<u16, char>,
    new_char_to_gid: &BTreeMap<char, u16>,
) -> Vec<u8> {
    let ops = parse_ops(bytes);
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut last_copied = 0usize;
    let mut in_bt = false;
    let mut cur_font: Vec<u8> = Vec::new();

    for op in &ops {
        match op.keyword.as_slice() {
            b"BT" => {
                in_bt = true;
                cur_font.clear();
            }
            b"ET" => {
                in_bt = false;
            }
            b"Tf" if in_bt => {
                if let Some(Operand::Name(name)) = op.operands.first() {
                    cur_font = name.clone();
                }
            }
            b"Tj" if in_bt && cur_font == target_font => {
                if let Some(Operand::Str(str_bytes)) = op.operands.first() {
                    let new_raw = remap_gids(str_bytes, old_gid_to_char, new_char_to_gid);
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    out.extend_from_slice(&encode_str_hex(&new_raw));
                    out.push(b' ');
                    out.extend_from_slice(b"Tj\n");
                    last_copied = op.end;
                }
            }
            b"TJ" if in_bt && cur_font == target_font => {
                if let Some(Operand::Array(arr)) = op.operands.first() {
                    out.extend_from_slice(&bytes[last_copied..op.start]);
                    out.push(b'[');
                    for elem in arr {
                        match elem {
                            ArrElem::Str(b) => {
                                let new_raw = remap_gids(b, old_gid_to_char, new_char_to_gid);
                                out.extend_from_slice(&encode_str_hex(&new_raw));
                            }
                            ArrElem::Num(n) => {
                                let s = format!("{}", n);
                                out.extend_from_slice(s.as_bytes());
                            }
                        }
                        out.push(b' ');
                    }
                    out.extend_from_slice(b"] TJ\n");
                    last_copied = op.end;
                }
            }
            _ => {}
        }
    }
    out.extend_from_slice(&bytes[last_copied..]);
    out
}

/// Remap 2-byte-per-char GID bytes: old GID → char → new GID.
/// Unknown GIDs are kept as-is (maps to GID 0 = .notdef fallback).
fn remap_gids(
    raw: &[u8],
    old_gid_to_char: &BTreeMap<u16, char>,
    new_char_to_gid: &BTreeMap<char, u16>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    if raw.len().is_multiple_of(2) {
        for chunk in raw.chunks(2) {
            let old_gid = u16::from_be_bytes([chunk[0], chunk[1]]);
            let new_gid = old_gid_to_char
                .get(&old_gid)
                .and_then(|ch| new_char_to_gid.get(ch))
                .copied()
                .unwrap_or(0);
            out.extend_from_slice(&new_gid.to_be_bytes());
        }
    } else {
        out.extend_from_slice(raw);
    }
    out
}

// ---------------------------------------------------------------------------
// Top-level orchestration (called from document::finalize)
// ---------------------------------------------------------------------------

/// All information needed to process one ReplaceResubset batch for a single
/// PDF font resource.
pub(crate) struct ResubsetWork {
    /// PDF font resource name (e.g. `b"HR0"`).
    pub font_name: Vec<u8>,
    /// Original (unsubsetted) font bytes provided by the caller.
    pub font_bytes: Vec<u8>,
    /// (page_id, old_text, new_text) triples that need replacement on that font.
    pub replacements: Vec<(ObjectId, String, String)>,
    /// Wrap parameters for text replacements (old_text → WrapParams).
    #[allow(dead_code)]
    pub wrap_params_by_old_text: std::collections::HashMap<String, crate::replace::WrapParams>,
}

/// Perform the full re-subsetting pipeline for a single font.
///
/// Steps:
/// 1. Collect existing chars + old GID map across all pages.
/// 2. Build expanded char set and new subset.
/// 3. Update PDF font objects (FontFile, /W, ToUnicode).
/// 4. Re-encode content streams if GIDs changed.
/// 5. Apply text replacement using the existing preserve-font path.
pub(crate) fn resubset_and_replace(
    doc: &mut lopdf::Document,
    work: &ResubsetWork,
    all_page_ids: &[ObjectId],
) -> Result<()> {
    if work.replacements.is_empty() {
        return Ok(());
    }

    // --- Step 1: collect existing chars and old GID map ---
    // Use the first page that has this font to get FontInfo.
    let anchor_page = work.replacements.first().map(|(pid, _, _)| *pid).unwrap();
    let fonts_on_anchor = collect_fonts(doc, anchor_page);
    let streams_on_anchor = page_content_streams(doc, anchor_page);
    let (old_gid_to_char, mut all_chars) =
        collect_chars_for_font(&streams_on_anchor, &work.font_name, &fonts_on_anchor);

    // Collect chars from remaining pages too.
    for &pid in all_page_ids {
        if pid == anchor_page {
            continue;
        }
        let f = collect_fonts(doc, pid);
        if !f.contains_key(work.font_name.as_slice()) {
            continue;
        }
        let s = page_content_streams(doc, pid);
        let (_, page_chars) = collect_chars_for_font(&s, &work.font_name, &f);
        all_chars.extend(page_chars);
    }

    // Add new chars from all replacements.
    for (_, _, new_text) in &work.replacements {
        all_chars.extend(new_text.chars());
    }

    // --- Step 2: create new subset ---
    let all_chars_vec: Vec<char> = all_chars.into_iter().collect();
    let subset = subset_font(&work.font_bytes, &all_chars_vec)?;
    let new_char_to_gid: BTreeMap<char, u16> = subset
        .gid_to_char
        .iter()
        .map(|(&gid, &ch)| (ch, gid))
        .collect();

    // --- Step 3: update PDF font objects ---
    // Find an anchor page that has this font to resolve object IDs.
    let ids = resolve_cid_font_ids(doc, anchor_page, &work.font_name)?;
    update_cid_font(
        doc,
        &ids,
        subset.bytes,
        &subset.gid_to_char,
        &subset.gid_to_advance,
        subset.units_per_em,
    )?;

    // --- Step 4: re-encode content streams if GIDs changed ---
    let gids_changed = old_gid_to_char.iter().any(|(&old_gid, &ch)| {
        new_char_to_gid
            .get(&ch)
            .is_none_or(|&new_gid| new_gid != old_gid)
    });

    if gids_changed {
        for &pid in all_page_ids {
            let f = collect_fonts(doc, pid);
            if !f.contains_key(work.font_name.as_slice()) {
                continue;
            }
            let streams = page_content_streams(doc, pid);
            let mut new_content: Vec<u8> = Vec::new();
            for s in &streams {
                let reencoded = reencode_stream_for_font(
                    s,
                    &work.font_name,
                    &old_gid_to_char,
                    &new_char_to_gid,
                );
                new_content.extend_from_slice(&reencoded);
                if !new_content.ends_with(b"\n") {
                    new_content.push(b'\n');
                }
            }
            let new_id =
                doc.add_object(Object::Stream(Stream::new(Dictionary::new(), new_content)));
            doc.get_object_mut(pid)?
                .as_dict_mut()?
                .set("Contents", Object::Reference(new_id));
        }
    }

    // --- Step 5: apply text replacement via preserve-font path ---
    // Group replacements by page.
    let mut by_page: HashMap<ObjectId, Vec<crate::replace::TextReplacePreserveOp>> = HashMap::new();
    for (pid, old_text, new_text) in &work.replacements {
        by_page
            .entry(*pid)
            .or_default()
            .push(crate::replace::TextReplacePreserveOp {
                old_text: old_text.clone(),
                new_text: new_text.clone(),
            });
    }
    for (pid, ops) in by_page {
        let wrap_params_by_old_text = if work.wrap_params_by_old_text.is_empty() {
            None
        } else {
            Some(&work.wrap_params_by_old_text)
        };
        let new_content = crate::replace::rewrite_page_streams_preserve_font(
            doc,
            pid,
            &ops,
            wrap_params_by_old_text,
        )?;
        let new_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), new_content)));
        doc.get_object_mut(pid)?
            .as_dict_mut()?
            .set("Contents", Object::Reference(new_id));
    }

    Ok(())
}
