/// Tests for link annotations and bookmarks (v0.5 features).
mod helpers;

use harumi::{Document, Error};
use helpers::minimal_pdf_bytes;

// ---------------------------------------------------------------------------
// Link URL annotations
// ---------------------------------------------------------------------------

#[test]
fn add_link_url_basic() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();

    doc.page(1)
        .unwrap()
        .add_link_url([72.0, 700.0, 200.0, 20.0], "https://example.com")
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    // The page should have an /Annots entry after save.
    let page_ids = reloaded.get_pages();
    let page_id = *page_ids.get(&1).unwrap();
    let page_dict = reloaded.get_object(page_id).unwrap().as_dict().unwrap();
    assert!(
        page_dict.get(b"Annots").is_ok(),
        "/Annots must be present on page"
    );
}

#[test]
fn add_link_url_annot_subtype_is_link() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();

    doc.page(1)
        .unwrap()
        .add_link_url([0.0, 0.0, 100.0, 20.0], "https://example.org")
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    let page_ids = reloaded.get_pages();
    let page_id = *page_ids.get(&1).unwrap();
    let page_dict = reloaded.get_object(page_id).unwrap().as_dict().unwrap();
    let annots = page_dict.get(b"Annots").unwrap().as_array().unwrap();
    assert!(!annots.is_empty(), "Annots array must not be empty");

    // Dereference the first annotation and check its /Subtype.
    let annot_ref = annots[0].as_reference().unwrap();
    let annot = reloaded.get_object(annot_ref).unwrap().as_dict().unwrap();
    let subtype = annot.get(b"Subtype").unwrap().as_name().unwrap();
    assert_eq!(subtype, b"Link");
}

#[test]
fn add_link_url_empty_url_returns_error() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();
    let result = doc
        .page(1)
        .unwrap()
        .add_link_url([0.0, 0.0, 100.0, 20.0], "");
    assert!(matches!(result, Err(Error::InvalidInput(_))));
}

#[test]
fn add_link_url_nan_rect_returns_error() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();
    let result = doc
        .page(1)
        .unwrap()
        .add_link_url([f32::NAN, 0.0, 100.0, 20.0], "https://example.com");
    assert!(matches!(result, Err(Error::InvalidInput(_))));
}

#[test]
fn add_multiple_url_links_on_same_page() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();

    doc.page(1)
        .unwrap()
        .add_link_url([0.0, 700.0, 200.0, 15.0], "https://a.example.com")
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_link_url([0.0, 680.0, 200.0, 15.0], "https://b.example.com")
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    let page_ids = reloaded.get_pages();
    let page_id = *page_ids.get(&1).unwrap();
    let page_dict = reloaded.get_object(page_id).unwrap().as_dict().unwrap();
    let annots = page_dict.get(b"Annots").unwrap().as_array().unwrap();
    assert_eq!(annots.len(), 2, "Should have exactly two annotations");
}

// ---------------------------------------------------------------------------
// Internal link annotations
// ---------------------------------------------------------------------------

#[test]
fn add_link_internal_two_page_doc() {
    // Build a 2-page document.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.insert_blank_page(1, (595.0, 842.0)).unwrap();
    assert_eq!(doc.page_count(), 2);

    doc.page(1)
        .unwrap()
        .add_link_internal([72.0, 700.0, 200.0, 20.0], 2)
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    let page_ids = reloaded.get_pages();
    let page_id = *page_ids.get(&1).unwrap();
    let page_dict = reloaded.get_object(page_id).unwrap().as_dict().unwrap();
    let annots = page_dict.get(b"Annots").unwrap().as_array().unwrap();
    assert!(!annots.is_empty());

    let annot_ref = annots[0].as_reference().unwrap();
    let annot = reloaded.get_object(annot_ref).unwrap().as_dict().unwrap();
    assert_eq!(annot.get(b"Subtype").unwrap().as_name().unwrap(), b"Link");
    // Internal links use /Dest, not /A.
    assert!(
        annot.get(b"Dest").is_ok(),
        "/Dest must be present for internal links"
    );
    assert!(
        annot.get(b"A").is_err(),
        "/A must NOT be present for internal links"
    );
}

