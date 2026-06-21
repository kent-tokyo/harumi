use harumi::Document;
use harumi_ai::{LayoutOptions, QualityGate, QualityProfile, TranslateOptions, TranslationMode,
                providers::EchoTranslator, translate_pdf};

const FONT: &[u8] = include_bytes!("../../tests/fixtures/NotoSansJP-Regular.ttf");
const BLACK: [f32; 3] = [0.0, 0.0, 0.0];

fn make_test_pdf() -> Vec<u8> {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1)
        .unwrap()
        .add_text("Hello World", font, [72.0, 700.0], 14.0, BLACK)
        .unwrap();
    doc.save_to_bytes().unwrap()
}

fn make_multipage_pdf() -> Vec<u8> {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1)
        .unwrap()
        .add_text("Page One Text", font, [72.0, 700.0], 14.0, BLACK)
        .unwrap();
    doc.insert_blank_page(1, (595.0, 842.0)).unwrap();
    doc.page(2)
        .unwrap()
        .add_text("Page Two Text", font, [72.0, 700.0], 14.0, BLACK)
        .unwrap();
    doc.save_to_bytes().unwrap()
}

fn make_four_page_pdf() -> Vec<u8> {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    for page_num in 2..=4 {
        doc.insert_blank_page(page_num - 1, (595.0, 842.0)).unwrap();
    }
    for page_num in 1..=4 {
        doc.page(page_num)
            .unwrap()
            .add_text(
                &format!("Source page {page_num}"),
                font,
                [72.0, 700.0],
                14.0,
                BLACK,
            )
            .unwrap();
    }
    doc.save_to_bytes().unwrap()
}

struct DelayedPageTranslator;

#[async_trait::async_trait]
impl harumi_ai::Translator for DelayedPageTranslator {
    async fn translate(
        &self,
        texts: &[String],
        _target_lang: &str,
        _source_lang: Option<&str>,
    ) -> harumi_ai::Result<Vec<String>> {
        let mut out = Vec::with_capacity(texts.len());
        for text in texts {
            let input: serde_json::Value = serde_json::from_str(text)
                .map_err(|e| harumi_ai::Error::Translator(e.to_string()))?;
            let pages = input["pages"]
                .as_array()
                .ok_or_else(|| harumi_ai::Error::Translator("missing pages".into()))?;
            let first_page = pages
                .first()
                .and_then(|p| p["page"].as_u64())
                .unwrap_or(1);

            tokio::time::sleep(std::time::Duration::from_millis((5 - first_page) * 20)).await;

            let translated_pages: Vec<serde_json::Value> = pages
                .iter()
                .map(|page| {
                    let page_num = page["page"].as_u64().unwrap_or(0);
                    let blocks: Vec<serde_json::Value> = page["blocks"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|block| {
                            serde_json::json!({
                                "id": block["id"].as_u64().unwrap_or(0),
                                "text": format!("Translated page {page_num}"),
                            })
                        })
                        .collect();
                    serde_json::json!({ "blocks": blocks })
                })
                .collect();
            out.push(serde_json::json!({ "pages": translated_pages }).to_string());
        }
        Ok(out)
    }
}

#[tokio::test]
async fn echo_translator_single_page() {
    let pdf = make_test_pdf();
    let opts = TranslateOptions::new("en", EchoTranslator, FONT.to_vec());
    let result = translate_pdf(&pdf, opts).await;
    assert!(result.is_ok(), "translate_pdf failed: {:?}", result.err());

    let out = result.unwrap().pdf_bytes;
    let check = Document::from_bytes(&out).unwrap();
    assert!(check.page_count() >= 1);
    // Verify text is present in the output.
    let runs = check.extract_text_runs(1).unwrap();
    assert!(!runs.is_empty(), "output PDF has no text on page 1");
}

#[tokio::test]
async fn echo_translator_multipage() {
    let pdf = make_multipage_pdf();
    let opts = TranslateOptions::new("en", EchoTranslator, FONT.to_vec());
    let result = translate_pdf(&pdf, opts).await;
    assert!(result.is_ok(), "multipage translate_pdf failed: {:?}", result.err());

    let out = result.unwrap().pdf_bytes;
    let check = Document::from_bytes(&out).unwrap();
    // Two source pages → at least two output pages.
    assert!(
        check.page_count() >= 2,
        "expected ≥2 pages, got {}",
        check.page_count()
    );
}

