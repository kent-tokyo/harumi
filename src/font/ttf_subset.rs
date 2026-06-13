//! Internal pure-Rust TTF font subsetter.
//!
//! Replaces the `subsetter` crate to minimize dependencies.
//! Handles TrueType (glyf-based) fonts; CFF fonts are rejected as unsupported.

use std::collections::{BTreeMap, BTreeSet};
use ttf_parser::Face;

/// Tracks which glyphs (by original GID) should be preserved in the subset.
/// Provides GID remapping: original GID → new GID (0..N after subsetting).
pub(super) struct GlyphRemapper {
    orig_gids: BTreeSet<u16>,
}

impl GlyphRemapper {
    /// Create a new empty remapper.
    pub fn new() -> Self {
        GlyphRemapper {
            orig_gids: BTreeSet::new(),
        }
    }

    /// Mark a glyph (by original GID) to be preserved in the subset.
    pub fn remap(&mut self, gid: u16) {
        self.orig_gids.insert(gid);
    }

    /// Get sorted list of original GIDs to preserve (for internal use).
    fn sorted_gids(&self) -> Vec<u16> {
        self.orig_gids.iter().copied().collect()
    }
}

/// Subset a TrueType font to include only the specified glyphs.
///
/// Returns the subsetted font bytes and the final sorted set of original GIDs
/// that were included (which may be larger than the remapper's set when
/// composite glyphs pull in additional component GIDs).
///
/// # Errors
/// Returns an error if the font is malformed, CFF-based, or subsetting fails.
pub(super) fn subset(
    data: &[u8],
    face_index: u32,
    remapper: &GlyphRemapper,
) -> Result<(Vec<u8>, BTreeSet<u16>), Box<dyn std::error::Error>> {
    let face = Face::parse(data, face_index)?;

    // For TTC (TrueType Collection), find the font data at the specified face_index.
    let is_ttc = data.len() >= 4 && &data[0..4] == b"ttcf";
    let font_data_start = if is_ttc {
        // TTC format: read face offset from TTC header.
        if data.len() < 12 + (face_index as usize + 1) * 4 {
            return Err("TTC header truncated".into());
        }
        u32::from_be_bytes([
            data[12 + face_index as usize * 4],
            data[12 + face_index as usize * 4 + 1],
            data[12 + face_index as usize * 4 + 2],
            data[12 + face_index as usize * 4 + 3],
        ]) as usize
    } else {
        0
    };

    // Parse the font structure using the font data offset.
    // For TTC: font_data is the face data, table offsets are absolute from TTC file start.
    // For TTF: font_data is the same as data, table offsets are from file start.
    let font_data = &data[font_data_start..];
    let offset_table = parse_offset_table(font_data)?;

    // Parse table records with raw offsets (not yet validated as slices).
    let table_recs_raw = parse_table_records_raw(font_data, &offset_table)?;

    // For TTC, offsets in the table directory are absolute from the TTC file start (offset 0 of full data).
    // When we read from font_data, the offsets are still absolute (they're just numbers in the directory).
    // For TTF, font_data_start == 0, so offsets are naturally correct.
    let table_records: Vec<(String, &[u8])> = {
        let mut result = Vec::new();
        for (tag, raw_offset, length) in table_recs_raw {
            // raw_offset is always absolute from the full data start.
            if raw_offset + length <= data.len() {
                result.push((tag, &data[raw_offset..raw_offset + length]));
            }
        }
        result
    };

    // Get required tables from the records.
    let mut tables = std::collections::HashMap::new();
    for (tag, slice) in table_records.iter() {
        tables.insert(tag.as_str(), *slice);
    }

    let head = *tables.get("head").ok_or("required table head not found")?;
    let hhea = *tables.get("hhea").ok_or("required table hhea not found")?;
    let maxp = *tables.get("maxp").ok_or("required table maxp not found")?;
    let glyf = *tables.get("glyf").ok_or("required table glyf not found")?;
    let loca = *tables.get("loca").ok_or("required table loca not found")?;
    let hmtx = *tables.get("hmtx").ok_or("required table hmtx not found")?;

    // Read head to determine loca format.
    let index_to_loc_format = if head.len() >= 52 {
        u16::from_be_bytes([head[50], head[51]]) & 0x0001
    } else {
        return Err("head table too short".into());
    };

    // Read number of metrics from hhea.
    let num_h_metrics = if hhea.len() >= 34 {
        u16::from_be_bytes([hhea[34], hhea[35]]) as usize
    } else {
        return Err("hhea table too short".into());
    };

    // Read numGlyphs from maxp.
    let num_glyphs = if maxp.len() >= 4 {
        u16::from_be_bytes([maxp[4], maxp[5]]) as usize
    } else {
        return Err("maxp table too short".into());
    };

    // Parse loca to get glyph offsets.
    let glyph_offsets = parse_loca(loca, num_glyphs, index_to_loc_format as u8)?;

    // Collect all glyphs to preserve (including composite dependencies).
    let sorted_gids = remapper.sorted_gids();
    let mut gids_to_keep = BTreeSet::new();
    for &gid in &sorted_gids {
        gids_to_keep.insert(gid);
        collect_composite_deps(glyf, &glyph_offsets, gid as usize, &mut gids_to_keep)?;
    }

    // Build new glyf and loca tables.
    // Also rewrite composite glyph component GIDs to their new positions.
    let gid_remap: BTreeMap<u16, u16> = gids_to_keep
        .iter()
        .enumerate()
        .map(|(new_idx, &orig_gid)| (orig_gid, new_idx as u16))
        .collect();
    let (new_glyf, new_glyph_offsets) = build_glyf(&glyph_offsets, glyf, &gids_to_keep, &gid_remap)?;
    let new_loca = build_loca(&new_glyph_offsets, index_to_loc_format as u8)?;

    // Build new hmtx table.
    let new_hmtx = build_hmtx(
        hmtx,
        num_h_metrics,
        face.units_per_em() as usize,
        &gids_to_keep,
    )?;

    // Patch head, hhea, maxp tables with new metrics.
    let mut new_head = head.to_vec();
    let mut new_hhea = hhea.to_vec();
    let mut new_maxp = maxp.to_vec();

    // Update head.indexToLocFormat if needed.
    if new_loca.len() > (num_glyphs + 1) * 2 && new_head.len() > 50 {
        new_head[50] = 1; // long format
    }

    // Update hhea.numberOfHMetrics.
    let num_new_metrics = gids_to_keep.len().min(num_h_metrics);
    if new_hhea.len() >= 36 {
        let bytes = (num_new_metrics as u16).to_be_bytes();
        new_hhea[34..36].copy_from_slice(&bytes);
    }

    // Update maxp.numGlyphs.
    if new_maxp.len() >= 6 {
        let bytes = (gids_to_keep.len() as u16).to_be_bytes();
        new_maxp[4..6].copy_from_slice(&bytes);
    }

    // Collect other tables to preserve (copy as-is).
    let mut output_tables: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    output_tables.insert("head".into(), new_head);
    output_tables.insert("hhea".into(), new_hhea);
    output_tables.insert("maxp".into(), new_maxp);
    output_tables.insert("glyf".into(), new_glyf);
    output_tables.insert("loca".into(), new_loca);
    output_tables.insert("hmtx".into(), new_hmtx);

    // For PDF CIDFont embedding with Identity-H, only core TrueType tables and
    // hinting tables are safe to include. All other tables contain GID references
    // (GSUB, GPOS, gvar, etc.) or glyph counts (post, vhea/vmtx) that become
    // inconsistent after subsetting, causing Core Text / PDF viewers to reject
    // the embedded font and render all glyphs as replacement characters (●).
    for (tag, data_slice) in table_records.iter() {
        // Skip tables already rebuilt above.
        if matches!(tag.as_str(), "head" | "hhea" | "maxp" | "glyf" | "loca" | "hmtx") {
            continue;
        }
        // Include only hinting tables: safe (no GID references).
        if matches!(tag.as_str(), "fpgm" | "prep" | "cvt " | "gasp") {
            output_tables.insert(tag.clone(), data_slice.to_vec());
        }
        // Everything else is dropped:
        // - GSUB/GPOS/GDEF/BASE: OpenType layout with stale GID refs after subsetting
        // - gvar/fvar/avar/HVAR/STAT: variable-font tables with GID-indexed data
        // - post: numGlyphs field mismatches maxp.numGlyphs after subsetting
        // - vhea/vmtx: vertical metrics indexed by GID, not rebuilt for subset
        // - cmap/OS/2/VORG: PDF Identity-H handles encoding; not needed
        // - name/kern/morx/mort/JSTF/...: not used for PDF CIDFont rendering
    }

    // Assemble the new font binary.
    let font_bytes = assemble_font(&output_tables, offset_table.is_truetype)?;
    Ok((font_bytes, gids_to_keep))
}

