# harumi — Examples

These are small entry points. See [`API.md`](API.md) for signatures and
[`FEATURES.md`](FEATURES.md) for the complete capability list.

## Load, edit, and save

```rust
use harumi::Document;

let mut doc = Document::from_file("input.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJKjp-Regular.ttf"))?;
doc.page(1)?.add_text("CONFIDENTIAL", font, [72.0, 700.0], 12.0, [0.8, 0.0, 0.0])?;
doc.save("output.pdf")?;
# Ok::<(), harumi::Error>(())
```

For byte-oriented applications use `Document::from_bytes` and
`save_to_bytes()`.

## Invisible OCR layer

```rust
doc.page(1)?.add_invisible_text(
    "検索可能な日本語",
    font,
    [72.0, 700.0], // PDF points; origin is bottom-left
    12.0,
)?;
```

For many OCR runs, use `add_invisible_text_runs` so fonts are subsetted once.

## Extract and replace text

```rust
let runs = doc.extract_text_runs(1)?;
for run in &runs {
    println!("{} at ({}, {})", run.text, run.x, run.y);
}

let matches = doc.page(1)?.replace_text("old", "new", font)?;
println!("{matches} match(es)");
```

Use `replace_text_preserve_font` when the existing font contains every glyph.
Use `replace_text_resubset` with the original full font when a subset must be
expanded.

## Pages and documents

```rust
doc.rotate_page(1, 90)?;
doc.insert_blank_page(0, (595.0, 842.0))?;
let selected = doc.extract_pages(&[1, 3])?;

let mut merged = Document::from_file("a.pdf")?;
merged.merge_from(Document::from_file("b.pdf")?)?;
merged.save("merged.pdf")?;
```

To create a new one-page PDF, use `Document::new((width, height))?`.

## Drawing (`draw` feature)

```toml
harumi = { version = "1", features = ["draw"] }
```

```rust
doc.page(1)?.add_rect([72.0, 680.0, 200.0, 24.0], [1.0, 1.0, 0.0], 0.4)?;
doc.page(1)?.add_line_dashed(
    [72.0, 640.0], [272.0, 640.0],
    [0.0, 0.0, 0.0], 1.0, 1.0,
    &[6.0, 3.0], 0.0,
)?;
```

Dash arrays require finite positive lengths. `add_polyline_dashed` uses the
same pattern for an open multi-segment path.

## Images and annotations

Enable `image` for JPEG/PNG embedding and scanned-image extraction:

```rust
let image = std::fs::read("photo.png")?;
doc.page(1)?.add_image(&image, [72.0, 400.0, 200.0, 120.0], 1.0)?;
```

Common annotations include `add_link_url`, `add_link_internal`,
`add_highlight`, `add_underline`, `add_strikeout`, and `add_sticky_note`.

## Flow and HTML

`flow` provides auto-pagination, tables, headers, footers, fallback fonts, and
layout controls. `html` adds the supported HTML-to-PDF path:

```rust
use harumi::{FlowDocument, FlowOptions};

let mut flow = FlowDocument::new(font_bytes, FlowOptions::default())?;
flow.push_heading("Report", 1)?;
flow.push_paragraph("A paragraph that will paginate automatically.")?;
let pdf = flow.render()?;
```

For HTML, use `render_html_to_pdf(html, options)` with the `html` feature.

## SVG (`svg` feature)

`svg_to_pdf` converts the documented self-contained SVG subset to one vector
PDF page. It accepts paths, transforms, markers, CSS named colors, `rgb()`,
and non-rendering `metadata` elements.

```rust
let pdf = harumi::svg_to_pdf(svg_bytes, font_bytes)?;
std::fs::write("figure.pdf", pdf)?;
```

## Metadata, forms, and security

```rust
let metadata = doc.metadata()?;
doc.set_metadata(&metadata)?;
let fields = doc.form_fields()?;
doc.fill_form(&[("Name", "Harumi")])?;
doc.set_encryption("user-password", "owner-password")?;
```

Digital signatures require the `digital-signature` feature. See the API guide
for signing and verification contracts.

## Coordinates and review

Coordinates use PDF points (1/72 inch) with the origin at the bottom-left.
OCR tools that use top-left pixel coordinates should use the `ocr` feature
helpers. Overlay output is best-effort; render complex PDFs for visual review.
