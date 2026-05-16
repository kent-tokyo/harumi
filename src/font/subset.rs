use std::collections::{BTreeMap, HashSet};

use allsorts::{
    font_data::FontData,
    subset::{subset, CmapTarget, SubsetProfile},
    binary::read::ReadScope,
};
use ttf_parser::Face;

use crate::error::{Error, Result};
use super::FontKind;

pub struct SubsetResult {
    pub bytes: Vec<u8>,
    /// Maps **new** GID (0..N, post-subset) → Unicode char.
    pub gid_to_char: BTreeMap<u16, char>,
    /// Maps **new** GID (0..N, post-subset) → advance width in font design units.
    pub gid_to_advance: BTreeMap<u16, u16>,
    pub units_per_em: u16,
    pub font_kind: FontKind,
}

/// Returns true if the OTF/CFF file contains a `CFF2` table (variable font).
/// allsorts v0.17 cannot subset CFF2; callers should use the TTF variant.
fn has_cff2_table(data: &[u8]) -> bool {
    if data.len() < 12 {
        return false;
    }
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    for i in 0..num_tables {
        let base = 12 + i * 16;
        if base + 4 > data.len() {
            break;
        }
        if &data[base..base + 4] == b"CFF2" {
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

    if matches!(font_kind, FontKind::Cff) && has_cff2_table(ttf_bytes) {
        return Err(Error::FontParse(
            "CFF2 variable font is not supported by allsorts v0.17; \
             use the TTF variant (e.g. NotoSansCJKjp-Regular.ttf) instead"
                .into(),
        ));
    }

    // Use ttf-parser for char→GID mapping (simpler API).
    let face = Face::parse(ttf_bytes, 0)
        .map_err(|e| Error::FontParse(e.to_string()))?;

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

    // Use allsorts for the actual font subsetting.
    let scope = ReadScope::new(ttf_bytes);
    let font_file = scope
        .read::<FontData<'_>>()
        .map_err(|e| Error::FontParse(e.to_string()))?;
    let provider = font_file
        .table_provider(0)
        .map_err(|e| Error::FontParse(e.to_string()))?;

    let subsetted = subset(
        &provider,
        &gids,
        &SubsetProfile::Pdf,
        CmapTarget::Unicode,
    )
    .map_err(|e| Error::FontParse(e.to_string()))?;

    // allsorts reassigns GIDs to 0..N in the order of the input `gids` slice.
    // Guard: new GIDs are u16 indices, so the subset cannot exceed 65535 glyphs.
    if gids.len() > u16::MAX as usize {
        return Err(Error::FontParse(format!(
            "font has {} glyphs; maximum supported is {}",
            gids.len(), u16::MAX
        )));
    }
    let orig_to_new: BTreeMap<u16, u16> = gids.iter()
        .enumerate()
        .map(|(new_gid, &orig_gid)| (orig_gid, new_gid as u16))
        .collect();

    let gid_to_char: BTreeMap<u16, char> = orig_gid_to_char
        .into_iter()
        .filter_map(|(orig, ch)| orig_to_new.get(&orig).map(|&new| (new, ch)))
        .collect();

    let gid_to_advance: BTreeMap<u16, u16> = orig_gid_to_advance
        .into_iter()
        .filter_map(|(orig, adv)| orig_to_new.get(&orig).map(|&new| (new, adv)))
        .collect();

    Ok(SubsetResult {
        bytes: subsetted,
        gid_to_char,
        gid_to_advance,
        units_per_em,
        font_kind,
    })
}
