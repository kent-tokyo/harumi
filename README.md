# harumi

**Position-aware CJK PDF write-back in pure Rust.**

harumi extracts positioned text from existing PDFs, lets you translate or replace it,
and writes the result back into inferred layout regions. Overlay mode retains the
existing page content, but pixel-identical layout is not guaranteed: complex,
rotated, vertical, or image-backed pages can require review.
CID fonts, CMaps, Unicode mapping, font subsetting, text fitting, and layout
collision checks are all handled automatically.

[![harumi on crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![harumi-ai on crates.io](https://img.shields.io/crates/v/harumi-ai.svg?label=harumi-ai)](https://crates.io/crates/harumi-ai)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Demo](https://img.shields.io/badge/demo-live-brightgreen)](https://kent-tokyo.github.io/harumi/)

[中文](README_zh.md) | [日本語](README_ja.md) | [한국어](README_kr.md)

**[Try the live browser demo →](https://kent-tokyo.github.io/harumi/)** — annotation editor (text · rect · line · freehand pen) running entirely in your browser via WASM

**Use harumi for:**
- Digital PDF translation (extract text → translate with an LLM → best-effort layout-aware write-back)
- Scanned PDF translation (pass OCR JSON → translate → mask original → overlay translated text)
- OCR searchable text layers on scanned PDFs
- Japanese / Chinese / Korean text overlays and stamps
- Page manipulation, annotation, form editing, and PDF merging
- WASM, Lambda, Tauri, and MCP-based AI document workflows

The detailed, version-pinned comparison boundary is documented in
[PDF ecosystem comparison contract](docs/PDF_ECOSYSTEM.md). It separates
rendering, new-report generation, low-level object editing, bulk extraction, and
existing-PDF CJK write-back instead of treating them as one capability.

> Not an OCR engine. Not a PDF viewer.  
> harumi is the PDF write-back layer for document automation.

---

## What harumi solves

**Before (without harumi):**  
Hand-assemble CID font objects from the PDF spec. Implement CMap generation, GID mapping, and subsetting in hundreds of lines. Still fight character rendering bugs.

**After (with harumi):**

```rust
let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;
doc.page(1)?.add_invisible_text("検索対象テキスト", font, [72.0, 700.0], 12.0)?;
doc.save("searchable.pdf")?;
```

Font subsetting, CID encoding, and ToUnicode CMap generation are all automatic. The library handles it.

---

## Translate PDFs — digital and scanned

**harumi-ai** translates both digital and scanned PDFs via a single entry point:

- **Digital PDFs** — extract existing text, translate with any LLM, and write back
  with layout-aware overlay or in-place replacement.
- **Scanned PDFs** — pass OCR JSON, translate recognized regions, mask the original
  image text, and overlay translated Unicode/CJK text without rasterizing the page.

```rust
use harumi_ai::{InputTextSource, TranslateOptions, translate_pdf};

// Digital PDF (default)
let opts = TranslateOptions::new("en", my_llm, font_bytes);
let output = translate_pdf(&digital_pdf, opts).await?;

// Scanned PDF — pass OCR JSON from ocrs-cjk, PaddleOCR, or any compatible tool
let ocr_json = std::fs::read("ocr.json")?;
let mut opts = TranslateOptions::new("en", my_llm, font_bytes);
opts.input_source = InputTextSource::OcrJson(ocr_json);
let output = translate_pdf(&scanned_pdf, opts).await?;
```

OCR can come from ocrs-cjk, PaddleOCR, hOCR, ALTO, or any tool that produces
text + bounding box + confidence in HierText format.

**Getting started with scanned PDFs:**

```bash
# 1. Install the CJK OCR CLI
cargo install ocrs-cjk-cli

# 2. Run OCR — produces text + bounding boxes + confidence scores
ocrs scanned.pdf --json -o ocr.json

# 3. Translate and write back via harumi-ai
# (set InputTextSource::OcrJson in your TranslateOptions)
```

### Searchable text layer (no translation)

To add an invisible searchable text layer without translating — useful for RAG
indexing and PDF search — use `add_invisible_text` directly with the OCR output:

```bash
cargo run --example ocrs_cjk_to_searchable_pdf -- \
  examples/fixtures/scanned_sample.pdf \
  examples/fixtures/ocrs_sample.json \
  /path/to/NotoSansCJKjp-Regular.ttf \
  searchable.pdf
```

### AI-assisted OCR correction

OCR engines sometimes misread characters with similar shapes — especially in CJK scripts.
An LLM can correct these errors while preserving the original bounding boxes.

**The key constraint: AI corrects text only. Bounding boxes must not move.**

> **OCR correction ≠ translation.** Translation write-back uses region-level layout with
> `extract_layout_regions` → AI → `plan_text_for_regions` → quality gate. See `harumi-ai`.

See `examples/fixtures/ocrs_sample_raw.json`, `ocrs_sample_corrected.json`, and
`ai_correction_report.json` for a worked example of the raw → corrected → report pattern.

harumi is not an OCR engine. For the translation path, use `harumi-ai` on top of any LLM.

---

## What you get

**[Full feature list →](docs/FEATURES.md)**

| Challenge | harumi's answer |
|---|---|
| CJK font subsetting is complex | One `embed_font()` call — only used glyphs are included, GIDs correctly remapped |
| Need to preserve existing page content | Overlay mode appends new content; replacement modes rewrite only targeted content streams |
| Need to run in WASM / Lambda / cross-compile | Pure Rust — zero C/C++ dependencies |
| Need OCR text at specific coordinates | `add_invisible_text` / batch `add_invisible_text_runs` |
| Need to replace text in an existing PDF | `replace_text` / `replace_text_preserve_font` / `replace_text_resubset` |
| Need layout-aware translation with quality gate | `extract_layout_regions` → `plan_text_for_regions` → `assess_page_layout_quality` |

---

## Comparison with similar tools

| Feature | **harumi** | pdf-lib (JS) | printpdf (Rust) | lopdf (Rust) | pdfium-render (Rust) |
|---|:---:|:---:|:---:|:---:|:---:|
| Pure Rust — no C/C++ deps | Yes | N/A | Yes | Yes | No (C++ PDFium) |
| WASM / cross-platform | Yes | Yes | Yes | Yes | Partial (complex setup) |
| CJK text on existing PDF | Yes | Yes | No (new PDFs only) | No (manual) | Yes |
| Text extraction | Yes (CID + simple) | Partial (basic) | No | Partial (basic) | Yes full |
| Text replacement (with re-subsetting) | Yes | No | No | No | No |
| Page manipulation | Yes | Yes | Partial (limited) | Yes (low-level) | Yes |
| Draw shapes | Yes | Yes | Yes | No (manual) | Yes |
| Flow document / auto-pagination | Yes | No | No | No | No |
| HTML → PDF | Yes | No | No | No | No |
| Encryption (read/write) | Yes | Yes | No | Partial | Yes |
| Digital signature creation | Yes† (v1.2.2+) | No | No | No | No |

> Yes = supported  Partial = partial / limited  No = not supported  N/A = language-level feature  
> † API complete; third-party PDF validator (Adobe Reader/qpdf/veraPDF) verification pending.  
> Comparison based on pinned crate documentation and README snapshots as of 2026-09-05.

---

## Comparison with modern Rust PDF alternatives

| Feature | **harumi** | unpdf | pdf_oxide | justpdf-core |
|---|:---:|:---:|:---:|:---:|
| **Direction** | Read + Write | Read only | Full lifecycle | Full lifecycle |
| **Primary use case** | CJK text overlay on existing PDFs | PDF → Markdown/text extraction | Multi-language PDF ops | Comprehensive PDF engine |
| Pure Rust (zero C/C++ deps) | Yes | Yes | Likely | Yes |
| WASM support | Yes (verified) | Yes | Yes | Not documented |
| CJK font embedding + subsetting | Yes ⭐ | N/A | Partial | Yes |
| Text replacement (in-place) | Yes ⭐ | N/A | Unknown | Yes |
| Layout region extraction | Yes ⭐ | No | Unknown | Unknown |

**Key differences:**
- **harumi** — Specialized for *writing* CJK text onto existing PDFs; layout regions, quality gate, scanned PDF translation
- **unpdf** — Specialized for *reading* PDFs; superior CJK extraction (XY-Cut, RTL, Form XObject)
- **pdf_oxide** — General-purpose PDF lifecycle and extraction; its broader API and bindings are a different trade-off
- **justpdf-core** — Full PDF engine; region-specific CID orderings for legacy PDF compatibility

> This is a role comparison, not a benchmark. See [`docs/PDF_ECOSYSTEM.md`](docs/PDF_ECOSYSTEM.md) for pinned versions and measured fixture results.

---

## Why this gap existed

JS has [`pdf-lib`](https://pdf-lib.js.org/) — it handles font subsetting, CMap generation, and text layer composition transparently. In Rust, the existing options force you to choose between:

- **`lopdf`** — low-level binary surgery; you hand-assemble CID font objects from the PDF spec
- **`printpdf` / `genpdf`** — new-document and report generation; use them when the input is a document model, while `harumi` Flow is a separate pure-Rust option
- **`pdfium-render`** — capable rendering/editing wrapper, but its Pdfium C++ runtime and target-specific deployment are separate from harumi's default pure-Rust/WASM boundary

For existing PDFs, harumi is the write-back layer: use its extraction, CJK font
embedding, overlay, replacement, page operations, and layout diagnostics. The
new-report boundary and fixed comparison fixture are documented in
[`docs/PDF_ECOSYSTEM.md`](docs/PDF_ECOSYSTEM.md).

`harumi` fills the gap.

---

## MCP Server (harumi-mcp)

Use harumi directly from Claude Code, Cursor, or Continue via the **[harumi-mcp](harumi-mcp/)** Model Context Protocol server:

```bash
cargo build -p harumi-mcp
# MCP tools: pdf_extract_text, pdf_extract_all_pages, pdf_replace_text,
#            pdf_add_invisible_text, pdf_html_to_pdf, pdf_merge, pdf_page_info
```

Register on [smithery.ai](https://smithery.ai) or [mcp.so](https://mcp.so) for one-click installation.

---

## Quick Start

```toml
[dependencies]
harumi = "1"
```

### Getting Fonts for CJK Support

Download **NotoSansCJK** fonts from Google Fonts (free, OFL licensed):

```bash
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKjp-Regular.ttf
```

Or search "Noto Sans CJK" on [fonts.google.com](https://fonts.google.com).

### Invisible OCR text layer

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

### Visible text overlay

```rust
// Overlay a red stamp centered on the page
let (w, h) = doc.page(1)?.size()?;
doc.page(1)?.add_text(
    "CONFIDENTIAL",
    font,
    [w / 2.0 - 60.0, h / 2.0],
    24.0,
    [0.8, 0.0, 0.0], // red (RGB 0.0–1.0)
)?;
```

**[More examples →](docs/EXAMPLES.md)** — page ops, merge, HTML→PDF, annotations, forms, draw, images, FlowDocument, digital signatures

**[API reference →](docs/API.md)** — coordinate system, feature flags, supported fonts, internals

---

## Why "harumi"

晴海 — *haru* (clear sky) + *umi* (sea). Calm on the surface, a lot going on underneath.

---

## Status and roadmap

See [CHANGELOG.md](CHANGELOG.md) for the full version history.

Current release versions: **v1.22.0** (harumi) / **v0.10.1** (harumi-ai).
This release adds shared measured paragraph/table layout, HTML break semantics,
renderer comparison diagnostics, and bounded large-report verification.

| Milestone | Status |
|---|---|
| Core PDF write-back, CJK fonts, WASM | v0.1–v0.8 ✓ |
| Text extraction, replace, FlowDocument, HTML→PDF | v0.4–v0.8 ✓ |
| Layout regions, collision detection, quality gate | v1.9–v1.16 ✓ |
| harumi-ai: LLM translation, overlay, in-place, scanned PDF | v0.1–v0.10 ✓ |
| Automated publish CI | v1.21 ✓ |
| PDF ecosystem fixtures, spec corpus, and pinned adapter checks | v1.21 ✓ / see `docs/PDF_ECOSYSTEM.md` |
| Competitive new-document typesetting (paragraphs/tables) | v1.22 ✓ for the published contract; nested tables and split rowspans remain planned / see `tasks/todo.md` |
| `InputTextSource::RunOcr` (direct OCR without external CLI) | planned |
| Multi-page OcrJson translation | v0.10 ✓ |

---

## Contributing

Issues and PRs welcome at [github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi).

The most complex part of this codebase is `src/font/embed.rs` — the CID font object graph construction. When reporting rendering bugs in a specific PDF viewer, include the viewer name and version in your issue.

---

## License

MIT OR Apache-2.0
