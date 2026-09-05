//! Integration tests for overlay_from, scale_page_content, resize_page_with_content,
//! clear_outline, attach_file, and list_attachments.

mod helpers;

use harumi::Document;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn two_page_pdf_with_text(font_bytes: &[u8]) -> Vec<u8> {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.insert_blank_page(1, (595.0, 842.0)).unwrap(); // now 2 pages
    let font = doc.embed_font(font_bytes).unwrap();
    doc.page(1)
        .unwrap()
        .add_text("Page One", font, [72.0, 700.0], 14.0, [0.0, 0.0, 0.0])
        .unwrap();
    doc.page(2)
        .unwrap()
        .add_text("Page Two", font, [72.0, 700.0], 14.0, [0.0, 0.0, 0.0])
        .unwrap();
    doc.save_to_bytes().unwrap()
}

// ---------------------------------------------------------------------------
// scale_page_content
// ---------------------------------------------------------------------------

#[test]
fn scale_page_content_returns_ok() {
    let bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&bytes).unwrap();
    doc.page(1)
        .unwrap()
        .scale_page_content(1.414, 1.414)
        .unwrap();
    let out = doc.save_to_bytes().unwrap();
    assert!(!out.is_empty());
    // Verify the saved PDF can be re-loaded.
    let _ = Document::from_bytes(&out).unwrap();
}

#[test]
fn scale_page_content_rejects_zero_scale() {
    let bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&bytes).unwrap();
    let err = doc.page(1).unwrap().scale_page_content(0.0, 1.0);
    assert!(err.is_err());
}

#[test]
fn scale_page_content_rejects_negative_scale() {
    let bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&bytes).unwrap();
    let err = doc.page(1).unwrap().scale_page_content(1.0, -1.0);
    assert!(err.is_err());
}

#[test]
fn scale_page_content_rejects_nan() {
    let bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&bytes).unwrap();
    let err = doc.page(1).unwrap().scale_page_content(f32::NAN, 1.0);
    assert!(err.is_err());
}

// ---------------------------------------------------------------------------
// resize_page_with_content
// ---------------------------------------------------------------------------

#[test]
fn resize_page_with_content_changes_media_box() {
    let bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&bytes).unwrap();
    {
        let mut ph = doc.page(1).unwrap();
        ph.resize_page_with_content(842.0, 1190.0).unwrap(); // A4 → A3
    }
    let out = doc.save_to_bytes().unwrap();
    let mut reloaded = Document::from_bytes(&out).unwrap();
    let (w, h) = reloaded.page(1).unwrap().size().unwrap();
    assert!((w - 842.0).abs() < 1.0, "expected width ~842, got {w}");
    assert!((h - 1190.0).abs() < 1.0, "expected height ~1190, got {h}");
}

// ---------------------------------------------------------------------------
// overlay_from
// ---------------------------------------------------------------------------

#[test]
fn overlay_from_produces_valid_pdf() {
    let font_bytes = include_bytes!("fixtures/NotoSansJP-Regular.ttf");
    let base_bytes = two_page_pdf_with_text(font_bytes);
    let overlay_bytes = two_page_pdf_with_text(font_bytes);

    let base_saved = Document::from_bytes(&base_bytes)
        .unwrap()
        .save_to_bytes()
        .unwrap();
    let overlay_saved = Document::from_bytes(&overlay_bytes)
        .unwrap()
        .save_to_bytes()
        .unwrap();

    let mut base = Document::from_bytes(&base_saved).unwrap();
    let overlay = Document::from_bytes(&overlay_saved).unwrap();
    base.overlay_from(overlay).unwrap();

    let out = base.save_to_bytes().unwrap();
    assert!(!out.is_empty());

    // Verify structure with lopdf directly.
    let inner = lopdf::Document::load_mem(&out).unwrap();
    assert_eq!(
        inner.get_pages().len(),
        2,
        "base doc must still have 2 pages"
    );

    // Page 1 must have OVRL0 in /Resources/XObject and 'Do' in its content.
    let page1_id = *inner.get_pages().get(&1).unwrap();
    let page1_dict = inner.get_object(page1_id).unwrap().as_dict().unwrap();

    let res = page1_dict.get(b"Resources").unwrap();
    let res_dict = match res {
        lopdf::Object::Dictionary(d) => d.clone(),
        lopdf::Object::Reference(r) => inner.get_object(*r).unwrap().as_dict().unwrap().clone(),
        _ => panic!("unexpected Resources type"),
    };
    let xobj = res_dict.get(b"XObject").unwrap().as_dict().unwrap();
    assert!(
        xobj.get(b"OVRL0").is_ok(),
        "OVRL0 must be present in /Resources/XObject"
    );

    // Content must include the Do operator.
    let content_bytes = inner.get_page_content(page1_id).unwrap();
    let content_str = String::from_utf8_lossy(&content_bytes);
    assert!(
        content_str.contains("Do"),
        "page 1 content must contain 'Do' operator"
    );
}

