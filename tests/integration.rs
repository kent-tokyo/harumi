mod helpers;

use harumi::Document;

/// Verifies that loading a PDF and saving it immediately preserves page count.
#[test]
fn roundtrip_no_modification() {
    let pdf_bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf_bytes).expect("load");
    let mut out = Vec::new();
    doc.save_to_writer(&mut out).expect("save");
    // Re-load and verify page count.
    let doc2 = Document::from_bytes(&out).expect("reload");
    assert_eq!(
        doc2.page_count(),
        1,
        "page count should be preserved after round-trip"
    );
}

/// Embeds a TrueType font, overlays invisible ASCII text, and verifies
/// the resulting PDF has a Type0 font object in the page resources.
#[test]
fn embed_font_and_invisible_text_ascii() {
    let Ok(font_bytes) = std::fs::read("/System/Library/Fonts/Geneva.ttf") else {
        eprintln!("Geneva.ttf not found — skipping (macOS only)");
        return;
    };

    let pdf_bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf_bytes).expect("load");

    let font = doc.embed_font(&font_bytes).expect("embed_font");

    doc.page(1)
        .expect("page 1")
        .add_invisible_text("Hello harumi", font, [72.0, 500.0], 12.0)
        .expect("add_invisible_text");

    let mut out = Vec::new();
    doc.save_to_writer(&mut out).expect("save");

    // Reload and inspect the PDF object graph.
    let reloaded = lopdf::Document::load_from(out.as_slice()).expect("reload lopdf");

    // The page should now reference a font resource.
    let pages = reloaded.get_pages();
    let page_id = pages[&1];
    let page = reloaded.get_object(page_id).unwrap().as_dict().unwrap();

    let resources = match page.get(b"Resources").unwrap() {
        lopdf::Object::Reference(r) => reloaded.get_object(*r).unwrap().as_dict().unwrap(),
        lopdf::Object::Dictionary(d) => d,
        other => panic!("unexpected Resources type: {:?}", other),
    };

    let font_dict = resources
        .get(b"Font")
        .expect("no /Font in /Resources")
        .as_dict()
        .expect("/Font should be a dict");

    assert!(!font_dict.is_empty(), "/Font dict should not be empty");

    // Find the Type0 font object.
    let (_, font_ref) = font_dict.iter().next().unwrap();
    let font_id = font_ref
        .as_reference()
        .expect("font entry should be a reference");
    let font_obj = reloaded.get_object(font_id).unwrap().as_dict().unwrap();

    let subtype = font_obj.get(b"Subtype").unwrap().as_name().unwrap();
    assert_eq!(subtype, b"Type0", "embedded font should be Type0");

    let encoding = font_obj.get(b"Encoding").unwrap().as_name().unwrap();
    assert_eq!(encoding, b"Identity-H");

    // ToUnicode stream must exist.
    let to_unicode_id = font_obj
        .get(b"ToUnicode")
        .expect("Type0 must have /ToUnicode")
        .as_reference()
        .expect("ToUnicode should be a reference");
    let to_unicode_stream = reloaded
        .get_object(to_unicode_id)
        .unwrap()
        .as_stream()
        .unwrap();
    let cmap_text = String::from_utf8(to_unicode_stream.content.clone()).unwrap();
    assert!(
        cmap_text.contains("begincmap"),
        "ToUnicode should contain begincmap"
    );
}

/// CJK test using NotoSansJP variable font (TrueType, magic 0x00010000).
#[test]
fn embed_font_and_invisible_text_japanese() {
    let font_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/NotoSansJP-Regular.ttf"
    );
    let font_bytes = std::fs::read(font_path).expect("font not found");

    let pdf_bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf_bytes).expect("load");

    let font = doc.embed_font(&font_bytes).expect("embed_font");
    doc.page(1)
        .expect("page 1")
        .add_invisible_text("日本語テスト", font, [72.0, 700.0], 14.0)
        .expect("add_invisible_text");

    let mut out = Vec::new();
    doc.save_to_writer(&mut out).expect("save");

    // Basic smoke check: output is larger than input (font was embedded).
    assert!(out.len() > pdf_bytes.len());

    // Reload and verify ToUnicode contains Japanese mappings.
    let reloaded = lopdf::Document::load_from(out.as_slice()).expect("reload");
    let pages = reloaded.get_pages();
    let page_id = pages[&1];
    let page = reloaded.get_object(page_id).unwrap().as_dict().unwrap();
    let resources = match page.get(b"Resources").unwrap() {
        lopdf::Object::Reference(r) => reloaded.get_object(*r).unwrap().as_dict().unwrap(),
        lopdf::Object::Dictionary(d) => d,
        _ => panic!("unexpected Resources"),
    };
    let font_dict = resources.get(b"Font").unwrap().as_dict().unwrap();
    let (_, font_ref) = font_dict.iter().next().unwrap();
    let font_id = font_ref.as_reference().unwrap();
    let font_obj = reloaded.get_object(font_id).unwrap().as_dict().unwrap();
    let to_unicode_id = font_obj.get(b"ToUnicode").unwrap().as_reference().unwrap();
    let stream = reloaded
        .get_object(to_unicode_id)
        .unwrap()
        .as_stream()
        .unwrap();
    let cmap = String::from_utf8(stream.content.clone()).unwrap();

    // '日' U+65E5, '本' U+672C, '語' U+8A9E should appear in ToUnicode.
    assert!(cmap.contains("65E5"), "ToUnicode should map '日'");
    assert!(cmap.contains("672C"), "ToUnicode should map '本'");
    assert!(cmap.contains("8A9E"), "ToUnicode should map '語'");
}

