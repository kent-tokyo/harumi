use std::collections::{BTreeMap, HashSet};

use ttf_parser::Face;

use super::FontKind;
use super::ttf_subset::{GlyphRemapper, subset as subsetter_subset};
use crate::error::{Error, Result};

pub struct SubsetResult {
    pub bytes: Vec<u8>,
    /// Maps **new** GID (0..N, post-subset) → Unicode char (one per GID).
    /// Used for the ToUnicode CMap. When multiple chars share a glyph, only the
    /// first char seen is kept here.
    pub gid_to_char: BTreeMap<u16, char>,
    /// Maps every input char → its **new** GID. This is the authoritative mapping
    /// for content stream encoding. Unlike `gid_to_char`, this map includes ALL
    /// requested chars even when two chars share the same underlying glyph (both
    /// map to the same new GID, which is correct for rendering).
    pub char_to_gid: BTreeMap<char, u16>,
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

    let (subsetted, gids_to_keep) = subsetter_subset(ttf_bytes, 0, &remapper)
        .map_err(|e| Error::FontParse(format!("font subsetting failed: {}", e)))?;

    // gids_to_keep is the final sorted set of original GIDs in the subset
    // (includes composite glyph dependencies beyond what remapper requested).
    // Guard: new GIDs are u16 indices, so the subset cannot exceed 65535 glyphs.
    if gids_to_keep.len() > u16::MAX as usize {
        return Err(Error::FontParse(format!(
            "font has {} glyphs; maximum supported is {}",
            gids_to_keep.len(),
            u16::MAX
        )));
    }

    // Build orig_gid → new_gid from the final kept-glyph order (sorted, including
    // composite deps). This is the authoritative position map for the subset font.
    let orig_to_new_gid: BTreeMap<u16, u16> = gids_to_keep
        .iter()
        .enumerate()
        .map(|(new_idx, &orig_gid)| (orig_gid, new_idx as u16))
        .collect();

    // gid_to_char: one char per new GID (for ToUnicode CMap).
    // When multiple chars share a glyph, only the first char seen is kept here.
    let gid_to_char: BTreeMap<u16, char> = gids_to_keep
        .iter()
        .filter_map(|orig_gid| {
            let new_gid = *orig_to_new_gid.get(orig_gid)?;
            orig_gid_to_char.get(orig_gid).map(|&ch| (new_gid, ch))
        })
        .collect();

    // char_to_gid: all input chars → new GID (for content stream encoding).
    // Built directly from the input chars so that every char gets a mapping,
    // even when two chars share the same underlying glyph (both map to same new GID).
    let char_to_gid: BTreeMap<char, u16> = chars
        .iter()
        .filter_map(|&ch| {
            face.glyph_index(ch)
                .filter(|gid| gid.0 != 0)
                .and_then(|gid| orig_to_new_gid.get(&gid.0))
                .map(|&new_gid| (ch, new_gid))
        })
        .collect();

    let gid_to_advance: BTreeMap<u16, u16> = gids_to_keep
        .iter()
        .filter_map(|orig_gid| {
            let new_gid = *orig_to_new_gid.get(orig_gid)?;
            orig_gid_to_advance.get(orig_gid).map(|&adv| (new_gid, adv))
        })
        .collect();

    Ok(SubsetResult {
        bytes: subsetted,
        gid_to_char,
        char_to_gid,
        gid_to_advance,
        units_per_em,
        font_kind,
    })
}
