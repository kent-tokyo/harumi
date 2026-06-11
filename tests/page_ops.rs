//! Integration tests for page manipulation: rotate_page, remove_page,
//! insert_blank_page, reorder_pages.

mod helpers;

use harumi::{Document, Error};
use lopdf::Object;

// ---------------------------------------------------------------------------
// Helper: build a 3-page PDF where each page has a distinct MediaBox height
// (100, 200, 300 pt) so that page identity can be verified after reorder/remove.
// ---------------------------------------------------------------------------

fn three_page_pdf() -> Vec<u8> {
    use lopdf::{Document as LDoc, Stream, dictionary};

    let mut doc = LDoc::with_version("1.4");
    let pages_id = doc.new_object_id();

    let make_page = |d: &mut LDoc, parent, height: i64| {
        let sid = d.add_object(Object::Stream(Stream::new(
            dictionary! {},
            b"q Q\n".to_vec(),
        )));
        d.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(parent),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(595), Object::Integer(height),
            ]),
            "Contents" => Object::Reference(sid),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {})
            }),
        }))
    };

    let p1 = make_page(&mut doc, pages_id, 100); // page 1: height 100
    let p2 = make_page(&mut doc, pages_id, 200); // page 2: height 200
    let p3 = make_page(&mut doc, pages_id, 300); // page 3: height 300

    doc.objects.insert(
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
    let cat = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(cat));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

/// Returns the MediaBox height for page `n` (1-indexed) after reloading `bytes`.
fn page_height(bytes: &[u8], n: u32) -> f32 {
    let reloaded = lopdf::Document::load_from(bytes).unwrap();
    let pages = reloaded.get_pages();
    let page_id = pages[&n];
    let page = reloaded.get_object(page_id).unwrap().as_dict().unwrap();
    let mb = page.get(b"MediaBox").unwrap().as_array().unwrap();
    match &mb[3] {
        Object::Integer(v) => *v as f32,
        Object::Real(v) => *v,
        _ => panic!("unexpected MediaBox entry"),
    }
}

// ---------------------------------------------------------------------------
// rotate_page
// ---------------------------------------------------------------------------

#[test]
fn smoke_rotate_page_90() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    doc.rotate_page(1, 90).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = reloaded.get_pages();
    let page = reloaded.get_object(pages[&1]).unwrap().as_dict().unwrap();
    let rotate = page.get(b"Rotate").unwrap().as_i64().unwrap();
    assert_eq!(rotate, 90, "expected /Rotate=90");
}

#[test]
fn smoke_rotate_page_accumulate() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    doc.rotate_page(1, 90).unwrap();
    doc.rotate_page(1, 90).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = reloaded.get_pages();
    let page = reloaded.get_object(pages[&1]).unwrap().as_dict().unwrap();
    let rotate = page.get(b"Rotate").unwrap().as_i64().unwrap();
    assert_eq!(rotate, 180, "two 90° rotations should give 180");
}

#[test]
fn smoke_rotate_page_negative() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    doc.rotate_page(1, -90).unwrap(); // CCW 90° = CW 270°
    let out = doc.save_to_bytes().unwrap();

    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = reloaded.get_pages();
    let page = reloaded.get_object(pages[&1]).unwrap().as_dict().unwrap();
    let rotate = page.get(b"Rotate").unwrap().as_i64().unwrap();
    assert_eq!(rotate, 270, "negative rotation should wrap to 270");
}

#[test]
fn smoke_rotate_invalid_degrees() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let err = doc.rotate_page(1, 45).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "non-multiple of 90 should return InvalidInput"
    );
}

#[test]
fn smoke_rotate_page_not_found() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let err = doc.rotate_page(99, 90).unwrap_err();
    assert!(matches!(err, Error::PageNotFound(99)));
}

// ---------------------------------------------------------------------------
// remove_page
// ---------------------------------------------------------------------------

