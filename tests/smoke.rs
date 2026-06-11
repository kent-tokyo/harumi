//! Smoke tests that simulate real-world PDF structures.
//!
//! Real PDFs produced by different authoring tools use different layouts for
//! /Contents and /Resources. harumi must handle all of them.

mod helpers;

use harumi::Document;
use lopdf::{Object, Stream, dictionary};

// ---------------------------------------------------------------------------
// Helper: build a PDF with the given /Contents value
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn pdf_with_contents(contents: Object, inline_resources: bool) -> Vec<u8> {
    use lopdf::Document as LDoc;

    let mut doc = LDoc::with_version("1.4");

    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();

    let resources_obj = lopdf::Object::Dictionary(dictionary! {
        "Font" => Object::Dictionary(dictionary! {})
    });

    let mut page_dict = dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(595), Object::Integer(842),
        ]),
    };
    page_dict.set("Contents", contents);

    if inline_resources {
        page_dict.set("Resources", resources_obj);
    } else {
        let res_id = doc.add_object(resources_obj);
        page_dict.set("Resources", Object::Reference(res_id));
    }

    doc.objects.insert(page_id, Object::Dictionary(page_dict));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
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
}

fn font_bytes() -> Vec<u8> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/NotoSansJP-Regular.ttf"
    );
    std::fs::read(path).expect("NotoSansJP-Regular.ttf not found in test fixtures")
}

fn overlay_and_verify(pdf: &[u8], text: &str) {
    let font = font_bytes();
    let mut doc = Document::from_bytes(pdf).expect("load");
    let fh = doc.embed_font(&font).expect("embed_font");
    doc.page(1)
        .expect("page 1")
        .add_invisible_text(text, fh, [72.0, 600.0], 12.0)
        .expect("add_invisible_text");
    let mut out = Vec::new();
    doc.save_to_writer(&mut out).expect("save");

    // Must re-load without error.
    let reloaded = Document::from_bytes(&out).expect("reload");
    assert_eq!(reloaded.page_count(), 1);

    // Output must be larger (font was embedded).
    assert!(
        out.len() > pdf.len(),
        "output should be larger after embedding"
    );
}

// ---------------------------------------------------------------------------
// 1. /Contents is absent (blank page — created by some scanning software)
// ---------------------------------------------------------------------------
#[test]
fn smoke_contents_missing() {
    // Build a page with no /Contents key at all (blank scanned page).
    let pdf = {
        use lopdf::Document as LDoc;
        let mut doc = LDoc::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Page".to_vec()),
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(595), Object::Integer(842),
                ]),
                "Resources" => Object::Dictionary(dictionary! {
                    "Font" => Object::Dictionary(dictionary! {})
                }),
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
        let cat = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set("Root", Object::Reference(cat));
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    };
    overlay_and_verify(&pdf, "blank page overlay");
}

// ---------------------------------------------------------------------------
// 2. /Contents is a single indirect Reference (most common real-world case)
// ---------------------------------------------------------------------------
#[test]
fn smoke_contents_single_reference() {
    use lopdf::Document as LDoc;
    let mut doc = LDoc::with_version("1.4");
    let pages_id = doc.new_object_id();

    let stream_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        b"q Q\n".to_vec(),
    )));

    let page_id = doc.new_object_id();
    doc.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792), // US Letter
            ]),
            "Contents" => Object::Reference(stream_id),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {})
            }),
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
    let cat = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(cat));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();

    overlay_and_verify(&buf, "single reference contents");
}

// ---------------------------------------------------------------------------
// 3. /Contents is already an Array of References (multi-stream page)
// ---------------------------------------------------------------------------
#[test]
fn smoke_contents_array() {
    use lopdf::Document as LDoc;
    let mut doc = LDoc::with_version("1.4");
    let pages_id = doc.new_object_id();

    let s1 = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        b"q Q\n".to_vec(),
    )));
    let s2 = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        b"q Q\n".to_vec(),
    )));

    let page_id = doc.new_object_id();
    doc.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(595), Object::Integer(842),
            ]),
            "Contents" => Object::Array(vec![
                Object::Reference(s1),
                Object::Reference(s2),
            ]),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {})
            }),
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
    let cat = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(cat));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();

    overlay_and_verify(&buf, "array contents");
}

// ---------------------------------------------------------------------------
// 4. /Resources is an indirect object (common in PDFs from Adobe Acrobat)
// ---------------------------------------------------------------------------
#[test]
fn smoke_indirect_resources() {
    use lopdf::Document as LDoc;
    let mut doc = LDoc::with_version("1.4");
    let pages_id = doc.new_object_id();
    let res_id = doc.add_object(Object::Dictionary(dictionary! {
        "Font" => Object::Dictionary(dictionary! {})
    }));
    let stream_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        b"q Q\n".to_vec(),
    )));
    let page_id = doc.new_object_id();
    doc.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(595), Object::Integer(842),
            ]),
            "Contents" => Object::Reference(stream_id),
            "Resources" => Object::Reference(res_id),
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
    let cat = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(cat));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();

    overlay_and_verify(&buf, "間接リソース");
}

