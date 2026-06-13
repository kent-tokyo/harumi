//! End-to-end test with NotoSansJP-Regular.ttf (TrueType, magic 0x00010000).
//!
//! Verifies:
//!   1. Invisible Japanese text is embedded and the ToUnicode CMap maps every character.
//!   2. Visible Japanese text produces "0 Tr" and the RGB color operator in the content stream.
//!   3. Batch runs (add_invisible_text_runs) embed all characters correctly.
//!   4. Two pages each get their own content stream; both have the font in /Resources.
//!   5. save_to_bytes() produces a reload-able PDF.

mod helpers;

use harumi::{Document, TextRun};
use lopdf::Object;

const FONT_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/NotoSansJP-Regular.ttf"
);

fn font() -> Vec<u8> {
    std::fs::read(FONT_PATH).expect("NotoSansJP-Regular.ttf not found in test fixtures")
}

fn minimal_pdf() -> Vec<u8> {
    helpers::minimal_pdf_bytes()
}

// ---------------------------------------------------------------------------
// 1. Invisible text: ToUnicode CMap must contain every input character
// ---------------------------------------------------------------------------
#[test]
fn e2e_invisible_text_cmap_coverage() {
    let input = "日本語PDF検索晴海";
    let expected_codepoints = [
        "65E5", // 日
        "672C", // 本
        "8A9E", // 語
        "0050", // P (ASCII 0x50)
        "0044", // D (ASCII 0x44)
        "0046", // F (ASCII 0x46)
        "691C", // 検
        "7D22", // 索
        "6674", // 晴
        "6D77", // 海
    ];

    let mut doc = Document::from_bytes(&minimal_pdf()).expect("load");
    let fh = doc.embed_font(&font()).expect("embed_font");
    doc.page(1)
        .unwrap()
        .add_invisible_text(input, fh, [72.0, 700.0], 14.0)
        .unwrap();

    let out = doc.save_to_bytes().expect("save_to_bytes");
    assert!(!out.is_empty());

    let reloaded = lopdf::Document::load_from(out.as_slice()).expect("reload");
    let cmap = extract_to_unicode_cmap(&reloaded, 1);

    // Every CJK character must appear in the ToUnicode CMap.
    for cp in &expected_codepoints {
        assert!(
            cmap.to_uppercase().contains(&cp.to_uppercase()),
            "ToUnicode CMap missing codepoint {}",
            cp
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Visible text: content stream has "0 Tr" and RGB color
// ---------------------------------------------------------------------------
#[test]
fn e2e_visible_text_render_mode_and_color() {
    let mut doc = Document::from_bytes(&minimal_pdf()).expect("load");
    let fh = doc.embed_font(&font()).expect("embed_font");
    doc.page(1)
        .unwrap()
        .add_text("晴海テスト", fh, [72.0, 600.0], 18.0, [0.2, 0.4, 0.8])
        .unwrap();

    let out = doc.save_to_bytes().expect("save");
    let reloaded = lopdf::Document::load_from(out.as_slice()).expect("reload");

    let pages = reloaded.get_pages();
    let page_id = pages[&1];
    let content = reloaded.get_page_content(page_id).expect("content");
    let s = String::from_utf8_lossy(&content);

    assert!(s.contains("0 Tr"), "visible text must use Tr 0");
    assert!(s.contains("rg"), "must contain RGB color operator");
    assert!(!s.contains("3 Tr"), "must not use invisible mode");
    // Check the approximate RGB values
    assert!(s.contains("0.2"), "red component 0.2 should appear");
    assert!(s.contains("0.4"), "green component 0.4 should appear");
    assert!(s.contains("0.8"), "blue component 0.8 should appear");
}

// ---------------------------------------------------------------------------
// 3. Batch runs: all characters across all runs are in the CMap
// ---------------------------------------------------------------------------
#[test]
fn e2e_batch_runs_cmap_coverage() {
    let runs = [
        ("晴海ライブラリ", 72.0_f32, 750.0_f32),
        ("日本語のPDF検索", 72.0, 720.0),
        ("純Rust製・CJK対応", 72.0, 690.0),
    ];

    let mut doc = Document::from_bytes(&minimal_pdf()).expect("load");
    let fh = doc.embed_font(&font()).expect("embed_font");
    doc.page(1)
        .unwrap()
        .add_invisible_text_runs(
            &runs
                .iter()
                .map(|(text, x, y)| TextRun {
                    text: text.to_string(),
                    font: fh,
                    x: *x,
                    y: *y,
                    font_size: 12.0,
                    render_mode: 3,
                    color: harumi::Color::Rgb([0.0; 3]),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();

    let out = doc.save_to_bytes().expect("save");
    let reloaded = lopdf::Document::load_from(out.as_slice()).expect("reload");
    let cmap = extract_to_unicode_cmap(&reloaded, 1);

    // Spot-check a selection of characters from each run.
    for cp in &["6674", "6D77", "65E5", "672C", "7D14"] {
        assert!(
            cmap.to_uppercase().contains(&cp.to_uppercase()),
            "ToUnicode missing U+{}",
            cp
        );
    }
}

// ---------------------------------------------------------------------------
// 4. Multi-page: each page has its own content stream and font resource
// ---------------------------------------------------------------------------
#[test]
fn e2e_multipage_font_resources() {
    use lopdf::{Stream, dictionary};

    // Build a 3-page PDF manually.
    let mut lpdf = lopdf::Document::with_version("1.4");
    let pages_id = lpdf.new_object_id();

    let make_page = |d: &mut lopdf::Document, parent: lopdf::ObjectId| {
        let sid = d.add_object(Object::Stream(Stream::new(
            dictionary! {},
            b"q Q\n".to_vec(),
        )));
        d.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(parent),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(595), Object::Integer(842),
            ]),
            "Contents" => Object::Reference(sid),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {})
            }),
        }))
    };

    let p1 = make_page(&mut lpdf, pages_id);
    let p2 = make_page(&mut lpdf, pages_id);
    let p3 = make_page(&mut lpdf, pages_id);

    lpdf.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![
                Object::Reference(p1),
                Object::Reference(p2),
                Object::Reference(p3),
            ]),
            "Count" => Object::Integer(3),
        }),
    );
    let cat = lpdf.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    lpdf.trailer.set("Root", Object::Reference(cat));
    let mut base_buf = Vec::new();
    lpdf.save_to(&mut base_buf).unwrap();

    // Now overlay text on all 3 pages using harumi.
    let mut doc = Document::from_bytes(&base_buf).expect("load");
    let fh = doc.embed_font(&font()).expect("embed_font");

    doc.page(1)
        .unwrap()
        .add_invisible_text("ページ一：晴海", fh, [72.0, 700.0], 12.0)
        .unwrap();
    doc.page(2)
        .unwrap()
        .add_invisible_text("ページ二：テスト", fh, [72.0, 700.0], 12.0)
        .unwrap();
    doc.page(3)
        .unwrap()
        .add_invisible_text("ページ三：検索可能", fh, [72.0, 700.0], 12.0)
        .unwrap();

    let out = doc.save_to_bytes().expect("save");

    let reloaded = lopdf::Document::load_from(out.as_slice()).expect("reload");
    assert_eq!(reloaded.get_pages().len(), 3, "3 pages must be preserved");

    // Each page must have a /Font entry in its /Resources.
    for page_num in 1u32..=3 {
        let pages = reloaded.get_pages();
        let page_id = pages[&page_num];
        let page = reloaded.get_object(page_id).unwrap().as_dict().unwrap();
        let resources = match page.get(b"Resources").unwrap() {
            Object::Reference(r) => reloaded.get_object(*r).unwrap().as_dict().unwrap(),
            Object::Dictionary(d) => d,
            other => panic!("unexpected Resources: {:?}", other),
        };
        let font_dict = resources
            .get(b"Font")
            .expect("missing /Font")
            .as_dict()
            .unwrap();
        assert!(
            !font_dict.is_empty(),
            "page {} must have font in /Resources",
            page_num
        );
    }
}

