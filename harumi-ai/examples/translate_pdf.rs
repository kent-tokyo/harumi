//! Smoke-test example: translate a PDF using the Anthropic Claude API.
//!
//! # Usage
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-xxx \
//! TRANSLATE_FONT=path/to/NotoSansJP-Regular.ttf \
//! cargo run -p harumi-ai --example translate_pdf --features anthropic \
//!     -- input.pdf output.pdf en [ja]
//! ```
//!
//! Arguments:
//!   1. input PDF path
//!   2. output PDF path
//!   3. target language (BCP-47, e.g. "en", "zh", "ja")
//!   4. source language (optional, BCP-47; omit for auto-detect)
//!
//! Environment variables:
//!   ANTHROPIC_API_KEY  — required
//!   ANTHROPIC_MODEL    — optional, default: claude-sonnet-4-6
//!   TRANSLATE_FONT     — path to TTF font embedded in output PDF
//!                        default: NotoSansJP-Regular.ttf in current directory

use std::env;
use harumi_ai::{TranslateOptions, translate_pdf, providers::AnthropicTranslator};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "Usage: translate_pdf <input.pdf> <output.pdf> <target_lang> [source_lang]\n\
             Env: ANTHROPIC_API_KEY, ANTHROPIC_MODEL, TRANSLATE_FONT"
        );
        std::process::exit(1);
    }

    let input_path  = &args[1];
    let output_path = &args[2];
    let target_lang = &args[3];
    let source_lang = args.get(4).map(String::as_str);

    // ── API key & model ───────────────────────────────────────────────────────
    let api_key = env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY is not set");
    let model = env::var("ANTHROPIC_MODEL")
        .unwrap_or_else(|_| "claude-sonnet-4-6".to_owned());

    // ── Font ─────────────────────────────────────────────────────────────────
    let font_path = env::var("TRANSLATE_FONT")
        .unwrap_or_else(|_| "NotoSansJP-Regular.ttf".to_owned());
    let font = std::fs::read(&font_path)
        .unwrap_or_else(|e| panic!("Cannot read font '{font_path}': {e}"));

    // ── Input PDF ─────────────────────────────────────────────────────────────
    let pdf_bytes = std::fs::read(input_path)
        .unwrap_or_else(|e| panic!("Cannot read '{input_path}': {e}"));

    // ── Translate ─────────────────────────────────────────────────────────────
    let translator = AnthropicTranslator::builder()
        .api_key(api_key)
        .model(model)
        .build();

    let mut opts = TranslateOptions::new(target_lang.as_str(), translator, font);
    opts.source_lang = source_lang.map(str::to_owned);

    let src_display = source_lang.unwrap_or("auto");
    println!("[harumi-ai] Translating: {input_path}  ({src_display} → {target_lang})");

    let translated = translate_pdf(&pdf_bytes, opts).await
        .unwrap_or_else(|e| panic!("Translation failed: {e}"));

    std::fs::write(output_path, &translated)
        .unwrap_or_else(|e| panic!("Cannot write '{output_path}': {e}"));

    println!("[harumi-ai] Done: {output_path} ({} bytes)", translated.len());
    Ok(())
}
