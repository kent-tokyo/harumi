use std::collections::{BTreeMap, HashMap};

use lopdf::{Dictionary, Object, ObjectId};

use crate::error::Result;

/// A text fragment extracted from a page content stream.
///
/// Returned by [`crate::Document::extract_text_runs`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct TextFragment {
    /// Decoded Unicode text.
    pub text: String,
    /// X coordinate in PDF points (origin: bottom-left of page).
    pub x: f32,
    /// Y coordinate in PDF points (origin: bottom-left of page).
    pub y: f32,
    /// Estimated text width in PDF points, computed from the font's advance widths.
    pub width: f32,
    /// Font size in PDF points.
    pub font_size: f32,
    /// PDF resource name of the font at this position (e.g. `"HR0"`, `"F1"`).
    pub font_name: String,
    /// RGB fill color at this position, each component in `0.0..=1.0`.
    /// Defaults to black `[0.0, 0.0, 0.0]` when no color operator precedes the text.
    pub color: [f32; 3],
    /// `true` if the text render mode is 3 (invisible / OCR search layer).
    pub invisible: bool,
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
// Public entry point
// ---------------------------------------------------------------------------

pub(crate) fn extract_text_runs_from_page(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> Result<Vec<TextFragment>> {
    let streams = page_content_streams(doc, page_id);
    let fonts = collect_fonts(doc, page_id);

    let mut fragments = Vec::new();
    for stream_bytes in &streams {
        parse_content_stream(stream_bytes, &fonts, &mut fragments);
    }
    Ok(fragments)
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
                if let Object::Reference(id) = o { Some(*id) } else { None }
            })
            .collect(),
        _ => return vec![],
    };

    let mut result = Vec::new();
    for id in ids {
        let Ok(stream_obj) = doc.get_object(id) else { continue };
        let Ok(stream) = stream_obj.as_stream() else { continue };
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

pub(crate) fn resolve_dict<'a>(doc: &'a lopdf::Document, obj: &'a Object) -> Option<&'a Dictionary> {
    match obj {
        Object::Dictionary(d) => Some(d),
        Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok(),
        _ => None,
    }
}

pub(crate) fn collect_fonts(doc: &lopdf::Document, page_id: ObjectId) -> HashMap<Vec<u8>, FontInfo> {
    collect_fonts_inner(doc, page_id).unwrap_or_default()
}

fn collect_fonts_inner(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> Option<HashMap<Vec<u8>, FontInfo>> {
    let mut fonts = HashMap::new();

    let page_dict = doc.get_object(page_id).ok()?.as_dict().ok()?;
    let resources_obj = page_dict.get(b"Resources").ok()?;
    let resources_dict = resolve_dict(doc, resources_obj)?;
    let font_obj = resources_dict.get(b"Font").ok()?;
    let font_dict = resolve_dict(doc, font_obj)?;

    for (name, font_ref) in font_dict.iter() {
        let Object::Reference(font_id) = font_ref else { continue };
        let Ok(font_obj) = doc.get_object(*font_id) else { continue };
        let Ok(fd) = font_obj.as_dict() else { continue };

        let subtype = fd
            .get(b"Subtype")
            .ok()
            .and_then(|o| if let Object::Name(n) = o { Some(n.as_slice()) } else { None });

        let font_info = match subtype {
            Some(b"Type0") => match collect_type0_font(fd, doc) {
                Some(fi) => fi,
                None => continue,
            },
            Some(b"Type1") | Some(b"MMType1") | Some(b"TrueType") => {
                collect_simple_font(fd, doc)
            }
            _ => continue,
        };

        fonts.insert(name.clone(), font_info);
    }

    Some(fonts)
}

fn collect_type0_font(fd: &Dictionary, doc: &lopdf::Document) -> Option<FontInfo> {
    let to_unicode = try_parse_to_unicode(fd, doc).unwrap_or_default();
    // When ToUnicode is absent and the encoding is Identity-H/V, fall back to treating
    // the 2-byte character code directly as a Unicode scalar (best-effort).
    let identity_fallback = to_unicode.is_empty() && is_identity_cmap(fd);

    let desc_obj = fd.get(b"DescendantFonts").ok()?;
    let Object::Array(desc_arr) = desc_obj else { return None };
    let Some(Object::Reference(cid_id)) = desc_arr.first() else { return None };
    let Ok(cid_obj) = doc.get_object(*cid_id) else { return None };
    let Ok(cid_dict) = cid_obj.as_dict() else { return None };

    let dw = cid_dict
        .get(b"DW")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .map(|n| n as u32)
        .unwrap_or(1000);

    let w_runs = cid_dict
        .get(b"W")
        .ok()
        .and_then(|o| if let Object::Array(a) = o { Some(a.as_slice()) } else { None })
        .map(parse_w_array)
        .unwrap_or_default();

    Some(FontInfo { to_unicode, dw, w_runs, bytes_per_char: 2, identity_fallback })
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

fn collect_simple_font(fd: &Dictionary, doc: &lopdf::Document) -> FontInfo {
    let to_unicode = if let Some(map) = try_parse_to_unicode(fd, doc) {
        map
    } else {
        build_encoding_map(fd, doc)
    };

    let (w_runs, dw) = collect_simple_font_widths(fd, doc);
    FontInfo { to_unicode, dw, w_runs, bytes_per_char: 1, identity_fallback: false }
}

fn try_parse_to_unicode(
    fd: &Dictionary,
    doc: &lopdf::Document,
) -> Option<BTreeMap<u16, char>> {
    let to_uni_ref = fd.get(b"ToUnicode").ok()?;
    let Object::Reference(to_uni_id) = to_uni_ref else { return None };
    let Ok(to_uni_obj) = doc.get_object(*to_uni_id) else { return None };
    let Ok(stream) = to_uni_obj.as_stream() else { return None };
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

fn collect_simple_font_widths(
    fd: &Dictionary,
    doc: &lopdf::Document,
) -> (Vec<WidthRun>, u32) {
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
    (vec![WidthRun { start_gid: first_char, widths }], dw)
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
        .and_then(|o| if let Object::Name(n) = o { Some(n.as_slice()) } else { None })
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

fn apply_differences(
    enc_dict: &Dictionary,
    mut map: BTreeMap<u16, char>,
) -> BTreeMap<u16, char> {
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
    AGL_TABLE
        .binary_search_by_key(&s, |&(n, _)| n)
        .ok()
        .map(|i| AGL_TABLE[i].1)
}

/// Sorted by glyph name (required for binary_search_by_key).
static AGL_TABLE: &[(&str, char)] = &[
    // A
    ("A", 'A'), ("AE", 'Æ'), ("Aacute", 'Á'), ("Abreve", 'Ă'), ("Acircumflex", 'Â'),
    ("Adieresis", 'Ä'), ("Agrave", 'À'), ("Amacron", 'Ā'), ("Aogonek", 'Ą'),
    ("Aring", 'Å'), ("Atilde", 'Ã'),
    // B–D
    ("B", 'B'), ("C", 'C'), ("Cacute", 'Ć'), ("Ccaron", 'Č'), ("Ccedilla", 'Ç'),
    ("D", 'D'), ("Dcaron", 'Ď'), ("Dcroat", 'Đ'), ("Delta", '∆'),
    // E
    ("E", 'E'), ("Eacute", 'É'), ("Ecaron", 'Ě'), ("Ecircumflex", 'Ê'), ("Edieresis", 'Ë'),
    ("Egrave", 'È'), ("Emacron", 'Ē'), ("Eogonek", 'Ę'), ("Eth", 'Ð'), ("Euro", '€'),
    // F–H
    ("F", 'F'), ("G", 'G'), ("Gbreve", 'Ğ'), ("H", 'H'),
    // I–K
    ("I", 'I'), ("Iacute", 'Í'), ("Icircumflex", 'Î'), ("Idieresis", 'Ï'),
    ("Idotaccent", 'İ'), ("Igrave", 'Ì'), ("Imacron", 'Ī'), ("Iogonek", 'Į'),
    ("J", 'J'), ("K", 'K'),
    // L
    ("L", 'L'), ("Lacute", 'Ĺ'), ("Lcaron", 'Ľ'), ("Lcommaaccent", 'Ļ'), ("Lslash", 'Ł'),
    // M–N
    ("M", 'M'), ("N", 'N'), ("Nacute", 'Ń'), ("Ncaron", 'Ň'), ("Ncommaaccent", 'Ņ'),
    ("Ntilde", 'Ñ'),
    // O
    ("O", 'O'), ("OE", 'Œ'), ("Oacute", 'Ó'), ("Ocircumflex", 'Ô'), ("Odblacute", 'Ő'),
    ("Odieresis", 'Ö'), ("Ograve", 'Ò'), ("Omacron", 'Ō'), ("Omega", '\u{2126}'),
    ("Oslash", 'Ø'), ("Otilde", 'Õ'),
    // P–R
    ("P", 'P'), ("Q", 'Q'), ("R", 'R'), ("Racute", 'Ŕ'), ("Rcaron", 'Ř'),
    ("Rcommaaccent", 'Ŗ'),
    // S
    ("S", 'S'), ("Sacute", 'Ś'), ("Scaron", 'Š'), ("Scedilla", 'Ş'),
    ("Scommaaccent", 'Ș'),
    // T
    ("T", 'T'), ("Tcaron", 'Ť'), ("Tcedilla", 'Ţ'), ("Tcommaaccent", 'Ț'), ("Thorn", 'Þ'),
    // U
    ("U", 'U'), ("Uacute", 'Ú'), ("Ucircumflex", 'Û'), ("Udblacute", 'Ű'), ("Udieresis", 'Ü'),
    ("Ugrave", 'Ù'), ("Umacron", 'Ū'), ("Uogonek", 'Ų'), ("Uring", 'Ů'),
    ("V", 'V'), ("W", 'W'), ("X", 'X'),
    // Y–Z
    ("Y", 'Y'), ("Yacute", 'Ý'), ("Ydieresis", 'Ÿ'),
    ("Z", 'Z'), ("Zacute", 'Ź'), ("Zcaron", 'Ž'), ("Zdotaccent", 'Ż'),
    // a
    ("a", 'a'), ("aacute", 'á'), ("abreve", 'ă'), ("acircumflex", 'â'), ("adieresis", 'ä'),
    ("ae", 'æ'), ("agrave", 'à'), ("amacron", 'ā'), ("ampersand", '&'), ("aogonek", 'ą'),
    ("approxequal", '≈'), ("aring", 'å'), ("asciicircum", '^'), ("asciitilde", '~'),
    ("asterisk", '*'), ("at", '@'), ("atilde", 'ã'),
    // b–c
    ("b", 'b'), ("backslash", '\\'), ("bar", '|'), ("braceleft", '{'),
    ("braceright", '}'), ("bracketleft", '['), ("bracketright", ']'),
    ("breve", '˘'), ("brokenbar", '¦'), ("bullet", '•'),
    ("c", 'c'), ("cacute", 'ć'), ("caron", 'ˇ'), ("ccaron", 'č'), ("ccedilla", 'ç'),
    ("cedilla", '¸'), ("cent", '¢'), ("circumflex", 'ˆ'), ("colon", ':'), ("comma", ','),
    ("copyright", '©'), ("currency", '¤'),
    // d
    ("d", 'd'), ("dagger", '†'), ("daggerdbl", '‡'), ("dcaron", 'ď'), ("dcroat", 'đ'),
    ("degree", '°'), ("dieresis", '¨'), ("divide", '÷'), ("dollar", '$'),
    ("dotaccent", '˙'), ("dotlessi", 'ı'),
    // e
    ("e", 'e'), ("eacute", 'é'), ("ecaron", 'ě'), ("ecircumflex", 'ê'), ("edieresis", 'ë'),
    ("egrave", 'è'), ("eight", '8'), ("ellipsis", '…'), ("emacron", 'ē'), ("emdash", '—'),
    ("endash", '–'), ("eogonek", 'ę'), ("equal", '='), ("eth", 'ð'), ("euro", '€'),
    ("exclam", '!'), ("exclamdown", '¡'),
    // f
    ("f", 'f'), ("ff", '\u{FB00}'), ("ffi", '\u{FB03}'), ("ffl", '\u{FB04}'),
    ("fi", '\u{FB01}'), ("five", '5'), ("fl", '\u{FB02}'), ("florin", 'ƒ'),
    ("four", '4'), ("fraction", '⁄'),
    // g
    ("g", 'g'), ("gbreve", 'ğ'), ("germandbls", 'ß'), ("grave", '`'), ("greater", '>'),
    ("greaterequal", '≥'), ("guillemotleft", '«'), ("guillemotright", '»'),
    ("guilsinglleft", '‹'), ("guilsinglright", '›'),
    // h–i
    ("h", 'h'), ("hungarumlaut", '˝'), ("hyphen", '-'),
    ("i", 'i'), ("iacute", 'í'), ("icircumflex", 'î'), ("idieresis", 'ï'),
    ("idotaccent", 'ı'), ("igrave", 'ì'), ("imacron", 'ī'), ("infinity", '∞'),
    ("integral", '∫'), ("iogonek", 'į'),
    // j–k
    ("j", 'j'), ("k", 'k'),
    // l
    ("l", 'l'), ("lacute", 'ĺ'), ("lcaron", 'ľ'), ("lcommaaccent", 'ļ'),
    ("less", '<'), ("lessequal", '≤'), ("logicalnot", '¬'), ("lozenge", '◊'), ("lslash", 'ł'),
    // m–n
    ("m", 'm'), ("macron", '¯'), ("mu", 'µ'), ("multiply", '×'),
    ("n", 'n'), ("nacute", 'ń'), ("ncaron", 'ň'), ("ncommaaccent", 'ņ'), ("nine", '9'),
    ("notequal", '≠'), ("ntilde", 'ñ'), ("numbersign", '#'),
    // o
    ("o", 'o'), ("oacute", 'ó'), ("ocircumflex", 'ô'), ("odblacute", 'ő'), ("odieresis", 'ö'),
    ("oe", 'œ'), ("ogonek", '˛'), ("ograve", 'ò'), ("omacron", 'ō'), ("one", '1'),
    ("onehalf", '½'), ("onequarter", '¼'), ("onesuperior", '¹'),
    ("ordfeminine", 'ª'), ("ordmasculine", 'º'), ("oslash", 'ø'), ("otilde", 'õ'),
    // p–q
    ("p", 'p'), ("paragraph", '¶'), ("parenleft", '('), ("parenright", ')'),
    ("partialdiff", '∂'), ("percent", '%'), ("period", '.'),
    ("periodcentered", '·'), ("perthousand", '‰'), ("pi", 'π'),
    ("plus", '+'), ("plusminus", '±'), ("product", '∏'),
    ("q", 'q'), ("question", '?'), ("questiondown", '¿'),
    ("quotedbl", '"'), ("quotedblbase", '„'), ("quotedblleft", '"'),
    ("quotedblright", '"'), ("quoteleft", '\u{2018}'),
    ("quoteright", '\u{2019}'), ("quotesinglbase", '‚'), ("quotesingle", '\''),
    // r
    ("r", 'r'), ("racute", 'ŕ'), ("radical", '√'), ("rcaron", 'ř'), ("rcommaaccent", 'ŗ'),
    ("registered", '®'), ("ring", '˚'),
    // s
    ("s", 's'), ("sacute", 'ś'), ("scaron", 'š'), ("scedilla", 'ş'),
    ("scommaaccent", 'ș'), ("section", '§'), ("semicolon", ';'),
    ("seven", '7'), ("six", '6'), ("slash", '/'), ("space", ' '),
    ("sterling", '£'), ("summation", '∑'),
    // t
    ("t", 't'), ("tcaron", 'ť'), ("tcedilla", 'ţ'), ("tcommaaccent", 'ț'),
    ("thorn", 'þ'), ("three", '3'), ("threequarters", '¾'),
    ("threesuperior", '³'), ("tilde", '˜'), ("trademark", '™'),
    ("two", '2'), ("twosuperior", '²'),
    // u
    ("u", 'u'), ("uacute", 'ú'), ("ucircumflex", 'û'), ("udblacute", 'ű'), ("udieresis", 'ü'),
    ("ugrave", 'ù'), ("umacron", 'ū'), ("underscore", '_'), ("uogonek", 'ų'), ("uring", 'ů'),
    // v–x
    ("v", 'v'), ("w", 'w'), ("x", 'x'),
    // y–z
    ("y", 'y'), ("yacute", 'ý'), ("ydieresis", 'ÿ'), ("yen", '¥'),
    ("z", 'z'), ("zacute", 'ź'), ("zcaron", 'ž'), ("zdotaccent", 'ż'), ("zero", '0'),
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
    let gid_tok = match parts.next() { Some(s) => s, None => return };
    let uni_tok = match parts.next() { Some(s) => s, None => return };

    let gid_hex = gid_tok.trim_start_matches('<').trim_end_matches('>');
    let uni_hex = uni_tok.trim_start_matches('<').trim_end_matches('>');

    let Ok(gid) = u16::from_str_radix(gid_hex, 16) else { return };

    let ch = hex_to_char(uni_hex);
    if let Some(ch) = ch {
        map.insert(gid, ch);
    }
}

fn parse_bfrange_line(line: &str, map: &mut BTreeMap<u16, char>) {
    // <lo> <hi> <dst>  or  <lo> <hi> [<u1> <u2> ...]
    // Use split_ascii_whitespace so tabs / multiple spaces between tokens are handled.
    let mut toks = line.split_ascii_whitespace();
    let lo_tok = match toks.next() { Some(s) => s, None => return };
    let hi_tok = match toks.next() { Some(s) => s, None => return };
    // Reconstruct rest from the original line starting at the third non-whitespace span.
    let rest = {
        let skip2 = line
            .trim_start()
            .trim_start_matches(|c: char| !c.is_ascii_whitespace()) // skip lo_tok
            .trim_start_matches(|c: char| c.is_ascii_whitespace())  // skip ws
            .trim_start_matches(|c: char| !c.is_ascii_whitespace()) // skip hi_tok
            .trim_start();
        if skip2.is_empty() { return }
        skip2
    };

    let lo_hex = lo_tok.trim_start_matches('<').trim_end_matches('>');
    let hi_hex = hi_tok.trim_start_matches('<').trim_end_matches('>');
    let Ok(lo) = u16::from_str_radix(lo_hex, 16) else { return };
    let Ok(hi) = u16::from_str_radix(hi_hex, 16) else { return };
    if lo > hi { return; }

    if rest.starts_with('[') {
        // Explicit array form: [<u1> <u2> ...]
        let inner = rest.trim_start_matches('[').trim_end_matches(']');
        let mut code = lo;
        for tok in inner.split_whitespace() {
            if code > hi { break; }
            let hex = tok.trim_start_matches('<').trim_end_matches('>');
            if let Some(ch) = hex_to_char(hex) {
                map.insert(code, ch);
            }
            code = code.saturating_add(1);
        }
    } else {
        // Contiguous range: <dst_start>
        let dst_hex = rest.trim_start_matches('<').trim_end_matches('>');
        let Ok(dst_start) = u32::from_str_radix(dst_hex, 16) else { return };
        for i in 0..=(hi as u32).saturating_sub(lo as u32) {
            let code = lo + i as u16;
            // Guard against adversarially crafted CMaps with dst_start near u32::MAX.
            let Some(cp) = dst_start.checked_add(i) else { break };
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
                let cp = 0x10000u32
                    + ((hi as u32 - 0xD800) << 10)
                    + (lo as u32 - 0xDC00);
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
            Err(_) => { i += 1; continue; }
        };
        i += 1;
        if i >= arr.len() { break; }

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
                    Err(_) => { i += 1; continue; }
                };
                i += 1;
                if i >= arr.len() { break; }
                let w = match arr[i].as_i64() {
                    Ok(n) => n as u32,
                    Err(_) => { i += 1; continue; }
                };
                i += 1;
                let count = (end_gid as usize).saturating_sub(start_gid as usize) + 1;
                runs.push(WidthRun { start_gid, widths: vec![w; count] });
            }
            _ => { i += 1; }
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

        if is_pdf_whitespace(b) { i += 1; continue; }
        if b == b'%' {
            while i < input.len() && input[i] != b'\r' && input[i] != b'\n' { i += 1; }
            continue;
        }
        if b == b'<' {
            if i + 1 < input.len() && input[i + 1] == b'<' {
                // Dictionary literal — skip until >>
                i += 2;
                while i + 1 < input.len()
                    && !(input[i] == b'>' && input[i + 1] == b'>')
                {
                    i += 1;
                }
                if i + 1 < input.len() { i += 2; }
                continue;
            }
            // Hex string
            i += 1;
            let start = i;
            while i < input.len() && input[i] != b'>' { i += 1; }
            let hex = &input[start..i];
            if i < input.len() { i += 1; }
            tokens.push(Token::HexStr(decode_hex_bytes(hex)));
            continue;
        }
        if b == b'/' {
            i += 1;
            let start = i;
            while i < input.len()
                && !is_pdf_whitespace(input[i])
                && !is_pdf_delimiter(input[i])
            {
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
        if b == b']' { i += 1; continue; }
        if b == b'(' {
            let (bytes, end_i) = parse_literal_string(input, i + 1);
            i = end_i;
            tokens.push(Token::LitStr(bytes));
            continue;
        }

        // Number or keyword
        let start = i;
        while i < input.len()
            && !is_pdf_whitespace(input[i])
            && !is_pdf_delimiter(input[i])
        {
            i += 1;
        }
        let word = &input[start..i];
        if word.is_empty() { i += 1; continue; }
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
        if is_pdf_whitespace(b) { i += 1; continue; }
        if b == b']' { i += 1; return (tokens, i); }
        if b == b'<' && (i + 1 >= input.len() || input[i + 1] != b'<') {
            i += 1;
            let start = i;
            while i < input.len() && input[i] != b'>' { i += 1; }
            let hex = &input[start..i];
            if i < input.len() { i += 1; }
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
        while i < input.len()
            && !is_pdf_whitespace(input[i])
            && !is_pdf_delimiter(input[i])
        {
            i += 1;
        }
        let word = &input[start..i];
        if word.is_empty() { i += 1; continue; }
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
                if i >= input.len() { break; }
                match input[i] {
                    b'n'  => { out.push(b'\n'); i += 1; }
                    b'r'  => { out.push(b'\r'); i += 1; }
                    b't'  => { out.push(b'\t'); i += 1; }
                    b'\\' => { out.push(b'\\'); i += 1; }
                    b'('  => { out.push(b'(');  i += 1; }
                    b')'  => { out.push(b')');  i += 1; }
                    b'\r' => {
                        // Line continuation: \<CR> or \<CR><LF>
                        i += 1;
                        if i < input.len() && input[i] == b'\n' { i += 1; }
                    }
                    b'\n' => { i += 1; } // \<LF> line continuation
                    d @ b'0'..=b'7' => {
                        // Octal escape: 1–3 digits
                        let mut val = (d - b'0') as u16;
                        i += 1;
                        let mut count = 1;
                        while count < 3
                            && i < input.len()
                            && (b'0'..=b'7').contains(&input[i])
                        {
                            val = val * 8 + (input[i] - b'0') as u16;
                            i += 1;
                            count += 1;
                        }
                        out.push((val & 0xFF) as u8);
                    }
                    _ => { out.push(input[i]); i += 1; }
                }
            }
            b'(' => { depth += 1; out.push(b'('); i += 1; }
            b')' => {
                depth -= 1;
                if depth > 0 { out.push(b')'); }
                i += 1;
            }
            b => { out.push(b); i += 1; }
        }
    }
    (out, i)
}

pub(crate) fn decode_hex_bytes(hex: &[u8]) -> Vec<u8> {
    let cleaned: Vec<u8> =
        hex.iter().filter(|&&b| !is_pdf_whitespace(b)).copied().collect();
    let mut padded = cleaned;
    if !padded.len().is_multiple_of(2) { padded.push(b'0'); }
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
    matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

// ---------------------------------------------------------------------------
// Step 4: State machine over token stream
// ---------------------------------------------------------------------------

fn parse_content_stream(
    bytes: &[u8],
    fonts: &HashMap<Vec<u8>, FontInfo>,
    out: &mut Vec<TextFragment>,
) {
    let tokens = tokenize(bytes);
    let mut stack: Vec<Token> = Vec::new();
    let mut in_bt = false;
    let mut font_name: Vec<u8> = Vec::new();
    let mut font_size: f32 = 12.0;
    let mut x: f32 = 0.0;
    let mut y: f32 = 0.0;
    let mut cur_color: [f32; 3] = [0.0, 0.0, 0.0];
    let mut cur_render_mode: u8 = 0;

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
                    if let (Some(Token::Number(size)), Some(Token::Name(name))) =
                        (top, second)
                    {
                        font_name = name;
                        font_size = size;
                    }
                    stack.clear();
                }
                b"Td" | b"TD" if in_bt => {
                    let top = stack.pop();
                    let second = stack.pop();
                    if let (Some(Token::Number(ty)), Some(Token::Number(tx))) =
                        (top, second)
                    {
                        x += tx;
                        y += ty;
                    }
                    stack.clear();
                }
                b"Tm" if in_bt => {
                    let pop_f = stack.pop();
                    let pop_e = stack.pop();
                    for _ in 0..4 { stack.pop(); }
                    if let (Some(Token::Number(fy)), Some(Token::Number(ex))) =
                        (pop_f, pop_e)
                    {
                        x = ex;
                        y = fy;
                    }
                    stack.clear();
                }
                b"Tr" => {
                    if let Some(Token::Number(mode)) = stack.pop() {
                        cur_render_mode = mode as u8;
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
                        cur_color = [rv, gv, bv];
                    }
                    stack.clear();
                }
                b"g" => {
                    if let Some(Token::Number(gray)) = stack.pop() {
                        cur_color = [gray, gray, gray];
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
                            &char_bytes, &font_name, font_size, x, y, fonts,
                            cur_color, cur_render_mode,
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
                                        b, &font_name, font_size, cur_x, y, fonts,
                                        cur_color, cur_render_mode,
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
                _ => { stack.clear(); }
            },
            other => { stack.push(other); }
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
    if char_bytes.is_empty() { return None; }
    let font_info = fonts.get(font_name)?;

    let mut text = String::new();
    let mut total_width = 0.0f32;

    match font_info.bytes_per_char {
        2 => {
            if !char_bytes.len().is_multiple_of(2) { return None; }
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
                let Some(&ch) = font_info.to_unicode.get(&code) else { continue };
                text.push(ch);
                let aw = font_info.advance_width(code);
                total_width += aw as f32 / 1000.0 * font_size;
            }
        }
    }

    if text.is_empty() { return None; }
    Some(TextFragment {
        text,
        x,
        y,
        width: total_width,
        font_size,
        font_name: String::from_utf8_lossy(font_name).into_owned(),
        color,
        invisible: render_mode == 3,
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
            .filter_map(|t| if let Token::Keyword(k) = t { Some(k.as_slice()) } else { None })
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
            w_runs: vec![WidthRun { start_gid: 5, widths: vec![600] }],
            bytes_per_char: 2,
            identity_fallback: false,
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
}