#[test]
fn add_link_internal_out_of_range_returns_error() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();
    let result = doc
        .page(1)
        .unwrap()
        .add_link_internal([0.0, 0.0, 100.0, 20.0], 99);
    assert!(matches!(result, Err(Error::PageNotFound(99))));
}

// ---------------------------------------------------------------------------
// Bookmarks / document outline
// ---------------------------------------------------------------------------

#[test]
fn add_bookmark_adds_outlines_to_catalog() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.add_bookmark("Introduction", 1, 800.0).unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();
    let root_ref = reloaded
        .trailer
        .get(b"Root")
        .unwrap()
        .as_reference()
        .unwrap();
    let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
    assert!(
        catalog.get(b"Outlines").is_ok(),
        "/Outlines must be present in /Catalog"
    );
}

#[test]
fn add_bookmark_count_and_linked_list() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.insert_blank_page(1, (595.0, 842.0)).unwrap();
    doc.insert_blank_page(2, (595.0, 842.0)).unwrap();
    assert_eq!(doc.page_count(), 3);

    doc.add_bookmark("Chapter 1", 1, 800.0).unwrap();
    doc.add_bookmark("Chapter 2", 2, 800.0).unwrap();
    doc.add_bookmark("Chapter 3", 3, 800.0).unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    let root_ref = reloaded
        .trailer
        .get(b"Root")
        .unwrap()
        .as_reference()
        .unwrap();
    let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
    let outlines_ref = catalog.get(b"Outlines").unwrap().as_reference().unwrap();
    let outlines = reloaded
        .get_object(outlines_ref)
        .unwrap()
        .as_dict()
        .unwrap();

    let count = outlines.get(b"Count").unwrap().as_i64().unwrap();
    assert_eq!(count, 3, "/Count must equal number of bookmarks");

    // First and Last must exist.
    assert!(outlines.get(b"First").is_ok());
    assert!(outlines.get(b"Last").is_ok());
}

#[test]
fn add_bookmark_first_item_has_no_prev() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.insert_blank_page(1, (595.0, 842.0)).unwrap();
    doc.add_bookmark("First", 1, 800.0).unwrap();
    doc.add_bookmark("Second", 2, 800.0).unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    let root_ref = reloaded
        .trailer
        .get(b"Root")
        .unwrap()
        .as_reference()
        .unwrap();
    let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
    let outlines_ref = catalog.get(b"Outlines").unwrap().as_reference().unwrap();
    let outlines = reloaded
        .get_object(outlines_ref)
        .unwrap()
        .as_dict()
        .unwrap();

    let first_ref = outlines.get(b"First").unwrap().as_reference().unwrap();
    let first = reloaded.get_object(first_ref).unwrap().as_dict().unwrap();
    assert!(first.get(b"Prev").is_err(), "First item must have no /Prev");
    assert!(first.get(b"Next").is_ok(), "First item must have a /Next");
}

#[test]
fn add_bookmark_out_of_range_page_returns_error() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();
    let result = doc.add_bookmark("Ghost", 99, 800.0);
    assert!(matches!(result, Err(Error::PageNotFound(99))));
}

#[test]
fn add_bookmark_after_save_returns_error() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc
        .embed_font(include_bytes!("fixtures/NotoSansJP-Regular.ttf"))
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("x", font, [10.0, 10.0], 10.0)
        .unwrap();
    doc.save_to_bytes().unwrap(); // triggers finalize()

    let result = doc.add_bookmark("Late", 1, 800.0);
    assert!(matches!(result, Err(Error::InvalidInput(_))));
}

#[test]
fn add_bookmark_cjk_title_roundtrips() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.add_bookmark("第1章　はじめに", 1, 800.0).unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    // Just verify the document saves without error and has /Outlines.
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();
    let root_ref = reloaded
        .trailer
        .get(b"Root")
        .unwrap()
        .as_reference()
        .unwrap();
    let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
    assert!(catalog.get(b"Outlines").is_ok());
}

// ---------------------------------------------------------------------------
// Bookmarks + text ops coexist
// ---------------------------------------------------------------------------

