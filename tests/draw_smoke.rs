//! Integration tests for the `draw` and `image` features.

mod helpers;

use harumi::Document;
use lopdf::Object;

fn minimal_pdf() -> Vec<u8> {
    helpers::minimal_pdf_bytes()
}

fn reload_page_resources(pdf: &[u8]) -> lopdf::Dictionary {
    let doc = lopdf::Document::load_from(pdf).unwrap();
    let pages = doc.get_pages();
    let page_id = pages[&1];
    let page = doc.get_object(page_id).unwrap().as_dict().unwrap();
    match page.get(b"Resources").unwrap() {
        Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
        Object::Dictionary(d) => d.clone(),
        other => panic!("unexpected Resources: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// draw feature: add_rect
// ---------------------------------------------------------------------------

#[cfg(feature = "draw")]
#[test]
fn smoke_add_rect_registers_extgstate() {
    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    doc.page(1)
        .unwrap()
        .add_rect([50.0, 100.0, 200.0, 30.0], [1.0, 1.0, 0.0], 0.5)
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let res = reload_page_resources(&out);

    // /ExtGState must exist and have at least one entry.
    let ext_g = res
        .get(b"ExtGState")
        .expect("ExtGState should be in Resources")
        .as_dict()
        .expect("ExtGState should be a dict");
    assert!(
        !ext_g.is_empty(),
        "ExtGState dict should have at least one entry"
    );

    // Verify opacity value is present in one of the GS dicts.
    let all_gs: Vec<_> = ext_g.iter().collect();
    let found_opacity = all_gs.iter().any(|(_, v)| {
        v.as_dict()
            .ok()
            .and_then(|d| d.get(b"ca").ok())
            .and_then(|ca| ca.as_float().ok())
            .map(|f| (f - 0.5).abs() < 0.01)
            .unwrap_or(false)
    });
    assert!(
        found_opacity,
        "ExtGState should contain ca=0.5 for the given opacity"
    );
}

#[cfg(feature = "draw")]
#[test]
fn smoke_add_rect_content_stream() {
    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    doc.page(1)
        .unwrap()
        .add_rect([10.0, 20.0, 100.0, 40.0], [0.0, 0.5, 1.0], 1.0)
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let lopdf_doc = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = lopdf_doc.get_pages();
    let content = lopdf_doc.get_page_content(pages[&1]).unwrap();
    let s = String::from_utf8_lossy(&content);

    assert!(
        s.contains("re\nf"),
        "content should contain filled rectangle operator"
    );
    assert!(s.contains("rg"), "content should set fill color");
}

// ---------------------------------------------------------------------------
// draw feature: add_line
// ---------------------------------------------------------------------------

#[cfg(feature = "draw")]
#[test]
fn smoke_add_line_content_stream() {
    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    doc.page(1)
        .unwrap()
        .add_line([72.0, 600.0], [200.0, 600.0], [0.0, 0.0, 0.0], 1.5, 1.0)
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let lopdf_doc = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = lopdf_doc.get_pages();
    let content = lopdf_doc.get_page_content(pages[&1]).unwrap();
    let s = String::from_utf8_lossy(&content);

    assert!(s.contains(" m\n"), "content should have moveto (m)");
    assert!(s.contains(" l\n"), "content should have lineto (l)");
    assert!(s.contains("\nS\n"), "content should have stroke (S)");
    assert!(s.contains("1.5000 w"), "content should set line width");
}

// ---------------------------------------------------------------------------
// draw feature: mixed rect + text on same page
// ---------------------------------------------------------------------------

#[cfg(feature = "draw")]
#[test]
fn smoke_rect_and_text_same_page() {
    let Ok(font_bytes) = std::fs::read("/System/Library/Fonts/Geneva.ttf") else {
        eprintln!("Geneva.ttf not found — skipping (macOS only)");
        return;
    };

    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    let mut page = doc.page(1).unwrap();

    // yellow highlight rect first, then text on top
    page.add_rect([72.0, 690.0, 200.0, 14.0], [1.0, 1.0, 0.0], 0.4)
        .unwrap();
    page.add_invisible_text("highlighted text", font, [72.0, 695.0], 12.0)
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    // Should reload without error and have both font and ExtGState resources.
    let res = reload_page_resources(&out);
    assert!(res.get(b"Font").is_ok(), "should have Font resource");
    assert!(
        res.get(b"ExtGState").is_ok(),
        "should have ExtGState resource"
    );
}

// ---------------------------------------------------------------------------
// image feature: add_image (JPEG)
// ---------------------------------------------------------------------------

#[cfg(feature = "image")]
#[test]
fn smoke_add_jpeg_image() {
    let jpeg_bytes = include_bytes!("fixtures/red_1x1.jpg");

    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    doc.page(1)
        .unwrap()
        .add_image(jpeg_bytes, [100.0, 500.0, 72.0, 72.0])
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let res = reload_page_resources(&out);

    let xobj = res
        .get(b"XObject")
        .expect("XObject should be in Resources")
        .as_dict()
        .expect("XObject should be a dict");
    assert!(
        !xobj.is_empty(),
        "XObject dict should have at least one entry"
    );

    // Verify the XObject is an Image.
    let (_, xobj_ref) = xobj.iter().next().unwrap();
    let xobj_id = xobj_ref
        .as_reference()
        .expect("XObject entry should be a reference");
    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    let xobj_stream = reloaded.get_object(xobj_id).unwrap().as_stream().unwrap();
    let subtype = xobj_stream.dict.get(b"Subtype").unwrap().as_name().unwrap();
    assert_eq!(subtype, b"Image", "XObject Subtype should be Image");

    let filter = xobj_stream.dict.get(b"Filter").unwrap().as_name().unwrap();
    assert_eq!(filter, b"DCTDecode", "JPEG should use DCTDecode filter");
}

// ---------------------------------------------------------------------------
// image feature: add_image (PNG)
// ---------------------------------------------------------------------------

#[cfg(feature = "image")]
#[test]
fn smoke_add_png_image() {
    let png_bytes = include_bytes!("fixtures/red_1x1.png");

    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    doc.page(1)
        .unwrap()
        .add_image(png_bytes, [100.0, 400.0, 50.0, 50.0])
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let res = reload_page_resources(&out);
    let xobj = res
        .get(b"XObject")
        .expect("XObject in Resources")
        .as_dict()
        .unwrap();
    assert!(!xobj.is_empty());

    let (_, xobj_ref) = xobj.iter().next().unwrap();
    let xobj_id = xobj_ref.as_reference().unwrap();
    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    let xobj_stream = reloaded.get_object(xobj_id).unwrap().as_stream().unwrap();
    let subtype = xobj_stream.dict.get(b"Subtype").unwrap().as_name().unwrap();
    assert_eq!(subtype, b"Image");
    // PNG decoded to raw RGB uses FlateDecode (or no filter for uncompressed).
    let has_filter = xobj_stream.dict.get(b"Filter").is_ok();
    // Either FlateDecode or no filter (raw) — both are valid.
    let _ = has_filter;
}

// ---------------------------------------------------------------------------
// image feature: PNG with alpha creates SMask sub-object
// ---------------------------------------------------------------------------

#[cfg(feature = "image")]
#[test]
fn smoke_png_alpha_creates_smask() {
    let png_bytes = include_bytes!("fixtures/red_semitransparent_1x1.png");

    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    doc.page(1)
        .unwrap()
        .add_image(png_bytes, [100.0, 400.0, 50.0, 50.0])
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let res = reload_page_resources(&out);
    let xobj = res
        .get(b"XObject")
        .expect("XObject in Resources")
        .as_dict()
        .unwrap();
    let (_, xobj_ref) = xobj.iter().next().unwrap();
    let xobj_id = xobj_ref.as_reference().unwrap();
    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    let xobj_stream = reloaded.get_object(xobj_id).unwrap().as_stream().unwrap();

    // Main image must have an /SMask entry pointing to a sub-object.
    let smask = xobj_stream
        .dict
        .get(b"SMask")
        .expect("transparent PNG should produce an SMask entry");
    assert!(
        smask.as_reference().is_ok(),
        "SMask must be an indirect reference"
    );

    // The SMask sub-object must be a DeviceGray image.
    let smask_id = smask.as_reference().unwrap();
    let smask_stream = reloaded.get_object(smask_id).unwrap().as_stream().unwrap();
    let cs = smask_stream
        .dict
        .get(b"ColorSpace")
        .unwrap()
        .as_name()
        .unwrap();
    assert_eq!(cs, b"DeviceGray", "SMask must be DeviceGray");
}

#[cfg(feature = "image")]
#[test]
fn smoke_png_opaque_has_no_smask() {
    let png_bytes = include_bytes!("fixtures/red_1x1.png");

    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    doc.page(1)
        .unwrap()
        .add_image(png_bytes, [100.0, 400.0, 50.0, 50.0])
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let res = reload_page_resources(&out);
    let xobj = res
        .get(b"XObject")
        .expect("XObject in Resources")
        .as_dict()
        .unwrap();
    let (_, xobj_ref) = xobj.iter().next().unwrap();
    let xobj_id = xobj_ref.as_reference().unwrap();
    let reloaded = lopdf::Document::load_from(out.as_slice()).unwrap();
    let xobj_stream = reloaded.get_object(xobj_id).unwrap().as_stream().unwrap();

    assert!(
        xobj_stream.dict.get(b"SMask").is_err(),
        "opaque PNG must not have SMask"
    );
}

// ---------------------------------------------------------------------------
// image feature: add_image_with_opacity sets ExtGState
// ---------------------------------------------------------------------------

#[cfg(feature = "image")]
#[test]
fn smoke_add_image_with_opacity() {
    let jpeg_bytes = include_bytes!("fixtures/red_1x1.jpg");

    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    doc.page(1)
        .unwrap()
        .add_image_with_opacity(jpeg_bytes, [100.0, 500.0, 72.0, 72.0], 0.75)
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let res = reload_page_resources(&out);
    let ext_g = res
        .get(b"ExtGState")
        .expect("ExtGState for opacity")
        .as_dict()
        .unwrap();
    assert!(
        !ext_g.is_empty(),
        "ExtGState should exist for image with opacity"
    );
}

// ---------------------------------------------------------------------------
// image feature: mixed rect + image on same page
// ---------------------------------------------------------------------------

#[cfg(feature = "image")]
#[test]
fn smoke_mixed_rect_and_image() {
    let jpeg_bytes = include_bytes!("fixtures/red_1x1.jpg");

    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    let mut page = doc.page(1).unwrap();
    page.add_rect([50.0, 50.0, 100.0, 100.0], [0.0, 1.0, 0.0], 0.3)
        .unwrap();
    page.add_image_with_opacity(jpeg_bytes, [200.0, 200.0, 50.0, 50.0], 0.8)
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let res = reload_page_resources(&out);
    assert!(
        res.get(b"ExtGState").is_ok(),
        "ExtGState should cover both rect and image opacity"
    );
    assert!(
        res.get(b"XObject").is_ok(),
        "XObject should be registered for image"
    );
}

// ---------------------------------------------------------------------------
// draw feature: add_rect_stroke
// ---------------------------------------------------------------------------

#[cfg(feature = "draw")]
#[test]
fn smoke_add_rect_stroke_content_stream() {
    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    doc.page(1)
        .unwrap()
        .add_rect_stroke([50.0, 100.0, 200.0, 30.0], [0.0, 0.0, 1.0], 2.0, 1.0)
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let lopdf_doc = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = lopdf_doc.get_pages();
    let content = lopdf_doc.get_page_content(pages[&1]).unwrap();
    let s = String::from_utf8_lossy(&content);

    assert!(
        s.contains("re\nS"),
        "content should contain stroked rectangle operator"
    );
    assert!(s.contains("RG"), "content should set stroke color");
    assert!(s.contains("2.0000 w"), "content should set line width");
    assert!(
        !s.contains("re\nf"),
        "content should NOT fill the rectangle"
    );
}

// ---------------------------------------------------------------------------
// draw feature: add_polygon
// ---------------------------------------------------------------------------

#[cfg(feature = "draw")]
#[test]
fn smoke_add_polygon_filled() {
    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    // Triangle: three vertices
    doc.page(1)
        .unwrap()
        .add_polygon(
            &[[100.0, 500.0], [150.0, 600.0], [200.0, 500.0]],
            [1.0, 0.5, 0.0],
            1.0,
            true,
            0.0,
        )
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let lopdf_doc = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = lopdf_doc.get_pages();
    let content = lopdf_doc.get_page_content(pages[&1]).unwrap();
    let s = String::from_utf8_lossy(&content);

    assert!(s.contains(" m\n"), "content should have moveto");
    assert!(s.contains(" l\n"), "content should have lineto");
    assert!(s.contains("h\n"), "content should close path");
    assert!(s.contains("\nf\n"), "content should fill polygon");
    assert!(s.contains("rg"), "content should set fill color");
}

// ---------------------------------------------------------------------------
// add_text_box: multi-line wrapping
// ---------------------------------------------------------------------------

#[test]
fn smoke_add_text_box_wraps() {
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("NotoSansJP-Regular.ttf not found");

    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();

    // 80pt-wide box with a long English sentence → should produce multiple text runs
    doc.page(1)
        .unwrap()
        .add_text_box(
            "This is a long sentence that should definitely wrap inside a narrow bounding box.",
            font,
            [72.0, 400.0, 80.0, 200.0],
            12.0,
            [0.0, 0.0, 0.0],
            0.0,
        )
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let lopdf_doc = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = lopdf_doc.get_pages();
    let content = lopdf_doc.get_page_content(pages[&1]).unwrap();
    let s = String::from_utf8_lossy(&content);

    let bt_count = s.matches("BT\n").count();
    assert!(
        bt_count >= 2,
        "expected multiple BT/ET blocks for wrapped text, got {}",
        bt_count
    );
}

// ---------------------------------------------------------------------------
// Phase 8: text opacity, VerticalAlign, polyline
// ---------------------------------------------------------------------------

#[cfg(feature = "draw")]
#[test]
fn smoke_text_with_opacity() {
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("NotoSansJP-Regular.ttf not found");

    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();

    doc.page(1)
        .unwrap()
        .add_text_with_opacity("DRAFT", font, [100.0, 400.0], 48.0, [0.5, 0.5, 0.5], 0.3)
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let lpdf = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = lpdf.get_pages();
    let content = lpdf.get_page_content(pages[&1]).unwrap();
    let s = String::from_utf8_lossy(&content);

    assert!(s.contains(" gs\n"), "opacity text should emit gs operator");
    assert!(s.contains("BT\n"), "should emit text block");
}

#[test]
fn smoke_text_box_center_align() {
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("NotoSansJP-Regular.ttf not found");

    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();

    doc.page(1)
        .unwrap()
        .add_text_box_aligned(
            "Line one\nLine two\nLine three",
            font,
            [72.0, 300.0, 300.0, 200.0],
            12.0,
            [0.0, 0.0, 0.0],
            0.0,
            harumi::VerticalAlign::Center,
        )
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let lpdf = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = lpdf.get_pages();
    let content = lpdf.get_page_content(pages[&1]).unwrap();
    let s = String::from_utf8_lossy(&content);

    let bt_count = s.matches("BT\n").count();
    assert!(
        bt_count >= 3,
        "Center align: expected 3 BT blocks, got {}",
        bt_count
    );
}

#[cfg(feature = "draw")]
#[test]
fn smoke_polyline_three_segments() {
    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();

    doc.page(1)
        .unwrap()
        .add_polyline(
            &[[10.0, 10.0], [100.0, 10.0], [100.0, 100.0]],
            [0.0, 0.0, 1.0],
            2.0,
            1.0,
        )
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let lpdf = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = lpdf.get_pages();
    let content = lpdf.get_page_content(pages[&1]).unwrap();
    let s = String::from_utf8_lossy(&content);

    let l_count = s.matches(" l\n").count();
    assert_eq!(l_count, 2, "3 points → 2 lineto operators");
    assert!(s.contains("\nS\n"), "should stroke without close");
    assert!(
        !s.contains("\nh\n"),
        "must NOT close path (polyline != polygon)"
    );
}

#[cfg(feature = "draw")]
#[test]
fn smoke_add_ellipse_filled() {
    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    doc.page(1)
        .unwrap()
        .add_ellipse([50.0, 100.0, 200.0, 150.0], [0.0, 0.5, 1.0], 1.0, true, 0.0)
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let lpdf = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = lpdf.get_pages();
    let content = lpdf.get_page_content(pages[&1]).unwrap();
    let s = String::from_utf8_lossy(&content);

    // 4 cubic Bézier curves (4 'c' operators)
    let c_count = s.matches(" c\n").count();
    assert_eq!(
        c_count, 4,
        "ellipse should have 4 Bézier curve operators, got {c_count}"
    );
    // should close path and fill
    assert!(s.contains("h\n"), "should close path");
    assert!(
        s.contains("\nf\n"),
        "filled ellipse should use 'f' operator"
    );
    assert!(
        !s.contains("\nS\n"),
        "filled ellipse should not use stroke 'S'"
    );
    // fill color (rg)
    assert!(s.contains(" rg\n"), "filled ellipse should set rg color");
    // moveto
    assert!(s.contains(" m\n"), "should have a moveto operator");
}

#[cfg(feature = "draw")]
#[test]
fn smoke_add_ellipse_stroked() {
    let mut doc = Document::from_bytes(&minimal_pdf()).unwrap();
    doc.page(1)
        .unwrap()
        .add_ellipse([10.0, 10.0, 100.0, 80.0], [1.0, 0.0, 0.0], 0.8, false, 1.5)
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    let lpdf = lopdf::Document::load_from(out.as_slice()).unwrap();
    let pages = lpdf.get_pages();
    let content = lpdf.get_page_content(pages[&1]).unwrap();
    let s = String::from_utf8_lossy(&content);

    assert!(
        s.contains("\nS\n"),
        "stroked ellipse should use 'S' operator"
    );
    assert!(
        !s.contains("\nf\n"),
        "stroked ellipse should not use fill 'f'"
    );
    assert!(s.contains(" RG\n"), "stroked ellipse should set RG color");
}
