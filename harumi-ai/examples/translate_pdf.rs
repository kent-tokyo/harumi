//! Smoke-test example: translate a PDF using the Anthropic Claude API.
//!
//! # Usage
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-xxx \
//! TRANSLATE_FONT=path/to/NotoSansJP-Regular.ttf \
//! cargo run -p harumi-ai --example translate_pdf --features anthropic \
//!     -- input.pdf output.pdf en [ja] [overlay]
//! ```
//!
//! Arguments:
//!   1. input PDF path
//!   2. output PDF path
//!   3. target language (BCP-47, e.g. "en", "zh", "ja")
//!   4. source language (optional, BCP-47; omit for auto-detect)
//!   5. mode (optional, "new" for a regenerated document or "overlay"; default: "overlay")
//!
//! Environment variables:
//!   ANTHROPIC_API_KEY  — required
//!   ANTHROPIC_MODEL    — optional, default: claude-sonnet-4-6
//!   TRANSLATE_FONT     — path to TTF font embedded in output PDF
//!                        default: NotoSansJP-Regular.ttf in current directory

use std::env;
use harumi_ai::{TranslationMode, translate_pdf, providers::AnthropicTranslator};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "Usage: translate_pdf <input.pdf> <output.pdf> <target_lang> [source_lang] [mode]\n\
             Mode: \"overlay\" (default) or \"new\"\n\
             Env: ANTHROPIC_API_KEY, ANTHROPIC_MODEL, TRANSLATE_FONT"
        );
        std::process::exit(1);
    }

    let input_path  = &args[1];
    let output_path = &args[2];
    let target_lang = &args[3];
    let source_lang = args.get(4).map(String::as_str);
    let mode_arg    = args.get(5).map(String::as_str).unwrap_or("overlay");

    let mode = match mode_arg {
        "new"     => TranslationMode::NewDocument,
        "inplace" => TranslationMode::InPlace,
        _         => TranslationMode::Overlay,
    };

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

    let opts = harumi_ai::TranslateOptionsBuilder::default()
        .target_lang(target_lang.as_str())
        .translator(translator)
        .font(font)
        .mode(mode)
        .on_progress(|done, total| {
            eprint!("\r  page {done}/{total}   ");
        })
        .build();
    let opts = {
        let mut o = opts;
        o.source_lang = source_lang.map(str::to_owned);
        o
    };

    let src_display = source_lang.unwrap_or("auto");
    println!("[harumi-ai] Translating: {input_path}  ({src_display} → {target_lang})  mode={mode_arg}");

    let translated = translate_pdf(&pdf_bytes, opts).await
        .unwrap_or_else(|e| panic!("Translation failed: {e}"));

    std::fs::write(output_path, &translated.pdf_bytes)
        .unwrap_or_else(|e| panic!("Cannot write '{output_path}': {e}"));

    println!("[harumi-ai] Done: {output_path} ({} bytes)", translated.pdf_bytes.len());
    Ok(())
}