// ---------------------------------------------------------------------------
// 5. Page size query
// ---------------------------------------------------------------------------
#[test]
fn e2e_page_size() {
    let mut doc = Document::from_bytes(&minimal_pdf()).expect("load");
    let (w, h) = doc.page(1).expect("page").size().expect("size");
    assert!((w - 595.0).abs() < 1.0, "A4 width ~595pt, got {w}");
    assert!((h - 842.0).abs() < 1.0, "A4 height ~842pt, got {h}");
}

// ---------------------------------------------------------------------------
// 6. Write a real output file for manual inspection (only when env var set)
// ---------------------------------------------------------------------------
#[test]
fn e2e_write_output_pdf() {
    let out_path = match std::env::var("HARUMI_E2E_OUT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "Skipping e2e_write_output_pdf (set HARUMI_E2E_OUT=/path/to/output.pdf to enable)"
            );
            return;
        }
    };

    let mut doc = Document::from_bytes(&minimal_pdf()).expect("load");
    let fh = doc.embed_font(&font()).expect("embed_font");

    // Invisible OCR layer
    doc.page(1)
        .unwrap()
        .add_invisible_text_runs(&[
            TextRun {
                text: "晴海ライブラリ".into(),
                font: fh,
                x: 72.0,
                y: 750.0,
                font_size: 14.0,
                render_mode: 3,
                color: harumi::Color::Rgb([0.0; 3]),
            },
            TextRun {
                text: "日本語のPDF検索".into(),
                font: fh,
                x: 72.0,
                y: 720.0,
                font_size: 12.0,
                render_mode: 3,
                color: harumi::Color::Rgb([0.0; 3]),
            },
            TextRun {
                text: "純Rust製・CJK対応".into(),
                font: fh,
                x: 72.0,
                y: 690.0,
                font_size: 12.0,
                render_mode: 3,
                color: harumi::Color::Rgb([0.0; 3]),
            },
        ])
        .unwrap();

    // Visible label
    doc.page(1)
        .unwrap()
        .add_text("E2E テスト出力", fh, [72.0, 650.0], 16.0, [0.0, 0.3, 0.7])
        .unwrap();

    doc.save(&out_path).expect("save to output file");
    println!("Written: {out_path}");
    println!("Open in a PDF viewer and press Cmd+A to select the invisible text.");
}

