use harumi::Document;

const FONT: &[u8] = include_bytes!("fixtures/NotoSansJP-Regular.ttf");

/// Helper: create a minimal PDF with a known text run, save to bytes.
fn pdf_with_text(text: &str) -> Vec<u8> {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().add_invisible_text(text, font, [72.0, 700.0], 14.0).unwrap();
    doc.save_to_bytes().unwrap()
}

#[test]
fn replace_text_latin_present_in_output() {
    let bytes = pdf_with_text("Hello");

    let mut doc = Document::from_bytes(&bytes).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().replace_text("Hello", "World", font).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let check = Document::from_bytes(&out).unwrap();
    let frags = check.extract_text_runs(1).unwrap();
    let texts: Vec<&str> = frags.iter().map(|f| f.text.as_str()).collect();

    // Replacement text must appear
    assert!(
        texts.iter().any(|&t| t.contains("World")),
        "expected 'World' in extracted text, got: {:?}",
        texts
    );
    // Original text must be gone
    assert!(
        !texts.iter().any(|&t| t.contains("Hello")),
        "expected 'Hello' to be gone, got: {:?}",
        texts
    );
}

#[test]
fn replace_text_no_match_is_noop() {
    let bytes = pdf_with_text("Alpha");

    let mut doc = Document::from_bytes(&bytes).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().replace_text("Beta", "Gamma", font).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let check = Document::from_bytes(&out).unwrap();
    let frags = check.extract_text_runs(1).unwrap();
    let texts: Vec<&str> = frags.iter().map(|f| f.text.as_str()).collect();

    assert!(
        texts.iter().any(|&t| t.contains("Alpha")),
        "expected 'Alpha' to remain, got: {:?}",
        texts
    );
    assert!(
        !texts.iter().any(|&t| t.contains("Gamma")),
        "expected 'Gamma' to be absent, got: {:?}",
        texts
    );
}

#[test]
fn replace_text_cjk() {
    let bytes = pdf_with_text("日本語");

    let mut doc = Document::from_bytes(&bytes).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().replace_text("日本語", "英語", font).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let check = Document::from_bytes(&out).unwrap();
    let frags = check.extract_text_runs(1).unwrap();
    let texts: Vec<&str> = frags.iter().map(|f| f.text.as_str()).collect();

    assert!(
        texts.iter().any(|&t| t.contains("英語")),
        "expected '英語' in extracted text, got: {:?}",
        texts
    );
    assert!(
        !texts.iter().any(|&t| t.contains("日本語")),
        "expected '日本語' to be gone, got: {:?}",
        texts
    );
}

#[test]
fn replace_preserve_font_success() {
    // Two runs ensure both glyph sets land in the same font subset.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().add_invisible_text("Alpha", font, [72.0, 700.0], 14.0).unwrap();
    doc.page(1).unwrap().add_invisible_text("Beta", font, [72.0, 680.0], 14.0).unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let mut doc2 = Document::from_bytes(&bytes).unwrap();
    doc2.page(1).unwrap().replace_text_preserve_font("Alpha", "Beta").unwrap();
    let out = doc2.save_to_bytes().unwrap();

    let check = Document::from_bytes(&out).unwrap();
    let frags = check.extract_text_runs(1).unwrap();
    let all: String = frags.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join("");
    assert!(all.contains("Beta"), "expected 'Beta' in: {:?}", all);
    assert!(!all.contains("Alpha"), "expected 'Alpha' gone from: {:?}", all);
}

#[test]
fn replace_preserve_font_missing_glyph_returns_err() {
    // Only "Hello" glyphs (H,e,l,o) are in the subset; "World" needs W,r,d which are absent.
    let bytes = pdf_with_text("Hello");

    let mut doc = Document::from_bytes(&bytes).unwrap();
    doc.page(1).unwrap().replace_text_preserve_font("Hello", "World").unwrap();
    let result = doc.save_to_bytes();
    assert!(result.is_err(), "expected Err for char not in font subset, got Ok");
}

#[test]
fn replace_preserve_font_cjk() {
    // Include both strings so all glyphs are in the subset.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().add_invisible_text("日本語", font, [72.0, 700.0], 14.0).unwrap();
    doc.page(1).unwrap().add_invisible_text("英語", font, [72.0, 680.0], 14.0).unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let mut doc2 = Document::from_bytes(&bytes).unwrap();
    doc2.page(1).unwrap().replace_text_preserve_font("日本語", "英語").unwrap();
    let out = doc2.save_to_bytes().unwrap();

    let check = Document::from_bytes(&out).unwrap();
    let frags = check.extract_text_runs(1).unwrap();
    let all: String = frags.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join("");
    assert!(all.contains("英語"), "expected '英語' in: {:?}", all);
    assert!(!all.contains("日本語"), "expected '日本語' gone from: {:?}", all);
}

#[test]
fn replace_multiple_on_same_page() {
    // Two distinct runs on the same page, both get replaced
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().add_invisible_text("Foo", font, [72.0, 700.0], 14.0).unwrap();
    doc.page(1).unwrap().add_invisible_text("Bar", font, [72.0, 680.0], 14.0).unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let mut doc2 = Document::from_bytes(&bytes).unwrap();
    let font2 = doc2.embed_font(FONT).unwrap();
    doc2.page(1).unwrap().replace_text("Foo", "Baz", font2).unwrap();
    doc2.page(1).unwrap().replace_text("Bar", "Qux", font2).unwrap();
    let out = doc2.save_to_bytes().unwrap();

    let check = Document::from_bytes(&out).unwrap();
    let frags = check.extract_text_runs(1).unwrap();
    let all_text: String = frags.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join("");

    assert!(all_text.contains("Baz"), "expected Baz in: {:?}", all_text);
    assert!(all_text.contains("Qux"), "expected Qux in: {:?}", all_text);
    assert!(!all_text.contains("Foo"), "expected Foo gone: {:?}", all_text);
    assert!(!all_text.contains("Bar"), "expected Bar gone: {:?}", all_text);
}
