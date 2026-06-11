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
