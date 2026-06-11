use std::collections::{BTreeMap, HashSet};

use ttf_parser::Face;

use super::FontKind;
use super::ttf_subset::{GlyphRemapper, subset as subsetter_subset};
use crate::error::{Error, Result};

pub struct SubsetResult {
    pub bytes: Vec<u8>,
    /// Maps **new** GID (0..N, post-subset) → Unicode char.
    pub gid_to_char: BTreeMap<u16, char>,
    /// Maps **new** GID (0..N, post-subset) → advance width in font design units.
    pub gid_to_advance: BTreeMap<u16, u16>,
    pub units_per_em: u16,
    pub font_kind: FontKind,
}

/// Returns true if the font is CFF (OpenType with CFF outlines).
/// subsetter does not support CFF fonts; only TrueType (glyf) is supported.
fn is_cff_font(data: &[u8]) -> bool {
    if data.len() < 12 {
        return false;
    }
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    for i in 0..num_tables {
        let base = 12 + i * 16;
        if base + 4 > data.len() {
            break;
        }
        if &data[base..base + 4] == b"CFF " || &data[base..base + 4] == b"CFF2" {
            return true;
        }
    }
    false
}

pub fn subset_font(ttf_bytes: &[u8], chars: &[char]) -> Result<SubsetResult> {
    let font_kind = match FontKind::detect(ttf_bytes) {
        Some(kind) => kind,
        None => return Err(Error::FontParse("unrecognised font magic bytes".into())),
    };

    // subsetter only supports TrueType (glyf), not CFF.
    if is_cff_font(ttf_bytes) {
        return Err(Error::FontParse(
            "CFF fonts are not supported by subsetter; \
             use the TrueType variant (e.g. NotoSansCJKjp-Regular.ttf) instead"
                .into(),
        ));
    }

    // Use ttf-parser for char→GID mapping (simpler API).
    let face = Face::parse(ttf_bytes, 0).map_err(|e| Error::FontParse(e.to_string()))?;

    let units_per_em = face.units_per_em();
    let mut gids: Vec<u16> = vec![0]; // always include .notdef
    let mut gids_seen: HashSet<u16> = HashSet::new();
    gids_seen.insert(0);
    let mut orig_gid_to_char: BTreeMap<u16, char> = BTreeMap::new();

    for &ch in chars {
        if let Some(glyph_id) = face.glyph_index(ch) {
            let gid = glyph_id.0;
            if gid != 0 {
                orig_gid_to_char.entry(gid).or_insert(ch);
                if gids_seen.insert(gid) {
                    gids.push(gid);
                }
            }
        }
    }

    gids.sort_unstable();

    // Collect advance widths keyed by original GID (before subsetting).
    let mut orig_gid_to_advance: BTreeMap<u16, u16> = BTreeMap::new();
    for &gid in &gids {
        let advance = face
            .glyph_hor_advance(ttf_parser::GlyphId(gid))
            .unwrap_or(units_per_em);
        orig_gid_to_advance.insert(gid, advance);
    }

    // Use subsetter for the actual font subsetting.
    // Build a GlyphRemapper that includes all glyphs we need.
    let mut remapper = GlyphRemapper::new();
    for &gid in &gids {
        remapper.remap(gid);
    }

    let subsetted = subsetter_subset(ttf_bytes, 0, &remapper)
        .map_err(|e| Error::FontParse(format!("font subsetting failed: {}", e)))?;

    // subsetter reassigns GIDs via the remapper.
    // Guard: new GIDs are u16 indices, so the subset cannot exceed 65535 glyphs.
    if gids.len() > u16::MAX as usize {
        return Err(Error::FontParse(format!(
            "font has {} glyphs; maximum supported is {}",
            gids.len(),
            u16::MAX
        )));
    }

    let gid_to_char: BTreeMap<u16, char> = orig_gid_to_char
        .into_iter()
        .filter_map(|(orig, ch)| remapper.get(orig).map(|new| (new as u16, ch)))
        .collect();

    let gid_to_advance: BTreeMap<u16, u16> = orig_gid_to_advance
        .into_iter()
        .filter_map(|(orig, adv)| remapper.get(orig).map(|new| (new as u16, adv)))
        .collect();

    Ok(SubsetResult {
        bytes: subsetted,
        gid_to_char,
        gid_to_advance,
        units_per_em,
        font_kind,
    })
}
