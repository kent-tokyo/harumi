// overlay.rs — in-place overlay translation with AI layout evaluation

use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};

use futures::stream::{self, StreamExt};
use harumi::{FontHandle, detect_text_columns, sort_by_reading_order, Document, TextFragment};
use ttf_parser::Face;

use crate::{
    Error, OverflowStrategy, Result, TranslateOptions,
    builder::OutputBlock,
    extractor,
    prompts::layout_correction_prompt,
};

// ── Font-fallback helpers ─────────────────────────────────────────────────────

/// Split `text` into runs of (substring, font_index).
///
/// Font index 0 = primary font; 1+ = fallbacks in order.
/// Characters not covered by any font are assigned to the primary font (index 0).
pub(crate) fn split_by_font(text: &str, faces: &[&Face]) -> Vec<(String, usize)> {
    if faces.len() <= 1 {
        return vec![(text.to_owned(), 0)];
    }
    let mut runs: Vec<(String, usize)> = Vec::new();
    for ch in text.chars() {
        let idx = faces
            .iter()
            .position(|f| f.glyph_index(ch).is_some())
            .unwrap_or(0);
        if let Some(last) = runs.last_mut()
            && last.1 == idx
        {
            last.0.push(ch);
        } else {
            runs.push((ch.to_string(), idx));
        }
    }
    runs
}

// ── Internal types ────────────────────────────────────────────────────────────

pub(crate) struct OverlayLine {
    pub(crate) x: f32,
    pub(crate) y: f32,
    /// Actual right edge of the text run (x + width of rightmost fragment).
    pub(crate) right: f32,
    /// Right boundary of the column this line belongs to (used for avail_w).
    pub(crate) col_right: f32,
    pub(crate) line_height: f32,
    pub(crate) is_heading: bool,
    #[allow(dead_code)]
    pub(crate) is_bold: bool,
    #[allow(dead_code)]
    pub(crate) page_width: f32,
    pub(crate) text: String,
    /// Original fragment texts (individual Tj runs) for text-layer blanking.
    #[allow(dead_code)]
    pub(crate) fragment_texts: Vec<String>,
    /// Font size of the original text (PDF points), derived from TextFragment.font_size.
    pub(crate) font_size: f32,
}

pub(crate) struct OverlayPage {
    pub(crate) page_num: u32,
    pub(crate) lines: Vec<OverlayLine>,
    pub(crate) body_font_size: f32,
    /// Bboxes of invisible (render-mode-3) text fragments to also white-out.
    pub(crate) invisible_rects: Vec<[f32; 4]>,
}

// ── Extraction helpers ────────────────────────────────────────────────────────

struct RawLine {
    x: f32,
    y: f32,
    right: f32,
    col_right: f32,
    text: String,
    fragments: Vec<String>,
    /// True if any fragment on this line is bold.
    is_bold: bool,
    /// Max font_size across fragments on this line (PDF points).
    font_size: f32,
}

/// Group a slice of fragments (already sorted top-to-bottom within a column)
/// into text lines using a Y-tolerance merge, then return `RawLine`s.
fn group_into_raw_lines(frags: &[&TextFragment], col_right: f32) -> Vec<RawLine> {
    const Y_TOL: f32 = 2.0;
    let mut raw_lines: Vec<RawLine> = Vec::new();
    for frag in frags {
        let frag_right = frag.x + frag.width.max(0.0);
        if let Some(last) = raw_lines.last_mut()
            && (frag.y - last.y).abs() <= Y_TOL
        {
            if !last.text.is_empty() && !last.text.ends_with(' ') {
                last.text.push(' ');
            }
            last.text.push_str(&frag.text);
            last.x = last.x.min(frag.x);
            last.right = last.right.max(frag_right);
            last.fragments.push(frag.text.clone());
            last.is_bold = last.is_bold || frag.is_bold;
            last.font_size = last.font_size.max(frag.font_size);
            continue;
        }
        raw_lines.push(RawLine {
            x: frag.x,
            y: frag.y,
            right: frag_right,
            col_right,
            text: frag.text.clone(),
            fragments: vec![frag.text.clone()],
            is_bold: frag.is_bold,
            font_size: frag.font_size,
        });
    }
    raw_lines
}