#[test]
fn overlay_from_inherited_resources() {
    // Build a PDF where /Resources is on the /Pages node (not the page itself),
    // like InDesign or Word exports. overlay_from must still find them.
    let font_bytes = include_bytes!("fixtures/NotoSansJP-Regular.ttf");
    let overlay_bytes = two_page_pdf_with_text(font_bytes);
    let overlay_saved = Document::from_bytes(&overlay_bytes)
        .unwrap()
        .save_to_bytes()
        .unwrap();

    // Build a base PDF with inherited resources using lopdf directly.
    let base_bytes = {
        use lopdf::{Document as LDoc, Object, Stream, dictionary};
        let mut doc = LDoc::with_version("1.4");
        let pages_id = doc.new_object_id();
        let content = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            b"q Q\n".to_vec(),
        )));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(595), Object::Integer(842),
            ]),
            "Contents" => Object::Reference(content),
        }));
        // Resources on the /Pages node (inherited), NOT on the page dict.
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Pages".to_vec()),
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
                "Resources" => Object::Dictionary(dictionary! {}),
            }),
        );
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    };

    let mut base = Document::from_bytes(&base_bytes).unwrap();
    let overlay = Document::from_bytes(&overlay_saved).unwrap();
    base.overlay_from(overlay).unwrap();
    let out = base.save_to_bytes().unwrap();
    // Verify it reloads cleanly.
    let _ = Document::from_bytes(&out).unwrap();
    // Content must include 'Do'.
    let inner = lopdf::Document::load_mem(&out).unwrap();
    let page1_id = *inner.get_pages().get(&1).unwrap();
    let content_bytes = inner.get_page_content(page1_id).unwrap();
    let content_str = String::from_utf8_lossy(&content_bytes);
    assert!(
        content_str.contains("Do"),
        "overlay content must include 'Do' operator even with inherited resources"
    );
}

#[test]
fn overlay_from_pending_ops_returns_err() {
    let font_bytes = include_bytes!("fixtures/NotoSansJP-Regular.ttf");
    // other has pending ops (not yet saved).
    let mut other = Document::new((595.0, 842.0)).unwrap();
    let font = other.embed_font(font_bytes).unwrap();
    other
        .page(1)
        .unwrap()
        .add_text("stamp", font, [72.0, 700.0], 12.0, [0.0, 0.0, 0.0])
        .unwrap();

    let base_bytes = helpers::minimal_pdf_bytes();
    let mut base = Document::from_bytes(&base_bytes).unwrap();
    let err = base.overlay_from(other);
    assert!(err.is_err());
}

// ---------------------------------------------------------------------------
// clear_outline
// ---------------------------------------------------------------------------

#[test]
fn clear_outline_removes_pending_bookmarks() {
    let bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&bytes).unwrap();
    doc.add_bookmark("Chapter 1", 1, 700.0).unwrap();
    doc.clear_outline().unwrap();
    // After clearing, save should produce a PDF with no /Outlines.
    let out = doc.save_to_bytes().unwrap();
    let reloaded_inner = lopdf::Document::load_mem(&out).unwrap();
    let root_ref = reloaded_inner
        .trailer
        .get(b"Root")
        .unwrap()
        .as_reference()
        .unwrap();
    let catalog = reloaded_inner
        .get_object(root_ref)
        .unwrap()
        .as_dict()
        .unwrap();
    assert!(
        catalog.get(b"Outlines").is_err(),
        "/Outlines should be absent after clear_outline()"
    );
}