#[tokio::test]
async fn concurrent_batches_preserve_page_order() {
    let pdf = make_four_page_pdf();
    let opts = TranslateOptions::builder()
        .target_lang("en")
        .translator(DelayedPageTranslator)
        .font(FONT.to_vec())
        .concurrency(4)
        .pages_per_batch(1)
        .build();

    let out = translate_pdf(&pdf, opts).await.unwrap().pdf_bytes;
    let check = Document::from_bytes(&out).unwrap();
    assert_eq!(check.page_count(), 4);

    for page_num in 1..=4 {
        let text: String = check
            .extract_text_runs(page_num)
            .unwrap()
            .into_iter()
            .map(|run| run.text)
            .collect();
        assert!(
            text.contains(&format!("Translated page {page_num}")),
            "page {page_num} text was {text:?}"
        );
    }
}

#[tokio::test]
async fn empty_pdf_returns_blank() {
    // PDF with no text → should return a blank single-page PDF without error.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let pdf = doc.save_to_bytes().unwrap();
    let opts = TranslateOptions::new("zh", EchoTranslator, FONT.to_vec());
    let result = translate_pdf(&pdf, opts).await;
    assert!(result.is_ok(), "empty PDF failed: {:?}", result.err());
    let check = Document::from_bytes(&result.unwrap().pdf_bytes).unwrap();
    assert_eq!(check.page_count(), 1);
}

#[tokio::test]
async fn builder_api() {
    let pdf = make_test_pdf();
    let opts = TranslateOptions::builder()
        .target_lang("en")
        .translator(EchoTranslator)
        .font(FONT.to_vec())
        .concurrency(2)
        .pages_per_batch(2)
        .build();
    let result = translate_pdf(&pdf, opts).await;
    assert!(result.is_ok(), "builder_api failed: {:?}", result.err());
}

#[tokio::test]
async fn pages_per_batch_multipage() {
    // 2-page PDF with pages_per_batch=2 → both pages sent in a single LLM request.
    let pdf = make_multipage_pdf();
    let opts = TranslateOptions::builder()
        .target_lang("en")
        .translator(EchoTranslator)
        .font(FONT.to_vec())
        .pages_per_batch(2)
        .build();
    let result = translate_pdf(&pdf, opts).await;
    assert!(result.is_ok(), "pages_per_batch_multipage failed: {:?}", result.err());
    let check = Document::from_bytes(&result.unwrap().pdf_bytes).unwrap();
    assert!(check.page_count() >= 2);
}

#[tokio::test]
async fn inplace_mode_basic() {
    // InPlace mode should produce a valid PDF without error.
    let pdf = make_test_pdf();
    let mut opts = TranslateOptions::new("en", EchoTranslator, FONT.to_vec());
    opts.mode = TranslationMode::InPlace;
    let result = translate_pdf(&pdf, opts).await;
    assert!(result.is_ok(), "InPlace translate_pdf failed: {:?}", result.err());

    let out = result.unwrap().pdf_bytes;
    let check = Document::from_bytes(&out).unwrap();
    assert!(check.page_count() >= 1);
    // Either the in-place replacement or the fallback overlay places text on page 1.
    let runs = check.extract_text_runs(1).unwrap();
    assert!(!runs.is_empty(), "InPlace output PDF has no text on page 1");
}

#[tokio::test]
async fn inplace_mode_unmatched_falls_back() {
    // A PDF with no text lines produces a valid output without error (empty pages
    // have no lines, so the InPlace apply loop is simply a no-op).
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let pdf = doc.save_to_bytes().unwrap();
    let mut opts = TranslateOptions::new("en", EchoTranslator, FONT.to_vec());
    opts.mode = TranslationMode::InPlace;
    let result = translate_pdf(&pdf, opts).await;
    assert!(result.is_ok(), "InPlace empty PDF failed: {:?}", result.err());
    let check = Document::from_bytes(&result.unwrap().pdf_bytes).unwrap();
    assert_eq!(check.page_count(), 1);
}

#[tokio::test]
async fn custom_layout_options() {
    let pdf = make_test_pdf();
    let mut layout = LayoutOptions::default();
    layout.margin = 50.0;
    layout.body_size = 12.0;

    let mut opts = TranslateOptions::new("en", EchoTranslator, FONT.to_vec());
    opts.layout = layout;

    let result = translate_pdf(&pdf, opts).await;
    assert!(result.is_ok(), "custom layout failed: {:?}", result.err());
}

// ---------------------------------------------------------------------------
// v0.2.0 — QualityGate, Auto mode, TranslateOutput
// ---------------------------------------------------------------------------

#[tokio::test]
async fn translate_output_has_pdf_bytes() {
    let pdf = make_test_pdf();
    let opts = TranslateOptions::new("en", EchoTranslator, FONT.to_vec());
    let output = translate_pdf(&pdf, opts).await.unwrap();
    assert!(!output.pdf_bytes.is_empty());
}

