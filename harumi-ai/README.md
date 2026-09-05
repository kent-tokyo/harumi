# harumi-ai

AI-powered PDF translation for digital and scanned PDFs, built on [harumi](https://crates.io/crates/harumi).

[![harumi-ai on crates.io](https://img.shields.io/crates/v/harumi-ai.svg)](https://crates.io/crates/harumi-ai)
[![docs.rs](https://docs.rs/harumi-ai/badge.svg)](https://docs.rs/harumi-ai)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](../LICENSE)

## What it does

- **Digital PDFs** — extract positioned text, translate with any LLM, and write back into inferred regions (overlay or in-place replacement). Existing content is retained in overlay mode, but pixel-identical layout is not guaranteed.
- **Scanned PDFs** — accept OCR JSON from ocrs-cjk, PaddleOCR, or any HierText-format tool; translate regions; mask original image text; overlay translated CJK/Unicode text without rasterizing the page.

## Quick start

```toml
[dependencies]
harumi-ai = "0.10"

# optional built-in providers
# harumi-ai = { version = "0.10", features = ["anthropic"] }
# harumi-ai = { version = "0.10", features = ["openai"] }
```

```rust
use harumi_ai::{translate_pdf, TranslateOptions, providers::EchoTranslator};

#[tokio::main]
async fn main() {
    let font = std::fs::read("NotoSansCJKjp-Regular.ttf").unwrap();
    let pdf  = std::fs::read("source.pdf").unwrap();

    let opts   = TranslateOptions::new("en", EchoTranslator, font);
    let output = translate_pdf(&pdf, opts).await.unwrap();
    std::fs::write("translated.pdf", output.pdf_bytes).unwrap();
}
```

## Scanned PDF (OCR JSON input)

```rust
use harumi_ai::{translate_pdf, TranslateOptions, InputTextSource};

let ocr_json = std::fs::read("ocr.json").unwrap(); // HierText format
let mut opts = TranslateOptions::new("en", my_llm, font);
opts.input_source = InputTextSource::OcrJson(ocr_json);
let output = translate_pdf(&scanned_pdf, opts).await.unwrap();
```

OCR JSON can come from [ocrs-cjk](https://crates.io/crates/ocrs-cjk), PaddleOCR, hOCR, or any tool producing text + bounding boxes + confidence in HierText format.

## Built-in LLM providers

| Feature flag | Provider |
|---|---|
| `anthropic` | Claude (Anthropic API) |
| `openai` | OpenAI-compatible APIs |
| *(none)* | `EchoTranslator` (testing/dev) |

Implement the `Translator` trait to use any other LLM.

## Quality gate

`TranslateOutput` includes per-page layout quality scores. `QualityResult::Pass` / `Warn` / `Fail` indicate whether the implemented geometry checks found overflow or collision issues. A pass is not a guarantee of visual identity; review complex, rotated, vertical, or image-backed pages.

## Getting CJK fonts

```bash
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKjp-Regular.ttf
```

## License

MIT OR Apache-2.0