// ---------------------------------------------------------------------------
// attach_file / list_attachments
// ---------------------------------------------------------------------------

#[test]
fn attach_and_list_single_file() {
    let bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&bytes).unwrap();
    doc.attach_file("hello.txt", b"Hello, world!", "text/plain")
        .unwrap();
    let out = doc.save_to_bytes().unwrap();

    let reloaded = Document::from_bytes(&out).unwrap();
    let attachments = reloaded.list_attachments().unwrap();
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].filename, "hello.txt");
    assert_eq!(attachments[0].size, 13);
    assert_eq!(attachments[0].mime_type.as_deref(), Some("text/plain"));
}

#[test]
fn attach_multiple_files() {
    let bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&bytes).unwrap();
    doc.attach_file("a.txt", b"aaa", "text/plain").unwrap();
    doc.attach_file(
        "b.bin",
        &[0xDE, 0xAD, 0xBE, 0xEF],
        "application/octet-stream",
    )
    .unwrap();
    let out = doc.save_to_bytes().unwrap();

    let reloaded = Document::from_bytes(&out).unwrap();
    let attachments = reloaded.list_attachments().unwrap();
    assert_eq!(attachments.len(), 2);
    let names: Vec<&str> = attachments.iter().map(|a| a.filename.as_str()).collect();
    assert!(names.contains(&"a.txt"));
    assert!(names.contains(&"b.bin"));
}

#[test]
fn attach_file_empty_filename_returns_err() {
    let bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&bytes).unwrap();
    let err = doc.attach_file("", b"data", "text/plain");
    assert!(err.is_err());
}

#[test]
fn list_attachments_on_clean_pdf_returns_empty() {
    let bytes = helpers::minimal_pdf_bytes();
    let doc = Document::from_bytes(&bytes).unwrap();
    let attachments = doc.list_attachments().unwrap();
    assert!(attachments.is_empty());
}

#[test]
fn attach_files_sorted_in_names_array() {
    // Add files in reverse order — the raw /Names array must come out alphabetically sorted.
    let bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&bytes).unwrap();
    doc.attach_file("z_last.txt", b"zzz", "text/plain").unwrap();
    doc.attach_file("a_first.txt", b"aaa", "text/plain")
        .unwrap();
    let out = doc.save_to_bytes().unwrap();

    // Verify raw /Names array ordering via lopdf.
    let inner = lopdf::Document::load_mem(&out).unwrap();
    let root_ref = inner.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let catalog = inner.get_object(root_ref).unwrap().as_dict().unwrap();
    let names_ref = catalog.get(b"Names").unwrap().as_reference().unwrap();
    let names_dict = inner.get_object(names_ref).unwrap().as_dict().unwrap();
    let ef_ref = names_dict
        .get(b"EmbeddedFiles")
        .unwrap()
        .as_reference()
        .unwrap();
    let ef_dict = inner.get_object(ef_ref).unwrap().as_dict().unwrap();
    let arr = ef_dict.get(b"Names").unwrap().as_array().unwrap();
    // First key must be "a_first.txt" (alphabetically smaller).
    let first_key = match &arr[0] {
        lopdf::Object::String(b, _) => String::from_utf8_lossy(b).into_owned(),
        _ => panic!("expected string key"),
    };
    assert_eq!(
        first_key, "a_first.txt",
        "/Names array must be sorted alphabetically"
    );
}

#[test]
fn attach_file_without_mime_type() {
    let bytes = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&bytes).unwrap();
    doc.attach_file("data.bin", b"raw", "").unwrap();
    let out = doc.save_to_bytes().unwrap();

    let reloaded = Document::from_bytes(&out).unwrap();
    let attachments = reloaded.list_attachments().unwrap();
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].filename, "data.bin");
    assert!(attachments[0].mime_type.is_none());
}