#[test]
fn bookmark_and_text_on_same_document() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc
        .embed_font(include_bytes!("fixtures/NotoSansJP-Regular.ttf"))
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_text("Hello", font, [72.0, 700.0], 12.0, [0.0, 0.0, 0.0])
        .unwrap();
    doc.add_bookmark("Start", 1, 800.0).unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    let root_ref = reloaded
        .trailer
        .get(b"Root")
        .unwrap()
        .as_reference()
        .unwrap();
    let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
    assert!(
        catalog.get(b"Outlines").is_ok(),
        "Bookmarks must survive alongside text ops"
    );
}

/// A document with only bookmarks (no text ops) must still save successfully.
#[test]
fn bookmarks_only_no_text_ops() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.add_bookmark("Single Bookmark", 1, 800.0).unwrap();
    let bytes = doc.save_to_bytes().unwrap();
    assert!(!bytes.is_empty());

    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();
    let root_ref = reloaded
        .trailer
        .get(b"Root")
        .unwrap()
        .as_reference()
        .unwrap();
    let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
    assert!(catalog.get(b"Outlines").is_ok());
}

// ---------------------------------------------------------------------------
// Regression: add_bookmark must not clobber pre-existing /Outlines
// ---------------------------------------------------------------------------

/// Save a doc with one bookmark, reload it, add a second bookmark, save again.
/// Both bookmarks must be present in the final /Outlines linked list.
#[test]
fn add_bookmark_preserves_existing_outlines_on_reload() {
    // Round 1: create a document with one bookmark.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.add_bookmark("First", 1, 800.0).unwrap();
    let bytes1 = doc.save_to_bytes().unwrap();

    // Round 2: reload and add a second bookmark.
    let mut doc2 = Document::from_bytes(&bytes1).unwrap();
    doc2.add_bookmark("Second", 1, 600.0).unwrap();
    let bytes2 = doc2.save_to_bytes().unwrap();

    // Verify: both bookmarks survive in the final PDF.
    let reloaded = harumi::lopdf::Document::load_from(bytes2.as_slice()).unwrap();
    let root_ref = reloaded
        .trailer
        .get(b"Root")
        .unwrap()
        .as_reference()
        .unwrap();
    let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
    let outlines_ref = catalog.get(b"Outlines").unwrap().as_reference().unwrap();
    let outlines = reloaded
        .get_object(outlines_ref)
        .unwrap()
        .as_dict()
        .unwrap();

    let count = outlines.get(b"Count").unwrap().as_i64().unwrap();
    assert_eq!(
        count, 2,
        "Both bookmarks must be present after reload+add cycle"
    );

    // Walk the linked list: First → Second.
    let first_ref = outlines.get(b"First").unwrap().as_reference().unwrap();
    let first_item = reloaded.get_object(first_ref).unwrap().as_dict().unwrap();
    let first_title = match first_item.get(b"Title").unwrap() {
        harumi::lopdf::Object::String(b, _) => b.clone(),
        _ => panic!("Expected String"),
    };
    // "First" is ASCII, stored as a literal byte string.
    assert_eq!(first_title, b"First", "First bookmark title mismatch");

    let second_ref = first_item.get(b"Next").unwrap().as_reference().unwrap();
    let second_item = reloaded.get_object(second_ref).unwrap().as_dict().unwrap();
    let second_title = match second_item.get(b"Title").unwrap() {
        harumi::lopdf::Object::String(b, _) => b.clone(),
        _ => panic!("Expected String"),
    };
    assert_eq!(second_title, b"Second", "Second bookmark title mismatch");

    // The last item must have no /Next.
    assert!(
        second_item.get(b"Next").is_err(),
        "Last item must not have /Next"
    );
}

// ---------------------------------------------------------------------------
// Semantic / round-trip correctness
// ---------------------------------------------------------------------------

