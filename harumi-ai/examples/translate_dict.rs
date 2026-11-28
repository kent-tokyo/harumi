//! Offline layout test harness: translate a PDF without any API key.
//!
//! Translations come from a replacements JSON file
//! (`[{"old_text": "...", "new_text": "..."}, ...]`). Strings not found in the
//! dictionary are replaced by a pseudo-translation that is roughly as long as an
//! English rendering of CJK text (each CJK character becomes ~2 Latin letters),
//! so layout stress is realistic even without a real translator.
//!
//! Usage:
//!   cargo run -p harumi-ai --example translate_dict -- \
//!     <input.pdf> <output.pdf> <font.ttf> [replacements.json] [mode]
//! Mode: overlay | inplace | auto | new (default: auto)

use async_trait::async_trait;
use harumi_ai::providers::{SegmentAdapter, SegmentTranslator};
use harumi_ai::{TranslateOptionsBuilder, TranslationMode, translate_pdf};
use std::collections::HashMap;
use std::sync::Mutex;

struct DictTranslator {
    dict: HashMap<String, String>,
    misses: Mutex<Vec<String>>,
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
}

fn is_punct_only(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_whitespace() || c.is_ascii_punctuation() || "：　、。・（）".contains(c))
}

fn pseudo(text: &str) -> String {
    const SYL: [&str; 8] = ["ka", "to", "re", "mi", "su", "an", "el", "or"];
    let mut out = String::new();
    let mut n = 0usize;
    for c in text.chars() {
        if is_cjk(c) {
            if c == '　' {
                out.push(' ');
                continue;
            }
            out.push_str(SYL[n % SYL.len()]);
            n += 1;
            if n.is_multiple_of(4) {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out.trim_end().to_owned()
}

#[async_trait]
impl SegmentTranslator for DictTranslator {
    async fn translate_segments(
        &self,
        texts: &[String],
        _target: &str,
        _source: Option<&str>,
    ) -> harumi_ai::Result<Vec<String>> {
        Ok(texts
            .iter()
            .map(|t| {
                let key = t.trim();
                if let Some(v) = self.dict.get(t).or_else(|| self.dict.get(key)) {
                    return v.clone();
                }
                if is_punct_only(t) {
                    t.replace('：', ":").replace('　', " ")
                } else if t.chars().any(is_cjk) {
                    self.misses.lock().unwrap().push(t.clone());
                    pseudo(t)
                } else {
                    t.clone()
                }
            })
            .collect())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "Usage: translate_dict <input.pdf> <output.pdf> <font.ttf> [replacements.json] [mode]"
        );
        std::process::exit(1);
    }
    let pdf = std::fs::read(&args[1])?;
    let font = std::fs::read(&args[3])?;
    let mut dict = HashMap::new();
    if let Some(p) = args.get(4).filter(|p| !p.is_empty() && *p != "-") {
        let v: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(p)?)?;
        for e in v {
            if let (Some(o), Some(n)) = (e["old_text"].as_str(), e["new_text"].as_str()) {
                dict.insert(o.to_owned(), n.to_owned());
                dict.insert(o.trim().to_owned(), n.trim().to_owned());
            }
        }
    }
    let mode = match args.get(5).map(String::as_str).unwrap_or("auto") {
        "overlay" => TranslationMode::Overlay,
        "inplace" => TranslationMode::InPlace,
        "new" => TranslationMode::NewDocument,
        _ => TranslationMode::Auto,
    };
    let translator = std::sync::Arc::new(DictTranslator {
        dict,
        misses: Mutex::new(Vec::new()),
    });
    let opts = TranslateOptionsBuilder::default()
        .target_lang("en")
        .translator(SegmentAdapter::new(ArcT(translator.clone())))
        .font(font)
        .mode(mode)
        .build();
    let out = translate_pdf(&pdf, opts).await?;
    std::fs::write(&args[2], &out.pdf_bytes)?;
    println!(
        "mode_used={:?} overall={:?} fallback={:?}",
        out.quality.mode_used, out.quality.overall, out.quality.fallback_reason
    );
    for p in &out.quality.pages {
        println!("page {}: {} issues", p.page_num, p.issues.len());
        for i in p.issues.iter().take(15) {
            println!("   {i:?}");
        }
    }
    let misses = translator.misses.lock().unwrap();
    eprintln!("dictionary misses: {}", misses.len());
    for m in misses.iter().take(20) {
        eprintln!("  miss: {m:?}");
    }
    Ok(())
}

struct ArcT(std::sync::Arc<DictTranslator>);
#[async_trait]
impl SegmentTranslator for ArcT {
    async fn translate_segments(
        &self,
        texts: &[String],
        target: &str,
        source: Option<&str>,
    ) -> harumi_ai::Result<Vec<String>> {
        self.0.translate_segments(texts, target, source).await
    }
}