// ──────────────────────────────────────────────────────────────────────────

struct OffsetTable {
    is_truetype: bool,
    num_tables: usize,
}

fn parse_offset_table(data: &[u8]) -> Result<OffsetTable, Box<dyn std::error::Error>> {
    if data.len() < 12 {
        return Err("font too short for offset table".into());
    }

    let scaler = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let is_truetype = scaler == 0x00010000 || scaler == 0x74727565; // 0x00010000 or 'true'

    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    Ok(OffsetTable {
        is_truetype,
        num_tables,
    })
}

#[allow(clippy::type_complexity)]
fn parse_table_records_raw(
    data: &[u8],
    offset_table: &OffsetTable,
) -> Result<Vec<(String, usize, usize)>, Box<dyn std::error::Error>> {
    let mut records = Vec::new();
    for i in 0..offset_table.num_tables {
        let base = 12 + i * 16;
        if base + 16 > data.len() {
            return Err("table directory truncated".into());
        }
        let tag = String::from_utf8_lossy(&data[base..base + 4]).to_string();
        let offset = u32::from_be_bytes([
            data[base + 8],
            data[base + 9],
            data[base + 10],
            data[base + 11],
        ]) as usize;
        let length = u32::from_be_bytes([
            data[base + 12],
            data[base + 13],
            data[base + 14],
            data[base + 15],
        ]) as usize;
        records.push((tag, offset, length));
    }
    Ok(records)
}