#[test]
fn smoke_remove_page_middle() {
    let mut doc = Document::from_bytes(&three_page_pdf()).unwrap();
    doc.remove_page(2).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    assert_eq!(
        reloaded.get_pages().len(),
        2,
        "should have 2 pages after removal"
    );

    // Original pages 1 (h=100) and 3 (h=300) should remain.
    assert_eq!(
        page_height(&out, 1),
        100.0,
        "new page 1 should be old page 1"
    );
    assert_eq!(
        page_height(&out, 2),
        300.0,
        "new page 2 should be old page 3"
    );
}

#[test]
fn smoke_remove_page_first() {
    let mut doc = Document::from_bytes(&three_page_pdf()).unwrap();
    doc.remove_page(1).unwrap();
    let out = doc.save_to_bytes().unwrap();

    assert_eq!(
        page_height(&out, 1),
        200.0,
        "new page 1 should be old page 2"
    );
    assert_eq!(
        page_height(&out, 2),
        300.0,
        "new page 2 should be old page 3"
    );
}

#[test]
fn smoke_remove_page_only_one() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let err = doc.remove_page(1).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "should error on last page"
    );
}

#[test]
fn smoke_remove_page_not_found() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let err = doc.remove_page(99).unwrap_err();
    assert!(matches!(err, Error::PageNotFound(99)));
}

// ---------------------------------------------------------------------------
// insert_blank_page
// ---------------------------------------------------------------------------

#[test]
fn smoke_insert_blank_prepend() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    doc.insert_blank_page(0, (200.0, 300.0)).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    assert_eq!(reloaded.get_pages().len(), 2, "should have 2 pages");
    assert_eq!(
        page_height(&out, 1),
        300.0,
        "page 1 should be the new blank (h=300)"
    );
    assert_eq!(
        page_height(&out, 2),
        842.0,
        "page 2 should be the original A4"
    );
}

#[test]
fn smoke_insert_blank_append() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    doc.insert_blank_page(1, (200.0, 400.0)).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    assert_eq!(reloaded.get_pages().len(), 2);
    assert_eq!(
        page_height(&out, 1),
        842.0,
        "page 1 should be the original A4"
    );
    assert_eq!(
        page_height(&out, 2),
        400.0,
        "page 2 should be the new blank (h=400)"
    );
}

#[test]
fn smoke_insert_blank_middle() {
    let mut doc = Document::from_bytes(&three_page_pdf()).unwrap();
    doc.insert_blank_page(1, (595.0, 500.0)).unwrap(); // insert after page 1
    let out = doc.save_to_bytes().unwrap();

    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    assert_eq!(reloaded.get_pages().len(), 4);
    assert_eq!(page_height(&out, 1), 100.0, "page 1 unchanged");
    assert_eq!(page_height(&out, 2), 500.0, "page 2 is new blank");
    assert_eq!(page_height(&out, 3), 200.0, "page 3 is old page 2");
    assert_eq!(page_height(&out, 4), 300.0, "page 4 is old page 3");
}

#[test]
fn smoke_insert_blank_after_exceeds_count() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let err = doc.insert_blank_page(99, (595.0, 842.0)).unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)));
}

// ---------------------------------------------------------------------------
// reorder_pages
// ---------------------------------------------------------------------------

#[test]
fn smoke_reorder_reverse() {
    let mut doc = Document::from_bytes(&three_page_pdf()).unwrap();
    doc.reorder_pages(&[3, 2, 1]).unwrap();
    let out = doc.save_to_bytes().unwrap();

    assert_eq!(page_height(&out, 1), 300.0, "new page 1 = old page 3");
    assert_eq!(page_height(&out, 2), 200.0, "new page 2 = old page 2");
    assert_eq!(page_height(&out, 3), 100.0, "new page 3 = old page 1");
}

#[test]
fn smoke_reorder_rotate_left() {
    let mut doc = Document::from_bytes(&three_page_pdf()).unwrap();
    doc.reorder_pages(&[2, 3, 1]).unwrap();
    let out = doc.save_to_bytes().unwrap();

    assert_eq!(page_height(&out, 1), 200.0);
    assert_eq!(page_height(&out, 2), 300.0);
    assert_eq!(page_height(&out, 3), 100.0);
}

