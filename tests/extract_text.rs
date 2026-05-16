//! Integration tests for Document::extract_text_runs.

use harumi::{Document, Error, TextRun};

// ---------------------------------------------------------------------------
// Helper: build a minimal single-page PDF with a Type1 WinAnsiEncoding font
// and the given raw content stream bytes.
// ---------------------------------------------------------------------------

fn minimal_winansi_pdf(content_stream: &[u8]) -> Vec<u8> {
    use harumi::lopdf::{dictionary, Document as LDoc, Object, Stream};

    let mut doc = LDoc::with_version("1.4");
    let pages_id = doc.new_object_id();

    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));

    let stream_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        content_stream.to_vec(),
    )));

    let page_id = doc.new_object_id();
    doc.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "F1" => Object::Reference(font_id),
                }),
            }),
            "Contents" => Object::Reference(stream_id),
        }),
    );
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
        }),
    );
    let cat_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(cat_id));

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

fn font_bytes() -> Vec<u8> {
    std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("tests/fixtures/NotoSansJP-Regular.ttf not found")
}

// ---------------------------------------------------------------------------
// Basic cases
// ---------------------------------------------------------------------------

#[test]
fn empty_page_returns_empty() {
    let doc = Document::new((595.0, 842.0)).unwrap();
    let fragments = doc.extract_text_runs(1).unwrap();
    assert!(fragments.is_empty(), "blank page should have no fragments");
}

#[test]
fn page_not_found_error() {
    let doc = Document::new((595.0, 842.0)).unwrap();
    let err = doc.extract_text_runs(99).unwrap_err();
    assert!(
        matches!(err, Error::PageNotFound(99)),
        "expected PageNotFound(99), got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Round-trip: add text → save → reload → extract
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_invisible_text() {
    let font_bytes = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("Hello", font, [72.0, 700.0], 12.0)
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let fragments = doc2.extract_text_runs(1).unwrap();

    assert_eq!(fragments.len(), 1, "expected 1 fragment, got {}", fragments.len());
    assert_eq!(fragments[0].text, "Hello");
    assert!(
        (fragments[0].x - 72.0).abs() < 1.0,
        "x should be ~72, got {}",
        fragments[0].x
    );
    assert!(
        (fragments[0].y - 700.0).abs() < 1.0,
        "y should be ~700, got {}",
        fragments[0].y
    );
    assert!(
        (fragments[0].font_size - 12.0).abs() < 0.5,
        "font_size should be ~12, got {}",
        fragments[0].font_size
    );
}

#[test]
fn roundtrip_cjk_text() {
    let font_bytes = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("日本語", font, [100.0, 500.0], 14.0)
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let fragments = doc2.extract_text_runs(1).unwrap();

    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0].text, "日本語");
}

#[test]
fn roundtrip_multiple_runs() {
    let font_bytes = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text_runs(&[
            TextRun {
                text: "First line".into(),
                font,
                x: 72.0,
                y: 700.0,
                font_size: 11.0,
                render_mode: 3,
                color: [0.0; 3],
            },
            TextRun {
                text: "Second line".into(),
                font,
                x: 72.0,
                y: 685.0,
                font_size: 11.0,
                render_mode: 3,
                color: [0.0; 3],
            },
        ])
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let fragments = doc2.extract_text_runs(1).unwrap();

    assert_eq!(fragments.len(), 2, "expected 2 fragments, got {}", fragments.len());
    assert_eq!(fragments[0].text, "First line");
    assert_eq!(fragments[1].text, "Second line");
    assert!(
        (fragments[1].y - 685.0).abs() < 1.0,
        "second run y should be ~685, got {}",
        fragments[1].y
    );
}

#[test]
fn width_nonzero() {
    let font_bytes = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("Hello", font, [72.0, 700.0], 12.0)
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let fragments = doc2.extract_text_runs(1).unwrap();

    assert!(!fragments.is_empty());
    assert!(
        fragments[0].width > 0.0,
        "width should be positive, got {}",
        fragments[0].width
    );
}

#[test]
fn pending_ops_not_visible_before_save() {
    let font_bytes = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    // Queue a text op but do NOT save
    doc.page(1)
        .unwrap()
        .add_invisible_text("Unsaved", font, [72.0, 700.0], 12.0)
        .unwrap();

    // extract_text_runs on the unflushed document sees the pre-save state
    let fragments = doc.extract_text_runs(1).unwrap();
    assert!(
        fragments.is_empty(),
        "pending (unflushed) text should not appear before save"
    );
}

// ---------------------------------------------------------------------------
// Simple font (Type1 / WinAnsiEncoding) extraction tests
// ---------------------------------------------------------------------------

#[test]
fn simple_font_winansi_hex_string() {
    // <48656C6C6F> = "Hello" in WinAnsi/ASCII
    let content = b"BT\n/F1 12 Tf\n72 700 Td\n<48656C6C6F> Tj\nET\n";
    let pdf_bytes = minimal_winansi_pdf(content);
    let doc = Document::from_bytes(&pdf_bytes).unwrap();
    let frags = doc.extract_text_runs(1).unwrap();
    assert_eq!(frags.len(), 1, "expected 1 fragment, got {}", frags.len());
    assert_eq!(frags[0].text, "Hello");
    assert!((frags[0].x - 72.0).abs() < 1.0, "x={}", frags[0].x);
    assert!((frags[0].y - 700.0).abs() < 1.0, "y={}", frags[0].y);
    assert!((frags[0].font_size - 12.0).abs() < 0.5, "font_size={}", frags[0].font_size);
}

#[test]
fn simple_font_winansi_literal_string() {
    let content = b"BT\n/F1 12 Tf\n72 700 Td\n(Hello) Tj\nET\n";
    let pdf_bytes = minimal_winansi_pdf(content);
    let doc = Document::from_bytes(&pdf_bytes).unwrap();
    let frags = doc.extract_text_runs(1).unwrap();
    assert_eq!(frags.len(), 1, "expected 1 fragment, got {}", frags.len());
    assert_eq!(frags[0].text, "Hello");
}

#[test]
fn simple_font_tj_array_mixed() {
    // [(Hel) -50 (lo)] TJ — TJ produces one fragment per string element
    let content = b"BT\n/F1 12 Tf\n72 700 Td\n[(Hel) -50 (lo)] TJ\nET\n";
    let pdf_bytes = minimal_winansi_pdf(content);
    let doc = Document::from_bytes(&pdf_bytes).unwrap();
    let frags = doc.extract_text_runs(1).unwrap();
    let combined: String = frags.iter().map(|f| f.text.as_str()).collect();
    assert_eq!(combined, "Hello");
}

#[test]
fn simple_font_encoding_fallback_no_tounicode() {
    // 0xE9 = 'é' in WinAnsiEncoding — no /ToUnicode in font dict
    let content = b"BT\n/F1 12 Tf\n72 700 Td\n<E9> Tj\nET\n";
    let pdf_bytes = minimal_winansi_pdf(content);
    let doc = Document::from_bytes(&pdf_bytes).unwrap();
    let frags = doc.extract_text_runs(1).unwrap();
    assert_eq!(frags.len(), 1, "expected 1 fragment, got {}", frags.len());
    assert_eq!(frags[0].text, "é");
}
