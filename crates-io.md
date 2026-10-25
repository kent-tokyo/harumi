# harumi

**Position-aware CJK PDF write-back in pure Rust.**

Extract positioned text from existing PDFs, translate or replace it, and write
it back into inferred regions. CJK font embedding, CMaps, subsetting, and
ToUnicode mapping are handled automatically.

[![Crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/kent-tokyo/harumi#license)

## Quick start

```rust
use harumi::Document;

let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJKjp-Regular.ttf"))?;
doc.page(1)?.add_invisible_text("検索可能なテキスト", font, [72.0, 700.0], 12.0)?;
doc.save("searchable.pdf")?;
# Ok::<(), harumi::Error>(())
```

For in-memory output, use `save_to_bytes()`. For positioned extraction, use
`extract_text_runs()` or `extract_text_chunks()`.

## Main APIs

| Need | API |
|---|---|
| OCR/search layer | `embed_font`, `add_invisible_text` |
| Visible text and watermarks | `add_text`, `add_text_with_rotation` |
| Text replacement | `replace_text`, `replace_text_preserve_font`, `replace_text_resubset` |
| Page operations | `merge_from`, `extract_pages`, `rotate_page`, `reorder_pages` |
| Shapes and dashed strokes | `add_rect`, `add_line_dashed`, `add_polyline_dashed`, `add_path` |
| Images | `add_image`, `add_image_with_opacity` (`image`) |
| Reports and HTML | `FlowDocument` (`flow`), `render_html_to_pdf` (`html`) |
| SVG vector conversion | `svg_to_pdf` / `svg_to_pdf_with_options` (`svg`) — paths, curves, markers, CSS colors, and `rgb()` |
| Layout diagnostics | `extract_layout_regions`, `fit_text_to_box`, `detect_collisions` |

## Feature flags

```toml
[dependencies]
harumi = "1"
# Optional: draw, image, svg, ocr, flow, html, digital-signature
```

The default build is pure Rust and WASM-compatible. `pdfium-render` is only an
optional host-side verification tool and is not a runtime dependency.

## Font support

- TrueType (`.ttf`): subsetted and embedded as CIDFontType2.
- TrueType collections (`.ttc`): select a face with `embed_font_at`.
- OpenType CFF (`.otf`): accepted and embedded without subsetting.

Use a CJK TrueType font such as Noto Sans CJK for Japanese, Chinese, or Korean.

## Boundaries

harumi is not an OCR engine, viewer, general SVG renderer, or pixel-perfect
converter. Geometry-only SVG does not require a font; SVG text does. Extraction,
overlay, and SVG conversion are best-effort outside the documented subset.
Render output when visual fidelity matters.

## Further reading

- [API guide](https://github.com/kent-tokyo/harumi/blob/master/docs/API.md)
- [Examples](https://github.com/kent-tokyo/harumi/blob/master/docs/EXAMPLES.md)
- [Feature list](https://github.com/kent-tokyo/harumi/blob/master/docs/FEATURES.md)
- [MCP server](https://github.com/kent-tokyo/harumi/tree/master/harumi-mcp)

MIT OR Apache-2.0