#[test]
fn smoke_reorder_length_mismatch() {
    let mut doc = Document::from_bytes(&three_page_pdf()).unwrap();
    let err = doc.reorder_pages(&[1, 2]).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "length mismatch should be InvalidInput"
    );
}

#[test]
fn smoke_reorder_duplicate() {
    let mut doc = Document::from_bytes(&three_page_pdf()).unwrap();
    let err = doc.reorder_pages(&[1, 1, 3]).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "duplicate should be InvalidInput"
    );
}

#[test]
fn smoke_reorder_out_of_range() {
    let mut doc = Document::from_bytes(&three_page_pdf()).unwrap();
    let err = doc.reorder_pages(&[1, 2, 99]).unwrap_err();
    assert!(matches!(err, Error::PageNotFound(99)));
}

#[test]
fn smoke_reorder_zero_entry() {
    let mut doc = Document::from_bytes(&three_page_pdf()).unwrap();
    let err = doc.reorder_pages(&[0, 2, 3]).unwrap_err();
    assert!(matches!(err, Error::PageNotFound(0)));
}

// ---------------------------------------------------------------------------
// finalized guard
// ---------------------------------------------------------------------------

#[test]
fn smoke_page_ops_after_save_returns_error() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    // embed a font and add text so finalize() actually does work
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("NotoSansJP-Regular.ttf not found");
    let font = doc.embed_font(&font_bytes).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("test", font, [72.0, 700.0], 12.0)
        .unwrap();
    doc.save_to_bytes().unwrap(); // sets finalized = true

    let err = doc.rotate_page(1, 90).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "rotate after save should error"
    );

    let err = doc.remove_page(1).unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)));

    let err = doc.insert_blank_page(0, (595.0, 842.0)).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "insert_blank_page after save should error"
    );

    let err = doc.reorder_pages(&[1]).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "reorder_pages after save should error"
    );

    assert!(doc.page(1).is_err(), "page() after save should error");
}

// ---------------------------------------------------------------------------
// rotate_page: Real /Rotate and i64 arithmetic
// ---------------------------------------------------------------------------

#[test]
fn smoke_rotate_real_rotate_value() {
    // Craft a page dict with /Rotate stored as Object::Real (some PDF generators do this).
    use lopdf::{Document as LDoc, Object, Stream, dictionary};
    let mut ldoc = LDoc::with_version("1.4");
    let pages_id = ldoc.new_object_id();
    let sid = ldoc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        b"q Q\n".to_vec(),
    )));
    let page_id = ldoc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(595), Object::Integer(842),
        ]),
        "Rotate" => Object::Real(270.0_f32), // Real instead of Integer
        "Contents" => Object::Reference(sid),
        "Resources" => Object::Dictionary(dictionary! {}),
    }));
    ldoc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
        }),
    );
    let cat = ldoc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    ldoc.trailer.set("Root", Object::Reference(cat));
    let mut buf = Vec::new();
    ldoc.save_to(&mut buf).unwrap();

    let mut doc = Document::from_bytes(&buf).unwrap();
    // Adding 90° to /Rotate 270.0 should produce 0, not 90 (the bug before the fix).
    doc.rotate_page(1, 90).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = reloaded.get_pages();
    let page = reloaded.get_object(pages[&1]).unwrap().as_dict().unwrap();
    let rotate = page.get(b"Rotate").unwrap().as_i64().unwrap();
    assert_eq!(rotate, 0, "270 + 90 should wrap to 0, got {rotate}");
}

#[test]
fn smoke_rotate_zero_is_noop() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    doc.rotate_page(1, 0).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = reloaded.get_pages();
    let page = reloaded.get_object(pages[&1]).unwrap().as_dict().unwrap();
    // /Rotate 0 may or may not be written; either way the value is 0.
    let rotate = page
        .get(b"Rotate")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0);
    assert_eq!(rotate, 0, "zero rotation should leave page at 0°");
}

