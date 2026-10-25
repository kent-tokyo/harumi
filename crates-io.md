# harumi

> **HARUMI** — **H**igh-level **A**PI for **R**ust-native **U**nicode **M**anipulation and **I**njection

**Position-aware CJK PDF write-back in pure Rust.**
Extract positioned text from existing PDFs, translate or replace it, and write it
back into inferred layout regions — CID fonts, CMaps, and font subsetting handled
automatically. Existing content is retained in overlay mode; pixel-identical layout
is not guaranteed for complex, rotated, vertical, or image-backed pages.

Current release: **v1.23.8**. Use the root README for the short introduction and
the linked reference documents for API details and limitations.

### Choose harumi when

You already have a PDF and need to translate, replace, or add searchable CJK
text while preserving the existing page. harumi sits between low-level object
editing (`lopdf`) and new-document generators (`genpdf` / `printpdf`): it uses
position-aware extraction, CID font subsetting, ToUnicode CMaps, and layout
quality checks to write text back into an existing document.

For screen rendering and viewer-like editing, use `pdfium-render` as an
optional host-side renderer. For bulk extraction, compare `unpdf` or
`pdf_oxide`; for a minimal low-level writer, compare `pdf-writer`. These are
complementary roles, not a single universal ranking. See
[`docs/PDF_ECOSYSTEM.md`](docs/PDF_ECOSYSTEM.md) for the pinned comparison
contract.

[![Crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![docs.rs](https://img.shields.io/badge/docs.rs-passing-brightgreen)](https://docs.rs/harumi)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/kent-tokyo/harumi#license)

---

## Minimal example

```rust
use harumi::Document;

let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;

// Invisible layer for search/copy
doc.page(1)?
    .add_invisible_text("日本語テキスト", font, [72.0, 700.0], 12.0)?;

doc.save("searchable.pdf")?;
```

## More Info

- **Full documentation** — [docs.rs/harumi](https://docs.rs/harumi)
- **Live demo** — [Browser annotation editor](https://kent-tokyo.github.io/harumi/) (WASM)
- **Source code** — [github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi)
- **License** — MIT OR Apache-2.0

---

For examples, API details, current limitations, and the active roadmap, see the
[full README](https://github.com/kent-tokyo/harumi) and the linked documents.
