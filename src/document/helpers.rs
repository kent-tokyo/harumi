use lopdf::{Dictionary, Object, ObjectId, Stream};
use ttf_parser::Face;

use crate::error::{Error, Result};

use super::types::{Color, FieldType, FormField};

/// The font selected for a character by [`diagnose_font_fallback`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphResolution {
    /// The primary font contains a glyph for the character.
    Primary,
    /// The fallback font contains a glyph while the primary font does not.
    Fallback,
    /// Neither configured font contains a glyph.
    Missing,
}

/// Resolution of one distinct character in a primary/fallback font chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphFallbackDiagnostic {
    pub character: char,
    pub resolution: GlyphResolution,
}

/// Reports how each distinct Unicode scalar in `text` resolves across the
/// primary font and one optional fallback font.
///
/// Results preserve first-seen order. This is a diagnostic only; it does not
/// shape text or apply script-specific fallback rules.
pub fn diagnose_font_fallback(
    text: &str,
    primary_font_bytes: &[u8],
    fallback_font_bytes: Option<&[u8]>,
) -> Vec<GlyphFallbackDiagnostic> {
    let primary = Face::parse(primary_font_bytes, 0).ok();
    let fallback = fallback_font_bytes.and_then(|bytes| Face::parse(bytes, 0).ok());
    let mut diagnostics = Vec::new();
    for character in text.chars() {
        if diagnostics
            .iter()
            .any(|item: &GlyphFallbackDiagnostic| item.character == character)
        {
            continue;
        }
        let resolution = if primary
            .as_ref()
            .is_some_and(|face| face.glyph_index(character).is_some())
        {
            GlyphResolution::Primary
        } else if fallback
            .as_ref()
            .is_some_and(|face| face.glyph_index(character).is_some())
        {
            GlyphResolution::Fallback
        } else {
            GlyphResolution::Missing
        };
        diagnostics.push(GlyphFallbackDiagnostic {
            character,
            resolution,
        });
    }
    diagnostics
}