// ---------------------------------------------------------------------------
// insert_blank_page: invalid size
// ---------------------------------------------------------------------------

#[test]
fn smoke_insert_blank_nan_size() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let err = doc.insert_blank_page(0, (f32::NAN, 842.0)).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "NaN size should return InvalidInput"
    );
}

#[test]
fn smoke_insert_blank_zero_size() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let err = doc.insert_blank_page(0, (0.0, 842.0)).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "zero width should return InvalidInput"
    );
}

#[test]
fn smoke_insert_blank_negative_size() {
    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let err = doc.insert_blank_page(0, (595.0, -100.0)).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "negative height should return InvalidInput"
    );
}

// ---------------------------------------------------------------------------
// remove_page: last page of multi-page doc
// ---------------------------------------------------------------------------

#[test]
fn smoke_remove_last_page() {
    let mut doc = Document::from_bytes(&three_page_pdf()).unwrap();
    doc.remove_page(3).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    assert_eq!(reloaded.get_pages().len(), 2);
    assert_eq!(page_height(&out, 1), 100.0, "page 1 unchanged");
    assert_eq!(page_height(&out, 2), 200.0, "page 2 unchanged");
}

// ---------------------------------------------------------------------------
// insert_blank_page then add text on the new page
// ---------------------------------------------------------------------------

#[test]
fn smoke_insert_blank_then_add_text() {
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("NotoSansJP-Regular.ttf not found");

    let mut doc = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    doc.insert_blank_page(0, (595.0, 842.0)).unwrap(); // page 1 is now blank, old page is 2

    let font = doc.embed_font(&font_bytes).unwrap();
    // Add text to the newly inserted blank page (page 1)
    doc.page(1)
        .unwrap()
        .add_invisible_text("新しいページ", font, [72.0, 700.0], 12.0)
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    assert_eq!(reloaded.get_pages().len(), 2, "should have 2 pages");
}

// ---------------------------------------------------------------------------
// merge_from
// ---------------------------------------------------------------------------

#[test]
fn smoke_merge_appends_pages() {
    // 1-page base + 3-page other = 4 pages
    let mut base = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let other = Document::from_bytes(&three_page_pdf()).unwrap();
    base.merge_from(other).unwrap();

    let out = base.save_to_bytes().unwrap();
    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    assert_eq!(reloaded.get_pages().len(), 4, "1 + 3 = 4 pages");
}

#[test]
fn smoke_merge_preserves_content() {
    // 1-page base (height 842) + 3-page other (heights 100, 200, 300)
    // After merge: page 1=842, page 2=100, page 3=200, page 4=300
    let mut base = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let other = Document::from_bytes(&three_page_pdf()).unwrap();
    base.merge_from(other).unwrap();

    let out = base.save_to_bytes().unwrap();
    assert_eq!(page_height(&out, 1), 842.0, "base page preserved");
    assert_eq!(page_height(&out, 2), 100.0, "other page 1 appended");
    assert_eq!(page_height(&out, 3), 200.0, "other page 2 appended");
    assert_eq!(page_height(&out, 4), 300.0, "other page 3 appended");
}

#[test]
fn smoke_merge_then_add_text() {
    // After merge, add text to a page from the merged document.
    let mut base = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let other = Document::from_bytes(&three_page_pdf()).unwrap();
    base.merge_from(other).unwrap();

    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("NotoSansJP-Regular.ttf not found");
    let font = base.embed_font(&font_bytes).unwrap();
    // Add to page 2 (originally other's page 1) and page 1 (base's original page)
    base.page(1)
        .unwrap()
        .add_invisible_text("Hello", font, [72.0, 700.0], 12.0)
        .unwrap();
    base.page(2)
        .unwrap()
        .add_invisible_text("World", font, [72.0, 600.0], 12.0)
        .unwrap();

    let out = base.save_to_bytes().unwrap();
    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    assert_eq!(
        reloaded.get_pages().len(),
        4,
        "page count unchanged after add_text"
    );
}

