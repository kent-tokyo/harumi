//! Fixed contract for the minimal pure-Rust writer surface.

#![cfg(feature = "image")]

use harumi::{Document, PdfMetadata};

const RED_PNG: &[u8] = include_bytes!("fixtures/red_1x1.png");
const FONT: &[u8] = include_bytes!("fixtures/NotoSansJP-Regular.ttf");

fn build_writer_fixture() -> Vec<u8> {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.set_metadata(&PdfMetadata {
        title: Some("harumi writer contract".into()),
        author: Some("harumi".into()),
        ..Default::default()
    })
    .unwrap();
    doc.page(1)
        .unwrap()
        .add_text(
            "日本語 writer contract",
            font,
            [72.0, 700.0],
            14.0,
            [0.0; 3],
        )
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_image(RED_PNG, [72.0, 500.0, 24.0, 24.0])
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_link_url([72.0, 700.0, 180.0, 20.0], "https://example.com/harumi")
        .unwrap();
    doc.save_to_bytes().unwrap()
}

#[test]
fn writer_roundtrip_preserves_cjk_image_link_metadata_and_bytes() {
    let bytes = build_writer_fixture();
    assert_eq!(
        bytes,
        build_writer_fixture(),
        "writer output must be deterministic"
    );

    let doc = Document::from_bytes(&bytes).unwrap();
    assert_eq!(doc.page_count(), 1);
    assert!(
        doc.extract_text(1)
            .unwrap()
            .contains("日本語 writer contract")
    );
    assert_eq!(
        doc.metadata().unwrap().title.as_deref(),
        Some("harumi writer contract")
    );
    assert_eq!(doc.extract_page_images(1).unwrap().len(), 1);

    let pdf = harumi::lopdf::Document::load_mem(&bytes).unwrap();
    let page_id = *pdf.get_pages().get(&1).unwrap();
    let page = pdf.get_object(page_id).unwrap().as_dict().unwrap();
    let annots = page.get(b"Annots").unwrap().as_array().unwrap();
    assert_eq!(annots.len(), 1);
    let annot_id = annots[0].as_reference().unwrap();
    let annot = pdf.get_object(annot_id).unwrap().as_dict().unwrap();
    assert_eq!(annot.get(b"Subtype").unwrap().as_name().unwrap(), b"Link");
    let action = annot.get(b"A").unwrap().as_dict().unwrap();
    assert_eq!(
        action.get(b"URI").unwrap().as_str().unwrap(),
        b"https://example.com/harumi"
    );
}