/// Visible text (Tr 0) with red color is embedded and the content stream
/// contains "0 Tr" and the RGB color operator.
#[test]
fn add_text_visible_with_color() {
    let Ok(font_bytes) = std::fs::read("/System/Library/Fonts/Geneva.ttf") else {
        eprintln!("Geneva.ttf not found — skipping (macOS only)");
        return;
    };

    let pdf_bytes = helpers::minimal_pdf_bytes();
    let mut doc = harumi::Document::from_bytes(&pdf_bytes).expect("load");
    let font = doc.embed_font(&font_bytes).expect("embed_font");

    doc.page(1)
        .expect("page 1")
        .add_text("Hello", font, [72.0, 400.0], 14.0, [1.0, 0.0, 0.0])
        .expect("add_text");

    let mut out = Vec::new();
    doc.save_to_writer(&mut out).expect("save");

    // Decode the content streams and verify render mode and color.
    let reloaded = lopdf::Document::load_from(out.as_slice()).expect("reload");
    let pages = reloaded.get_pages();
    let page_id = pages[&1];
    let content = reloaded.get_page_content(page_id).expect("content");
    let content_str = String::from_utf8_lossy(&content);

    assert!(content_str.contains("0 Tr"), "visible mode should use Tr 0");
    assert!(
        content_str.contains("rg"),
        "should contain RGB color operator"
    );
    assert!(
        !content_str.contains("3 Tr"),
        "should not use invisible mode"
    );
}

/// page.size() returns correct (width, height) for the A4 minimal fixture.
#[test]
fn page_size_a4() {
    let pdf_bytes = helpers::minimal_pdf_bytes();
    let mut doc = harumi::Document::from_bytes(&pdf_bytes).expect("load");
    let (w, h) = doc.page(1).expect("page 1").size().expect("size");
    assert!((w - 595.0).abs() < 1.0, "width should be ~595pt (A4)");
    assert!((h - 842.0).abs() < 1.0, "height should be ~842pt (A4)");
}

/// save_to_bytes() round-trip: reload and verify page count and font resource.
#[test]
fn save_to_bytes_roundtrip() {
    let Ok(font_bytes) = std::fs::read("/System/Library/Fonts/Geneva.ttf") else {
        eprintln!("Geneva.ttf not found — skipping (macOS only)");
        return;
    };

    let pdf_bytes = helpers::minimal_pdf_bytes();
    let mut doc = harumi::Document::from_bytes(&pdf_bytes).expect("load");
    let font = doc.embed_font(&font_bytes).expect("embed_font");
    doc.page(1)
        .unwrap()
        .add_invisible_text("hello", font, [72.0, 500.0], 12.0)
        .unwrap();

    let out = doc.save_to_bytes().expect("save_to_bytes");
    assert!(!out.is_empty());

    let reloaded = harumi::Document::from_bytes(&out).expect("reload");
    assert_eq!(reloaded.page_count(), 1, "page count preserved");
}

/// OTF font is no longer rejected with UnsupportedFontKind at embed_font() time.
///
/// allsorts v0.17 may fail to subset certain CFF variants (e.g. CFF2 variable fonts)
/// at save() time — that is an upstream limitation. This test verifies that harumi
/// at least accepts OTF fonts through embed_font() and routes errors correctly.
#[test]
fn otf_no_longer_rejected_at_embed() {
    // Find any OTF (OTTO magic) on the system.
    let otf_bytes: Option<Vec<u8>> = std::fs::read_dir("/System/Library/Fonts/Supplemental")
        .ok()
        .and_then(|entries| {
            entries
                .filter_map(|e| e.ok())
                .find(|e| {
                    let p = e.path();
                    if p.extension().map(|x| x == "otf").unwrap_or(false) {
                        if let Ok(b) = std::fs::read(&p) {
                            return b.starts_with(b"OTTO");
                        }
                    }
                    false
                })
                .and_then(|e| std::fs::read(e.path()).ok())
        });

    let Some(font_bytes) = otf_bytes else {
        eprintln!("No OTF font found — skipping otf_no_longer_rejected_at_embed");
        return;
    };

    let pdf_bytes = helpers::minimal_pdf_bytes();
    let mut doc = harumi::Document::from_bytes(&pdf_bytes).expect("load");

    // embed_font() must NOT return UnsupportedFontKind for an OTF file.
    let font = doc
        .embed_font(&font_bytes)
        .expect("embed_font must accept OTF");

    doc.page(1)
        .unwrap()
        .add_invisible_text("OTF test", font, [72.0, 500.0], 12.0)
        .unwrap();

    // save() may fail with FontParse if allsorts can't subset this particular CFF variant,
    // but it must NOT fail with UnsupportedFontKind.
    match doc.save_to_bytes() {
        Ok(_) => { /* full pipeline works for this font */ }
        Err(harumi::Error::UnsupportedFontKind) => {
            panic!("OTF font must not be rejected with UnsupportedFontKind");
        }
        Err(harumi::Error::FontParse(_)) => {
            // allsorts does not support all CFF variants — expected for some fonts.
        }
        Err(e) => panic!("unexpected error: {e}"),
    }
}

/// Save → reload verifies that the PDF object graph round-trips without corruption.
#[test]
fn roundtrip_save_reload_preserves_page_count() {
    let pdf_bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf_bytes).expect("load");
    let expected = doc.page_count();
    let bytes = doc.save_to_bytes().expect("save_to_bytes");
    let reloaded = Document::from_bytes(&bytes).expect("reload");
    assert_eq!(reloaded.page_count(), expected);
}
