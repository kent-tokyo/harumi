//! Integration tests for the `flow` feature.
//! Run with: cargo test --features flow

#![cfg(feature = "flow")]

use harumi::{Document, FlowDocument, FlowOptions, InlineSpan, Margins};

const NOTO: &[u8] = include_bytes!("fixtures/NotoSansJP-Regular.ttf");

#[test]
fn smoke_single_page() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_heading("Title", 1).unwrap();
    doc.push_paragraph("This is a body paragraph.").unwrap();
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"), "output must be a PDF");
    assert!(bytes.len() > 100);
}

#[test]
fn auto_pagination() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    for i in 0..80 {
        doc.push_paragraph(&format!(
            "Paragraph {} with some content to fill the page.",
            i
        ))
        .unwrap();
    }
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert!(
        reloaded.page_count() >= 2,
        "should have paginated to at least 2 pages"
    );
}

#[test]
fn heading_levels() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    for level in 1..=6 {
        doc.push_heading(&format!("Heading {}", level), level)
            .unwrap();
        doc.push_paragraph("Supporting text.").unwrap();
    }
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn key_value_table_smoke() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_key_value_table(&[("Name", "Alice"), ("Age", "30"), ("City", "Tokyo")])
        .unwrap();
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn empty_list_no_panic() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_list(&[], false).unwrap();
    doc.push_list(&[], true).unwrap();
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn ordered_and_unordered_list() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_list(&["Alpha", "Beta", "Gamma"], false).unwrap();
    doc.push_list(&["First", "Second", "Third"], true).unwrap();
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn explicit_page_break() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_paragraph("Page one.").unwrap();
    doc.push_page_break().unwrap();
    doc.push_paragraph("Page two.").unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 2);
}

#[test]
fn custom_margins() {
    let opts = FlowOptions {
        margins: Margins::uniform(36.0),
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    doc.push_heading("Narrow Margins", 1).unwrap();
    doc.push_paragraph("Content with custom 36pt margins.")
        .unwrap();
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn cjk_paragraph_e2e() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_heading("日本語の見出し", 1).unwrap();
    doc.push_paragraph(
        "これは日本語のサンプルテキストです。PDFに正しく出力されることを確認します。\
         長いテキストが複数行に折り返されることも検証します。",
    )
    .unwrap();
    doc.push_key_value_table(&[("名前", "田中健太郎"), ("住所", "東京都渋谷区")])
        .unwrap();
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"));

    if std::env::var("HARUMI_FLOW_OUT").is_ok() {
        std::fs::write("flow_out.pdf", &bytes).unwrap();
        eprintln!("Written to flow_out.pdf");
    }
}

#[test]
fn max_pages_limit_returns_error() {
    let opts = harumi::FlowOptions {
        max_pages: 2,
        ..harumi::FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    // Fill until we hit the limit.
    let result = (0..500).try_for_each(|i| doc.push_paragraph(&format!("Paragraph {}", i)));
    assert!(
        result.is_err(),
        "should return error when max_pages exceeded"
    );
}

#[test]
fn many_table_rows_paginate() {
    let rows: Vec<(String, String)> = (0..50)
        .map(|i| (format!("Key {}", i), format!("Value {}", i)))
        .collect();
    let rows_ref: Vec<(&str, &str)> = rows.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_key_value_table(&rows_ref).unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert!(
        reloaded.page_count() >= 2,
        "50 rows should span at least 2 pages"
    );
}

// ---------------------------------------------------------------------------
// InlineSpan / push_paragraph_styled tests
// ---------------------------------------------------------------------------

#[test]
fn inline_spans_plain() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_paragraph_styled(&[InlineSpan::plain("Hello "), InlineSpan::plain("world")])
        .unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 1);
    let text: String = reloaded
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect();
    assert!(
        text.contains("Hello") && text.contains("world"),
        "text: {:?}",
        text
    );
}

#[test]
fn inline_spans_bold_italic_color() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_paragraph_styled(&[
        InlineSpan::bold("Bold "),
        InlineSpan::italic("Italic "),
        InlineSpan::colored("Red", [1.0, 0.0, 0.0]),
    ])
    .unwrap();
    let bytes = doc.render().unwrap();
    // Just verify it produces a valid PDF without panic.
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 1);
}

#[test]
fn inline_spans_cjk_mixed_style() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_paragraph_styled(&[InlineSpan::plain("日本語 "), InlineSpan::bold("太字")])
        .unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 1);
    let text: String = reloaded
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect();
    assert!(text.contains("日本語"), "text: {:?}", text);
}
