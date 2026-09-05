//! E2E smoke test: ocrs-cjk fixture → harumi invisible text → searchable PDF.
//!
//! Proves that:
//!   1. High-confidence words end up in extract_text_runs output.
//!   2. Low-confidence words (< 0.50) are NOT embedded.
//!   3. Page count and MediaBox are unchanged after write-back.

use harumi::Document;
use serde::Deserialize;

const FONT_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/NotoSansJP-Regular.ttf"
);
const PDF_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/fixtures/scanned_sample.pdf"
);
const JSON_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/fixtures/ocrs_sample.json"
);

const MIN_CONFIDENCE: f32 = 0.50;

// ── Minimal ocrs-cjk JSON schema (matches HierText format) ──────────────────

#[derive(Deserialize)]
struct OcrsOutput {
    image_width: u32,
    image_height: u32,
    paragraphs: Vec<Paragraph>,
}

#[derive(Deserialize)]
struct Paragraph {
    lines: Vec<Line>,
}

#[derive(Deserialize)]
struct Line {
    words: Vec<Word>,
}

#[derive(Deserialize)]
struct Word {
    text: String,
    confidence: f32,
    vertices: Vec<[u32; 2]>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn apply_ocrs_json(
    doc: &mut Document,
    font: harumi::FontHandle,
    ocrs: &OcrsOutput,
) -> Result<usize, harumi::Error> {
    let (page_w, page_h) = doc.page(1)?.size()?;
    let scale_x = if ocrs.image_width > 0 {
        page_w / ocrs.image_width as f32
    } else {
        1.0
    };
    let scale_y = if ocrs.image_height > 0 {
        page_h / ocrs.image_height as f32
    } else {
        1.0
    };

    let mut placed = 0;
    for para in &ocrs.paragraphs {
        for line in &para.lines {
            for word in &line.words {
                if word.confidence < MIN_CONFIDENCE
                    || word.text.trim().is_empty()
                    || word.vertices.len() < 4
                {
                    continue;
                }
                let [x_tl, y_tl] = word.vertices[2]; // top-left
                let [_, y_bl] = word.vertices[1]; // bottom-left
                let pdf_x = x_tl as f32 * scale_x;
                let pdf_y = page_h - (y_bl as f32 * scale_y);
                let font_size = ((y_bl as f32 - y_tl as f32) * scale_y).max(4.0);
                doc.page(1)?
                    .add_invisible_text(&word.text, font, [pdf_x, pdf_y], font_size)?;
                placed += 1;
            }
        }
    }
    Ok(placed)
}

// ── Test ─────────────────────────────────────────────────────────────────────

#[test]
fn ocrs_cjk_fixture_becomes_searchable_without_rasterizing() {
    // ── Setup ────────────────────────────────────────────────────────────────

    let font_bytes = std::fs::read(FONT_PATH).expect("NotoSansJP-Regular.ttf missing");
    let pdf_bytes = std::fs::read(PDF_PATH).expect("scanned_sample.pdf missing");
    let json_bytes = std::fs::read(JSON_PATH).expect("ocrs_sample.json missing");

    let ocrs: OcrsOutput =
        serde_json::from_slice(&json_bytes).expect("failed to parse ocrs_sample.json");

    // ── Build searchable PDF ─────────────────────────────────────────────────

    let mut doc = Document::from_bytes(&pdf_bytes).expect("from_bytes");
    let page_count_before = doc.page_count();
    let page_size_before = doc.page(1).unwrap().size().unwrap();

    let font = doc.embed_font(&font_bytes).expect("embed_font");
    let placed = apply_ocrs_json(&mut doc, font, &ocrs).expect("apply_ocrs_json");
    assert!(
        placed > 0,
        "no words were placed — fixture or threshold issue"
    );

    let out = doc.save_to_bytes().expect("save_to_bytes");
    assert!(!out.is_empty());

    // ── Reload and verify ────────────────────────────────────────────────────

    let mut doc2 = Document::from_bytes(&out).expect("reload");

    // 3. Page count unchanged.
    assert_eq!(doc2.page_count(), page_count_before, "page count changed");

    // 4. MediaBox unchanged (within rounding).
    let page_size_after = doc2.page(1).unwrap().size().unwrap();
    assert!(
        (page_size_after.0 - page_size_before.0).abs() < 1.0
            && (page_size_after.1 - page_size_before.1).abs() < 1.0,
        "MediaBox changed: before={page_size_before:?} after={page_size_after:?}"
    );

    let runs = doc2.extract_text_runs(1).expect("extract_text_runs");
    let text: String = runs
        .iter()
        .map(|r| r.text.as_str())
        .collect::<Vec<_>>()
        .join("");

    // 1. High-confidence words are present.
    assert!(
        text.contains("請求書"),
        "missing 請求書  (conf 0.97) in: {text:?}"
    );
    assert!(
        text.contains("株式会社サンプル"),
        "missing 株式会社サンプル (conf 0.93) in: {text:?}"
    );
    assert!(
        text.contains("品名"),
        "missing 品名  (conf 0.88) in: {text:?}"
    );
    assert!(
        text.contains("金額"),
        "missing 金額  (conf 0.91) in: {text:?}"
    );

    // 2. Low-confidence word is absent.
    assert!(
        !text.contains("低品質"),
        "低品質のサンプル (conf 0.35) should NOT be embedded, found in: {text:?}"
    );
}
