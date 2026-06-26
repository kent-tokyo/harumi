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

## In-memory output

```rust
let pdf_bytes: Vec<u8> = doc.save_to_bytes()?;
```

## Multi-line text box

```rust
doc.page(1)?.add_text_box(
    "This is a long sentence that wraps inside a 200pt-wide bounding box.",
    font,
    [72.0, 400.0, 200.0, 120.0], // [x, y, width, height]
    12.0,
    [0.0, 0.0, 0.0],
    0.0,  // 0.0 = use font_size * 1.2 line height
)?;
```

## Page manipulation

```rust
for page_num in 1..=doc.page_count() {
    doc.rotate_page(page_num, 90)?;
}
doc.remove_page(1)?;
doc.insert_blank_page(0, (595.0, 842.0))?;
doc.reorder_pages(&[3, 2, 1])?;
doc.save("output.pdf")?;
```

## Merge PDFs

```rust
let mut base = Document::from_file("a.pdf")?;
let appendix = Document::from_file("b.pdf")?;
base.merge_from(appendix)?;
base.save("merged.pdf")?;
```

## Create a blank PDF

```rust
let mut doc = Document::new((595.0, 842.0))?;   // blank A4
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;
doc.page(1)?.add_text("Hello, world!", font, [72.0, 700.0], 24.0, [0.0, 0.0, 0.0])?;
doc.save("output.pdf")?;
```

## Extract pages

```rust
let doc = Document::from_file("large.pdf")?;
let mut excerpt = doc.extract_pages(&[3, 5, 7])?;
excerpt.save("excerpt.pdf")?;
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

## Replace text using the original embedded font

```rust
let mut doc = Document::from_file("contract.pdf")?;
match doc.page(1)?.replace_text_preserve_font("Draft", replacement) {
    Ok(n) if n > 0 => { /* queued */ }
    Ok(_) => { /* not found */ }
    Err(_) => {
        let font = doc.embed_font(include_bytes!("font.ttf"))?;
        doc.page(1)?.replace_text("Draft", replacement, font)?;
    }
}
doc.save("output.pdf")?;
```

## Replace text with font subset expansion

```rust
let font_bytes = include_bytes!("NotoSansJP-Regular.ttf");
let mut doc = Document::from_file("contract.pdf")?;
let n = doc.page(1)?.replace_text_resubset("Hello", "日本語", font_bytes)?;
doc.save("output.pdf")?;
```

## Read/write PDF metadata

```rust
use harumi::{Document, PdfMetadata};

let mut doc = Document::from_file("report.pdf")?;
let meta = doc.metadata()?;

doc.set_metadata(&PdfMetadata {
    title: Some("Annual Report 2026".into()),
    author: Some("Harumi Team".into()),
    ..Default::default()
})?;
doc.save("report_with_meta.pdf")?;
```

## Draw shapes (`draw` feature)

```rust
// Filled highlight rectangle
doc.page(1)?.add_rect([72.0, 690.0, 200.0, 14.0], [1.0, 1.0, 0.0], 0.4)?;

// Stroke-only border
doc.page(1)?.add_rect_stroke([72.0, 400.0, 200.0, 100.0], [0.0, 0.0, 1.0], 1.5, 1.0)?;

// Filled polygon
doc.page(1)?.add_polygon(
    &[[100.0, 500.0], [150.0, 600.0], [200.0, 500.0]],
    [1.0, 0.5, 0.0], 1.0, true, 0.0,
)?;

// Line
doc.page(1)?.add_line([72.0, 600.0], [300.0, 600.0], [0.0, 0.0, 0.0], 1.5, 1.0)?;

// Ellipse
doc.page(1)?.add_ellipse([200.0, 300.0, 150.0, 100.0], [0.0, 0.4, 1.0], 0.7, true, 0.0)?;

// Rotated watermark
let (w, h) = doc.page(1)?.size()?;
doc.page(1)?.add_text_with_rotation(
    "CONFIDENTIAL", font, [w / 2.0, h / 2.0], 48.0,
    [0.8, 0.0, 0.0], 0.3, 45.0,
)?;
```

## Embed images (`image` feature)

```rust
let jpeg = std::fs::read("stamp.jpg")?;
doc.page(1)?.add_image(&jpeg, [72.0, 500.0, 100.0, 100.0])?;
doc.page(1)?.add_image_with_opacity(&jpeg, [72.0, 400.0, 100.0, 100.0], 0.75)?;

// PNG with transparency
let sig_png = std::fs::read("signature.png")?;
doc.page(1)?.add_image(&sig_png, [72.0, 300.0, 200.0, 80.0])?;
```

## Extract an embedded image from a scanned PDF (`image` feature)

```rust
use harumi::{Document, PageImageFormat};

let doc = Document::from_file("scanned.pdf")?;
let img = doc.extract_page_image(1)?;
match img.format {
    PageImageFormat::Jpeg => std::fs::write("page1.jpg", &img.bytes)?,
    PageImageFormat::Png  => std::fs::write("page1.png", &img.bytes)?,
}
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