#[test]
fn smoke_merge_rejects_pending() {
    // other has pending ops → InvalidInput
    let mut base = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let mut other = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();

    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("NotoSansJP-Regular.ttf not found");
    let font = other.embed_font(&font_bytes).unwrap();
    other
        .page(1)
        .unwrap()
        .add_invisible_text("pending", font, [72.0, 700.0], 12.0)
        .unwrap();

    let err = base.merge_from(other).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "expected InvalidInput, got {err:?}"
    );
}

#[test]
fn smoke_merge_rejects_finalized() {
    // self is finalized → InvalidInput.
    // finalized is only set to true when there were pending ops; add one.
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("NotoSansJP-Regular.ttf not found");
    let mut base = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let font = base.embed_font(&font_bytes).unwrap();
    base.page(1)
        .unwrap()
        .add_invisible_text("x", font, [72.0, 700.0], 12.0)
        .unwrap();
    let _ = base.save_to_bytes().unwrap(); // finalize (pending ops → finalized=true)

    let other = Document::from_bytes(&helpers::minimal_pdf_bytes()).unwrap();
    let err = base.merge_from(other).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "expected InvalidInput, got {err:?}"
    );
}

#[test]
fn smoke_merge_two_then_two() {
    // Merge a 2-page doc into a 2-page doc, result is 4 pages.
    let a = three_page_pdf(); // reuse three_page_pdf and take first 2 via remove
    let mut base = Document::from_bytes(&a).unwrap();
    base.remove_page(3).unwrap(); // now 2 pages
    let mut other_doc = Document::from_bytes(&a).unwrap();
    other_doc.remove_page(3).unwrap();
    let other_bytes = other_doc.save_to_bytes().unwrap();

    let other = Document::from_bytes(&other_bytes).unwrap();
    base.merge_from(other).unwrap();
    let out = base.save_to_bytes().unwrap();
    assert_eq!(
        lopdf::Document::load_from(out.as_slice())
            .unwrap()
            .get_pages()
            .len(),
        4
    );
}

// ---------------------------------------------------------------------------
// extract_pages
// ---------------------------------------------------------------------------

#[test]
fn smoke_extract_subset() {
    // Extract only page 2 from a 3-page doc → 1-page result with height=200
    let doc = Document::from_bytes(&three_page_pdf()).unwrap();
    let mut extracted = doc.extract_pages(&[2]).unwrap();
    assert_eq!(
        extracted.page_count(),
        1,
        "extracted doc should have 1 page"
    );
    let out = extracted.save_to_bytes().unwrap();
    assert_eq!(
        page_height(&out, 1),
        200.0,
        "extracted page should have height=200 (old page 2)"
    );
}

#[test]
fn smoke_extract_range() {
    // Extract pages [1, 2] from a 3-page doc → 2 pages with heights 100, 200
    let doc = Document::from_bytes(&three_page_pdf()).unwrap();
    let mut extracted = doc.extract_pages(&[1, 2]).unwrap();
    assert_eq!(extracted.page_count(), 2);
    let out = extracted.save_to_bytes().unwrap();
    assert_eq!(
        page_height(&out, 1),
        100.0,
        "page 1 should be old page 1 (h=100)"
    );
    assert_eq!(
        page_height(&out, 2),
        200.0,
        "page 2 should be old page 2 (h=200)"
    );
}

#[test]
fn smoke_extract_order_preserved() {
    // Extract [3, 1] → new page 1 = old page 3 (h=300), new page 2 = old page 1 (h=100)
    let doc = Document::from_bytes(&three_page_pdf()).unwrap();
    let mut extracted = doc.extract_pages(&[3, 1]).unwrap();
    assert_eq!(extracted.page_count(), 2);
    let out = extracted.save_to_bytes().unwrap();
    assert_eq!(
        page_height(&out, 1),
        300.0,
        "new page 1 should be old page 3 (h=300)"
    );
    assert_eq!(
        page_height(&out, 2),
        100.0,
        "new page 2 should be old page 1 (h=100)"
    );
}