/// The URI string passed to add_link_url must survive the save/reload cycle.
#[test]
fn add_link_url_uri_content_roundtrip() {
    let url = "https://example.com/harumi?v=0.5";
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();
    doc.page(1)
        .unwrap()
        .add_link_url([72.0, 700.0, 200.0, 20.0], url)
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    let page_ids = reloaded.get_pages();
    let page_id = *page_ids.get(&1).unwrap();
    let page_dict = reloaded.get_object(page_id).unwrap().as_dict().unwrap();
    let annots = page_dict.get(b"Annots").unwrap().as_array().unwrap();
    let annot_ref = annots[0].as_reference().unwrap();
    let annot = reloaded.get_object(annot_ref).unwrap().as_dict().unwrap();

    let action = annot.get(b"A").unwrap().as_dict().unwrap();
    let stored_url = action.get(b"URI").unwrap().as_str().unwrap();
    assert_eq!(
        stored_url,
        url.as_bytes(),
        "URI string must round-trip unchanged"
    );
}

/// add_link_internal must create a /Dest whose first element points to the
/// target page's object ID (not some other page).
#[test]
fn add_link_internal_dest_points_to_correct_page() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.insert_blank_page(1, (595.0, 842.0)).unwrap();
    doc.insert_blank_page(2, (595.0, 842.0)).unwrap();
    assert_eq!(doc.page_count(), 3);

    // Link from page 1 to page 3.
    doc.page(1)
        .unwrap()
        .add_link_internal([0.0, 0.0, 100.0, 20.0], 3)
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    let page_ids = reloaded.get_pages();
    let page1_id = *page_ids.get(&1).unwrap();
    // Resolve page 3's ObjectId from the reloaded document (object IDs survive save/reload).
    let page3_id = *page_ids.get(&3).unwrap();

    let page_dict = reloaded.get_object(page1_id).unwrap().as_dict().unwrap();
    let annots = page_dict.get(b"Annots").unwrap().as_array().unwrap();
    let annot_ref = annots[0].as_reference().unwrap();
    let annot = reloaded.get_object(annot_ref).unwrap().as_dict().unwrap();

    let dest = annot.get(b"Dest").unwrap().as_array().unwrap();
    let dest_page_id = dest[0].as_reference().unwrap();
    assert_eq!(
        dest_page_id, page3_id,
        "/Dest must reference the correct page object"
    );
}

/// add_bookmark with CJK title: round-trip the /Title and verify it decodes
/// back to the original string via the same UTF-16BE decode path that
/// lopdf_string_to_rust uses.
#[test]
fn add_bookmark_cjk_title_semantic_roundtrip() {
    let title = "第1章　はじめに";
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.add_bookmark(title, 1, 800.0).unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    let root_ref = reloaded
        .trailer
        .get(b"Root")
        .unwrap()
        .as_reference()
        .unwrap();
    let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
    let outlines_ref = catalog.get(b"Outlines").unwrap().as_reference().unwrap();
    let outlines = reloaded
        .get_object(outlines_ref)
        .unwrap()
        .as_dict()
        .unwrap();
    let first_ref = outlines.get(b"First").unwrap().as_reference().unwrap();
    let first = reloaded.get_object(first_ref).unwrap().as_dict().unwrap();

    // Decode the /Title — must be UTF-16BE with BOM.
    let title_obj = first.get(b"Title").unwrap();
    let title_bytes: &[u8] = match title_obj {
        harumi::lopdf::Object::String(bytes, _) => bytes,
        _ => panic!("Expected String object for /Title"),
    };
    assert!(
        title_bytes.starts_with(&[0xFE, 0xFF]),
        "/Title must start with UTF-16BE BOM"
    );

    let units: Vec<u16> = title_bytes[2..]
        .chunks(2)
        .map(|c| u16::from_be_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
        .collect();
    let decoded = String::from_utf16(&units).unwrap();
    assert_eq!(
        decoded, title,
        "UTF-16BE decoded title must match the original string"
    );
}

// ---------------------------------------------------------------------------
// Redact annotations
// ---------------------------------------------------------------------------

#[test]
fn redact_basic() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();

    doc.page(1)
        .unwrap()
        .redact([100.0, 500.0, 200.0, 30.0])
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    let page_ids = reloaded.get_pages();
    let page_id = *page_ids.get(&1).unwrap();
    let page_dict = reloaded.get_object(page_id).unwrap().as_dict().unwrap();
    assert!(
        page_dict.get(b"Annots").is_ok(),
        "/Annots must be present on page after redact"
    );
}