// ──────────────────────────────────────────────────────────────────────────

fn parse_loca(
    loca: &[u8],
    num_glyphs: usize,
    format: u8,
) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
    let mut offsets = Vec::new();
    if format == 0 {
        // Short format: offsets are u16 × 2
        for i in 0..=num_glyphs {
            if i * 2 + 2 > loca.len() {
                return Err("loca table truncated".into());
            }
            let offset = u16::from_be_bytes([loca[i * 2], loca[i * 2 + 1]]) as u32 * 2;
            offsets.push(offset);
        }
    } else {
        // Long format: offsets are u32
        for i in 0..=num_glyphs {
            if i * 4 + 4 > loca.len() {
                return Err("loca table truncated".into());
            }
            let offset = u32::from_be_bytes([
                loca[i * 4],
                loca[i * 4 + 1],
                loca[i * 4 + 2],
                loca[i * 4 + 3],
            ]);
            offsets.push(offset);
        }
    }
    Ok(offsets)
}

// ──────────────────────────────────────────────────────────────────────────

fn collect_composite_deps(
    glyf: &[u8],
    glyph_offsets: &[u32],
    gid: usize,
    visited: &mut BTreeSet<u16>,
) -> Result<(), Box<dyn std::error::Error>> {
    if gid >= glyph_offsets.len() - 1 {
        return Ok(());
    }

    let start = glyph_offsets[gid] as usize;
    let end = glyph_offsets[gid + 1] as usize;
    if start >= glyf.len() || end > glyf.len() {
        return Ok(()); // Invalid or empty glyph
    }

    let glyph_data = &glyf[start..end];
    if glyph_data.len() < 2 {
        return Ok(());
    }

    let num_contours = i16::from_be_bytes([glyph_data[0], glyph_data[1]]);
    if num_contours >= 0 {
        return Ok(()); // Simple glyph, no components
    }

    // Composite glyph: parse component records.
    let mut offset = 10; // Skip header (xMin, yMin, xMax, yMax)
    while offset < glyph_data.len() {
        if offset + 4 > glyph_data.len() {
            break;
        }

        let flags = u16::from_be_bytes([glyph_data[offset], glyph_data[offset + 1]]);
        let comp_gid = u16::from_be_bytes([glyph_data[offset + 2], glyph_data[offset + 3]]);

        if visited.insert(comp_gid) {
            collect_composite_deps(glyf, glyph_offsets, comp_gid as usize, visited)?;
        }

        offset += 4;

        // Skip arg bytes.
        if (flags & 0x0001) != 0 {
            offset += 4; // Two i16
        } else {
            offset += 2; // Two i8
        }

        // Skip scale/matrix.
        if (flags & 0x0008) != 0 {
            offset += 2; // F2Dot14
        } else if (flags & 0x0040) != 0 {
            offset += 4; // Two F2Dot14
        } else if (flags & 0x0080) != 0 {
            offset += 8; // 2x2 matrix
        }

        if (flags & 0x0020) == 0 {
            break; // No more components
        }
    }

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────

fn build_glyf(
    glyph_offsets: &[u32],
    glyf: &[u8],
    gids_to_keep: &BTreeSet<u16>,
    gid_remap: &BTreeMap<u16, u16>,
) -> Result<(Vec<u8>, Vec<u32>), Box<dyn std::error::Error>> {
    let mut new_glyf = Vec::new();
    let mut new_offsets = vec![0u32];

    for gid in gids_to_keep.iter() {
        let gid_usize = *gid as usize;
        if gid_usize + 1 >= glyph_offsets.len() {
            return Err("glyph index out of range".into());
        }
        let start = glyph_offsets[gid_usize] as usize;
        let end = glyph_offsets[gid_usize + 1] as usize;
        if start > glyf.len() || end > glyf.len() {
            return Err("glyph offset out of bounds".into());
        }
        let glyph_data = &glyf[start..end];
        if glyph_data.len() >= 2 {
            let num_contours = i16::from_be_bytes([glyph_data[0], glyph_data[1]]);
            if num_contours < 0 {
                // Composite glyph: copy with component GIDs rewritten to new positions.
                new_glyf.extend_from_slice(rewrite_composite_gids(glyph_data, gid_remap).as_slice());
            } else {
                new_glyf.extend_from_slice(glyph_data);
            }
        } else {
            new_glyf.extend_from_slice(glyph_data);
        }
        new_offsets.push(new_glyf.len() as u32);
    }

    Ok((new_glyf, new_offsets))
}

/// Rewrites composite glyph component GIDs from original to new positions.
fn rewrite_composite_gids(glyph_data: &[u8], gid_remap: &BTreeMap<u16, u16>) -> Vec<u8> {
    let mut out = glyph_data.to_vec();
    let mut offset = 10; // Skip glyph header (numberOfContours + bounding box = 10 bytes)

    while offset < out.len() {
        if offset + 4 > out.len() {
            break;
        }
        let flags = u16::from_be_bytes([out[offset], out[offset + 1]]);
        let orig_comp_gid = u16::from_be_bytes([out[offset + 2], out[offset + 3]]);

        // Rewrite the component GID to its new position.
        if let Some(&new_gid) = gid_remap.get(&orig_comp_gid) {
            let bytes = new_gid.to_be_bytes();
            out[offset + 2] = bytes[0];
            out[offset + 3] = bytes[1];
        }

        offset += 4;

        // Advance past argument bytes (same logic as collect_composite_deps).
        if (flags & 0x0001) != 0 {
            offset += 4; // Two i16
        } else {
            offset += 2; // Two i8
        }

        // Advance past scale/matrix.
        if (flags & 0x0008) != 0 {
            offset += 2; // F2Dot14
        } else if (flags & 0x0040) != 0 {
            offset += 4; // Two F2Dot14
        } else if (flags & 0x0080) != 0 {
            offset += 8; // 2x2 matrix
        }

        if (flags & 0x0020) == 0 {
            break; // No more components
        }
    }

    out
}

fn build_loca(glyph_offsets: &[u32], format: u8) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut loca = Vec::new();
    if format == 0 {
        for &offset in glyph_offsets {
            let half = (offset / 2) as u16;
            loca.extend_from_slice(&half.to_be_bytes());
        }
    } else {
        for &offset in glyph_offsets {
            loca.extend_from_slice(&offset.to_be_bytes());
        }
    }
    Ok(loca)
}