#[test]
fn smoke_extract_self_unchanged() {
    // extract_pages takes &self — original must still have 3 pages afterwards
    let doc = Document::from_bytes(&three_page_pdf()).unwrap();
    let _extracted = doc.extract_pages(&[2]).unwrap();
    assert_eq!(doc.page_count(), 3, "source doc page count must not change");
}

#[test]
fn smoke_extract_pending_excluded() {
    // Pending ops on self are NOT carried into the extracted document.
    // If they were, save_to_bytes() would fail (no raw_fonts to subset from).
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("NotoSansJP-Regular.ttf not found");

    let mut doc = Document::from_bytes(&three_page_pdf()).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("pending", font, [72.0, 700.0], 12.0)
        .unwrap();

    let mut extracted = doc.extract_pages(&[1]).unwrap();
    // Would error with InvalidFont if pending ops were incorrectly included.
    let out = extracted.save_to_bytes().unwrap();
    assert_eq!(Document::from_bytes(&out).unwrap().page_count(), 1);
}

#[test]
fn smoke_extract_empty_error() {
    let doc = Document::from_bytes(&three_page_pdf()).unwrap();
    let err = doc.extract_pages(&[]).map(|_| ()).unwrap_err();
    assert!(
        matches!(err, harumi::Error::InvalidInput(_)),
        "empty page_numbers should return InvalidInput, got {err:?}"
    );
}

#[test]
fn smoke_extract_duplicate_error() {
    let doc = Document::from_bytes(&three_page_pdf()).unwrap();
    let err = doc.extract_pages(&[2, 2]).map(|_| ()).unwrap_err();
    assert!(
        matches!(err, harumi::Error::InvalidInput(_)),
        "duplicate page number should return InvalidInput, got {err:?}"
    );
}

#[test]
fn smoke_extract_out_of_range() {
    let doc = Document::from_bytes(&three_page_pdf()).unwrap();
    let err = doc.extract_pages(&[99]).map(|_| ()).unwrap_err();
    assert!(
        matches!(err, harumi::Error::PageNotFound(99)),
        "out-of-range should return PageNotFound(99), got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// remove_page regression: pending ops cleared on remove
// ---------------------------------------------------------------------------

#[test]
fn regression_remove_page_pending_cleared() {
    // Reproduce the pre-fix bug: add text to a page, remove the page,
    // then save() must succeed (not error on the now-deleted page object).
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("NotoSansJP-Regular.ttf not found");

    let mut doc = Document::from_bytes(&three_page_pdf()).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    // Queue ops for page 2 then remove it before save.
    doc.page(2)
        .unwrap()
        .add_invisible_text("これは消えるページ", font, [72.0, 700.0], 12.0)
        .unwrap();
    doc.remove_page(2).unwrap();

    // Must not error — pre-fix would either fail (object gone) or silently write
    // orphaned data (if object was still there).
    let out = doc.save_to_bytes().unwrap();
    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    assert_eq!(
        reloaded.get_pages().len(),
        2,
        "page 2 removed: 3 - 1 = 2 pages"
    );
}

// ---------------------------------------------------------------------------
// Nested /Pages tree: inherited attribute preservation
//
// Some PDFs have a tree like:
//   /Pages root (no MediaBox)
//     /Pages intermediate (MediaBox [0 0 595 842])
//       /Page A (no MediaBox — relies on inheritance from intermediate)
//       /Page B (no MediaBox — relies on inheritance from intermediate)
//
// Before the fix, remove_page / insert_blank_page / reorder_pages would
// re-parent pages directly to root without copying inherited attributes,
// causing the pages to lose their effective MediaBox.
// ---------------------------------------------------------------------------

/// Build a 2-page PDF with a nested /Pages structure.
/// Root /Pages has NO MediaBox; intermediate /Pages has MediaBox [0 0 w h].
/// Both pages inherit MediaBox from the intermediate node.
fn nested_pages_pdf(width: f32, height: f32) -> Vec<u8> {
    use lopdf::{Document as LDoc, Stream, dictionary};

    let mut doc = LDoc::with_version("1.4");

    let root_pages_id = doc.new_object_id();
    let inter_pages_id = doc.new_object_id();

    let make_page = |d: &mut LDoc, parent| {
        let sid = d.add_object(Object::Stream(Stream::new(
            dictionary! {},
            b"q Q\n".to_vec(),
        )));
        d.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(parent),
            // No MediaBox — inherits from intermediate /Pages
            "Contents" => Object::Reference(sid),
            "Resources" => Object::Dictionary(dictionary! {})
        }))
    };

    let p1 = make_page(&mut doc, inter_pages_id);
    let p2 = make_page(&mut doc, inter_pages_id);

    // Intermediate /Pages: has the MediaBox that pages inherit.
    doc.set_object(
        inter_pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Parent" => Object::Reference(root_pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Real(width), Object::Real(height),
            ]),
            "Count" => Object::Integer(2),
            "Kids" => Object::Array(vec![
                Object::Reference(p1),
                Object::Reference(p2),
            ])
        }),
    );

    // Root /Pages: no MediaBox of its own.
    doc.set_object(
        root_pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Count" => Object::Integer(2),
            "Kids" => Object::Array(vec![Object::Reference(inter_pages_id)])
        }),
    );

    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(root_pages_id)
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn nested_pages_remove_page_preserves_mediabox() {
    let pdf = nested_pages_pdf(595.0, 842.0);
    let mut doc = Document::from_bytes(&pdf).unwrap();
    assert_eq!(doc.page_count(), 2);

    doc.remove_page(2).unwrap();
    assert_eq!(doc.page_count(), 1);

    let out = doc.save_to_bytes().unwrap();
    let mut reloaded = Document::from_bytes(&out).unwrap();
    let (w, h) = reloaded.page(1).unwrap().size().unwrap();
    assert!(
        (w - 595.0).abs() < 1.0 && (h - 842.0).abs() < 1.0,
        "inherited MediaBox should be 595×842 after remove_page, got {w}×{h}"
    );
}