// ---------------------------------------------------------------------------
// Helper: extract the raw ToUnicode CMap text for page N (1-indexed)
// ---------------------------------------------------------------------------
fn extract_to_unicode_cmap(doc: &lopdf::Document, page_num: u32) -> String {
    let pages = doc.get_pages();
    let page_id = pages[&page_num];
    let page = doc.get_object(page_id).unwrap().as_dict().unwrap();
    let resources = match page.get(b"Resources").unwrap() {
        Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap(),
        Object::Dictionary(d) => d,
        other => panic!("unexpected Resources type: {:?}", other),
    };
    let font_dict = resources.get(b"Font").unwrap().as_dict().unwrap();
    let (_, font_ref) = font_dict.iter().next().expect("no fonts in /Resources");
    let font_id = font_ref.as_reference().expect("font not a reference");
    let font_obj = doc.get_object(font_id).unwrap().as_dict().unwrap();
    let to_unicode_id = font_obj
        .get(b"ToUnicode")
        .expect("Type0 font must have /ToUnicode")
        .as_reference()
        .expect("ToUnicode must be a reference");
    let stream = doc.get_object(to_unicode_id).unwrap().as_stream().unwrap();
    String::from_utf8(stream.content.clone()).expect("ToUnicode must be valid UTF-8")
}