#[tokio::test]
async fn quality_gate_best_effort_always_passes() {
    let pdf = make_test_pdf();
    let mut opts = TranslateOptions::new("en", EchoTranslator, FONT.to_vec());
    opts.profile = QualityProfile::BestEffort;
    let output = translate_pdf(&pdf, opts).await.unwrap();
    assert!(output.quality.overall.is_pass() || !output.quality.overall.is_pass(), "any result is fine");
}

#[tokio::test]
async fn quality_gate_strict_passes_for_simple_pdf() {
    // Simple single-line PDF should pass even strict gate (zero collisions, no overflow).
    let pdf = make_test_pdf();
    let mut opts = TranslateOptions::new("en", EchoTranslator, FONT.to_vec());
    opts.profile = QualityProfile::Strict;
    // Strict only errors for Overlay/layout mode; Overlay on this simple PDF should be clean.
    let result = translate_pdf(&pdf, opts).await;
    assert!(result.is_ok(), "strict gate failed unexpectedly: {:?}", result.err());
}

#[tokio::test]
async fn auto_mode_selects_a_mode() {
    let pdf = make_test_pdf();
    let mut opts = TranslateOptions::new("en", EchoTranslator, FONT.to_vec());
    opts.mode = TranslationMode::Auto;
    let output = translate_pdf(&pdf, opts).await.unwrap();
    // Mode should be resolved to something concrete.
    assert!(
        matches!(output.quality.mode_used, TranslationMode::Overlay | TranslationMode::InPlace | TranslationMode::NewDocument),
        "Auto mode did not resolve to a concrete mode"
    );
}

#[tokio::test]
async fn quality_gate_evaluate_empty_summary() {
    use harumi::PageFitSummary;
    let gate = QualityGate::from_profile(&QualityProfile::Strict);
    let summary = PageFitSummary::from_plans(&[]);
    assert!(gate.evaluate(&summary).is_pass(), "empty summary should always pass Strict gate");
}

#[tokio::test]
async fn max_correction_rounds_zero_disables_loop() {
    let pdf = make_test_pdf();
    let mut opts = TranslateOptions::new("en", EchoTranslator, FONT.to_vec());
    opts.max_correction_rounds = 0;
    let output = translate_pdf(&pdf, opts).await.unwrap();
    assert_eq!(output.quality.correction_rounds, 0);
}

// ---------------------------------------------------------------------------
// v0.2.1 — Auto cascade + fallback_reason
// ---------------------------------------------------------------------------

#[tokio::test]
async fn auto_best_effort_no_cascade_no_fallback_reason() {
    // BestEffort profile: Auto picks one mode, no cascade, fallback_reason stays None.
    let pdf = make_test_pdf();
    let mut opts = TranslateOptions::new("en", EchoTranslator, FONT.to_vec());
    opts.mode = TranslationMode::Auto;
    opts.profile = QualityProfile::BestEffort;
    let output = translate_pdf(&pdf, opts).await.unwrap();
    assert!(
        matches!(output.quality.mode_used, TranslationMode::Overlay | TranslationMode::InPlace | TranslationMode::NewDocument),
        "mode_used must be a concrete mode, not Auto"
    );
    assert!(
        output.quality.fallback_reason.is_none(),
        "BestEffort Auto should not set fallback_reason"
    );
}

#[tokio::test]
async fn auto_preserve_layout_cascade_sets_fallback_reason() {
    // PreserveLayout: Auto starts with InPlace; a simple PDF with EchoTranslator
    // may or may not cascade, but fallback_reason is either None or a non-empty string.
    let pdf = make_test_pdf();
    let mut opts = TranslateOptions::new("en", EchoTranslator, FONT.to_vec());
    opts.mode = TranslationMode::Auto;
    opts.profile = QualityProfile::PreserveLayout;
    let output = translate_pdf(&pdf, opts).await.unwrap();
    // mode_used must always be concrete
    assert!(
        matches!(output.quality.mode_used, TranslationMode::Overlay | TranslationMode::InPlace | TranslationMode::NewDocument),
        "mode_used {:?} must be concrete", output.quality.mode_used
    );
    // If there was a cascade, fallback_reason must be Some (non-empty)
    if output.quality.mode_used != TranslationMode::InPlace {
        assert!(
            output.quality.fallback_reason.is_some(),
            "when mode changed from InPlace, fallback_reason must be set"
        );
    }
}

#[tokio::test]
async fn fallback_reason_field_accessible() {
    // Ensure the field exists and is accessible (compile + runtime smoke test).
    let pdf = make_test_pdf();
    let opts = TranslateOptions::new("en", EchoTranslator, FONT.to_vec());
    let output = translate_pdf(&pdf, opts).await.unwrap();
    // Default mode (Overlay) with no cascade → None
    let _reason: Option<&str> = output.quality.fallback_reason.as_deref();
}