pub(super) fn lopdf_string_to_rust(obj: &lopdf::Object) -> Option<String> {
    match obj {
        lopdf::Object::String(bytes, _) => {
            if bytes.starts_with(&[0xFE, 0xFF]) {
                let units: Vec<u16> = bytes[2..]
                    .chunks(2)
                    .map(|c| u16::from_be_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
                    .collect();
                String::from_utf16(&units).ok()
            } else {
                String::from_utf8(bytes.clone())
                    .ok()
                    .or_else(|| Some(bytes.iter().map(|&b| b as char).collect()))
            }
        }
        _ => None,
    }
}

/// Encodes a Rust `&str` as a PDF text string.
///
/// ASCII-only strings use a literal byte encoding. Strings containing non-ASCII
/// characters (e.g. CJK) are encoded as UTF-16BE with a 0xFE 0xFF BOM prefix,
/// which is the standard for PDF `/Title` and other text string fields.
pub(super) fn pdf_text_string(s: &str) -> Object {
    use lopdf::StringFormat;
    if s.is_ascii() {
        return Object::String(s.as_bytes().to_vec(), StringFormat::Literal);
    }
    let mut bytes: Vec<u8> = vec![0xFE, 0xFF]; // UTF-16BE BOM
    for unit in s.encode_utf16() {
        bytes.push((unit >> 8) as u8);
        bytes.push((unit & 0xFF) as u8);
    }
    Object::String(bytes, StringFormat::Literal)
}

/// Returns the ObjectId of the /AcroForm dictionary if one exists.
pub(super) fn acroform_id(doc: &lopdf::Document) -> Option<ObjectId> {
    let root_ref = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let catalog = doc.get_object(root_ref).ok()?.as_dict().ok()?;
    catalog.get(b"AcroForm").ok()?.as_reference().ok()
}

/// Ensures an /AcroForm entry exists in the document catalog.
/// Returns the ObjectId of the /AcroForm dictionary (either existing or newly created).
pub(super) fn ensure_acroform(doc: &mut lopdf::Document) -> Result<ObjectId> {
    // Check if AcroForm already exists
    if let Some(id) = acroform_id(doc) {
        return Ok(id);
    }

    // Create a new /AcroForm dictionary with an empty /Fields array
    let mut acroform_dict = Dictionary::new();
    acroform_dict.set("Fields", Object::Array(Vec::new()));
    let acroform_id = doc.add_object(Object::Dictionary(acroform_dict));

    // Get the catalog and add the /AcroForm reference
    let root_ref = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .ok_or(Error::InvalidInput("catalog not found".into()))?;

    let catalog = doc.get_object_mut(root_ref)?.as_dict_mut()?;
    catalog.set("AcroForm", Object::Reference(acroform_id));

    Ok(acroform_id)
}

/// Recursively collects `FormField` entries from a PDF field array.
pub(super) fn collect_fields_recursive(
    doc: &lopdf::Document,
    field_refs: &[Object],
    parent_name: &str,
    out: &mut Vec<FormField>,
) {
    for obj in field_refs {
        let id = match obj {
            Object::Reference(id) => *id,
            _ => continue,
        };
        let Ok(field_obj) = doc.get_object(id) else {
            continue;
        };
        let Ok(fd) = field_obj.as_dict() else {
            continue;
        };

        let partial = fd
            .get(b"T")
            .ok()
            .and_then(|o| match o {
                Object::String(b, _) => String::from_utf8(b.clone()).ok().or_else(|| {
                    if b.starts_with(&[0xFE, 0xFF]) {
                        let units: Vec<u16> = b[2..]
                            .chunks(2)
                            .map(|c| u16::from_be_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
                            .collect();
                        String::from_utf16(&units).ok()
                    } else {
                        None
                    }
                }),
                _ => None,
            })
            .unwrap_or_default();

        let full_name = if parent_name.is_empty() {
            partial.clone()
        } else if partial.is_empty() {
            parent_name.to_owned()
        } else {
            format!("{parent_name}.{partial}")
        };

        // If /Kids present: intermediate node, recurse.
        if let Ok(kids_obj) = fd.get(b"Kids") {
            let kids: Vec<Object> = match kids_obj {
                Object::Array(arr) => arr.clone(),
                Object::Reference(kid_id) => doc
                    .get_object(*kid_id)
                    .ok()
                    .and_then(|o| {
                        if let Object::Array(a) = o {
                            Some(a.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default(),
                _ => vec![],
            };
            collect_fields_recursive(doc, &kids, &full_name, out);
            continue;
        }

        // Leaf field.
        let ft = fd.get(b"FT").ok().and_then(|o| {
            if let Object::Name(n) = o {
                Some(n.as_slice())
            } else {
                None
            }
        });

        let field_type = match ft {
            Some(b"Tx") => FieldType::Text,
            Some(b"Btn") => {
                let flags = fd
                    .get(b"Ff")
                    .ok()
                    .and_then(|o| o.as_i64().ok())
                    .unwrap_or(0);
                if flags & (1 << 15) != 0 {
                    FieldType::Radio
                } else {
                    FieldType::Checkbox
                }
            }
            Some(b"Ch") => FieldType::Choice,
            Some(b"Sig") => FieldType::Signature,
            _ => FieldType::Unknown,
        };

        let value = fd
            .get(b"V")
            .ok()
            .map(|v| match v {
                Object::String(b, _) => {
                    if b.starts_with(&[0xFE, 0xFF]) {
                        let units: Vec<u16> = b[2..]
                            .chunks(2)
                            .map(|c| u16::from_be_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
                            .collect();
                        String::from_utf16(&units).unwrap_or_default()
                    } else {
                        String::from_utf8(b.clone()).unwrap_or_default()
                    }
                }
                Object::Name(n) => String::from_utf8_lossy(n).into_owned(),
                _ => String::new(),
            })
            .unwrap_or_default();

        if !full_name.is_empty() {
            out.push(FormField {
                name: full_name,
                field_type,
                value,
            });
        }
    }
}

/// Collects (ObjectId, FieldType, full_name) for all leaf fields under /AcroForm.
pub(super) fn collect_field_ids(
    doc: &lopdf::Document,
    acroform_id: ObjectId,
) -> Vec<(ObjectId, FieldType, String)> {
    let Ok(acroform) = doc.get_object(acroform_id).and_then(|o| o.as_dict()) else {
        return vec![];
    };
    let field_refs: Vec<Object> = match acroform.get(b"Fields") {
        Ok(Object::Array(arr)) => arr.clone(),
        Ok(Object::Reference(id)) => doc
            .get_object(*id)
            .ok()
            .and_then(|o| {
                if let Object::Array(a) = o {
                    Some(a.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default(),
        _ => return vec![],
    };

    let mut out = Vec::new();
    collect_field_ids_recursive(doc, &field_refs, "", &mut out);
    out
}

pub(super) fn collect_field_ids_recursive(
    doc: &lopdf::Document,
    field_refs: &[Object],
    parent_name: &str,
    out: &mut Vec<(ObjectId, FieldType, String)>,
) {
    for obj in field_refs {
        let id = match obj {
            Object::Reference(id) => *id,
            _ => continue,
        };
        let Ok(field_obj) = doc.get_object(id) else {
            continue;
        };
        let Ok(fd) = field_obj.as_dict() else {
            continue;
        };

        let partial = fd
            .get(b"T")
            .ok()
            .and_then(lopdf_string_to_rust)
            .unwrap_or_default();

        let full_name = if parent_name.is_empty() {
            partial.clone()
        } else if partial.is_empty() {
            parent_name.to_owned()
        } else {
            format!("{parent_name}.{partial}")
        };

        if let Ok(kids_obj) = fd.get(b"Kids") {
            let kids: Vec<Object> = match kids_obj {
                Object::Array(arr) => arr.clone(),
                Object::Reference(kid_id) => doc
                    .get_object(*kid_id)
                    .ok()
                    .and_then(|o| {
                        if let Object::Array(a) = o {
                            Some(a.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default(),
                _ => vec![],
            };
            collect_field_ids_recursive(doc, &kids, &full_name, out);
            continue;
        }

        let ft = fd.get(b"FT").ok().and_then(|o| {
            if let Object::Name(n) = o {
                Some(n.as_slice())
            } else {
                None
            }
        });
        let field_type = match ft {
            Some(b"Tx") => FieldType::Text,
            Some(b"Btn") => {
                let flags = fd
                    .get(b"Ff")
                    .ok()
                    .and_then(|o| o.as_i64().ok())
                    .unwrap_or(0);
                if flags & (1 << 15) != 0 {
                    FieldType::Radio
                } else {
                    FieldType::Checkbox
                }
            }
            Some(b"Ch") => FieldType::Choice,
            Some(b"Sig") => FieldType::Signature,
            _ => FieldType::Unknown,
        };

        if !full_name.is_empty() {
            out.push((id, field_type, full_name));
        }
    }
}

/// Builds a markup annotation dictionary (Highlight, Underline, StrikeOut).
pub(super) fn build_markup_annot(subtype: &[u8], rect: [f32; 4], color: Color) -> Dictionary {
    let x2 = rect[0] + rect[2];
    let y2 = rect[1] + rect[3];
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"Annot".to_vec()));
    d.set("Subtype", Object::Name(subtype.to_vec()));
    d.set(
        "Rect",
        Object::Array(vec![
            Object::Real(rect[0]),
            Object::Real(rect[1]),
            Object::Real(x2),
            Object::Real(y2),
        ]),
    );
    // QuadPoints: upper-left, upper-right, lower-left, lower-right (Acrobat convention)
    d.set(
        "QuadPoints",
        Object::Array(vec![
            Object::Real(rect[0]),
            Object::Real(y2),
            Object::Real(x2),
            Object::Real(y2),
            Object::Real(rect[0]),
            Object::Real(rect[1]),
            Object::Real(x2),
            Object::Real(rect[1]),
        ]),
    );
    let color_array = match color {
        Color::Rgb(c) => vec![Object::Real(c[0]), Object::Real(c[1]), Object::Real(c[2])],
        Color::Cmyk(c) => vec![
            Object::Real(c[0]),
            Object::Real(c[1]),
            Object::Real(c[2]),
            Object::Real(c[3]),
        ],
    };
    d.set("C", Object::Array(color_array));
    d.set(
        "Border",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(0),
        ]),
    );
    d
}

/// Builds the common fields of a /Link annotation dictionary (without the /A or /Dest key).
pub(super) fn build_link_annot_base(rect: [f32; 4]) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"Annot".to_vec()));
    d.set("Subtype", Object::Name(b"Link".to_vec()));
    d.set(
        "Rect",
        Object::Array(vec![
            Object::Real(rect[0]),
            Object::Real(rect[1]),
            Object::Real(rect[0] + rect[2]),
            Object::Real(rect[1] + rect[3]),
        ]),
    );
    // No visible border ([0 0 0] = no border)
    d.set(
        "Border",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(0),
        ]),
    );
    d
}

/// Appends an annotation object reference to the /Annots array of a page dictionary.
///
/// Handles both the case where /Annots is a direct array and where it is an
/// indirect reference to an array object.
pub(super) fn append_annotation_to_page(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    annot_id: ObjectId,
) -> Result<()> {
    let new_ref = Object::Reference(annot_id);

    // Read the current /Annots value without borrowing `doc` mutably.
    let annots_val = doc
        .get_object(page_id)?
        .as_dict()?
        .get(b"Annots")
        .ok()
        .cloned();

    match annots_val {
        Some(Object::Array(mut arr)) => {
            arr.push(new_ref);
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Annots", Object::Array(arr));
        }
        Some(Object::Reference(arr_id)) => {
            // /Annots points to an indirect array object.
            let is_array = doc
                .get_object(arr_id)
                .ok()
                .map(|o| matches!(o, Object::Array(_)))
                .unwrap_or(false);
            if is_array {
                doc.get_object_mut(arr_id)?.as_array_mut()?.push(new_ref);
            } else {
                // Indirect reference doesn't point to an array — replace with direct array.
                doc.get_object_mut(page_id)?
                    .as_dict_mut()?
                    .set("Annots", Object::Array(vec![new_ref]));
            }
        }
        _ => {
            // No /Annots entry (or malformed) — create a fresh direct array.
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Annots", Object::Array(vec![new_ref]));
        }
    }
    Ok(())
}

/// Reads a named page box (e.g. CropBox) from the page dict.
/// Returns `None` when the key is absent. Parses `[x1 y1 x2 y2]` → `[x, y, w, h]`.
pub(super) fn read_page_box(
    doc: &lopdf::Document,
    page_id: ObjectId,
    key: &[u8],
) -> Result<Option<[f32; 4]>> {
    let dict = doc.get_object(page_id)?.as_dict()?;
    match dict.get(key).ok().cloned() {
        Some(Object::Reference(ref_id)) => parse_box_array(doc.get_object(ref_id)?).map(Some),
        Some(obj) => parse_box_array(&obj).map(Some),
        None => Ok(None),
    }
}

pub(super) fn parse_box_array(obj: &Object) -> Result<[f32; 4]> {
    let arr = obj.as_array()?;
    if arr.len() < 4 {
        return Err(Error::Pdf(lopdf::Error::DictKey(
            "box array too short".to_string(),
        )));
    }
    let get = |i: usize| -> f32 {
        match &arr[i] {
            Object::Integer(v) => *v as f32,
            Object::Real(v) => *v,
            _ => 0.0,
        }
    };
    let (x1, y1, x2, y2) = (get(0), get(1), get(2), get(3));
    Ok([x1, y1, x2 - x1, y2 - y1])
}

/// Writes a named page box (e.g. CropBox) to the page dict.
/// Accepts `[x, y, w, h]` and stores as `[x1 y1 x2 y2]`.
pub(super) fn set_page_box(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    key: &[u8],
    rect: [f32; 4],
) -> Result<()> {
    let box_arr = Object::Array(vec![
        Object::Real(rect[0]),
        Object::Real(rect[1]),
        Object::Real(rect[0] + rect[2]),
        Object::Real(rect[1] + rect[3]),
    ]);
    doc.get_object_mut(page_id)?
        .as_dict_mut()?
        .set(key, box_arr);
    Ok(())
}

/// Returns a 16-byte pseudo-random document ID using system time + PID.
/// Used as the /ID trailer entry required by PDF encryption (RC4/AES).
pub(super) fn generate_file_id() -> [u8; 16] {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id() as u128;
    // LCG mix so that docs saved at the same nanosecond differ.
    // Mix time + PID using Knuth's MMIX LCG constants (a=6364136223846793005,
    // c=1442695040888963407) so that docs saved in the same process at similar
    // times produce distinct IDs. Not cryptographically secure, but sufficient
    // for PDF's /ID uniqueness requirement (PDF spec §14.4).
    let mixed = nanos
        .wrapping_mul(6364136223846793005u128)
        .wrapping_add(pid.wrapping_mul(1442695040888963407u128));
    let mut id = [0u8; 16];
    id.copy_from_slice(&mixed.to_le_bytes());
    id
}

pub(super) fn map_lopdf_password_err(e: lopdf::Error) -> Error {
    match e {
        lopdf::Error::InvalidPassword => Error::WrongPassword,
        lopdf::Error::IO(io_err) => Error::Io(io_err),
        other => Error::Pdf(other),
    }
}

pub(super) fn check_finite(values: &[f32], label: &str) -> Result<()> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err(Error::InvalidInput(format!(
            "{label} contains NaN or Infinity"
        )));
    }
    Ok(())
}

pub(super) fn check_positive_size(width: f32, height: f32, label: &str) -> Result<()> {
    if width <= 0.0 || height <= 0.0 {
        return Err(Error::InvalidInput(format!(
            "{label}: rect width and height must be positive, got ({width}, {height})"
        )));
    }
    Ok(())
}

/// Returns true for characters that can line-break at any position (CJK scripts).
pub(crate) fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1100..=0x11FF    // Hangul Jamo
        | 0x3000..=0x9FFF  // CJK unified ideographs, hiragana, katakana, etc.
        | 0xA960..=0xA97F  // Hangul Jamo Extended-A
        | 0xAC00..=0xD7FF  // Hangul syllables + Jamo Extended-B
        | 0xF900..=0xFAFF  // CJK compatibility ideographs
        | 0xFE30..=0xFE4F  // CJK compatibility forms
        | 0xFF00..=0xFFEF  // fullwidth / halfwidth forms
        | 0x20000..=0x2A6DF | 0x2A700..=0x2CEAF  // CJK extension B / C / D
    )
}

/// Returns true for punctuation that should not begin a line in CJK layout.
///
/// This is the small, deterministic subset shared by Flow, HTML, and text-box
/// fitting. Full Unicode Line Breaking / language-specific kinsoku remains a
/// future opt-in feature.
fn is_line_start_prohibited(ch: char) -> bool {
    matches!(
        ch,
        '、' | '。'
            | '，'
            | '．'
            | '：'
            | '；'
            | '！'
            | '？'
            | '・'
            | '〜'
            | '～'
            | '…'
            | '‥'
            | '〃'
            | '々'
            | 'ヽ'
            | 'ヾ'
            | 'ゝ'
            | 'ゞ'
            | '」'
            | '』'
            | '】'
            | '〕'
            | '〉'
            | '》'
            | '］'
            | '）'
            | '｝'
            | '”'
            | '’'
            | '»'
            | '›'
            | '!'
            | '?'
            | ','
            | '.'
            | ':'
            | ';'
    ) || is_non_spacing_or_joining_mark(ch)
        || is_non_breaking_character(ch)
}

/// Returns true for opening punctuation that should not end a line in CJK layout.
fn is_line_end_prohibited(ch: char) -> bool {
    matches!(
        ch,
        '(' | '['
            | '{'
            | '「'
            | '『'
            | '【'
            | '〔'
            | '〈'
            | '《'
            | '（'
            | '［'
            | '｛'
            | '｟'
            | '＜'
            | '“'
            | '‘'
            | '«'
            | '‹'
            | '﹁'
            | '﹃'
            | '﹙'
            | '﹝'
            | '﹤'
            | '〝'
            | '〖'
            | '〘'
    )
}

/// Unicode marks that must stay attached to the preceding grapheme cluster.
/// This is deliberately narrower than full UAX #14/#29 support, but prevents
/// the most damaging breaks for combining accents, variation selectors, and
/// emoji ZWJ sequences without adding a runtime dependency.
fn is_non_spacing_or_joining_mark(ch: char) -> bool {
    matches!(
        ch,
        '\u{0300}'..='\u{036f}'
            | '\u{1ab0}'..='\u{1aff}'
            | '\u{1dc0}'..='\u{1dff}'
            | '\u{20d0}'..='\u{20ff}'
            | '\u{fe00}'..='\u{fe0f}'
            | '\u{fe20}'..='\u{fe2f}'
            | '\u{200d}'
            | '\u{e0100}'..='\u{e01ef}'
    )
}

/// Characters that must not become a line boundary. This covers the common
/// Unicode no-break spaces and Word Joiner without claiming full UAX #14.
fn is_non_breaking_character(ch: char) -> bool {
    matches!(ch, '\u{00a0}' | '\u{202f}' | '\u{2060}' | '\u{feff}')
}

/// Width of one character in PDF points given the font face and font size.
/// Returns None if the character is not present in the font (no glyph mapping).
pub fn glyph_advance_pt(face: &Face, ch: char, font_size: f32) -> Option<f32> {
    let upem = face.units_per_em() as f32;
    face.glyph_index(ch)
        .and_then(|g| face.glyph_hor_advance(g))
        .map(|adv| adv as f32 * font_size / upem)
}

/// Return `true` when `font_bytes` contains a glyph for `ch`.
///
/// Uses [ttf-parser](https://docs.rs/ttf-parser) to check the font's `cmap` table.
/// Returns `false` when `font_bytes` cannot be parsed.
///
/// # Example
/// ```no_run
/// # let font_bytes = std::fs::read("NotoSansJP-Regular.ttf").unwrap();
/// assert!(harumi::font_covers_char(&font_bytes, '日'));
/// assert!(!harumi::font_covers_char(&font_bytes, '؟')); // Arabic question mark
/// ```
pub fn font_covers_char(font_bytes: &[u8], ch: char) -> bool {
    ttf_parser::Face::parse(font_bytes, 0)
        .map(|face| face.glyph_index(ch).is_some())
        .unwrap_or(false)
}

/// Calculate the total width of a text string in PDF points from raw TTF bytes.
///
/// This helper is useful for checking text overflow without needing access to Font objects.
/// Returns None if the font bytes are invalid or if any character is missing from the font.
pub fn calculate_text_width(text: &str, font_bytes: &[u8], font_size: f32) -> Option<f32> {
    let face = ttf_parser::Face::parse(font_bytes, 0).ok()?;
    Some(text_width_with_face(text, &face, font_size))
}

/// Like [`calculate_text_width`] but reuses a pre-parsed [`ttf_parser::Face`].
///
/// Use this inside loops (e.g. shrink-to-fit) to avoid re-parsing the font on
/// every iteration.  Characters missing from the font contribute 0 width.
pub(super) fn text_width_with_face(text: &str, face: &ttf_parser::Face<'_>, font_size: f32) -> f32 {
    text.chars()
        .filter_map(|ch| glyph_advance_pt(face, ch, font_size))
        .sum()
}

/// Greedy line-breaking for a single paragraph (no embedded newlines).
///
/// CJK text can break between characters, Latin text prefers ASCII-space word
/// boundaries, and a small kinsoku subset prevents closing punctuation from
/// becoming the first character of a line.
pub fn wrap_paragraph(paragraph: &str, face: &Face, font_size: f32, box_width: f32) -> Vec<String> {
    wrap_paragraph_with_fallback(paragraph, face, None, font_size, box_width)
}

/// Greedy line-breaking with an optional fallback face for missing glyphs.
pub(crate) fn wrap_paragraph_with_fallback(
    paragraph: &str,
    face: &Face,
    fallback: Option<&Face>,
    font_size: f32,
    box_width: f32,
) -> Vec<String> {
    // Validate inputs.
    // If font_size or box_width is invalid, return the paragraph as a single line.
    if !font_size.is_finite() || font_size <= 0.0 || !box_width.is_finite() || box_width <= 0.0 {
        return if paragraph.is_empty() {
            Vec::new()
        } else {
            vec![paragraph.to_owned()]
        };
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_w: f32 = 0.0;
    // byte index of last ASCII space in `current`; width after that space (= start of next word)
    let mut last_space_byte: Option<usize> = None;
    let mut width_at_word_start: f32 = 0.0;

    for ch in paragraph.chars() {
        let ch_w = glyph_advance_pt(face, ch, font_size)
            .or_else(|| fallback.and_then(|fallback| glyph_advance_pt(fallback, ch, font_size)))
            .unwrap_or(font_size * 0.5);

        if current_w + ch_w > box_width && !current.is_empty() {
            if current
                .chars()
                .last()
                .is_some_and(is_non_breaking_character)
            {
                // Keep the token following a no-break character attached. It is
                // better to exceed the nominal width than to create a semantic
                // break inside a protected boundary.
                current.push(ch);
                current_w += ch_w;
                continue;
            }
            if current.chars().count() > 1
                && current.chars().last().is_some_and(is_line_end_prohibited)
            {
                let last = current.pop().expect("last character was checked");
                let last_w = glyph_advance_pt(face, last, font_size)
                    .or_else(|| {
                        fallback.and_then(|fallback| glyph_advance_pt(fallback, last, font_size))
                    })
                    .unwrap_or(font_size * 0.5);
                lines.push(std::mem::take(&mut current));
                current.push(last);
                current_w = last_w;
                last_space_byte = None;
            }
            if is_cjk(ch) || last_space_byte.is_none() {
                // CJK or no word boundary found → break at the current character
                if is_line_start_prohibited(ch) {
                    // Keep closing punctuation with the preceding glyph. This
                    // may exceed the nominal width by one glyph, but avoids the
                    // more visible and semantically incorrect line-start mark.
                    current.push(ch);
                    current_w += ch_w;
                    continue;
                } else {
                    lines.push(std::mem::take(&mut current));
                    current_w = 0.0;
                    last_space_byte = None;
                }
            } else {
                // Break at the last space: emit everything before it, keep the word after
                let sp = last_space_byte.unwrap();
                let word = current[sp + 1..].to_owned(); // sp+1 safe: space is ASCII (1 byte)
                // Keep the boundary space in the emitted line. It remains visually
                // equivalent to trimming it at a line break, while preserving the
                // source text for extraction and downstream layout consumers.
                current.truncate(sp + 1);
                lines.push(std::mem::take(&mut current));
                current = word;
                current_w = (current_w - width_at_word_start).max(0.0);
                last_space_byte = None;
            }
        }

        if ch == ' ' {
            last_space_byte = Some(current.len()); // byte index of space before it is pushed
            width_at_word_start = current_w + ch_w; // total width including the space
        }
        current.push(ch);
        current_w += ch_w;
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Compute how `text` lays out inside a rectangle with the given font face, without
/// mutating any document state.  Used by [`crate::Document::fit_text_to_box`].
///
/// `line_height = fs * 1.2` — matches the constant in `replace_text_fragments_opts`.
pub(super) fn plan_text_fit(
    text: &str,
    face: &ttf_parser::Face<'_>,
    rect: [f32; 4],
    initial_font_size: f32,
    opts: &super::types::BoxFitOptions,
) -> super::types::FitResult {
    use super::types::{FitResult, OverflowPolicy, PlacementStatus};

    let [rx, ry, rw, rh] = rect;
    let fs0 = initial_font_size.max(opts.min_font_size.max(0.1));
    let min_fs = opts.min_font_size.max(0.1);

    let line_height = |fs: f32| fs * 1.2_f32;

    let (lines, fs, status) = match opts.overflow {
        OverflowPolicy::Shrink => {
            // No wrap — shrink until single line fits in width.
            let mut fs = fs0;
            loop {
                let w = text_width_with_face(text, face, fs);
                if w <= rw || fs <= min_fs {
                    break;
                }
                fs = (fs * rw / w).max(min_fs);
            }
            let status = if fs < fs0 {
                if fs <= min_fs {
                    PlacementStatus::ShrunkToMin
                } else {
                    PlacementStatus::Shrunk
                }
            } else {
                PlacementStatus::Ok
            };
            (vec![text.to_owned()], fs, status)
        }
        OverflowPolicy::WrapThenShrink => {
            let mut fs = fs0;
            let mut lines = if opts.wrap {
                wrap_paragraph(text, face, fs, rw)
            } else {
                vec![text.to_owned()]
            };
            // Shrink until total height fits or we hit min_font_size.
            loop {
                let total_h = lines.len() as f32 * line_height(fs);
                if total_h <= rh || fs <= min_fs {
                    break;
                }
                let factor = rh / total_h;
                fs = (fs * factor).max(min_fs);
                lines = if opts.wrap {
                    wrap_paragraph(text, face, fs, rw)
                } else {
                    vec![text.to_owned()]
                };
            }
            let status = if fs < fs0 {
                if fs <= min_fs {
                    PlacementStatus::ShrunkToMin
                } else {
                    PlacementStatus::Shrunk
                }
            } else {
                PlacementStatus::Ok
            };
            (lines, fs, status)
        }
        OverflowPolicy::Truncate => {
            let lines = if opts.wrap {
                wrap_paragraph(text, face, fs0, rw)
            } else {
                vec![text.to_owned()]
            };
            let original_count = lines.len();
            let lh = line_height(fs0);
            // When rh <= 0 the box fits nothing; use 0 so cap clamps to 0 and
            // take(cap.max(1)) keeps exactly one line (the minimum the API guarantees).
            let max_by_height = if lh > 0.0 && rh > 0.0 {
                (rh / lh).floor() as usize
            } else {
                0
            };
            let cap = opts.max_lines.unwrap_or(usize::MAX).min(max_by_height);
            let truncated: Vec<String> = lines.into_iter().take(cap.max(1)).collect();
            let status = if truncated.len() < original_count {
                PlacementStatus::Truncated
            } else {
                PlacementStatus::Ok
            };
            (truncated, fs0, status)
        }
        OverflowPolicy::Report => {
            let lines = if opts.wrap {
                wrap_paragraph(text, face, fs0, rw)
            } else {
                vec![text.to_owned()]
            };
            let original_report_count = lines.len();
            let lines: Vec<String> = if let Some(max) = opts.max_lines {
                lines.into_iter().take(max.max(1)).collect()
            } else {
                lines
            };
            // Track whether max_lines silently dropped lines; overflow flags are
            // finalized after this match.
            let status = if lines.len() < original_report_count {
                PlacementStatus::Truncated
            } else {
                PlacementStatus::Ok
            };
            (lines, fs0, status)
        }
    };

    let lh = line_height(fs);
    let used_h = lines.len() as f32 * lh;
    let used_w = lines
        .iter()
        .map(|l| text_width_with_face(l, face, fs))
        .fold(0.0_f32, f32::max)
        .min(rw);

    // Top-aligned within the requested rect (matching add_text_box placement).
    let used_rect = [rx, ry + rh - used_h, used_w, used_h];

    let overflow_horizontal = lines.iter().any(|l| text_width_with_face(l, face, fs) > rw);
    let overflow_vertical = used_h > rh;

    // Post-hoc status fixups using the computed overflow flags.
    let status = match opts.overflow {
        // Report: upgrade Ok→Overflow when text overflows, but don't override Truncated.
        OverflowPolicy::Report
            if !matches!(status, PlacementStatus::Truncated)
                && (overflow_horizontal || overflow_vertical) =>
        {
            PlacementStatus::Overflow
        }
        // Shrink / WrapThenShrink: when already at min_font_size and text still overflows,
        // report ShrunkToMin instead of Ok (loop exited because fs <= min_fs, not because
        // the text fit — e.g. initial_font_size == min_font_size).
        OverflowPolicy::Shrink | OverflowPolicy::WrapThenShrink
            if status == PlacementStatus::Ok
                && fs <= min_fs
                && (overflow_horizontal || overflow_vertical) =>
        {
            PlacementStatus::ShrunkToMin
        }
        _ => status,
    };

    FitResult {
        lines,
        font_size: fs,
        used_rect,
        overflow_horizontal,
        overflow_vertical,
        status,
    }
}

pub(super) fn root_pages_id(doc: &lopdf::Document) -> Result<ObjectId> {
    let root_ref = doc.trailer.get(b"Root")?.as_reference()?;
    let catalog = doc.get_object(root_ref)?.as_dict()?;
    Ok(catalog.get(b"Pages")?.as_reference()?)
}

/// Materialize inherited PDF page attributes onto `page_id` before the page's
/// `/Parent` is changed (i.e., before the tree is flattened).
///
/// The PDF spec allows `/MediaBox`, `/CropBox`, `/Rotate`, `/Resources`, and
/// `/UserUnit` to be placed on intermediate `/Pages` nodes and inherited by
/// descendant pages. When those intermediate nodes are bypassed (by re-parenting
/// pages directly to the root), any values they hold are no longer reachable via
/// the inheritance chain. This function copies the missing values directly onto
/// the page dict so they survive the re-parenting.
///
/// Closest ancestor wins: the first ancestor to provide a value for a given key
/// is used; outer ancestors' values for the same key are ignored.
pub(super) fn realize_page_inherited_attrs(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
) -> Result<()> {
    const INHERITABLE: &[&[u8]] = &[
        b"MediaBox",
        b"CropBox",
        b"Rotate",
        b"Resources",
        b"UserUnit",
    ];

    // Walk up the parent chain and collect attrs missing from the page itself.
    let mut to_apply: Vec<(Vec<u8>, Object)> = Vec::new();
    let mut cursor = page_id;
    let mut depth = 0u32;

    loop {
        if depth > 64 {
            break; // cycle / pathological-depth guard
        }
        depth += 1;

        // Get cursor's /Parent reference.
        let parent_id = match doc
            .get_object(cursor)
            .ok()
            .and_then(|o| o.as_dict().ok())
            .and_then(|d| d.get(b"Parent").ok())
            .and_then(|o| {
                if let Object::Reference(id) = o {
                    Some(*id)
                } else {
                    None
                }
            }) {
            Some(id) => id,
            None => break,
        };

        // Only inherit from /Pages nodes.
        let parent_is_pages = doc
            .get_object(parent_id)
            .ok()
            .and_then(|o| o.as_dict().ok())
            .and_then(|d| d.get(b"Type").ok())
            .and_then(|o| {
                if let Object::Name(n) = o {
                    Some(n.as_slice() == b"Pages")
                } else {
                    None
                }
            })
            .unwrap_or(false);

        if !parent_is_pages {
            break;
        }

        // Clone the parent dict so we can check its keys without holding a borrow.
        let parent_dict = match doc
            .get_object(parent_id)
            .ok()
            .and_then(|o| o.as_dict().ok())
        {
            Some(d) => d.clone(),
            None => break,
        };
        // Clone the page dict to check which keys are already present.
        let page_dict = match doc.get_object(page_id).ok().and_then(|o| o.as_dict().ok()) {
            Some(d) => d.clone(),
            None => break,
        };

        for &key in INHERITABLE {
            // Already on the page itself — skip.
            if page_dict.get(key).is_ok() {
                continue;
            }
            // Already queued from a closer ancestor — skip.
            if to_apply.iter().any(|(k, _)| k.as_slice() == key) {
                continue;
            }
            // Inherit from this ancestor.
            if let Ok(val) = parent_dict.get(key) {
                to_apply.push((key.to_vec(), val.clone()));
            }
        }

        cursor = parent_id;
    }

    // Write collected attributes onto the page dict.
    if !to_apply.is_empty() {
        let page_dict = doc.get_object_mut(page_id)?.as_dict_mut()?;
        for (key, val) in to_apply {
            page_dict.set(key, val);
        }
    }

    Ok(())
}

/// Inserts `new_stream_id` before all existing content streams in a page's `/Contents`.
pub(super) fn prepend_to_contents(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    new_stream_id: ObjectId,
) -> Result<()> {
    let contents_ref = doc
        .get_object(page_id)?
        .as_dict()?
        .get(b"Contents")
        .ok()
        .cloned();

    let new_ref = Object::Reference(new_stream_id);

    match contents_ref {
        Some(Object::Reference(r)) => {
            let is_array = doc
                .get_object(r)
                .ok()
                .map(|o| matches!(o, Object::Array(_)))
                .unwrap_or(false);
            if is_array {
                doc.get_object_mut(r)?.as_array_mut()?.insert(0, new_ref);
            } else {
                let arr = Object::Array(vec![new_ref, Object::Reference(r)]);
                doc.get_object_mut(page_id)?
                    .as_dict_mut()?
                    .set("Contents", arr);
            }
        }
        Some(Object::Array(mut arr)) => {
            arr.insert(0, new_ref);
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Contents", Object::Array(arr));
        }
        None => {
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Contents", new_ref);
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn append_to_contents(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    new_stream_id: ObjectId,
) -> Result<()> {
    let contents_ref = doc
        .get_object(page_id)?
        .as_dict()?
        .get(b"Contents")
        .ok()
        .cloned();

    let new_ref = Object::Reference(new_stream_id);

    match contents_ref {
        Some(Object::Reference(r)) => {
            // Check whether the reference points to an Array (indirect Contents array,
            // common in InDesign-generated PDFs) or a single content stream.
            let is_array = doc
                .get_object(r)
                .ok()
                .map(|o| matches!(o, Object::Array(_)))
                .unwrap_or(false);
            if is_array {
                let arr_obj = doc.get_object_mut(r)?.as_array_mut()?;
                arr_obj.push(new_ref);
            } else {
                let arr = Object::Array(vec![Object::Reference(r), new_ref]);
                doc.get_object_mut(page_id)?
                    .as_dict_mut()?
                    .set("Contents", arr);
            }
        }
        Some(Object::Array(mut arr)) => {
            arr.push(new_ref);
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Contents", Object::Array(arr));
        }
        None => {
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Contents", new_ref);
        }
        _ => {}
    }
    Ok(())
}

/// Wraps all existing content streams of a page in a `q`/`Q` pair to isolate any
/// unbalanced `cm` operators from affecting subsequently appended streams.
pub(super) fn wrap_page_contents_in_q_q(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
) -> Result<()> {
    let has_contents = doc
        .get_object(page_id)
        .ok()
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"Contents").ok().cloned())
        .is_some();
    if !has_contents {
        return Ok(());
    }
    let q_id = doc.add_object(Object::Stream(Stream::new(
        Dictionary::new(),
        b"q\n".to_vec(),
    )));
    let big_q_id = doc.add_object(Object::Stream(Stream::new(
        Dictionary::new(),
        b"Q\n".to_vec(),
    )));
    prepend_to_contents(doc, page_id, q_id)?;
    append_to_contents(doc, page_id, big_q_id)?;
    Ok(())
}

pub(super) fn add_font_to_resources(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    pdf_name: &[u8],
    type0_id: ObjectId,
) -> Result<()> {
    let resources_id: Option<ObjectId> = {
        let page_dict = doc.get_object(page_id)?.as_dict()?;
        match page_dict.get(b"Resources").ok() {
            Some(Object::Reference(r)) => Some(*r),
            _ => None,
        }
    };

    let font_ref = Object::Reference(type0_id);

    if let Some(res_id) = resources_id {
        let font_dict_id = {
            let res_dict = doc.get_object(res_id)?.as_dict()?;
            match res_dict.get(b"Font").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            }
        };
        if let Some(font_dict_id) = font_dict_id {
            doc.get_object_mut(font_dict_id)?
                .as_dict_mut()?
                .set(pdf_name, font_ref);
            return Ok(());
        }
        let res_dict = doc.get_object_mut(res_id)?.as_dict_mut()?;
        ensure_font_entry(res_dict, pdf_name, font_ref);
    } else {
        let inline_font_id = {
            let page_dict = doc.get_object(page_id)?.as_dict()?;
            match page_dict.get(b"Resources").ok() {
                Some(Object::Dictionary(resources)) => match resources.get(b"Font").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                },
                _ => None,
            }
        };
        if let Some(font_dict_id) = inline_font_id {
            doc.get_object_mut(font_dict_id)?
                .as_dict_mut()?
                .set(pdf_name, font_ref);
            return Ok(());
        }
        let page_dict = doc.get_object_mut(page_id)?.as_dict_mut()?;
        match page_dict.get_mut(b"Resources") {
            Ok(res_obj) => {
                let res_dict = res_obj.as_dict_mut()?;
                ensure_font_entry(res_dict, pdf_name, font_ref);
            }
            Err(_) => {
                let mut font_dict = Dictionary::new();
                font_dict.set(pdf_name, font_ref);
                let mut res_dict = Dictionary::new();
                res_dict.set("Font", Object::Dictionary(font_dict));
                page_dict.set("Resources", Object::Dictionary(res_dict));
            }
        }
    }

    Ok(())
}

/// Add a font to a Form XObject's own `/Resources/Font` dict.
///
/// The XObject's /Resources may be inline (Object::Dictionary in the stream dict) or
/// indirect (Object::Reference pointing to a separate dict object); both are handled.
pub(super) fn add_font_to_xobject_resources(
    doc: &mut lopdf::Document,
    xobj_id: lopdf::ObjectId,
    pdf_name: &[u8],
    type0_id: lopdf::ObjectId,
) -> Result<()> {
    let font_ref = Object::Reference(type0_id);

    // Determine whether /Resources is an indirect reference or inline.
    let resources_ref_id: Option<lopdf::ObjectId> = {
        let xobj_obj = doc.get_object(xobj_id)?;
        let xobj_stream = xobj_obj.as_stream()?;
        match xobj_stream.dict.get(b"Resources").ok() {
            Some(Object::Reference(r)) => Some(*r),
            _ => None,
        }
    };

    if let Some(res_id) = resources_ref_id {
        let res_dict = doc.get_object_mut(res_id)?.as_dict_mut()?;
        ensure_font_entry(res_dict, pdf_name, font_ref);
    } else {
        let xobj_obj = doc.get_object_mut(xobj_id)?;
        let xobj_stream = xobj_obj.as_stream_mut()?;
        match xobj_stream.dict.get_mut(b"Resources") {
            Ok(res_obj) => {
                if let Ok(res_dict) = res_obj.as_dict_mut() {
                    ensure_font_entry(res_dict, pdf_name, font_ref);
                }
            }
            Err(_) => {
                let mut font_dict = Dictionary::new();
                font_dict.set(pdf_name, font_ref);
                let mut res_dict = Dictionary::new();
                res_dict.set("Font", Object::Dictionary(font_dict));
                xobj_stream
                    .dict
                    .set("Resources", Object::Dictionary(res_dict));
            }
        }
    }

    Ok(())
}

pub(super) fn ensure_font_entry(res_dict: &mut Dictionary, pdf_name: &[u8], font_ref: Object) {
    match res_dict.get_mut(b"Font") {
        Ok(font_obj) => {
            if let Ok(fd) = font_obj.as_dict_mut() {
                fd.set(pdf_name, font_ref);
            }
        }
        Err(_) => {
            let mut font_dict = Dictionary::new();
            font_dict.set(pdf_name, font_ref);
            res_dict.set("Font", Object::Dictionary(font_dict));
        }
    }
}

/// Resolves the Resources dict for a page (direct or indirect) and applies `f`.
pub(super) fn with_resources_dict_mut<F>(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    f: F,
) -> Result<()>
where
    F: FnOnce(&mut Dictionary),
{
    let resources_id: Option<ObjectId> = {
        let page_dict = doc.get_object(page_id)?.as_dict()?;
        match page_dict.get(b"Resources").ok() {
            Some(Object::Reference(r)) => Some(*r),
            _ => None,
        }
    };

    if let Some(res_id) = resources_id {
        f(doc.get_object_mut(res_id)?.as_dict_mut()?);
    } else {
        let page_dict = doc.get_object_mut(page_id)?.as_dict_mut()?;
        match page_dict.get_mut(b"Resources") {
            Ok(res_obj) => f(res_obj.as_dict_mut()?),
            Err(_) => {
                let mut res_dict = Dictionary::new();
                f(&mut res_dict);
                page_dict.set("Resources", Object::Dictionary(res_dict));
            }
        }
    }
    Ok(())
}

#[cfg(feature = "draw")]
pub(super) fn add_ext_gstate_to_resources(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    registry: crate::draw::ExtGStateRegistry,
) -> Result<()> {
    let ext_g_dict = registry.to_lopdf_dict();
    with_resources_dict_mut(doc, page_id, |res| match res.get_mut(b"ExtGState") {
        Ok(obj) => {
            if let Ok(existing) = obj.as_dict_mut() {
                for (k, v) in ext_g_dict.iter() {
                    existing.set(k.as_slice(), v.clone());
                }
            }
        }
        Err(_) => {
            res.set("ExtGState", Object::Dictionary(ext_g_dict.clone()));
        }
    })
}

pub(super) fn add_xobject_to_resources(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    name: &[u8],
    xobj_id: ObjectId,
) -> Result<()> {
    let xobj_ref = Object::Reference(xobj_id);
    with_resources_dict_mut(doc, page_id, |res| match res.get_mut(b"XObject") {
        Ok(obj) => {
            if let Ok(d) = obj.as_dict_mut() {
                d.set(name, xobj_ref.clone());
            }
        }
        Err(_) => {
            let mut xobj_dict = Dictionary::new();
            xobj_dict.set(name, xobj_ref.clone());
            res.set("XObject", Object::Dictionary(xobj_dict));
        }
    })
}

/// Returns the MediaBox for a page as raw `[x1, y1, x2, y2]`, traversing the parent chain.
/// Falls back to A4 dimensions if no MediaBox is found.
pub(super) fn inherited_media_box_raw(doc: &lopdf::Document, page_id: ObjectId) -> [f32; 4] {
    let mut current_id = page_id;
    for _ in 0..32 {
        let Ok(dict) = doc.get_object(current_id).and_then(|o| o.as_dict()) else {
            break;
        };
        if let Ok(mb) = dict.get(b"MediaBox")
            && let Ok(arr) = mb.as_array()
            && arr.len() >= 4
        {
            let get = |i: usize| -> f32 {
                match &arr[i] {
                    Object::Integer(v) => *v as f32,
                    Object::Real(v) => *v,
                    _ => 0.0,
                }
            };
            return [get(0), get(1), get(2), get(3)]; // x1, y1, x2, y2
        }
        match dict.get(b"Parent").ok() {
            Some(Object::Reference(id)) => current_id = *id,
            _ => break,
        }
    }
    [0.0, 0.0, 595.0, 842.0] // A4 fallback
}

/// Returns the `/Resources` dictionary for a page, traversing the parent chain for inheritance.
pub(super) fn inherited_resources(doc: &lopdf::Document, page_id: ObjectId) -> Option<Dictionary> {
    let mut current_id = page_id;
    for _ in 0..32 {
        let dict = doc.get_object(current_id).ok()?.as_dict().ok()?;
        if let Ok(res) = dict.get(b"Resources") {
            return match res {
                Object::Dictionary(d) => Some(d.clone()),
                Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok().cloned(),
                _ => None,
            };
        }
        match dict.get(b"Parent").ok() {
            Some(Object::Reference(id)) => current_id = *id,
            _ => break,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::types::Document;
    use super::*;

    #[test]
    fn document_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Document>();
    }

    #[test]
    fn is_cjk_cjk_unified_ideographs() {
        assert!(is_cjk('日')); // U+65E5
        assert!(is_cjk('本')); // U+672C
        assert!(is_cjk('語')); // U+8A9E
    }

    #[test]
    fn is_cjk_hiragana_katakana() {
        assert!(is_cjk('あ')); // Hiragana U+3042
        assert!(is_cjk('ア')); // Katakana U+30A2
        assert!(is_cjk('ん')); // Hiragana U+3093
    }

    #[test]
    fn is_cjk_korean_hangul() {
        assert!(is_cjk('가')); // U+AC00
        assert!(is_cjk('나')); // U+B098
        assert!(is_cjk('힣')); // U+D7A3 (last Hangul syllable)
    }

    #[test]
    fn is_cjk_hangul_jamo() {
        assert!(is_cjk('ㄱ')); // Hangul Jamo U+1100
        assert!(is_cjk('ㅏ')); // Hangul Jamo U+1161
    }

    #[test]
    fn is_cjk_cjk_extension_planes() {
        assert!(is_cjk('\u{20000}')); // CJK Extension B U+20000
        assert!(is_cjk('\u{2A6D0}')); // CJK Extension C U+2A6D0
    }

    #[test]
    fn is_cjk_non_cjk_returns_false() {
        assert!(!is_cjk('a'));
        assert!(!is_cjk('A'));
        assert!(!is_cjk('1'));
        assert!(!is_cjk(' '));
        assert!(!is_cjk('é')); // Latin Extended
    }

    #[test]
    fn unicode_marks_are_not_line_starts() {
        for mark in ['\u{0301}', '\u{FE0F}', '\u{200D}', '\u{E0100}'] {
            assert!(
                is_line_start_prohibited(mark),
                "mark {mark:?} must stay attached"
            );
        }
        assert!(!is_line_start_prohibited('A'));
        assert!(is_line_end_prohibited('('));
        assert!(is_line_end_prohibited('「'));
        assert!(!is_line_end_prohibited('A'));
        for no_break in ['\u{00a0}', '\u{202f}', '\u{2060}', '\u{feff}'] {
            assert!(is_line_start_prohibited(no_break));
        }
    }

    #[test]
    fn wrap_paragraph_keeps_cjk_closing_punctuation_off_line_start() {
        let bytes = include_bytes!("../../tests/fixtures/NotoSansJP-Regular.ttf");
        let face = Face::parse(bytes, 0).expect("fixture font should parse");
        let lines = wrap_paragraph("あいうえお。", &face, 10.0, 40.0);

        assert!(lines.len() >= 2, "fixture should wrap: {lines:?}");
        assert!(
            lines.iter().all(|line| !line.starts_with('。')),
            "closing punctuation must not start a line: {lines:?}"
        );
        assert_eq!(
            lines.concat(),
            "あいうえお。",
            "wrapping must preserve text"
        );
    }

    #[test]
    fn wrap_paragraph_keeps_iteration_and_ellipsis_marks_off_boundaries() {
        let bytes = include_bytes!("../../tests/fixtures/NotoSansJP-Regular.ttf");
        let face = Face::parse(bytes, 0).expect("fixture font should parse");
        let lines = wrap_paragraph("あいう…々", &face, 10.0, 28.0);

        assert!(lines.len() >= 2, "fixture should wrap: {lines:?}");
        assert!(lines.iter().all(|line| {
            !line.starts_with('…') && !line.starts_with('々') && !line.ends_with('…')
        }));
        assert_eq!(lines.concat(), "あいう…々");
    }

    #[test]
    fn wrap_paragraph_preserves_ascii_boundary_space() {
        let bytes = include_bytes!("../../tests/fixtures/NotoSansJP-Regular.ttf");
        let face = Face::parse(bytes, 0).expect("fixture font should parse");
        let prefix = "alpha ";
        let width = super::text_width_with_face(prefix, &face, 10.0) + 1.0;
        let lines = wrap_paragraph("alpha beta", &face, 10.0, width);

        assert!(lines.len() >= 2, "fixture should wrap: {lines:?}");
        assert_eq!(lines.concat(), "alpha beta");
        assert!(
            lines[0].ends_with(' '),
            "boundary space was lost: {lines:?}"
        );
    }

    #[test]
    fn wrap_paragraph_keeps_opening_punctuation_with_following_text() {
        let bytes = include_bytes!("../../tests/fixtures/NotoSansJP-Regular.ttf");
        let face = Face::parse(bytes, 0).expect("fixture font should parse");
        let lines = wrap_paragraph("あいう（えお）", &face, 10.0, 50.0);

        assert!(lines.len() >= 2, "fixture should wrap: {lines:?}");
        assert!(
            lines.iter().all(|line| !line.ends_with('（')),
            "opening punctuation must not end a line: {lines:?}"
        );
        assert_eq!(lines.concat(), "あいう（えお）");
    }

    #[test]
    fn wrap_paragraph_preserves_unicode_no_break_boundaries() {
        let bytes = include_bytes!("../../tests/fixtures/NotoSansJP-Regular.ttf");
        let face = Face::parse(bytes, 0).expect("fixture font should parse");
        let lines = wrap_paragraph("left\u{00a0}right", &face, 10.0, 20.0);

        assert!(
            !lines.iter().any(|line| line.ends_with('\u{00a0}'))
                && !lines.iter().skip(1).any(|line| line.starts_with("right")),
            "NBSP boundary must stay intact: {lines:?}"
        );
        assert_eq!(lines.concat(), "left\u{00a0}right");
    }

    #[test]
    fn fallback_diagnostic_preserves_order_and_deduplicates() {
        let bytes = include_bytes!("../../tests/fixtures/NotoSansJP-Regular.ttf");
        let diagnostics = diagnose_font_fallback("A😀A", bytes, Some(bytes));

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].character, 'A');
        assert_eq!(diagnostics[0].resolution, GlyphResolution::Primary);
        assert_eq!(diagnostics[1].character, '😀');
        assert_eq!(diagnostics[1].resolution, GlyphResolution::Missing);
    }
}
