// overlay.rs — in-place overlay translation with AI layout evaluation

use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::{self, StreamExt};
use harumi::Document;
use ttf_parser::Face;

use crate::{
    Error, Result, TranslateOptions,
    builder::OutputBlock,
    extractor,
    prompts::layout_correction_prompt,
};

// ── Internal types ────────────────────────────────────────────────────────────

struct OverlayLine {
    pub x: f32,
    pub y: f32,
    pub line_height: f32,
    pub is_heading: bool,
    pub page_width: f32,
    pub text: String,
    /// Original fragment texts (individual Tj runs) for text-layer blanking.
    pub fragment_texts: Vec<String>,
}

struct OverlayPage {
    pub page_num: u32,
    pub lines: Vec<OverlayLine>,
    pub body_font_size: f32,
    /// Bboxes of invisible (render-mode-3) text fragments to also white-out.
    pub invisible_rects: Vec<[f32; 4]>,
}

// ── Extraction ────────────────────────────────────────────────────────────────

fn extract_overlay_pages(doc: &mut Document) -> Result<Vec<OverlayPage>> {
    let page_count = doc.page_count();
    let mut pages = Vec::new();

    for page_num in 1..=page_count {
        let size = {
            let ph = doc.page(page_num)?;
            let mb = ph.media_box()?;
            (mb[2], mb[3])
        };
        let page_width = size.0;

        let runs = doc.extract_text_runs(page_num)?;

        // Also collect invisible (OCR/render-mode-3) fragment bboxes for white-rect coverage.
        // These hidden text layers cause unexpected column selections in PDF viewers.
        let invisible_rects: Vec<[f32; 4]> = runs
            .iter()
            .filter(|r| r.invisible && !r.text.trim().is_empty())
            .map(|r| [r.x - 1.0, r.y - 1.0, r.width.max(2.0) + 2.0, r.height.max(2.0) + 2.0])
            .collect();

        let visible: Vec<_> = runs
            .iter()
            .filter(|r| !r.invisible && !r.text.trim().is_empty())
            .collect();

        if visible.is_empty() {
            pages.push(OverlayPage { page_num, lines: vec![], body_font_size: 12.0, invisible_rects });
            continue;
        }

        let mut sorted: Vec<_> = visible.clone();
        sorted.sort_by(|a, b| {
            b.y.partial_cmp(&a.y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
        });

        const Y_TOL: f32 = 2.0;
        struct RawLine { x: f32, y: f32, right: f32, text: String, fragments: Vec<String> }
        let mut raw_lines: Vec<RawLine> = Vec::new();
        for frag in &sorted {
            let frag_right = frag.x + frag.width.max(0.0);
            if let Some(last) = raw_lines.last_mut() {
                if (frag.y - last.y).abs() <= Y_TOL {
                    if !last.text.is_empty() && !last.text.ends_with(' ') {
                        last.text.push(' ');
                    }
                    last.text.push_str(&frag.text);
                    last.x = last.x.min(frag.x);
                    last.right = last.right.max(frag_right);
                    last.fragments.push(frag.text.clone());
                    continue;
                }
            }
            raw_lines.push(RawLine {
                x: frag.x, y: frag.y, right: frag_right,
                text: frag.text.clone(), fragments: vec![frag.text.clone()],
            });
        }

        let mut gaps: Vec<f32> = raw_lines
            .windows(2)
            .map(|w| (w[0].y - w[1].y).abs())
            .filter(|&g| g > 1.0 && g < 60.0)
            .collect();
        gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let body_line_h = if gaps.is_empty() { 14.0_f32 } else { gaps[gaps.len() / 5] };
        // CJK glyphs fill the full em-square, so use a larger leading ratio (1.6)
        // to leave enough visual breathing room between lines.
        let body_font_size = (body_line_h / 1.6).max(6.0);

        let lines = raw_lines
            .iter()
            .enumerate()
            .map(|(i, rl)| {
                let gap_above = if i > 0 { (raw_lines[i - 1].y - rl.y).abs() } else { body_line_h };
                let gap_below = if i + 1 < raw_lines.len() { (rl.y - raw_lines[i + 1].y).abs() } else { body_line_h };
                // A heading has a large gap above AND starts near the left margin.
                // Indented continuation lines (like sub-items) should not be treated
                // as headings even if they follow a large vertical gap.
                let is_heading = gap_above > body_line_h * 2.2 && rl.x < page_width * 0.2;
                let lh = gap_below.min(body_line_h * 1.5).max(body_line_h);
                OverlayLine {
                    x: rl.x, y: rl.y, line_height: lh, is_heading, page_width,
                    text: rl.text.clone(), fragment_texts: rl.fragments.clone(),
                }
            })
            .collect();

        pages.push(OverlayPage { page_num, lines, body_font_size, invisible_rects });
    }
    Ok(pages)
}

// ── Font sizing ───────────────────────────────────────────────────────────────

/// Measure total advance width of `text` at `font_size` using the given face.
fn measure_text_width(text: &str, face: &Face, font_size: f32) -> f32 {
    text.chars()
        .filter_map(|ch| harumi::glyph_advance_pt(face, ch, font_size))
        .sum()
}

/// Scale down font_size so text fits within max_width. Minimum 6pt.
fn fit_font_size(text: &str, face: &Face, desired_size: f32, max_width: f32) -> f32 {
    if max_width <= 0.0 { return desired_size; }
    let total_w = measure_text_width(text, face, desired_size);
    if total_w <= max_width || total_w == 0.0 { return desired_size; }
    (desired_size * max_width / total_w).max(6.0)
}

// ── AI layout evaluation ──────────────────────────────────────────────────────

/// Describes a single placed line for AI evaluation.
#[derive(serde::Serialize, serde::Deserialize)]
struct PlacedLineReport {
    id: usize,
    page: u32,
    original_text: String,
    translated_text: String,
    font_size: f32,
    text_width_pt: f32,
    avail_width_pt: f32,
    overflow: bool,
}

#[derive(serde::Deserialize)]
struct CorrectionResponse {
    corrections: Vec<Correction>,
}

#[derive(serde::Deserialize)]
struct Correction {
    id: usize,
    page: u32,
    text: String,
}

/// Ask the AI to evaluate layout and return corrected texts for overflowing lines.
/// Returns a map of (page_num, line_id) → corrected text.
async fn evaluate_layout(
    reports: &[PlacedLineReport],
    translator: &Arc<dyn crate::Translator>,
    target_lang: &str,
    source_lang: Option<&str>,
) -> Result<HashMap<(u32, usize), String>> {
    let overflows: Vec<&PlacedLineReport> = reports.iter().filter(|r| r.overflow).collect();
    if overflows.is_empty() {
        return Ok(HashMap::new());
    }

    let prompt = layout_correction_prompt(
        target_lang,
        source_lang,
        &serde_json::to_string_pretty(&overflows)
            .map_err(|e| Error::Translator(e.to_string()))?,
    );

    let raw_results = translator
        .translate(&[prompt], target_lang, source_lang)
        .await?;
    let raw = raw_results.into_iter().next()
        .ok_or_else(|| Error::Translator("AI returned empty correction response".into()))?;

    // Strip markdown fences if present.
    let json_str = {
        let s = raw.trim();
        let s = s.strip_prefix("```json").unwrap_or(s);
        let s = s.strip_prefix("```").unwrap_or(s);
        let s = s.strip_suffix("```").unwrap_or(s);
        s.trim()
    };

    let resp: CorrectionResponse = serde_json::from_str(json_str)
        .map_err(|e| Error::Translator(format!("AI correction JSON invalid: {e}. Raw: {json_str}")))?;

    let mut corrections = HashMap::new();
    for c in resp.corrections {
        corrections.insert((c.page, c.id), c.text);
    }
    Ok(corrections)
}

// ── Main entry point ──────────────────────────────────────────────────────────

pub async fn translate_pdf_overlay(pdf_bytes: &[u8], options: TranslateOptions) -> Result<Vec<u8>> {
    // ── Phase 1: Extract positioned lines ────────────────────────────────────
    let mut doc = Document::from_bytes(pdf_bytes)?;
    let overlay_pages = extract_overlay_pages(&mut doc)?;
    drop(doc);

    // Use a single global body_font_size across all pages so font sizes are
    // consistent. Take the median of per-page estimates; fall back to 12pt.
    let global_body_fs = {
        let mut sizes: Vec<f32> = overlay_pages.iter()
            .filter(|p| !p.lines.is_empty())
            .map(|p| p.body_font_size)
            .collect();
        if sizes.is_empty() {
            12.0_f32
        } else {
            sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            sizes[sizes.len() / 2]
        }
    };

    let page_contents: Vec<extractor::PageContent> = overlay_pages
        .iter()
        .map(|op| {
            let blocks = op.lines.iter().enumerate()
                .map(|(id, line)| extractor::Block { id, block_type: "paragraph".to_owned(), text: line.text.clone() })
                .collect();
            extractor::PageContent { page_num: op.page_num, size: (0.0, 0.0), blocks }
        })
        .collect();

    // ── Phase 2: Translate ────────────────────────────────────────────────────
    let translator = Arc::clone(&options.translator);
    let target_lang = options.target_lang.clone();
    let source_lang = options.source_lang.clone();
    let batch_size = options.pages_per_batch;

    let batches: Vec<Vec<extractor::PageContent>> = page_contents
        .chunks(batch_size)
        .map(<[_]>::to_vec)
        .collect();

    let mut page_translations: HashMap<u32, Vec<String>> = HashMap::new();

    let results: Vec<(u32, Vec<String>)> = stream::iter(batches)
        .map(|batch| {
            let translator = Arc::clone(&translator);
            let target = target_lang.clone();
            let src = source_lang.clone();
            async move {
                let batch_json = extractor::pages_to_json(&batch)?;
                let results = translator.translate(&[batch_json], &target, src.as_deref()).await?;
                let json = results.into_iter().next()
                    .ok_or_else(|| Error::Translator("translator returned empty result".into()))?;
                let page_block_lists = extractor::json_to_translated_pages(&json)?;

                let out: Vec<(u32, Vec<String>)> = batch
                    .iter()
                    .zip(page_block_lists.iter().chain(std::iter::repeat(&vec![])))
                    .map(|(orig, t_blocks)| {
                        let texts: Vec<String> = t_blocks.iter()
                            .filter_map(|tb| {
                                orig.blocks.iter().find(|b| b.id == tb.id)
                                    .map(|_| OutputBlock { block_type: "paragraph".to_owned(), text: tb.text.clone() }.text)
                            })
                            .collect();
                        (orig.page_num, texts)
                    })
                    .collect();

                Ok::<Vec<(u32, Vec<String>)>, Error>(out)
            }
        })
        .buffered(options.concurrency)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();

    for (page_num, texts) in results {
        page_translations.insert(page_num, texts);
    }

    // ── Phase 3: AI layout evaluation — compare original vs. translated ───────
    let face = Face::parse(&options.font, 0)
        .map_err(|e| Error::FontParse(e.to_string()))?;

    let mut reports: Vec<PlacedLineReport> = Vec::new();
    for overlay_page in &overlay_pages {
        let page_num = overlay_page.page_num;
        let body_fs = global_body_fs;
        let Some(translations) = page_translations.get(&page_num) else { continue };

        for (line_idx, (line, trans_text)) in overlay_page.lines.iter().zip(translations.iter()).enumerate() {
            let text = trans_text.trim();
            if text.is_empty() { continue; }
            let max_fs = line.line_height * 0.85;
            let desired = if line.is_heading { (body_fs * 1.4).min(max_fs) } else { body_fs.min(max_fs) };
            let avail_w = (line.page_width - line.x - 20.0).max(50.0);
            // Measure at the DESIRED size (before scaling down) to detect true overflow.
            let text_w_desired = measure_text_width(text, &face, desired);
            let actual_fs = fit_font_size(text, &face, desired, avail_w);
            // Report overflow when the line is even mildly too tight.
            // We prefer prompting the model for a shorter translation over letting
            // a line shrink noticeably relative to its neighbors.
            let overflow = text_w_desired > avail_w * 1.05 || actual_fs < desired * 0.9;
            reports.push(PlacedLineReport {
                id: line_idx,
                page: page_num,
                original_text: line.text.clone(),
                translated_text: text.to_owned(),
                font_size: actual_fs,
                text_width_pt: text_w_desired,
                avail_width_pt: avail_w,
                overflow,
            });
        }
    }

    // Ask AI to correct overflowing lines by comparing original ↔ translated layout.
    let corrections = evaluate_layout(&reports, &translator, &target_lang, source_lang.as_deref()).await?;
    eprintln!("[harumi-ai] Layout evaluation: {} overflows, {} corrected",
        reports.iter().filter(|r| r.overflow).count(), corrections.len());

    // Apply corrections back into page_translations.
    for ((page_num, line_idx), corrected_text) in &corrections {
        if let Some(texts) = page_translations.get_mut(page_num) {
            if let Some(slot) = texts.get_mut(*line_idx) {
                *slot = corrected_text.clone();
            }
        }
    }

    // ── Phase 4: Apply overlay to original PDF ────────────────────────────────
    let mut doc = Document::from_bytes(pdf_bytes)?;

    let font = doc.embed_font(&options.font)?;

    for overlay_page in &overlay_pages {
        let page_num = overlay_page.page_num;
        let body_fs = global_body_fs;
        let translated_texts = page_translations.get(&page_num);

        // First pass: white rectangles over original text.
        // Also cover invisible (OCR) text layers that would otherwise create
        // unexpected column selections in PDF viewers.
        for &rect in &overlay_page.invisible_rects {
            doc.page(page_num)?.add_rect(rect, [1.0f32, 1.0, 1.0], 1.0)?;
        }
        for line in &overlay_page.lines {
            let x = line.x - 1.0;
            // Extend just 8% of body_font_size below the baseline (~1pt for 12pt text)
            // to cover thin horizontal rules and descenders, without clipping pictograms.
            let below = body_fs * 0.08;
            let y = line.y - below;
            let w = (line.page_width - x - 20.0).max(10.0);
            let h = body_fs * 1.3 + below;
            doc.page(page_num)?.add_rect([x, y, w, h], [1.0f32, 1.0, 1.0], 1.0)?;
        }

        // Second pass: translated (and corrected) text.
        if let Some(translations) = translated_texts {
            for (line, trans_text) in overlay_page.lines.iter().zip(translations.iter()) {
                let text = trans_text.trim();
                if text.is_empty() { continue; }
                let max_fs = line.line_height * 0.85;
            let desired = if line.is_heading { (body_fs * 1.4).min(max_fs) } else { body_fs.min(max_fs) };
                let avail_w = (line.page_width - line.x - 20.0).max(50.0);
                let scaled = fit_font_size(text, &face, desired, avail_w).max(desired * 0.95);
                doc.page(page_num)?.add_text(
                    text, font, [line.x, line.y - scaled * 0.1], scaled, [0.0f32, 0.0, 0.0],
                )?;
            }
        }
    }

    doc.save_to_bytes().map_err(Into::into)
}