// ---------------------------------------------------------------------------
// Regression: embedded font must not contain tables with stale GID refs
// ---------------------------------------------------------------------------
// Fonts like NotoSansJP contain GSUB, GPOS, gvar and other OpenType/variable-font
// tables that reference GIDs across the full original glyph set. After subsetting to
// a handful of characters, those GID references become stale. macOS Core Text and
// PSPDFKit validate these tables when loading the embedded font and reject it as
// malformed, causing all glyphs to display as ● replacement characters.
// This test verifies the subsetter strips those tables from the embedded font.
#[test]
fn subset_font_excludes_tables_with_stale_gid_refs() {
    let font = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf").unwrap();
    let mut doc = harumi::Document::new((595.0, 842.0)).unwrap();
    let f = doc.embed_font(&font).unwrap();
    doc.page(1)
        .unwrap()
        .add_text("Hello World", f, [72.0, 700.0], 14.0, [0.0, 0.0, 0.0])
        .unwrap();
    let out = doc.save_to_bytes().unwrap();

    let embedded_font = extract_font_file2(&out);
    let tables = list_ttf_tables(&embedded_font);

    // Tables that must NOT appear: they contain GID references that are stale
    // after subsetting and would cause Core Text / PDF viewers to reject the font.
    let forbidden = ["GSUB", "GPOS", "GDEF", "BASE", "gvar", "fvar", "avar", "HVAR",
                     "STAT", "post", "vhea", "vmtx", "kern", "morx", "mort"];
    for &tag in &forbidden {
        assert!(
            !tables.contains(&tag.to_string()),
            "subset font must not contain '{tag}' table (has stale GID refs)"
        );
    }

    // Core TrueType tables must be present.
    for &tag in &["head", "hhea", "maxp", "glyf", "loca", "hmtx"] {
        assert!(
            tables.contains(&tag.to_string()),
            "subset font is missing required '{tag}' table"
        );
    }

    // maxp.numGlyphs must match the actual number of glyphs (small subset, not 7000+).
    let num_glyphs = read_maxp_num_glyphs(&embedded_font);
    assert!(
        num_glyphs < 100,
        "maxp.numGlyphs={num_glyphs} is unexpectedly large (expected small English subset)"
    );
}

fn extract_font_file2(pdf_bytes: &[u8]) -> Vec<u8> {
    // Find "Length1" in the PDF which marks the start of a FontFile2 stream dict.
    let pos = find_bytes(pdf_bytes, b"Length1").expect("FontFile2 Length1 not found");
    let stream_start = find_bytes(&pdf_bytes[pos..], b"stream\n")
        .map(|o| pos + o + 7)
        .or_else(|| find_bytes(&pdf_bytes[pos..], b"stream\r\n").map(|o| pos + o + 8))
        .expect("FontFile2 stream start not found");
    let end = find_bytes(&pdf_bytes[stream_start..], b"endstream")
        .expect("FontFile2 endstream not found");
    pdf_bytes[stream_start..stream_start + end].to_vec()
}

fn list_ttf_tables(font_data: &[u8]) -> Vec<String> {
    if font_data.len() < 12 {
        return vec![];
    }
    let num_tables = u16::from_be_bytes([font_data[4], font_data[5]]) as usize;
    let mut tags = Vec::new();
    for i in 0..num_tables {
        let base = 12 + i * 16;
        if base + 4 > font_data.len() {
            break;
        }
        if let Ok(s) = std::str::from_utf8(&font_data[base..base + 4]) {
            tags.push(s.to_string());
        }
    }
    tags
}

fn read_maxp_num_glyphs(font_data: &[u8]) -> u16 {
    if font_data.len() < 12 {
        return 0;
    }
    let num_tables = u16::from_be_bytes([font_data[4], font_data[5]]) as usize;
    for i in 0..num_tables {
        let base = 12 + i * 16;
        if base + 16 > font_data.len() {
            break;
        }
        if &font_data[base..base + 4] == b"maxp" {
            let offset = u32::from_be_bytes([
                font_data[base + 8], font_data[base + 9],
                font_data[base + 10], font_data[base + 11],
            ]) as usize;
            if offset + 6 <= font_data.len() {
                return u16::from_be_bytes([font_data[offset + 4], font_data[offset + 5]]);
            }
        }
    }
    0
}

