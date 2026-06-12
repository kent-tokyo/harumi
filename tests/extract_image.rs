#![cfg(feature = "image")]

use harumi::{Document, PageImage, PageImageFormat};

const JPEG: &[u8] = include_bytes!("fixtures/red_1x1.jpg");
const PNG: &[u8] = include_bytes!("fixtures/red_1x1.png");

fn roundtrip(image_bytes: &[u8]) -> PageImage {
    let mut doc = Document::new((10.0, 10.0)).unwrap();
    doc.page(1)
        .unwrap()
        .add_image(image_bytes, [0.0, 0.0, 10.0, 10.0])
        .unwrap();
    let pdf_bytes = doc.save_to_bytes().unwrap();

    let reloaded = Document::from_bytes(&pdf_bytes).unwrap();
    reloaded.extract_page_image(1).unwrap()
}

#[test]
fn extract_jpeg_roundtrip() {
    let img = roundtrip(JPEG);
    assert_eq!(img.format, PageImageFormat::Jpeg);
    assert!(
        img.bytes.starts_with(b"\xff\xd8\xff"),
        "should be JPEG magic bytes"
    );
    assert_eq!(img.width, 1);
    assert_eq!(img.height, 1);
}

#[test]
fn extract_png_roundtrip() {
    let img = roundtrip(PNG);
    assert_eq!(img.format, PageImageFormat::Png);
    assert!(
        img.bytes.starts_with(&[0x89, b'P', b'N', b'G']),
        "should be PNG magic bytes"
    );
    assert_eq!(img.width, 1);
    assert_eq!(img.height, 1);
}

#[test]
fn extract_multiple_xobjects_returns_largest() {
    // Add two images; the second (PNG, 1×1) and first (JPEG, 1×1) are the same size.
    // extract_page_image must not error — it returns one of them.
    let mut doc = Document::new((20.0, 10.0)).unwrap();
    let mut page = doc.page(1).unwrap();
    page.add_image(JPEG, [0.0, 0.0, 10.0, 10.0]).unwrap();
    page.add_image(PNG, [10.0, 0.0, 10.0, 10.0]).unwrap();

    let pdf_bytes = doc.save_to_bytes().unwrap();
    let reloaded = Document::from_bytes(&pdf_bytes).unwrap();
    // Must succeed — either image is acceptable.
    let img = reloaded.extract_page_image(1).unwrap();
    assert!(img.width > 0 && img.height > 0);
}

#[test]
fn extract_no_image_returns_error() {
    // A blank PDF page has no Image XObject.
    let doc = Document::new((100.0, 100.0)).unwrap();
    let err = doc.extract_page_image(1).unwrap_err();
    match err {
        harumi::Error::InvalidInput(msg) => {
            assert!(
                msg.contains("no Image XObject"),
                "unexpected message: {msg}"
            );
        }
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

#[test]
fn extract_page_not_found() {
    let doc = Document::new((100.0, 100.0)).unwrap();
    let err = doc.extract_page_image(99).unwrap_err();
    assert!(matches!(err, harumi::Error::PageNotFound(99)));
}

#[test]
fn extract_all_images_returns_both() {
    // Add two images of different formats on the same page.
    let mut doc = Document::new((20.0, 10.0)).unwrap();
    let mut page = doc.page(1).unwrap();
    page.add_image(JPEG, [0.0, 0.0, 10.0, 10.0]).unwrap();
    page.add_image(PNG, [10.0, 0.0, 10.0, 10.0]).unwrap();

    let pdf_bytes = doc.save_to_bytes().unwrap();
    let reloaded = Document::from_bytes(&pdf_bytes).unwrap();
    let images = reloaded.extract_page_images(1).unwrap();

    assert_eq!(images.len(), 2, "should return both images");
    // Check that we got both formats (order may vary).
    let has_jpeg = images.iter().any(|img| img.format == PageImageFormat::Jpeg);
    let has_png = images.iter().any(|img| img.format == PageImageFormat::Png);
    assert!(has_jpeg, "should have JPEG format");
    assert!(has_png, "should have PNG format");
}

#[test]
fn extract_all_images_no_image_returns_error() {
    // A blank PDF page has no Image XObject.
    let doc = Document::new((100.0, 100.0)).unwrap();
    let err = doc.extract_page_images(1).unwrap_err();
    match err {
        harumi::Error::InvalidInput(msg) => {
            assert!(
                msg.contains("no Image XObject"),
                "unexpected message: {msg}"
            );
        }
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

#[test]
fn extract_all_images_page_not_found() {
    let doc = Document::new((100.0, 100.0)).unwrap();
    let err = doc.extract_page_images(99).unwrap_err();
    assert!(matches!(err, harumi::Error::PageNotFound(99)));
}
