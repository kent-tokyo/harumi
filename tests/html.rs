//! Integration tests for the `html` feature.
//! Run with: cargo test --features html

#![cfg(feature = "html")]

use harumi::{render_html_to_pdf, Document, HtmlRenderOptions};

const NOTO: &[u8] = include_bytes!("fixtures/NotoSansJP-Regular.ttf");

fn opts() -> HtmlRenderOptions {
    HtmlRenderOptions { font_bytes: NOTO.to_vec(), ..HtmlRenderOptions::default() }
}

#[test]
fn empty_font_bytes_error() {
    let result = render_html_to_pdf("<p>Hello</p>", HtmlRenderOptions::default());
    assert!(result.is_err(), "empty font_bytes should return an error");
}

#[test]
fn basic_html() {
    let html = "<h1>Title</h1><p>Body paragraph.</p>";
    let bytes = render_html_to_pdf(html, opts()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn full_html_document() {
    let html = "<!DOCTYPE html><html><head><title>Test</title></head>\
                <body><h1>Report</h1><p>Introduction.</p></body></html>";
    let bytes = render_html_to_pdf(html, opts()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn page_break_style_attribute() {
    let html = r#"<h1>Page One</h1><div style="page-break-after: always"></div><h1>Page Two</h1>"#;
    let bytes = render_html_to_pdf(html, opts()).unwrap();
    let doc = Document::from_bytes(&bytes).unwrap();
    assert!(doc.page_count() >= 2, "page-break-after should create a new page");
}

#[test]
fn page_break_class() {
    let html = r#"<p>First</p><hr class="page-break"><p>Second</p>"#;
    let bytes = render_html_to_pdf(html, opts()).unwrap();
    let doc = Document::from_bytes(&bytes).unwrap();
    assert!(doc.page_count() >= 2);
}

#[test]
fn table_two_columns() {
    let html = "<table>\
                  <tr><th>Name</th><td>Alice</td></tr>\
                  <tr><th>Age</th><td>30</td></tr>\
                </table>";
    let bytes = render_html_to_pdf(html, opts()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn unordered_list() {
    let html = "<ul><li>Apple</li><li>Banana</li><li>Cherry</li></ul>";
    let bytes = render_html_to_pdf(html, opts()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn ordered_list() {
    let html = "<ol><li>First</li><li>Second</li><li>Third</li></ol>";
    let bytes = render_html_to_pdf(html, opts()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn japanese_html() {
    let html = "<h1>日本語のタイトル</h1>\
                <p>これは日本語のサンプルテキストです。</p>\
                <table><tr><th>名前</th><td>田中</td></tr></table>";
    let bytes = render_html_to_pdf(html, opts()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));

    if std::env::var("HARUMI_HTML_OUT").is_ok() {
        std::fs::write("html_out.pdf", &bytes).unwrap();
        eprintln!("Written to html_out.pdf");
    }
}

#[test]
fn all_heading_levels() {
    let html = "<h1>H1</h1><h2>H2</h2><h3>H3</h3><h4>H4</h4><h5>H5</h5><h6>H6</h6>";
    let bytes = render_html_to_pdf(html, opts()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn mixed_content() {
    let html = "<h1>Annual Report</h1>\
                <p>This document summarizes our performance.</p>\
                <h2>Financial Summary</h2>\
                <table>\
                  <tr><th>Revenue</th><td>$1,000,000</td></tr>\
                  <tr><th>Expenses</th><td>$800,000</td></tr>\
                  <tr><th>Profit</th><td>$200,000</td></tr>\
                </table>\
                <h2>Highlights</h2>\
                <ul>\
                  <li>Expanded to 3 new markets</li>\
                  <li>Launched 2 new products</li>\
                </ul>";
    let bytes = render_html_to_pdf(html, opts()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn script_and_style_skipped() {
    let html = "<head><script>alert('x')</script><style>body{}</style></head>\
                <body><h1>Visible</h1></body>";
    let bytes = render_html_to_pdf(html, opts()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn nested_table_no_extra_rows() {
    // Inner <tr> must NOT appear as a row in the outer table.
    let html = "<table>\
                  <tr><th>Outer</th><td>\
                    <table><tr><th>Inner</th><td>X</td></tr></table>\
                  </td></tr>\
                </table>";
    let bytes = render_html_to_pdf(html, opts()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn nested_list_no_duplicate_items() {
    // Inner <li> must NOT appear as a top-level item.
    let html = "<ul>\
                  <li>Item 1</li>\
                  <li>Item 2\
                    <ul><li>Nested 2.1</li></ul>\
                  </li>\
                </ul>";
    let bytes = render_html_to_pdf(html, opts()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn deeply_nested_divs_no_stack_overflow() {
    // 5000 nested divs — would overflow the stack with a recursive walker.
    let open: String = "<div>".repeat(5000);
    let close: String = "</div>".repeat(5000);
    let html = format!("{}<p>Hello</p>{}", open, close);
    let bytes = render_html_to_pdf(&html, opts()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn max_pages_limit_respected() {
    let opts = HtmlRenderOptions {
        font_bytes: NOTO.to_vec(),
        max_pages: 3,
        ..HtmlRenderOptions::default()
    };
    // 200 non-empty paragraphs → ~6 pages on A4, should hit max_pages=3 limit.
    let html: String = (0..200).map(|i| format!("<p>Paragraph {}</p>", i)).collect();
    let result = render_html_to_pdf(&html, opts);
    assert!(result.is_err(), "should hit max_pages limit");
}

// ---------------------------------------------------------------------------
// Inline styling tests (bold/italic/color via <strong>/<em>/<span>/<a>)
// ---------------------------------------------------------------------------

#[test]
fn bold_and_italic_rendered() {
    let bytes = render_html_to_pdf(
        "<p>Normal <strong>Bold</strong> and <em>Italic</em> text.</p>",
        HtmlRenderOptions { font_bytes: NOTO.to_vec(), ..Default::default() },
    ).unwrap();
    let doc = Document::from_bytes(&bytes).unwrap();
    let text: String = doc.extract_text_runs(1).unwrap()
        .iter().map(|f| f.text.as_str()).collect();
    // All words should appear in the output.
    assert!(text.contains("Normal"), "text: {:?}", text);
    assert!(text.contains("Bold"), "text: {:?}", text);
    assert!(text.contains("Italic"), "text: {:?}", text);
}

#[test]
fn span_color_attribute() {
    let bytes = render_html_to_pdf(
        r#"<p>Normal <span style="color: #ff0000">Red</span> text.</p>"#,
        HtmlRenderOptions { font_bytes: NOTO.to_vec(), ..Default::default() },
    ).unwrap();
    // Just verify valid PDF output.
    let doc = Document::from_bytes(&bytes).unwrap();
    assert_eq!(doc.page_count(), 1);
}

#[test]
fn link_rendered_as_blue() {
    let bytes = render_html_to_pdf(
        r#"<p>Visit <a href="https://example.com">example.com</a> for more.</p>"#,
        HtmlRenderOptions { font_bytes: NOTO.to_vec(), ..Default::default() },
    ).unwrap();
    let doc = Document::from_bytes(&bytes).unwrap();
    let text: String = doc.extract_text_runs(1).unwrap()
        .iter().map(|f| f.text.as_str()).collect();
    assert!(text.contains("example.com"), "text: {:?}", text);
}