// ── Extraction ────────────────────────────────────────────────────────────────

pub(crate) fn extract_overlay_pages(doc: &mut Document) -> Result<Vec<OverlayPage>> {
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

        // Collect invisible (OCR/render-mode-3) fragment bboxes for white-rect coverage.
        let invisible_rects: Vec<[f32; 4]> = runs
            .iter()
            .filter(|r| r.invisible && !r.text.trim().is_empty())
            .map(|r| [r.x - 1.0, r.y - 1.0, r.width.max(2.0) + 2.0, r.height.max(2.0) + 2.0])
            .collect();

        let mut visible: Vec<TextFragment> = runs
            .into_iter()
            .filter(|r| !r.invisible && !r.text.trim().is_empty())
            .collect();

        if visible.is_empty() {
            pages.push(OverlayPage { page_num, lines: vec![], body_font_size: 12.0, invisible_rects });
            continue;
        }

        // Use harumi's NaN-safe reading-order sort.
        sort_by_reading_order(&mut visible);

        // Detect column layout; fall back to full-page single column.
        let cols = detect_text_columns(&visible, page_width);
        let col_zones: Vec<(f32, f32)> = if cols.is_empty() {
            vec![(0.0, page_width)]
        } else {
            cols.iter().map(|c| (c.x_start, c.x_end)).collect()
        };

        // Group fragments into lines per column, then concatenate columns left→right.
        let mut all_raw_lines: Vec<RawLine> = Vec::new();
        for (col_x_start, col_x_end) in &col_zones {
            let col_frags: Vec<&TextFragment> = visible
                .iter()
                .filter(|f| f.x >= *col_x_start && f.x < *col_x_end)
                .collect();
            let mut col_lines = group_into_raw_lines(&col_frags, *col_x_end);
            all_raw_lines.append(&mut col_lines);
        }

        // Estimate body font size from inter-line gaps (bottom quintile = tight spacing).
        let mut gaps: Vec<f32> = all_raw_lines
            .windows(2)
            .map(|w| (w[0].y - w[1].y).abs())
            .filter(|&g| g > 1.0 && g < 60.0)
            .collect();
        gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let body_line_h = if gaps.is_empty() { 14.0_f32 } else { gaps[gaps.len() / 5] };
        // CJK glyphs fill the full em-square; use a larger leading ratio (1.6).
        let body_font_size = (body_line_h / 1.6).max(6.0);

        let raw_lines = &all_raw_lines;
        let lines = raw_lines
            .iter()
            .enumerate()
            .map(|(i, rl)| {
                let gap_above = if i > 0 { (raw_lines[i - 1].y - rl.y).abs() } else { body_line_h };
                let gap_below = if i + 1 < raw_lines.len() { (rl.y - raw_lines[i + 1].y).abs() } else { body_line_h };
                // Heading: large gap above AND starts near left margin, OR bold font.
                let is_heading = (gap_above > body_line_h * 2.2 && rl.x < page_width * 0.2)
                    || rl.is_bold;
                let lh = gap_below.min(body_line_h * 1.5).max(body_line_h);
                OverlayLine {
                    x: rl.x,
                    y: rl.y,
                    right: rl.right,
                    col_right: rl.col_right,
                    line_height: lh,
                    is_heading,
                    is_bold: rl.is_bold,
                    page_width,
                    text: rl.text.clone(),
                    fragment_texts: rl.fragments.clone(),
                    font_size: rl.font_size,
                }
            })
            .collect();

        pages.push(OverlayPage { page_num, lines, body_font_size, invisible_rects });
    }
    Ok(pages)
}

// ── Font sizing ───────────────────────────────────────────────────────────────

/// Measure total advance width of `text` at `font_size` using the given face.
pub(crate) fn measure_text_width(text: &str, face: &Face, font_size: f32) -> f32 {
    text.chars()
        .filter_map(|ch| harumi::glyph_advance_pt(face, ch, font_size))
        .sum()
}