#[test]
fn nested_pages_insert_blank_preserves_mediabox() {
    let pdf = nested_pages_pdf(595.0, 842.0);
    let mut doc = Document::from_bytes(&pdf).unwrap();

    doc.insert_blank_page(1, (612.0, 792.0)).unwrap();
    assert_eq!(doc.page_count(), 3);

    let out = doc.save_to_bytes().unwrap();
    let mut reloaded = Document::from_bytes(&out).unwrap();

    // Original page 1 (now page 1) should still have its inherited MediaBox.
    let (w1, h1) = reloaded.page(1).unwrap().size().unwrap();
    assert!(
        (w1 - 595.0).abs() < 1.0 && (h1 - 842.0).abs() < 1.0,
        "page 1 inherited MediaBox should be 595×842, got {w1}×{h1}"
    );
    // Inserted page (page 2) has its own explicit size.
    let (w2, h2) = reloaded.page(2).unwrap().size().unwrap();
    assert!(
        (w2 - 612.0).abs() < 1.0 && (h2 - 792.0).abs() < 1.0,
        "inserted page should be 612×792, got {w2}×{h2}"
    );
}

#[test]
fn nested_pages_reorder_preserves_mediabox() {
    let pdf = nested_pages_pdf(595.0, 842.0);
    let mut doc = Document::from_bytes(&pdf).unwrap();

    doc.reorder_pages(&[2, 1]).unwrap();
    assert_eq!(doc.page_count(), 2);

    let out = doc.save_to_bytes().unwrap();
    let mut reloaded = Document::from_bytes(&out).unwrap();

    // Both pages should still have their inherited MediaBox.
    for n in 1u32..=2 {
        let (w, h) = reloaded.page(n).unwrap().size().unwrap();
        assert!(
            (w - 595.0).abs() < 1.0 && (h - 842.0).abs() < 1.0,
            "page {n} inherited MediaBox should be 595×842 after reorder, got {w}×{h}"
        );
    }
}
