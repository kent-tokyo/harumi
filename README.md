# harumi

**Position-aware CJK PDF write-back in pure Rust.**

Current release: **1.23.3**. harumi reads existing PDFs, extracts positioned
text, and writes translated or replaced text back into inferred regions. It also
supports page operations, annotations, forms, vector drawing, images, and
paginated HTML/report output.

[![crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![codecov](https://codecov.io/gh/kent-tokyo/harumi/branch/master/graph/badge.svg)](https://codecov.io/gh/kent-tokyo/harumi)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Demo](https://img.shields.io/badge/demo-live-brightgreen)](https://kent-tokyo.github.io/harumi/)

[中文](README_zh.md) · [日本語](README_ja.md) · [한국어](README_kr.md)

> harumi is a PDF write-back library, not an OCR engine or PDF viewer.
> Overlay output is best-effort and should be rendered and reviewed for complex,
> rotated, vertical, or image-backed pages.

## Choose the right document

- Start here for installation and the product boundary.
- Use the [API guide](docs/API.md) for feature flags and signatures.
- Use the [examples](docs/EXAMPLES.md) for copyable code.
- Use the [feature list](docs/FEATURES.md) for capability discovery.
- Use the [roadmap](ROADMAP.md) for current scope, priorities, and evidence gates.
- Use the [PDF ecosystem contract](docs/PDF_ECOSYSTEM.md) for comparisons and
  evidence boundaries.

## Common workflows

### Add an invisible OCR layer

```rust
use harumi::Document;

let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJKjp-Regular.ttf"))?;
doc.page(1)?.add_invisible_text("検索可能な日本語", font, [72.0, 700.0], 12.0)?;
doc.save("searchable.pdf")?;
# Ok::<(), harumi::Error>(())
```

### Replace text in an existing PDF

```rust
let matches = doc.page(1)?.replace_text("old", "new", font)?;
println!("replaced {matches} match(es)");
```

Use `replace_text_preserve_font` when the original font contains every
replacement glyph, or `replace_text_resubset` when the original subset must be
expanded from the original font bytes.

### Draw shapes

The optional `draw` feature includes solid and dashed strokes:

```rust
doc.page(1)?.add_line_dashed(
    [72.0, 120.0], [240.0, 120.0],
    [0.0, 0.0, 0.0], 1.0, 1.0,
    &[6.0, 3.0], 0.0,
)?;
```

### Translate a digital or scanned PDF

`harumi-ai` adds LLM translation, region planning, and a page-level quality
report. Digital PDFs use extracted text; scanned PDFs accept OCR JSON from
ocrs-cjk, PaddleOCR, hOCR, ALTO, or another compatible tool.

```rust
use harumi_ai::{translate_pdf, TranslateOptions};

let options = TranslateOptions::new("en", my_translator, font_bytes);
let output = translate_pdf(&pdf_bytes, options).await?;
std::fs::write("translated.pdf", output.pdf_bytes)?;
```

## Feature summary

| Need | API or feature |
|---|---|
| CJK font embedding and OCR text | `embed_font`, `add_invisible_text` |
| Positioned extraction | `extract_text_runs`, `extract_text_chunks` |
| Text replacement | `replace_text`, `replace_text_preserve_font`, `replace_text_resubset` |
| Layout-aware translation | `extract_layout_regions`, `plan_text_for_regions`, quality reports |
| Shapes and images | `draw`, `image` |
| SVG to PDF | `svg` |
| Auto-pagination and HTML | `flow`, `html` |
| OCR coordinate helpers | `ocr` |
| Digital signatures | `digital-signature` |
| WASM and serverless builds | Core features are pure Rust; see the WASM checks in CI |

## Installation

```toml
[dependencies]
harumi = "1"

# Optional examples:
# harumi = { version = "1", features = ["draw", "image", "flow", "html"] }
```

For CJK, use a TrueType font such as Noto Sans CJK. TTC face selection is
available through `embed_font_at`; CFF/OpenType fonts are accepted as-is but are
not subsetted.

## MCP server

Use [harumi-mcp](harumi-mcp/) from Claude Code, Cursor, or another MCP client:

```bash
cargo install harumi-mcp
```

## Status and limitations

The core PDF write-back, extraction, CJK font, layout, Flow, HTML, and WASM
paths are maintained. Extraction of malformed PDFs, complex vertical writing,
deeply nested Form XObjects, and pixel-identical translation remains
best-effort. Review output with a renderer when fidelity matters.

See [ROADMAP.md](ROADMAP.md) for the planned work and completion criteria.

## Contributing and license

Issues and pull requests are welcome at
[github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi). When
reporting a rendering problem, include the PDF viewer and version.

MIT OR Apache-2.0