/// Scale down font_size so text fits within max_width, floored at `min_fs`.
pub(crate) fn fit_font_size(
    text: &str,
    face: &Face,
    desired_size: f32,
    max_width: f32,
    min_fs: f32,
) -> f32 {
    if max_width <= 0.0 { return desired_size; }
    let total_w = measure_text_width(text, face, desired_size);
    if total_w <= max_width || total_w == 0.0 { return desired_size; }
    (desired_size * max_width / total_w).max(min_fs)
}

/// Truncate `text` at `font_size` so it fits within `max_width`, appending `"…"`.
pub(crate) fn truncate_to_fit(text: &str, face: &Face, font_size: f32, max_width: f32) -> String {
    let ellipsis = "…";
    let ellipsis_w = measure_text_width(ellipsis, face, font_size);
    if ellipsis_w >= max_width { return ellipsis.to_owned(); }
    let budget = max_width - ellipsis_w;
    let chars: Vec<char> = text.chars().collect();
    let mut lo = 0usize;
    let mut hi = chars.len();
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let s: String = chars[..mid].iter().collect();
        if measure_text_width(&s, face, font_size) <= budget { lo = mid; } else { hi = mid - 1; }
    }
    if lo == 0 {
        ellipsis.to_owned()
    } else {
        format!("{}{}", chars[..lo].iter().collect::<String>(), ellipsis)
    }
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

// ── Shared extract + translate helper ────────────────────────────────────────

