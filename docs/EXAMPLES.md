# harumi — Code Examples

For the core API reference, see [API.md](API.md).
For the full feature list, see [FEATURES.md](FEATURES.md).

---

## Invisible OCR text layer

```rust
use harumi::{Document, TextRun};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::from_file("scanned.pdf")?;
    let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;

    doc.page(1)?.add_invisible_text(
        "ここにOCRで読み取った日本語テキスト",
        font,
        [100.0, 250.0], // x, y in PDF points (origin: bottom-left)
        12.0,
    )?;

    doc.save("searchable_japanese.pdf")?;
    Ok(())
}
```

## Visible text overlay

```rust
let (w, h) = doc.page(1)?.size()?;
doc.page(1)?.add_text(
    "CONFIDENTIAL",
    font,
    [w / 2.0 - 60.0, h / 2.0],
    24.0,
    [0.8, 0.0, 0.0], // red (RGB 0.0–1.0)
)?;
```

## Extract text runs from an existing PDF

```rust
let doc = Document::from_file("existing.pdf")?;
let runs = doc.extract_text_runs(1)?;
for frag in &runs {
    println!("{:?} at ({:.1}, {:.1})", frag.text, frag.x, frag.y);
}
```

## Replace text in an existing PDF

```rust
let mut doc = Document::from_file("contract.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansJP-Regular.ttf"))?;
let n = doc.page(1)?.replace_text("Hello", "こんにちは", font)?;
doc.save("translated.pdf")?;
```

## FlowDocument with auto-pagination (`flow` feature)

```rust
use harumi::{FlowDocument, FlowOptions};

let font = include_bytes!("NotoSansCJK-Regular.ttf");
let mut doc = FlowDocument::new(font.as_ref(), FlowOptions::default())?;
doc.push_heading("Annual Report", 1)?;
doc.push_paragraph("This document summarizes our performance.")?;
doc.push_key_value_table(&[
    ("Revenue", "$1,000,000"),
    ("Expenses", "$800,000"),
])?;
let pdf_bytes = doc.render()?;
```

For arbitrary-column reports, use `push_table`. Column widths may be
intrinsic, fixed points, or fractional weights; long cells wrap and continue
onto later pages.

```rust,no_run
use harumi::{FlowDocument, FlowOptions, TableColumnWidths, TableOptions};

let mut doc = FlowDocument::new(font.as_ref(), FlowOptions::default())?;
let rows = vec![
    vec!["Region".into(), "Revenue".into(), "Notes".into()],
    vec!["Tokyo".into(), "¥12,345,678".into(), "Long CJK/Latin cell".into()],
];
doc.push_table(
    &rows,
    TableOptions {
        column_widths: TableColumnWidths::Fractions(vec![1.0, 1.0, 2.0]),
        ..Default::default()
    },
)?;
```

Horizontal cell spans, repeated headers, per-cell padding/alignment, border
styles, vertical spans, and one-level nested blocks are supported. Nested blocks
are available through `push_table_blocks`; automatic footnote references and
endnotes remain outside the current Flow contract.

Use `doc.measure_table_widths(&rows, &options)` to inspect the resolved column
widths before rendering.

## Inline text styling in FlowDocument (`flow` feature)

```rust
use harumi::{FlowDocument, FlowOptions, InlineSpan};

let mut doc = FlowDocument::new(font_bytes, FlowOptions::default())?;
doc.push_paragraph_styled(&[
    InlineSpan::plain("Normal, "),
    InlineSpan::bold("bold, "),
    InlineSpan::italic("italic, "),
    InlineSpan::colored("red.", [0.8, 0.0, 0.0]),
])?;
let pdf = doc.render()?;
```

For mixed scripts, provide an optional fallback font. It is used only for
characters missing from the primary body font:

```rust
let opts = FlowOptions {
    fallback_font_bytes: Some(cjk_font_bytes.to_vec()),
    ..FlowOptions::default()
};
let mut doc = FlowDocument::new(latin_font_bytes, opts)?;
doc.push_paragraph("Latin 日本語 symbols ✓")?;
```

## Header / footer with page numbers (`flow` feature)

```rust
use harumi::{FlowDocument, FlowOptions, HeaderFooter};

let opts = FlowOptions {
    header: Some(HeaderFooter {
        left:  Some("harumi docs".into()),
        right: Some("v1".into()),
        ..Default::default()
    }),
    footer: Some(HeaderFooter::page_number()),
    auto_bookmarks: true,
    ..Default::default()
};
let mut doc = FlowDocument::new(font, opts)?;
doc.push_heading("Chapter 1", 1)?;
let pdf_bytes = doc.render()?;
```

## Link annotations

```rust
doc.page(1)?.add_link_url([72.0, 40.0, 200.0, 18.0], "https://example.com")?;
doc.page(1)?.add_link_internal([72.0, 700.0, 150.0, 18.0], 3)?;
```

## Markup annotations

```rust
doc.page(1)?.add_highlight([72.0, 690.0, 200.0, 14.0], [1.0, 1.0, 0.0])?;
doc.page(1)?.add_underline([72.0, 640.0, 200.0, 12.0], [1.0, 0.0, 0.0])?;
doc.page(1)?.add_strikeout([72.0, 590.0, 200.0, 12.0], [0.0, 0.0, 0.0])?;
doc.page(1)?.add_squiggly([72.0, 540.0, 200.0, 12.0], [0.0, 0.6, 0.2])?;
doc.page(1)?.add_sticky_note([500.0, 700.0], "Review this section")?;
```

## Password-protected PDFs

```rust
let mut doc = Document::from_file_with_password("protected.pdf", "secret")?;

let mut doc = Document::new((595.0, 842.0))?;
doc.set_encryption("userpass", "ownerpass")?;
doc.save("protected_output.pdf")?;
```

## AcroForm: read and fill form fields

```rust
let mut doc = Document::from_file("form.pdf")?;
for field in doc.form_fields()? {
    println!("{}: {:?} = {:?}", field.name, field.field_type, field.value);
}
let updated = doc.fill_form(&[
    ("FullName",   "Jane Doe"),
    ("Department", "Engineering"),
])?;
doc.save("filled_form.pdf")?;
```

## Page boxes (print workflow)

```rust
let cb = doc.page(1)?.crop_box()?;
doc.page(1)?.set_crop_box([10.0, 10.0, 575.0, 822.0])?;
doc.page(1)?.set_trim_box([0.0, 0.0, 595.0, 842.0])?;
doc.page(1)?.set_bleed_box([0.0, 0.0, 601.0, 848.0])?;
doc.save("print_ready.pdf")?;
```

## Document bookmarks (outline)

```rust
doc.add_bookmark("Chapter 1",  1, 800.0)?;
doc.add_bookmark("第2章 概要", 2, 800.0)?;
doc.save("report.pdf")?;
```

## Convert HTML to PDF (`html` feature)

```rust
use harumi::{render_html_to_pdf, HtmlRenderOptions};

let font = include_bytes!("NotoSansCJK-Regular.ttf").to_vec();
let html = r#"
    <h1>Annual Report</h1>
    <p>Introduction paragraph.</p>
    <table>
      <tr><th>Revenue</th><td>$1,000,000</td></tr>
    </table>
    <div style="page-break-after: always"></div>
    <h1>Page Two</h1>
"#;
let pdf_bytes = render_html_to_pdf(html, HtmlRenderOptions {
    font_bytes: font,
    ..HtmlRenderOptions::default()
})?;
```
