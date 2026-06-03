mod helpers;

use harumi::Document;

fn doc_with_cropbox() -> (Vec<u8>, [f32; 4]) {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let crop = [20.0, 30.0, 555.0, 782.0]; // [x, y, w, h]
    doc.page(1).unwrap().set_crop_box(crop).unwrap();
    (doc.save_to_bytes().unwrap(), crop)
}

// ---------------------------------------------------------------------------
// media_box
// ---------------------------------------------------------------------------

#[test]
fn media_box_matches_document_size() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let mb = doc.page(1).unwrap().media_box().unwrap();
    assert!((mb[0]).abs() < 0.1); // x = 0
    assert!((mb[1]).abs() < 0.1); // y = 0
    assert!((mb[2] - 595.0).abs() < 0.5); // w
    assert!((mb[3] - 842.0).abs() < 0.5); // h
}

#[test]
fn set_media_box_changes_page_size() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.page(1).unwrap().set_media_box([0.0, 0.0, 612.0, 792.0]).unwrap(); // Letter
    let bytes = doc.save_to_bytes().unwrap();

    let mut reloaded = Document::from_bytes(&bytes).unwrap();
    let (w, h) = reloaded.page(1).unwrap().size().unwrap();
    assert!((w - 612.0).abs() < 0.5);
    assert!((h - 792.0).abs() < 0.5);
}

// ---------------------------------------------------------------------------
// crop_box
// ---------------------------------------------------------------------------

#[test]
fn crop_box_none_when_not_set() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let cb = doc.page(1).unwrap().crop_box().unwrap();
    assert!(cb.is_none());
}

#[test]
fn set_crop_box_roundtrips() {
    let (bytes, expected) = doc_with_cropbox();
    let mut doc = Document::from_bytes(&bytes).unwrap();
    let cb = doc.page(1).unwrap().crop_box().unwrap().unwrap();
    assert!((cb[0] - expected[0]).abs() < 0.1);
    assert!((cb[1] - expected[1]).abs() < 0.1);
    assert!((cb[2] - expected[2]).abs() < 0.5);
    assert!((cb[3] - expected[3]).abs() < 0.5);
}

#[test]
fn set_crop_box_nan_returns_error() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    assert!(doc.page(1).unwrap().set_crop_box([f32::NAN, 0.0, 100.0, 100.0]).is_err());
}

// ---------------------------------------------------------------------------
// trim_box / bleed_box
// ---------------------------------------------------------------------------

#[test]
fn trim_box_none_when_not_set() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    assert!(doc.page(1).unwrap().trim_box().unwrap().is_none());
}

#[test]
fn set_trim_box_roundtrips() {
    let trim = [10.0, 10.0, 575.0, 822.0];
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.page(1).unwrap().set_trim_box(trim).unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let mut doc = Document::from_bytes(&bytes).unwrap();
    let tb = doc.page(1).unwrap().trim_box().unwrap().unwrap();
    assert!((tb[2] - trim[2]).abs() < 0.5);
}

#[test]
fn set_bleed_box_roundtrips() {
    let bleed = [0.0, 0.0, 601.0, 848.0];
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.page(1).unwrap().set_bleed_box(bleed).unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let mut doc = Document::from_bytes(&bytes).unwrap();
    let bb = doc.page(1).unwrap().bleed_box().unwrap().unwrap();
    assert!((bb[2] - bleed[2]).abs() < 0.5);
}