// ---------------------------------------------------------------------------
// 5. Multi-page document — overlay different text on each page
// ---------------------------------------------------------------------------
#[test]
fn smoke_multipage() {
    use lopdf::Document as LDoc;
    let mut doc = LDoc::with_version("1.4");
    let pages_id = doc.new_object_id();

    let make_page = |d: &mut LDoc, parent: lopdf::ObjectId, text: &[u8]| {
        let sid = d.add_object(Object::Stream(Stream::new(dictionary! {}, text.to_vec())));
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

    let p1 = make_page(&mut doc, pages_id, b"q Q\n");
    let p2 = make_page(&mut doc, pages_id, b"q Q\n");
    let p3 = make_page(&mut doc, pages_id, b"q Q\n");

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

    let font = font_bytes();
    let mut hdoc = Document::from_bytes(&buf).expect("load");
    let fh = hdoc.embed_font(&font).expect("embed");

    hdoc.page(1)
        .unwrap()
        .add_invisible_text("ページ一", fh, [72.0, 700.0], 12.0)
        .unwrap();
    hdoc.page(2)
        .unwrap()
        .add_invisible_text("ページ二", fh, [72.0, 700.0], 12.0)
        .unwrap();
    hdoc.page(3)
        .unwrap()
        .add_invisible_text("ページ三", fh, [72.0, 700.0], 12.0)
        .unwrap();

    let mut out = Vec::new();
    hdoc.save_to_writer(&mut out).expect("save");

    let reloaded = Document::from_bytes(&out).expect("reload");
    assert_eq!(
        reloaded.page_count(),
        3,
        "multi-page count should be preserved"
    );
}

// ---------------------------------------------------------------------------
// 6. Two fonts on the same page — both should appear in /Resources /Font
// ---------------------------------------------------------------------------
#[test]
fn smoke_two_fonts_same_page() {
    let font = font_bytes();
    let pdf = helpers::minimal_pdf_bytes();
    let mut doc = Document::from_bytes(&pdf).expect("load");

    // Register the same TTF twice (simulates two logical fonts with different handles).
    let fh1 = doc.embed_font(&font).expect("embed font 1");
    let fh2 = doc.embed_font(&font).expect("embed font 2");

    doc.page(1)
        .unwrap()
        .add_invisible_text("フォント一", fh1, [72.0, 700.0], 12.0)
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("フォント二", fh2, [72.0, 680.0], 12.0)
        .unwrap();

    let mut out = Vec::new();
    doc.save_to_writer(&mut out).expect("save");

    let reloaded = lopdf::Document::load_from(out.as_slice()).expect("reload");
    let pages = reloaded.get_pages();
    let page_id = pages[&1];
    let page = reloaded.get_object(page_id).unwrap().as_dict().unwrap();
    let resources = match page.get(b"Resources").unwrap() {
        Object::Reference(r) => reloaded.get_object(*r).unwrap().as_dict().unwrap(),
        Object::Dictionary(d) => d,
        other => panic!("unexpected resources: {:?}", other),
    };
    let font_dict = resources.get(b"Font").unwrap().as_dict().unwrap();
    assert_eq!(
        font_dict.len(),
        2,
        "should have two font entries: F0 and F1"
    );
}

// ---------------------------------------------------------------------------
// 7. page.size() for non-A4 formats
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 8. MediaBox inherited from Pages parent node (common in Acrobat-generated PDFs)
// ---------------------------------------------------------------------------
#[test]
fn smoke_mediabox_inherited() {
    use lopdf::Document as LDoc;
    let mut doc = LDoc::with_version("1.4");
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();

    // Page has NO MediaBox — must be inherited from the Pages node.
    doc.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "Resources" => Object::Dictionary(dictionary! {}),
        }),
    );
    // MediaBox lives on the Pages parent.
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(595), Object::Integer(842),
            ]),
        }),
    );
    let cat = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(cat));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();

    let mut hdoc = Document::from_bytes(&buf).expect("load");
    let (w, h) = hdoc
        .page(1)
        .expect("page")
        .size()
        .expect("size from parent");
    assert!((w - 595.0).abs() < 1.0, "A4 width should be 595pt, got {w}");
    assert!(
        (h - 842.0).abs() < 1.0,
        "A4 height should be 842pt, got {h}"
    );
}
#[test]
fn smoke_page_size_letter() {
    use lopdf::Document as LDoc;
    let mut doc = LDoc::with_version("1.4");
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    doc.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792), // US Letter
            ]),
            "Resources" => Object::Dictionary(dictionary! {}),
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
    let cat = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(cat));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();

    let mut hdoc = Document::from_bytes(&buf).expect("load");
    let (w, h) = hdoc.page(1).expect("page").size().expect("size");
    assert!(
        (w - 612.0).abs() < 1.0,
        "Letter width should be 612pt, got {w}"
    );
    assert!(
        (h - 792.0).abs() < 1.0,
        "Letter height should be 792pt, got {h}"
    );
}