// Regression test: the TTF subsetter once emitted a 14-byte offset table instead of
// 12 bytes (searchRange was written as u32 instead of u16), corrupting every table
// offset. This test verifies the subsetted font has a well-formed offset table and
// that text extraction (which depends on ToUnicode CMap) and glyph rendering (which
// depends on correct table offsets) both work.
#[test]
fn subset_offset_table_is_well_formed() {
    let font = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf").unwrap();
    let mut doc = harumi::Document::new((595.0, 842.0)).unwrap();
    let f = doc.embed_font(&font).unwrap();
    doc.page(1).unwrap()
        .add_text("Hello World NotoSansJP", f, [72.0, 700.0], 14.0, [0.0, 0.0, 0.0])
        .unwrap();
    let out = doc.save_to_bytes().unwrap();

    // Verify text round-trips correctly (ToUnicode CMap OK).
    let doc2 = harumi::Document::from_bytes(&out).unwrap();
    let runs = doc2.extract_text_runs(1).unwrap();
    let text: String = runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("");
    assert!(text.contains("Hello"), "text extraction failed: {text:?}");

    // Verify the embedded font has a valid TTF offset table (12 bytes).
    // Extract the FontFile2 stream from the PDF and parse its header.
    let raw = out.as_slice();
    if let Some(pos) = find_bytes(raw, b"Length1") {
        let stream_start = find_bytes(&raw[pos..], b"stream\n")
            .map(|o| pos + o + 7)
            .or_else(|| find_bytes(&raw[pos..], b"stream\r\n").map(|o| pos + o + 8));
        if let Some(start) = stream_start {
            if let Some(end) = find_bytes(&raw[start..], b"endstream") {
                let font_data = &raw[start..start + end];
                assert!(font_data.len() > 12, "embedded font too small");
                // sfVersion: 0x00010000
                let sf = u32::from_be_bytes([font_data[0], font_data[1], font_data[2], font_data[3]]);
                assert_eq!(sf, 0x00010000, "wrong sfVersion");
                let num_tables = u16::from_be_bytes([font_data[4], font_data[5]]) as usize;
                assert!(num_tables > 0 && num_tables <= 64, "implausible numTables={num_tables}");
                // All table offsets must be within the font data.
                for i in 0..num_tables {
                    let base = 12 + i * 16;
                    if base + 16 > font_data.len() { break; }
                    let offset = u32::from_be_bytes([
                        font_data[base+8], font_data[base+9], font_data[base+10], font_data[base+11],
                    ]) as usize;
                    let length = u32::from_be_bytes([
                        font_data[base+12], font_data[base+13], font_data[base+14], font_data[base+15],
                    ]) as usize;
                    assert!(
                        offset + length <= font_data.len(),
                        "table {i} offset={offset} length={length} out of bounds (font size={})",
                        font_data.len()
                    );
                }
            }
        }
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// Regression: hhea.numberOfHMetrics must equal maxp.numGlyphs in the subset font,
// and hmtx must be exactly numGlyphs*4 bytes (all entries as full longHorMetric).
// Before the fix, build_hmtx misread advance_width for "mono" glyphs (gid >=
// num_h_metrics in the source font), and numberOfHMetrics was capped at the source
// font's num_h_metrics rather than the subset glyph count.
#[test]
fn subset_hmtx_and_hhea_are_consistent() {
    let font = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf").unwrap();
    let mut doc = harumi::Document::new((595.0, 842.0)).unwrap();
    let f = doc.embed_font(&font).unwrap();
    doc.page(1).unwrap()
        .add_text("Hello", f, [72.0, 700.0], 14.0, [0.0, 0.0, 0.0])
        .unwrap();
    let out = doc.save_to_bytes().unwrap();
    let embedded = extract_font_file2(&out);
    let num_glyphs = read_maxp_num_glyphs(&embedded) as usize;

    // hhea.numberOfHMetrics (bytes 34-35 of hhea table) must equal maxp.numGlyphs.
    let hhea_num_metrics = read_hhea_num_h_metrics(&embedded);
    assert_eq!(
        hhea_num_metrics as usize, num_glyphs,
        "hhea.numberOfHMetrics ({hhea_num_metrics}) != maxp.numGlyphs ({num_glyphs})"
    );

    // hmtx must be exactly numGlyphs * 4 bytes (all longHorMetric, no lsb-only section).
    let hmtx_len = read_table_length(&embedded, b"hmtx");
    assert_eq!(
        hmtx_len, num_glyphs * 4,
        "hmtx length ({hmtx_len}) != numGlyphs*4 ({})", num_glyphs * 4
    );
}

fn read_hhea_num_h_metrics(font_data: &[u8]) -> u16 {
    let num_tables = u16::from_be_bytes([font_data[4], font_data[5]]) as usize;
    for i in 0..num_tables {
        let base = 12 + i * 16;
        if base + 16 > font_data.len() { break; }
        if &font_data[base..base + 4] == b"hhea" {
            let offset = u32::from_be_bytes([
                font_data[base + 8], font_data[base + 9],
                font_data[base + 10], font_data[base + 11],
            ]) as usize;
            // numberOfHMetrics is at bytes 34-35 of hhea (total header = 36 bytes)
            if offset + 36 <= font_data.len() {
                return u16::from_be_bytes([font_data[offset + 34], font_data[offset + 35]]);
            }
        }
    }
    0
}

// Regression: embedded font must have a valid head.checkSumAdjustment (not always 0).
// The value B1B0AFBA is the target: sum of all 32-bit words in the font plus
// checkSumAdjustment must equal 0xB1B0AFBA (mod 2^32).
#[test]
fn subset_head_checksum_adjustment_is_valid() {
    let font = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf").unwrap();
    let mut doc = harumi::Document::new((595.0, 842.0)).unwrap();
    let f = doc.embed_font(&font).unwrap();
    doc.page(1).unwrap()
        .add_text("Hello", f, [72.0, 700.0], 14.0, [0.0, 0.0, 0.0])
        .unwrap();
    let out = doc.save_to_bytes().unwrap();
    // extract_font_file2 may include a trailing '\n' added by lopdf before "endstream".
    // Strip trailing CR/LF bytes — the TTF binary itself never ends with newline characters.
    let mut embedded = extract_font_file2(&out);
    while embedded.last().map_or(false, |&b| b == b'\n' || b == b'\r') {
        embedded.pop();
    }

    // Compute the sum of all 32-bit big-endian words in the font (wrapping).
    let total_sum: u32 = embedded.chunks(4)
        .fold(0u32, |acc, chunk| {
            let word = if chunk.len() == 4 {
                u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
            } else {
                let mut buf = [0u8; 4];
                buf[..chunk.len()].copy_from_slice(chunk);
                u32::from_be_bytes(buf)
            };
            acc.wrapping_add(word)
        });

    assert_eq!(
        total_sum, 0xB1B0AFBA,
        "font checksum invalid: sum=0x{total_sum:08X}, expected 0xB1B0AFBA"
    );
}

fn read_table_length(font_data: &[u8], tag: &[u8; 4]) -> usize {
    let num_tables = u16::from_be_bytes([font_data[4], font_data[5]]) as usize;
    for i in 0..num_tables {
        let base = 12 + i * 16;
        if base + 16 > font_data.len() { break; }
        if &font_data[base..base + 4] == tag {
            return u32::from_be_bytes([
                font_data[base + 12], font_data[base + 13],
                font_data[base + 14], font_data[base + 15],
            ]) as usize;
        }
    }
    0
}

#[test]
fn diagnose_fragment_coords() {
    let pdf = std::fs::read("test_documents/kanto_chemical/J_10005.pdf");
    if pdf.is_err() { return; }
    let doc = harumi::Document::from_bytes(&pdf.unwrap()).unwrap();
    let runs = doc.extract_text_runs(1).unwrap();
    println!("Total fragments page 1: {}", runs.len());
    for r in runs.iter().take(10) {
        println!("  x={:.1} y={:.1} w={:.1} h={:.1} fs={:.1} inv={} text={:?}",
            r.x, r.y, r.width, r.height, r.font_size, r.invisible,
            r.text.chars().take(20).collect::<String>());
    }
}
