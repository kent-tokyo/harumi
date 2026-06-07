# harumi

**Pure-Rust PDF — CJK font embedding, OCR text overlay, text extraction, HTML→PDF, page merge/split.**

[![Crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![docs.rs](https://img.shields.io/badge/docs.rs-passing-brightgreen)](https://docs.rs/harumi)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/kent-tokyo/harumi#license)

---

## Why harumi?

| Need | API |
|---|---|
| **OCR invisible text layer** on scanned PDFs | `add_invisible_text` · `add_invisible_text_runs` |
| **Extract searchable text** from existing PDFs | `extract_text_runs` · `extract_text_chunks` · `extract_as_markdown` |
| **Watermark / stamp** visible text | `add_text` · `add_text_with_rotation` |
| **Merge or split** PDFs | `merge_from` · `extract_pages` |
| **Draw shapes** (rect, line, ellipse, polygon, path) | `add_rect` · `add_line` · `add_ellipse` · `add_polygon` |
| **Embed JPEG/PNG images** with transparency | `add_image` · `add_image_with_opacity` |
| **Convert HTML to PDF** | `render_html_to_pdf` (`html` feature) |
| **Use in WASM / Lambda / Edge** (no C/C++ deps) | All APIs work cross-platform |

---

## Quick Start

### Overlay invisible OCR text

```rust
use harumi::Document;

let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;

// Invisible layer for search/copy
doc.page(1)?
    .add_invisible_text("日本語テキスト", font, [72.0, 700.0], 12.0)?;

doc.save("searchable.pdf")?;
```

### Add visible watermark

```rust
// Red "CONFIDENTIAL" stamp centered on page
let (w, h) = doc.page(1)?.size()?;
doc.page(1)?.add_text(
    "CONFIDENTIAL",
    font,
    [w / 2.0 - 60.0, h / 2.0],
    24.0,
    [0.8, 0.0, 0.0], // RGB red
)?;
```

### Extract searchable text

```rust
let runs = doc.extract_text_runs(1)?;
for run in runs {
    println!("Text: {} at ({}, {})", run.text, run.x, run.y);
}
```

---

## Feature Flags

| Flag | What it enables | Dependencies |
|---|---|---|
| *(default)* | Text overlay, font embedding, text boxes, text extraction | `lopdf`, `ttf-parser`, `getrandom` |
| `draw` | Shapes: rect, line, ellipse, polygon, polyline | none |
| `image` | JPEG/PNG embed, extract from scanned PDFs | `png` crate |
| `ocr` | Tesseract coordinate conversion helpers | none |
| `flow` | FlowDocument: auto-pagination, headers/footers | none |
| `html` | HTML→PDF conversion (h1–h6, p, table, ul/ol) | none |

---

## Supported Fonts

| Format | Status |
|---|---|
| **TrueType** (`.ttf`) | ✅ Full support — pure-Rust subsetting |
| **TTC collections** (`.ttc`, multiple faces) | ✅ Full support — `embed_font_at(bytes, face_index)` |
| **OpenType CFF** (`.otf`) | ⚠️ Accepted, no subsetting (full font embedded) |

**Recommended fonts (CJK):**
- `NotoSansCJKjp-Regular.ttf` (Japanese)
- `NotoSansCJKsc-Regular.ttf` (Simplified Chinese)
- `NotoSansCJKtc-Regular.ttf` (Traditional Chinese)
- `NotoSansCJKkr-Regular.ttf` (Korean)

---

## Installation

```toml
[dependencies]
harumi = "1"
```

For image or HTML features:
```toml
harumi = { version = "1", features = ["image", "html"] }
```

---

## Why choose harumi?

✅ **Pure Rust** — zero C/C++ dependencies; works in WASM, Lambda, cross-compile  
✅ **CJK-native** — full support for Chinese, Japanese, Korean fonts  
✅ **Simple API** — complex font subsetting happens automatically at save time  
✅ **Text extraction** — decode CID fonts + standard fonts (Type1, TrueType, WinAnsi)  
✅ **Text replacement** — rewrite Tj/TJ operators with automatic re-subsetting  
✅ **Rich features** — draw shapes, embed images, page merge/split, HTML→PDF  
✅ **Well-tested** — 100+ unit + integration + E2E tests  

---

## More Info

- **Full documentation** — [docs.rs/harumi](https://docs.rs/harumi)
- **Live demo** — [Browser annotation editor](https://kent-tokyo.github.io/harumi/) (WASM)
- **Source code** — [github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi)
- **License** — MIT OR Apache-2.0

---

## Roadmap

- **v1.x** — Current stable
- **v2.0** — PDF/A compliance, true digital signature verification (RSA/ECDSA)
- **Future** — AES-256 write encryption, RTL text (Arabic/Hebrew)

See the [full README](https://github.com/kent-tokyo/harumi) on GitHub for extensive examples, API reference, and internals explanation.