#[test]
fn redact_subtype_is_redact() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();

    doc.page(1)
        .unwrap()
        .redact([50.0, 100.0, 150.0, 50.0])
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    let page_ids = reloaded.get_pages();
    let page_id = *page_ids.get(&1).unwrap();
    let page_dict = reloaded.get_object(page_id).unwrap().as_dict().unwrap();
    let annots = page_dict.get(b"Annots").unwrap().as_array().unwrap();
    assert!(!annots.is_empty(), "Annots array must not be empty");

    let annot_ref = annots[0].as_reference().unwrap();
    let annot = reloaded.get_object(annot_ref).unwrap().as_dict().unwrap();
    let subtype = annot.get(b"Subtype").unwrap().as_name().unwrap();
    assert_eq!(subtype, b"Redact", "/Subtype must be /Redact");
}

#[test]
fn redact_has_appearance_stream() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();

    doc.page(1)
        .unwrap()
        .redact([50.0, 100.0, 150.0, 50.0])
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    let page_ids = reloaded.get_pages();
    let page_id = *page_ids.get(&1).unwrap();
    let page_dict = reloaded.get_object(page_id).unwrap().as_dict().unwrap();
    let annots = page_dict.get(b"Annots").unwrap().as_array().unwrap();
    let annot_ref = annots[0].as_reference().unwrap();
    let annot = reloaded.get_object(annot_ref).unwrap().as_dict().unwrap();

    let ap = annot.get(b"AP").unwrap().as_dict().unwrap();
    let ap_n_ref = ap.get(b"N").unwrap().as_reference().unwrap();
    let ap_stream = reloaded.get_object(ap_n_ref).unwrap();
    assert!(
        matches!(ap_stream, harumi::lopdf::Object::Stream(_)),
        "/AP /N must be a stream"
    );
}

#[test]
fn redact_zero_size_returns_error() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();

    let result = doc.page(1).unwrap().redact([50.0, 100.0, 0.0, 50.0]);
    assert!(
        result.is_err(),
        "redact with zero width should return error"
    );

    let result = doc.page(1).unwrap().redact([50.0, 100.0, 150.0, 0.0]);
    assert!(
        result.is_err(),
        "redact with zero height should return error"
    );
}

#[test]
fn redact_nan_returns_error() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();

    let result = doc.page(1).unwrap().redact([f32::NAN, 100.0, 150.0, 50.0]);
    assert!(
        result.is_err(),
        "redact with NaN coordinate should return error"
    );
}

#[test]
fn redact_infinity_returns_error() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();

    let result = doc
        .page(1)
        .unwrap()
        .redact([f32::INFINITY, 100.0, 150.0, 50.0]);
    assert!(
        result.is_err(),
        "redact with Infinity coordinate should return error"
    );
}

#[test]
fn multiple_redacts_accumulate() {
    let pdf = minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).unwrap();

    doc.page(1)
        .unwrap()
        .redact([50.0, 100.0, 150.0, 50.0])
        .unwrap();
    doc.page(1)
        .unwrap()
        .redact([100.0, 200.0, 200.0, 40.0])
        .unwrap();
    doc.page(1)
        .unwrap()
        .redact([200.0, 300.0, 100.0, 30.0])
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

    let page_ids = reloaded.get_pages();
    let page_id = *page_ids.get(&1).unwrap();
    let page_dict = reloaded.get_object(page_id).unwrap().as_dict().unwrap();
    let annots = page_dict.get(b"Annots").unwrap().as_array().unwrap();
    assert_eq!(
        annots.len(),
        3,
        "Three redact() calls should produce three annotations"
    );

    for annot_ref_obj in annots {
        let annot_ref = annot_ref_obj.as_reference().unwrap();
        let annot = reloaded.get_object(annot_ref).unwrap().as_dict().unwrap();
        let subtype = annot.get(b"Subtype").unwrap().as_name().unwrap();
        assert_eq!(subtype, b"Redact", "All annotations should be /Redact");
    }
}
