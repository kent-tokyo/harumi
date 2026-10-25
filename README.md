# harumi

**Position-aware CJK PDF write-back in pure Rust.**

harumi reads text and coordinates from an existing PDF, then writes translated,
replaced, or OCR-searchable text back into the document. It embeds CJK fonts,
creates ToUnicode maps, and performs layout checks without a C/C++ runtime.

It is a write-back and document-automation library, not an OCR engine or a PDF
viewer. Complex, rotated, vertical, or image-backed pages still need visual review.

[![crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![Codecov](https://codecov.io/gh/kent-tokyo/harumi/branch/main/graph/badge.svg)](https://codecov.io/gh/kent-tokyo/harumi)
[![License](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

[日本語](README_ja.md) | [한국어](README_kr.md) | [中文](README_zh.md)

## Choose harumi when

- You need to edit an existing PDF while retaining its page content.
- CJK font embedding, text replacement, or OCR search layers are required.
- You need a Pure Rust library that can target WASM, Lambda, or Tauri.
- You need position-aware extraction before translation or replacement.

Use `pdfium-render` for viewer-style rendering, `printpdf`/`genpdf` for a new
report model, `lopdf` for direct object surgery, and `unpdf`/`pdf_oxide` for
bulk extraction. These are complementary roles. See the
[PDF ecosystem contract](docs/PDF_ECOSYSTEM.md) for the comparison boundary.

## Quick start

```toml
[dependencies]
harumi = "1"
```

Use a TrueType CJK font such as Noto Sans CJK. The TrueType variant (`.ttf`) is
the supported choice for subsetting.

```rust
use harumi::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::from_file("input.pdf")?;
    let font = doc.embed_font(include_bytes!("NotoSansCJKjp-Regular.ttf"))?;

    doc.page(1)?.add_invisible_text(
        "検索可能な日本語テキスト",
        font,
        [72.0, 700.0],
        12.0,
    )?;

    doc.save("output.pdf")?;
    Ok(())
}
```

The same page API can add visible text, replace existing text, add annotations,
or manipulate pages. `save_to_bytes()` is available for in-memory workflows.

## Main capabilities

| Task | API or feature |
|---|---|
| Extract text and coordinates | `extract_text_runs`, `extract_text_chunks` |
| Replace existing text | `replace_text`, `replace_text_preserve_font`, `replace_text_resubset` |
| Add OCR/search text | `add_invisible_text`, `add_invisible_text_runs` |
| CJK fonts | TrueType/TTC embedding, subsetting, ToUnicode maps |
| Pages and structure | merge, split, rotate, reorder, bookmarks, forms, metadata |
| Shapes and images | `draw` and `image` features |
| New paginated documents | `flow` and `html` features |
| Bulk handoff | JSON/Markdown extraction and page-level warnings |
| Digital signatures | `digital-signature` feature |

See [FEATURES.md](docs/FEATURES.md) for the complete API list and
[EXAMPLES.md](docs/EXAMPLES.md) for focused code samples.

## Translation and scanned PDFs

[`harumi-ai`](harumi-ai/README.md) provides a translation layer for digital PDFs
and OCR JSON from tools such as ocrs-cjk or PaddleOCR. It can overlay translated
regions or replace text in place, then reports overflow and collision warnings.
It does not perform OCR or guarantee pixel-identical output.

```toml
[dependencies]
harumi-ai = "0.10"
```

## MCP server

[`harumi-mcp`](harumi-mcp/README.md) exposes extraction, replacement, OCR-layer,
HTML, merge, rotation, and page-info tools to MCP clients.

```bash
cargo install harumi-mcp
harumi-mcp
```

## Limits and support

- Full Unicode line breaking, RTL, vertical writing, and browser-level CSS are
  incomplete or best-effort.
- OpenType CFF subsetting is not supported; prefer a TrueType font.
- harumi does not rasterize or display PDF pages by itself.
- Always inspect complex edited output with a renderer.

Current releases: **harumi v1.23.6**, **harumi-ai v0.10.1**.
See [ROADMAP.md](ROADMAP.md), [CHANGELOG.md](CHANGELOG.md), and the
[documentation index](docs/README.md) for priorities and history.

## Contributing

Issues and pull requests are welcome at
[github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi).

## License

MIT OR Apache-2.0