/// Phases 1 and 2 shared between Overlay and InPlace modes.
/// Returns (overlay_pages, page_translations, global_body_fs).
pub(crate) async fn extract_and_translate(
    pdf_bytes: &[u8],
    options: &TranslateOptions,
) -> Result<(Vec<OverlayPage>, HashMap<u32, Vec<String>>, f32)> {
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

    let total_pages = overlay_pages.len() as u32;
    let done_pages = Arc::new(AtomicU32::new(0));

    let results: Vec<(u32, Vec<String>)> = stream::iter(batches)
        .map(|batch| {
            let translator = Arc::clone(&translator);
            let target = target_lang.clone();
            let src = source_lang.clone();
            let batch_len = batch.len() as u32;
            let done_pages = Arc::clone(&done_pages);
            let progress = options.progress_fn.clone();
            async move {
                let batch_json = extractor::pages_to_json(&batch)?;
                let results = translator.translate(&[batch_json], &target, src.as_deref()).await?;
                let json = results.into_iter().next()
                    .ok_or_else(|| Error::Translator("translator returned empty result".into()))?;
                let page_block_lists = extractor::json_to_translated_pages(&json)?;

                let completed = done_pages.fetch_add(batch_len, Ordering::Relaxed) + batch_len;
                if let Some(f) = &progress { f(completed.min(total_pages), total_pages); }

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

    Ok((overlay_pages, page_translations, global_body_fs))
}

// ── Main entry point ──────────────────────────────────────────────────────────

pub async fn translate_pdf_overlay(pdf_bytes: &[u8], options: TranslateOptions) -> Result<Vec<u8>> {
    let (overlay_pages, mut page_translations, global_body_fs) =
        extract_and_translate(pdf_bytes, &options).await?;

    let translator = Arc::clone(&options.translator);
    let target_lang = options.target_lang.clone();
    let source_lang = options.source_lang.clone();

    // ── Phase 3: AI layout evaluation — compare original vs. translated ───────
    let face = Face::parse(&options.font, 0)
        .map_err(|e| Error::FontParse(e.to_string()))?;

    // Parse fallback faces (borrow from options; live for the whole function).
    let fallback_faces: Vec<Face<'_>> = options.font_fallbacks.iter()
        .filter_map(|b| Face::parse(b, 0).ok())
        .collect();
    let all_faces: Vec<&Face<'_>> = std::iter::once(&face)
        .chain(fallback_faces.iter())
        .collect();

    let min_fs = options.overflow.min_font_size();

    let mut reports: Vec<PlacedLineReport> = Vec::new();
    for overlay_page in &overlay_pages {
        let page_num = overlay_page.page_num;
        let Some(translations) = page_translations.get(&page_num) else { continue };

        for (line_idx, (line, trans_text)) in overlay_page.lines.iter().zip(translations.iter()).enumerate() {
            let text = trans_text.trim();
            if text.is_empty() { continue; }
            let max_fs = line.line_height * 0.85;
            let fs = if line.font_size > 0.0 { line.font_size } else { global_body_fs };
            let desired = if line.is_heading { (fs * 1.4).min(max_fs) } else { fs.min(max_fs) };
            let avail_w = (line.col_right - line.x).max(50.0);
            let text_w_desired = measure_text_width(text, &face, desired);
            let actual_fs = fit_font_size(text, &face, desired, avail_w, min_fs);
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
        if let Some(texts) = page_translations.get_mut(page_num)
            && let Some(slot) = texts.get_mut(*line_idx)
        {
            *slot = corrected_text.clone();
        }
    }

    // ── Phase 4: Apply overlay to original PDF ────────────────────────────────
    let mut doc = Document::from_bytes(pdf_bytes)?;
    // Embed primary font; fallback fonts are embedded lazily on first use.
    let primary_font = doc.embed_font(&options.font)?;
    let mut font_handles: Vec<Option<FontHandle>> =
        std::iter::once(Some(primary_font))
            .chain(std::iter::repeat(None).take(options.font_fallbacks.len()))
            .collect();

    let cover_color = options.cover_color.unwrap_or([1.0, 1.0, 1.0]);

    // Compute descender depth from the actual font (once, reused per line).
    let descender_ratio = (-face.descender() as f32 / face.units_per_em() as f32)
        .clamp(0.05, 0.35);

    for overlay_page in &overlay_pages {
        let page_num = overlay_page.page_num;
        let translated_texts = page_translations.get(&page_num);

        // First pass: cover rectangles over original text.
        for &rect in &overlay_page.invisible_rects {
            doc.page(page_num)?.add_rect(rect, cover_color, 1.0)?;
        }
        for line in &overlay_page.lines {
            let x = line.x - 1.0;
            // Extend below the baseline by the font's actual descender depth (per-line).
            let below = line.font_size.max(global_body_fs) * descender_ratio;
            let y = line.y - below;
            // Width: cover from left edge to actual text right edge + 2pt padding.
            let w = (line.right - x + 2.0).max(10.0);
            // Height: use per-line spacing, not a global fixed multiplier.
            let h = line.line_height + below;
            doc.page(page_num)?.add_rect([x, y, w, h], cover_color, 1.0)?;
        }

        // Second pass: translated (and corrected) text.
        if let Some(translations) = translated_texts {
            for (line, trans_text) in overlay_page.lines.iter().zip(translations.iter()) {
                let text = trans_text.trim();
                if text.is_empty() { continue; }
                let max_fs = line.line_height * 0.85;
                let fs = if line.font_size > 0.0 { line.font_size } else { global_body_fs };
                let desired = if line.is_heading { (fs * 1.4).min(max_fs) } else { fs.min(max_fs) };
                let avail_w = (line.col_right - line.x).max(50.0);
                let scaled = fit_font_size(text, &face, desired, avail_w, min_fs).max(desired * 0.95);
                // Apply overflow strategy: truncate if still too wide.
                let display_text: std::borrow::Cow<str> = match &options.overflow {
                    OverflowStrategy::Truncate { .. }
                        if measure_text_width(text, &face, scaled) > avail_w * 1.05 =>
                    {
                        truncate_to_fit(text, &face, scaled, avail_w).into()
                    }
                    _ => text.into(),
                };
                // Synthetic bold for headings and originally-bold lines.
                let bold = line.is_heading || line.is_bold;

                // Split text into font-specific runs and render each sub-run.
                let runs = split_by_font(&display_text, &all_faces);
                let mut run_x = line.x;
                for (run_text, fidx) in runs {
                    // Embed fallback font on first use.
                    if font_handles[fidx].is_none() {
                        let fb = &options.font_fallbacks[fidx - 1];
                        font_handles[fidx] = Some(doc.embed_font(fb)?);
                    }
                    let fh = font_handles[fidx].unwrap();
                    let run_face = all_faces[fidx];
                    doc.page(page_num)?.add_text_styled(
                        &run_text, fh, [run_x, line.y], scaled, [0.0f32, 0.0, 0.0], bold, false,
                    )?;
                    run_x += measure_text_width(&run_text, run_face, scaled);
                }
            }
        }
    }

    doc.save_to_bytes().map_err(Into::into)
}
