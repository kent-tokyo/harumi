//! Integration tests for Document::extract_text_chunks and extract_as_markdown.

use harumi::{ChunkType, Document};

fn font_bytes() -> Vec<u8> {
    std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("tests/fixtures/NotoSansJP-Regular.ttf not found")
}

#[test]
fn extract_text_chunks_empty_page() {
    let doc = Document::new((595.0, 842.0)).unwrap();
    let chunks = doc.extract_text_chunks(1).unwrap();
    assert!(chunks.is_empty(), "blank page should have no chunks");
}

#[test]
fn extract_text_chunks_paragraphs_only() {
    let font_bytes = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();

    // Add three paragraphs with same font size.
    doc.page(1)
        .unwrap()
        .add_text("First paragraph", font, [72.0, 700.0], 12.0, [0.0; 3])
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_text("Second paragraph", font, [72.0, 680.0], 12.0, [0.0; 3])
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_text("Third paragraph", font, [72.0, 660.0], 12.0, [0.0; 3])
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let chunks = doc2.extract_text_chunks(1).unwrap();

    // All three should be classified as paragraphs.
    assert_eq!(chunks.len(), 1, "same-font-size text should merge into one paragraph chunk");
    assert_eq!(chunks[0].chunk_type, ChunkType::Paragraph);
    assert!(chunks[0].text.contains("First"));
    assert!(chunks[0].text.contains("Second"));
    assert!(chunks[0].text.contains("Third"));
}

#[test]
fn extract_text_chunks_heading_and_paragraph() {
    let font_bytes = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();

    // Add a large heading followed by a smaller paragraph (with larger y gap).
    // Baseline y-distance between lines should be > max(font_size_heading*0.5, font_size_para*0.5)
    // = max(14, 6) = 14.0. Use y_heading=750, y_para=680 → gap=70 > 14.
    doc.page(1)
        .unwrap()
        .add_text("Document Title", font, [72.0, 750.0], 28.0, [0.0; 3])
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_text("This is body text.", font, [72.0, 680.0], 12.0, [0.0; 3])
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let chunks = doc2.extract_text_chunks(1).unwrap();

    // Should have 2 chunks: heading + paragraph.
    assert_eq!(chunks.len(), 2, "expected 2 chunks (heading and paragraph), got: {:?}", chunks);

    // First chunk should be heading (28pt is ~2.3× baseline 12pt).
    assert!(matches!(chunks[0].chunk_type, ChunkType::Heading(_)), "first chunk should be heading");
    assert_eq!(chunks[1].chunk_type, ChunkType::Paragraph, "second chunk should be paragraph");
    assert!(chunks[0].text.contains("Title"), "heading should contain 'Title'");
    assert!(chunks[1].text.contains("body text"), "paragraph should contain 'body text'");
}

#[test]
fn extract_text_chunks_bbox_is_valid() {
    let font_bytes = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();

    doc.page(1)
        .unwrap()
        .add_text("Test", font, [100.0, 500.0], 12.0, [0.0; 3])
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let chunks = doc2.extract_text_chunks(1).unwrap();

    assert!(!chunks.is_empty());
    let chunk = &chunks[0];
    let [x, y, w, h] = chunk.bbox;
    assert!(x >= 0.0 && y >= 0.0, "bbox origin should be non-negative");
    assert!(w > 0.0 && h > 0.0, "bbox should have positive dimensions");
}

#[test]
fn extract_text_chunks_filters_invisible() {
    let font_bytes = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();

    // Add visible text + invisible OCR layer.
    doc.page(1)
        .unwrap()
        .add_text("Visible text", font, [72.0, 700.0], 12.0, [0.0; 3])
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("OCR layer", font, [72.0, 700.0], 12.0)
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let chunks = doc2.extract_text_chunks(1).unwrap();

    // Should have only the visible text chunk, not the invisible OCR layer.
    assert!(!chunks.is_empty());
    assert!(chunks[0].text.contains("Visible"));
    assert!(!chunks[0].text.contains("OCR"));
}

#[test]
fn extract_as_markdown_basic() {
    let font_bytes = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();

    // Add heading and paragraph.
    doc.page(1)
        .unwrap()
        .add_text("My Title", font, [72.0, 750.0], 24.0, [0.0; 3])
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_text("Body text here.", font, [72.0, 700.0], 12.0, [0.0; 3])
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let markdown = doc2.extract_as_markdown(1).unwrap();

    // Should contain heading marker and text.
    assert!(markdown.contains("#"), "markdown should have heading marker");
    assert!(markdown.contains("Title"), "markdown should contain heading text");
    assert!(markdown.contains("Body text"), "markdown should contain paragraph text");
    // No trailing newlines.
    assert!(!markdown.ends_with('\n'), "markdown should be trimmed");
}

#[test]
fn extract_as_markdown_empty_page() {
    let doc = Document::new((595.0, 842.0)).unwrap();
    let markdown = doc.extract_as_markdown(1).unwrap();
    assert_eq!(markdown, "", "empty page should produce empty markdown");
}
