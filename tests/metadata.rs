//! Integration tests for Document::metadata and Document::set_metadata.

use harumi::{Document, PdfMetadata};

// ---------------------------------------------------------------------------
// Helper: build a minimal single-page PDF with an /Info dictionary.
// ---------------------------------------------------------------------------

fn pdf_with_info(title: &str, author: &str) -> Vec<u8> {
    use harumi::lopdf::{dictionary, Document as LDoc, Object, StringFormat};

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
    let cat_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(cat_id));

    let info_id = doc.add_object(Object::Dictionary(dictionary! {
        "Title" => Object::String(title.as_bytes().to_vec(), StringFormat::Literal),
        "Author" => Object::String(author.as_bytes().to_vec(), StringFormat::Literal),
    }));
    doc.trailer.set("Info", Object::Reference(info_id));

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn metadata_empty_when_no_info() {
    let doc = Document::new((595.0, 842.0)).unwrap();
    let meta = doc.metadata().unwrap();
    assert_eq!(meta, PdfMetadata::default());
    assert!(meta.title.is_none());
    assert!(meta.author.is_none());
}

#[test]
fn roundtrip_metadata() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.set_metadata(&PdfMetadata {
        title: Some("My PDF".into()),
        author: Some("Alice".into()),
        subject: Some("Testing".into()),
        keywords: Some("rust pdf".into()),
        creator: Some("harumi".into()),
    })
    .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let meta = doc2.metadata().unwrap();

    assert_eq!(meta.title.as_deref(), Some("My PDF"));
    assert_eq!(meta.author.as_deref(), Some("Alice"));
    assert_eq!(meta.subject.as_deref(), Some("Testing"));
    assert_eq!(meta.keywords.as_deref(), Some("rust pdf"));
    assert_eq!(meta.creator.as_deref(), Some("harumi"));
}

#[test]
fn partial_metadata_none_fields_absent() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.set_metadata(&PdfMetadata {
        title: Some("Only Title".into()),
        ..Default::default()
    })
    .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let meta = doc2.metadata().unwrap();

    assert_eq!(meta.title.as_deref(), Some("Only Title"));
    assert!(meta.author.is_none());
    assert!(meta.subject.is_none());
    assert!(meta.keywords.is_none());
    assert!(meta.creator.is_none());
}

#[test]
fn set_metadata_overwrites_existing() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.set_metadata(&PdfMetadata {
        title: Some("Old Title".into()),
        ..Default::default()
    })
    .unwrap();
    doc.set_metadata(&PdfMetadata {
        title: Some("New Title".into()),
        author: Some("Bob".into()),
        ..Default::default()
    })
    .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let meta = doc2.metadata().unwrap();

    assert_eq!(meta.title.as_deref(), Some("New Title"));
    assert_eq!(meta.author.as_deref(), Some("Bob"));
}

#[test]
fn metadata_reads_existing_info_dict() {
    let pdf_bytes = pdf_with_info("Existing Doc", "Carol");
    let doc = Document::from_bytes(&pdf_bytes).unwrap();
    let meta = doc.metadata().unwrap();

    assert_eq!(meta.title.as_deref(), Some("Existing Doc"));
    assert_eq!(meta.author.as_deref(), Some("Carol"));
    assert!(meta.subject.is_none());
}
