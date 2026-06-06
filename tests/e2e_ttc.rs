//! E2E tests for TTC (TrueType Collection) font embedding.
//!
//! TTC files contain multiple font faces in one file.
//! harumi uses face index 0 via ttf-parser and allsorts.
//!
//! Instead of bundling a large TTC fixture, we synthesize a valid 2-face TTC
//! at runtime by wrapping the existing NotoSansJP fixture in a TTC container.
//! Both faces point to the same underlying TTF data, which is legal per the
//! TTC spec and exercises the full TTC code path.

use harumi::Document;

const NOTO_TTF: &[u8] = include_bytes!("fixtures/NotoSansJP-Regular.ttf");

/// Build a minimal 2-face TTC from a single TTF byte slice.
/// Both faces reference the same underlying font data (valid per TTC spec).
///
/// In a TTC file, table-directory offsets are absolute from the start of the
/// TTC file, not from the start of the embedded font. This function patches
/// each table-directory entry in the TTF so its offset reflects the correct
/// absolute position within the TTC file.
fn make_ttc(ttf: &[u8]) -> Vec<u8> {
    let num_fonts: u32 = 2;
    // TTC header: tag(4) + version(4) + numFonts(4) + offsets[2 × 4] = 20 bytes
    let header_size: u32 = 4 + 4 + 4 + num_fonts * 4;
    let font_offset = header_size; // where the TTF data begins inside the TTC

    // Patch each table-directory entry's offset to be TTC-absolute.
    // OffsetTable layout: sfVersion(4) + numTables(2) + searchRange(2) +
    //                     entrySelector(2) + rangeShift(2) = 12 bytes header
    // Each table record: tag(4) + checkSum(4) + offset(4) + length(4) = 16 bytes
    let num_tables = u16::from_be_bytes([ttf[4], ttf[5]]) as usize;
    let mut patched = ttf.to_vec();
    for i in 0..num_tables {
        let off_pos = 12 + i * 16 + 8; // tag(4) + checksum(4) = 8 bytes before offset
        let old = u32::from_be_bytes(patched[off_pos..off_pos + 4].try_into().unwrap());
        patched[off_pos..off_pos + 4].copy_from_slice(&(old + font_offset).to_be_bytes());
    }

    let mut ttc = Vec::with_capacity(header_size as usize + ttf.len());
    ttc.extend_from_slice(b"ttcf");
    ttc.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version 1.0
    ttc.extend_from_slice(&num_fonts.to_be_bytes());
    ttc.extend_from_slice(&font_offset.to_be_bytes()); // face 0 offset
    ttc.extend_from_slice(&font_offset.to_be_bytes()); // face 1 offset (same data)
    ttc.extend_from_slice(&patched);
    ttc
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn ttc_embed_font_succeeds() {
    let ttc = make_ttc(NOTO_TTF);
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.embed_font(&ttc).expect("embed_font should accept TTC bytes");
}

#[test]
fn ttc_invisible_text_roundtrip() {
    let ttc = make_ttc(NOTO_TTF);
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&ttc).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("日本語テスト", font, [72.0, 700.0], 14.0)
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    let runs = reloaded.extract_text_runs(1).unwrap();
    let text: String = runs.iter().map(|f| f.text.as_str()).collect();
    assert!(
        text.contains("日本語テスト"),
        "TTC-embedded invisible text should be extractable; got: {text:?}"
    );
}

#[test]
fn ttc_visible_text_roundtrip() {
    let ttc = make_ttc(NOTO_TTF);
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&ttc).unwrap();
    doc.page(1)
        .unwrap()
        .add_text("TTC", font, [72.0, 700.0], 20.0, [0.0, 0.0, 0.0])
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    let runs = reloaded.extract_text_runs(1).unwrap();
    let text: String = runs.iter().map(|f| f.text.as_str()).collect();
    assert!(
        text.contains("TTC"),
        "TTC-embedded visible text should be extractable; got: {text:?}"
    );
}

#[test]
fn ttc_face1_same_as_face0() {
    // Both faces in our synthetic TTC are identical — face index 0 is what
    // harumi uses; verify ttf-parser can parse both.
    let ttc = make_ttc(NOTO_TTF);
    let face0 = ttf_parser::Face::parse(&ttc, 0).expect("face 0 should parse");
    let face1 = ttf_parser::Face::parse(&ttc, 1).expect("face 1 should parse");
    assert_eq!(face0.units_per_em(), face1.units_per_em());
}

#[test]
fn ttc_magic_bytes_detected() {
    // Verify our synthesized TTC starts with the 'ttcf' magic.
    let ttc = make_ttc(NOTO_TTF);
    assert_eq!(&ttc[..4], b"ttcf", "TTC must start with 'ttcf'");
}
