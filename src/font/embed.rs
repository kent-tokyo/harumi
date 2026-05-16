use std::collections::BTreeMap;

use lopdf::{Dictionary, Object, ObjectId, Stream};

use crate::error::Result;
use super::{cmap, FontKind};

pub struct EmbedParams<'a> {
    pub font_name: &'a str,
    pub subset_bytes: Vec<u8>,
    pub gid_to_char: BTreeMap<u16, char>,
    pub gid_to_advance: BTreeMap<u16, u16>,
    pub units_per_em: u16,
    pub font_bbox: [i32; 4],
    pub ascent: i32,
    pub descent: i32,
    pub cap_height: i32,
    /// TTF → `FontFile2`; CFF/OTF → `FontFile3` with `/Subtype /OpenType`.
    pub font_kind: FontKind,
}

/// Adds the four CID font objects to `doc` and returns the Type0 object ID.
///
/// Object graph:
///   Type0 → [CIDFontType2] + ToUnicode stream
///   CIDFontType2 → FontDescriptor
///   FontDescriptor → FontFile2 stream
pub fn embed_cid_font(doc: &mut lopdf::Document, params: EmbedParams<'_>) -> Result<ObjectId> {
    // --- FontDescriptor ---
    let mut descriptor = Dictionary::new();
    descriptor.set("Type", Object::Name(b"FontDescriptor".to_vec()));
    descriptor.set("FontName", Object::Name(params.font_name.as_bytes().to_vec()));
    descriptor.set("Flags", Object::Integer(4)); // Symbolic
    descriptor.set(
        "FontBBox",
        Object::Array(params.font_bbox.iter().map(|&v| Object::Integer(v as i64)).collect()),
    );
    descriptor.set("ItalicAngle", Object::Integer(0));
    descriptor.set("Ascent", Object::Integer(params.ascent as i64));
    descriptor.set("Descent", Object::Integer(params.descent as i64));
    descriptor.set("CapHeight", Object::Integer(params.cap_height as i64));
    descriptor.set("StemV", Object::Integer(80));

    // TTF → FontFile2; CFF/OTF → FontFile3 with /Subtype /OpenType
    match params.font_kind {
        FontKind::TrueType => {
            let mut ff2_dict = Dictionary::new();
            ff2_dict.set("Length1", Object::Integer(params.subset_bytes.len() as i64));
            let ff2_id = doc.add_object(Object::Stream(Stream::new(ff2_dict, params.subset_bytes)));
            descriptor.set("FontFile2", Object::Reference(ff2_id));
        }
        FontKind::Cff => {
            let mut ff3_dict = Dictionary::new();
            ff3_dict.set("Subtype", Object::Name(b"OpenType".to_vec()));
            ff3_dict.set("Length1", Object::Integer(params.subset_bytes.len() as i64));
            let ff3_id = doc.add_object(Object::Stream(Stream::new(ff3_dict, params.subset_bytes)));
            descriptor.set("FontFile3", Object::Reference(ff3_id));
        }
    }

    let descriptor_id = doc.add_object(Object::Dictionary(descriptor));

    // --- W (widths) array: [first_gid [w0 w1 ...]] format ---
    // Build contiguous runs to keep the array compact.
    let w_array = build_widths_array(&params.gid_to_advance, params.units_per_em);

    // --- CIDFontType2 ---
    let mut cid_font = Dictionary::new();
    cid_font.set("Type", Object::Name(b"Font".to_vec()));
    cid_font.set("Subtype", Object::Name(b"CIDFontType2".to_vec()));
    cid_font.set("BaseFont", Object::Name(params.font_name.as_bytes().to_vec()));
    cid_font.set(
        "CIDSystemInfo",
        Object::Dictionary({
            let mut d = Dictionary::new();
            d.set("Registry", Object::string_literal("Adobe"));
            d.set("Ordering", Object::string_literal("Identity"));
            d.set("Supplement", Object::Integer(0));
            d
        }),
    );
    cid_font.set("FontDescriptor", Object::Reference(descriptor_id));
    cid_font.set("DW", Object::Integer(1000));
    cid_font.set("W", Object::Array(w_array));
    cid_font.set("CIDToGIDMap", Object::Name(b"Identity".to_vec()));
    let cid_id = doc.add_object(Object::Dictionary(cid_font));

    // --- ToUnicode CMap stream ---
    let cmap_bytes = cmap::generate_to_unicode(&params.gid_to_char);
    let to_unicode_stream = Stream::new(Dictionary::new(), cmap_bytes);
    let to_unicode_id = doc.add_object(Object::Stream(to_unicode_stream));

    // --- Type0 font ---
    let mut type0 = Dictionary::new();
    type0.set("Type", Object::Name(b"Font".to_vec()));
    type0.set("Subtype", Object::Name(b"Type0".to_vec()));
    type0.set("BaseFont", Object::Name(params.font_name.as_bytes().to_vec()));
    type0.set("Encoding", Object::Name(b"Identity-H".to_vec()));
    type0.set("DescendantFonts", Object::Array(vec![Object::Reference(cid_id)]));
    type0.set("ToUnicode", Object::Reference(to_unicode_id));
    let type0_id = doc.add_object(Object::Dictionary(type0));

    Ok(type0_id)
}

/// Builds the /W array: [[gid [w ...]] ...] format for CIDFontType2.
/// Widths are in thousandths of a text-space unit (scaled to 1000 units_per_em).
fn build_widths_array(
    gid_to_advance: &BTreeMap<u16, u16>,
    units_per_em: u16,
) -> Vec<Object> {
    if gid_to_advance.is_empty() {
        return vec![];
    }

    let scale = |adv: u16| -> i64 {
        (adv as f64 * 1000.0 / units_per_em as f64).round() as i64
    };

    // Group consecutive GIDs into runs.
    let gids: Vec<u16> = gid_to_advance.keys().copied().collect();
    let mut result: Vec<Object> = Vec::new();
    let mut i = 0;

    while i < gids.len() {
        let run_start = gids[i];
        let mut run: Vec<Object> = vec![Object::Integer(scale(gid_to_advance.get(&gids[i]).copied().unwrap_or(units_per_em)))];
        let mut j = i + 1;
        while j < gids.len() && gids[j] == gids[j - 1] + 1 {
            run.push(Object::Integer(scale(gid_to_advance.get(&gids[j]).copied().unwrap_or(units_per_em))));
            j += 1;
        }
        result.push(Object::Integer(run_start as i64));
        result.push(Object::Array(run));
        i = j;
    }

    result
}
