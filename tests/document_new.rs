//! Tests for Document::new — creating blank PDFs from scratch.

use harumi::{Document, Error};

// ---------------------------------------------------------------------------
// smoke tests
// ---------------------------------------------------------------------------

#[test]
fn smoke_new_a4() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    assert_eq!(doc.page_count(), 1, "should have exactly 1 page");
    let (w, h) = doc.page(1).unwrap().size().unwrap();
    assert!((w - 595.0).abs() < 0.5, "width should be ~595, got {w}");
    assert!((h - 842.0).abs() < 0.5, "height should be ~842, got {h}");
    // Round-trip: save and reload
    let out = doc.save_to_bytes().unwrap();
    let reloaded = Document::from_bytes(&out).unwrap();
    assert_eq!(reloaded.page_count(), 1);
}

#[test]
fn smoke_new_letter() {
    let mut doc = Document::new((612.0, 792.0)).unwrap();
    assert_eq!(doc.page_count(), 1);
    let (w, h) = doc.page(1).unwrap().size().unwrap();
    assert!((w - 612.0).abs() < 0.5, "width should be ~612, got {w}");
    assert!((h - 792.0).abs() < 0.5, "height should be ~792, got {h}");
}

#[test]
fn smoke_new_then_add_text() {
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("NotoSansJP-Regular.ttf not found");

    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("Hello 日本語", font, [72.0, 700.0], 12.0)
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    assert!(!out.is_empty(), "should produce non-empty PDF bytes");
    let reloaded = Document::from_bytes(&out).unwrap();
    assert_eq!(reloaded.page_count(), 1);
}

// ---------------------------------------------------------------------------
// error cases
// ---------------------------------------------------------------------------

#[test]
fn smoke_new_nan_size() {
    let err = Document::new((f32::NAN, 842.0)).map(|_| ()).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "NaN size should return InvalidInput, got {err:?}"
    );
}

#[test]
fn smoke_new_zero_size() {
    let err = Document::new((0.0, 842.0)).map(|_| ()).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "zero width should return InvalidInput, got {err:?}"
    );
}

#[test]
fn smoke_new_negative_size() {
    let err = Document::new((595.0, -100.0)).map(|_| ()).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "negative height should return InvalidInput, got {err:?}"
    );
}