fn build_hmtx(
    hmtx: &[u8],
    num_h_metrics: usize,
    units_per_em: usize,
    gids_to_keep: &BTreeSet<u16>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut new_hmtx = Vec::new();

    for &gid in gids_to_keep.iter() {
        let gid_usize = gid as usize;
        let metrics_offset = if gid_usize < num_h_metrics {
            gid_usize * 4
        } else {
            num_h_metrics * 4 + (gid_usize - num_h_metrics) * 2
        };

        // Read advance width.
        if metrics_offset + 2 <= hmtx.len() {
            new_hmtx.extend_from_slice(&hmtx[metrics_offset..metrics_offset + 2]);
        } else {
            // Default advance.
            new_hmtx.extend_from_slice(&(units_per_em as u16).to_be_bytes());
        }

        // Read left side bearing.
        if metrics_offset + 4 <= hmtx.len() {
            new_hmtx.extend_from_slice(&hmtx[metrics_offset + 2..metrics_offset + 4]);
        } else {
            // Default lsb (0).
            new_hmtx.extend_from_slice(&0i16.to_be_bytes());
        }
    }

    Ok(new_hmtx)
}

// ──────────────────────────────────────────────────────────────────────────

fn assemble_font(
    tables: &BTreeMap<String, Vec<u8>>,
    is_truetype: bool,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let num_tables = tables.len();
    // TTF offset table fields are all u16 (2 bytes each).
    let search_range: u16 = (1u16 << (15 - (num_tables as u16).leading_zeros())) * 16;
    let entry_selector: u16 = 15 - (num_tables as u16).leading_zeros() as u16;
    let range_shift: u16 = num_tables as u16 * 16 - search_range;

    let mut font = Vec::new();

    // Offset table: sfVersion(4) + numTables(2) + searchRange(2) + entrySelector(2) + rangeShift(2) = 12 bytes.
    let scaler = if is_truetype {
        0x00010000u32
    } else {
        0x4F544F54u32
    }; // 0x00010000 or 'OTTO'
    font.extend_from_slice(&scaler.to_be_bytes());
    font.extend_from_slice(&(num_tables as u16).to_be_bytes());
    font.extend_from_slice(&search_range.to_be_bytes());
    font.extend_from_slice(&entry_selector.to_be_bytes());
    font.extend_from_slice(&range_shift.to_be_bytes());

    // Table directory (num_tables × 16 bytes).
    let mut dir_offset = 12 + num_tables * 16;
    let mut table_offsets = Vec::new();
    for (tag, data) in tables.iter() {
        // Align to 4-byte boundary.
        dir_offset = (dir_offset + 3) & !3;
        table_offsets.push((tag.clone(), dir_offset, data.len()));
        dir_offset += data.len();
    }

    for (tag, offset, length) in table_offsets.iter() {
        font.extend_from_slice(tag.as_bytes());
        // Placeholder checksum (0).
        font.extend_from_slice(&0u32.to_be_bytes());
        font.extend_from_slice(&(*offset as u32).to_be_bytes());
        font.extend_from_slice(&(*length as u32).to_be_bytes());
    }

    // Table data.
    let mut current_offset = font.len();
    for (_tag, data) in tables.iter() {
        // Align to 4-byte boundary.
        current_offset = (current_offset + 3) & !3;
        while font.len() < current_offset {
            font.push(0);
        }
        font.extend_from_slice(data);
        current_offset += data.len();
    }

    Ok(font)
}
