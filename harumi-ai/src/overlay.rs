// overlay.rs — in-place overlay translation with AI layout evaluation

use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};

use futures::stream::{self, StreamExt};
use harumi::{
    FontHandle, LayoutRegionKind, LayoutRegionOptions, LayoutRegionRole,
    detect_text_columns, extract_layout_regions, sort_by_reading_order, Document, TextFragment,
};
use ttf_parser::Face;

use crate::{
    Error, OverflowStrategy, Result, TranslateOptions,
    extractor,
    output::{CorrectionRound, TranslateOutput, TranslateQuality, PageQualityReport},
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
    /// Right boundary of the column (from detect_text_columns). Superseded by
    /// `region_usable_right` for avail_w calculations; retained for debugging.
    #[allow(dead_code)]
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
    /// Right edge of the containing layout region's usable_rect.
    /// Falls back to col_right when no region is matched.
    pub(crate) region_usable_right: f32,
    /// Semantic role of the containing layout region (HeaderFooter, ParagraphBody, …).
    pub(crate) region_role: LayoutRegionRole,
    /// True when this line must not be covered or translated.
    /// Set in `extract_and_translate` based on `skip_header_footer` / `auto_skip_math`.
    pub(crate) is_skip: bool,
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

        // Extract semantic layout regions to get precise usable_rect per line.
        let page_height = size.1;
        let regions = extract_layout_regions(
            &visible,
            page_width,
            page_height,
            LayoutRegionOptions::default(),
        );

        let raw_lines = &all_raw_lines;
        let lines: Vec<OverlayLine> = raw_lines
            .iter()
            .enumerate()
            .map(|(i, rl)| {
                let gap_above = if i > 0 { (raw_lines[i - 1].y - rl.y).abs() } else { body_line_h };
                let gap_below = if i + 1 < raw_lines.len() { (rl.y - raw_lines[i + 1].y).abs() } else { body_line_h };

                // Find the layout region that contains this line.
                // Require both y-overlap and x-containment to avoid cross-column matches.
                let matched = regions.iter()
                    .filter(|r| {
                        let b = r.source_bbox;
                        let y_ok = rl.y >= b[1] - 2.0 && rl.y <= b[1] + b[3] + 2.0;
                        let x_ok = rl.x >= b[0] - 5.0 && rl.x <= b[0] + b[2] + 5.0;
                        y_ok && x_ok
                    })
                    .min_by(|a, b_| {
                        let ca = a.source_bbox[0] + a.source_bbox[2] / 2.0;
                        let cb = b_.source_bbox[0] + b_.source_bbox[2] / 2.0;
                        (ca - rl.x).abs().partial_cmp(&(cb - rl.x).abs())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                let region_usable_right = matched
                    .map(|r| r.usable_rect[0] + r.usable_rect[2])
                    .unwrap_or(rl.col_right)
                    .max(rl.x + 50.0);

                let region_role = matched
                    .map(|r| r.role.clone())
                    .unwrap_or(LayoutRegionRole::Unknown);

                // Region-aware heading detection supplements the heuristic.
                let region_is_heading = matched.is_some_and(|r| {
                    matches!(r.kind, LayoutRegionKind::Heading(_))
                });

                let is_heading = region_is_heading
                    || (gap_above > body_line_h * 2.2 && rl.x < page_width * 0.2)
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
                    region_usable_right,
                    region_role,
                    is_skip: false,
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
        let mid = (lo + hi).div_ceil(2);
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

/// Parse an AI correction response and return a `(page, id) → corrected_text` map.
fn parse_correction_response(raw: &str) -> Result<HashMap<(u32, usize), String>> {
    let json_str = {
        let s = raw.trim();
        let s = s.strip_prefix("```json").unwrap_or(s);
        let s = s.strip_prefix("```").unwrap_or(s);
        let s = s.strip_suffix("```").unwrap_or(s);
        s.trim()
    };
    let resp: CorrectionResponse = serde_json::from_str(json_str)
        .map_err(|e| Error::Translator(format!("AI correction JSON invalid: {e}. Raw: {json_str}")))?;
    Ok(resp.corrections.into_iter().map(|c| ((c.page, c.id), c.text)).collect())
}

/// Multi-pass AI correction loop.
///
/// Each round identifies overflow lines AND lines involved in Moderate/Major
/// collisions, sends them to the AI for shortening, and records the changes.
/// Returns the correction rounds log and a per-page collision summary.
#[allow(clippy::too_many_arguments)]
async fn run_correction_loop(
    overlay_pages: &[OverlayPage],
    page_translations: &mut HashMap<u32, Vec<String>>,
    face: &Face<'_>,
    min_fs: f32,
    global_body_fs: f32,
    translator: &Arc<dyn crate::Translator>,
    target_lang: &str,
    source_lang: Option<&str>,
    max_rounds: usize,
) -> Result<Vec<CorrectionRound>> {
    let mut rounds = Vec::new();

    for round in 1..=max_rounds {
        // Build placed boxes and reports for this iteration.
        let mut all_reports: Vec<PlacedLineReport> = Vec::new();

        // Compute placed boxes per page for collision detection.
        let mut page_boxes: HashMap<u32, Vec<harumi::PlacedBox>> = HashMap::new();
        let mut page_line_map: HashMap<u32, Vec<usize>> = HashMap::new(); // page → report indices

        for overlay_page in overlay_pages {
            let page_num = overlay_page.page_num;
            let Some(translations) = page_translations.get(&page_num) else { continue };
            let mut boxes: Vec<harumi::PlacedBox> = Vec::new();
            let mut line_indices: Vec<usize> = Vec::new();

            for (line_idx, (line, trans_text)) in overlay_page.lines.iter().zip(translations.iter()).enumerate() {
                let text = trans_text.trim();
                if text.is_empty() {
                    continue;
                }
                let max_fs = line.line_height * 0.85;
                let fs = if line.font_size > 0.0 { line.font_size } else { global_body_fs };
                let desired = if line.is_heading { (fs * 1.4).min(max_fs) } else { fs.min(max_fs) };
                let avail_w = (line.region_usable_right - line.x).max(50.0);
                let text_w = measure_text_width(text, face, desired);
                let actual_fs = fit_font_size(text, face, desired, avail_w, min_fs);
                let overflow = text_w > avail_w * 1.05 || actual_fs < desired * 0.9;

                let placed_w = text_w.min(avail_w * 1.5); // cap for collision purposes
                let placed_h = line.line_height;
                let start_report_idx = all_reports.len();
                line_indices.push(start_report_idx);
                boxes.push(harumi::PlacedBox::new([line.x, line.y, placed_w, placed_h]));

                all_reports.push(PlacedLineReport {
                    id: line_idx,
                    page: page_num,
                    original_text: line.text.clone(),
                    translated_text: text.to_owned(),
                    font_size: actual_fs,
                    text_width_pt: text_w,
                    avail_width_pt: avail_w,
                    overflow,
                });
            }
            page_boxes.insert(page_num, boxes);
            page_line_map.insert(page_num, line_indices);
        }

        // Detect and classify collisions; mark involved lines as problems.
        let mut problem_report_ids: std::collections::HashSet<(u32, usize)> = std::collections::HashSet::new();
        for (page_num, boxes) in &page_boxes {
            let collisions = harumi::detect_collisions(boxes);
            for col in &collisions {
                let area_a = boxes.get(col.index_a).map(|b| b.rect[2] * b.rect[3]).unwrap_or(0.0);
                let area_b = boxes.get(col.index_b).map(|b| b.rect[2] * b.rect[3]).unwrap_or(0.0);
                let sev = harumi::collision_severity(col.overlap_area, area_a, area_b);
                if matches!(sev, harumi::CollisionSeverity::Moderate | harumi::CollisionSeverity::Major) {
                    // Map box indices back to report indices.
                    if let Some(line_indices) = page_line_map.get(page_num) {
                        if let Some(&report_idx_a) = line_indices.get(col.index_a)
                            && report_idx_a < all_reports.len() {
                            problem_report_ids.insert((*page_num, all_reports[report_idx_a].id));
                        }
                        if let Some(&report_idx_b) = line_indices.get(col.index_b)
                            && report_idx_b < all_reports.len() {
                            problem_report_ids.insert((*page_num, all_reports[report_idx_b].id));
                        }
                    }
                }
            }
        }

        // Collect all problem lines (overflow OR collision-involved).
        let problems: Vec<&PlacedLineReport> = all_reports
            .iter()
            .filter(|r| r.overflow || problem_report_ids.contains(&(r.page, r.id)))
            .collect();

        if problems.is_empty() {
            eprintln!("[harumi-ai] Round {round}: no problems found — stopping early");
            break;
        }

        let pages_with_problems = {
            let mut pages: std::collections::HashSet<u32> = std::collections::HashSet::new();
            for r in &problems { pages.insert(r.page); }
            pages.len()
        };

        eprintln!("[harumi-ai] Round {round}: {} problems ({} overflow, {} collision) on {} pages",
            problems.len(),
            problems.iter().filter(|r| r.overflow).count(),
            problem_report_ids.len(),
            pages_with_problems,
        );

        // Ask AI to shorten problem lines.
        let prompt = layout_correction_prompt(
            target_lang,
            source_lang,
            &serde_json::to_string_pretty(&problems)
                .map_err(|e| Error::Translator(e.to_string()))?,
        );
        let raw_results = translator.translate(&[prompt], target_lang, source_lang).await?;
        let raw = raw_results.into_iter().next()
            .ok_or_else(|| Error::Translator("AI returned empty correction response".into()))?;

        let corrections = parse_correction_response(&raw)?;
        let corrections_applied = corrections.len();

        // Apply corrections.
        for ((page_num, line_id), corrected_text) in &corrections {
            if let Some(texts) = page_translations.get_mut(page_num)
                && let Some(slot) = texts.get_mut(*line_id) {
                *slot = corrected_text.clone();
            }
        }

        rounds.push(CorrectionRound {
            round,
            lines_sent_to_ai: problems.len(),
            corrections_applied,
            pages_with_problems,
        });

        if corrections_applied == 0 {
            eprintln!("[harumi-ai] Round {round}: AI returned no corrections — stopping");
            break;
        }
    }

    Ok(rounds)
}

// ── Math-detection helpers ────────────────────────────────────────────────────

fn is_math_char(c: char) -> bool {
    let cp = c as u32;
    matches!(cp,
        0x0370..=0x03FF   |  // Greek and Coptic
        0x2200..=0x22FF   |  // Mathematical Operators
        0x1D400..=0x1D7FF |  // Mathematical Alphanumeric Symbols
        0x2070..=0x2079   |  // Superscript digits
        0x2080..=0x2089   |  // Subscript digits
        0x00B2 | 0x00B3 | 0x00B9  // ², ³, ¹
    )
}

/// Returns true when a line's text is primarily math/formula content.
///
/// Short text (≤ 20 chars): requires at least 2 math chars OR > 25 % math-char
/// fraction.  This prevents "β-carotene content" (1 Greek char in 17) from being
/// flagged while still catching "H₂SO₄" (2 subscripts in 6) and "α" (1 in 1).
/// Longer text: math chars must dominate AND ordinary alphabetic prose must be
/// sparse — "The coefficient α represents…" is not flagged.
fn text_is_primarily_math(text: &str) -> bool {
    if text.is_empty() { return false; }
    let total = text.chars().count();
    let math_count = text.chars().filter(|&c| is_math_char(c)).count();
    if math_count == 0 { return false; }
    let alpha_prose = text.chars()
        .filter(|c| c.is_alphabetic() && !is_math_char(*c))
        .count();
    // Short token: require ≥2 math chars OR >25% math fraction.
    // Prevents "β-carotene content" (1/17 ≈ 6%) from being dropped.
    if total <= 20 { return math_count >= 2 || math_count * 4 >= total; }
    // Long text: require math to be the majority AND prose to be sparse.
    math_count * 2 > total && alpha_prose * 4 < total
}

// ── Shared extract + translate helper ────────────────────────────────────────

/// Phases 1 and 2 shared between Overlay and InPlace modes.
/// Returns (overlay_pages, page_translations, global_body_fs).
///
/// When `options.skip_patterns` is non-empty, matching lines are kept
/// verbatim and excluded from the AI batch.  When `options.cache` is set,
/// cached translations are also resolved before the AI call; results from
/// the AI are stored back into the cache afterwards.
///
/// The returned `page_translations` Vec is positionally aligned with
/// `overlay_pages[i].lines` — i.e. `page_translations[page_num][j]` is
/// the translation of `overlay_pages[i].lines[j]`.
pub(crate) async fn extract_and_translate(
    pdf_bytes: &[u8],
    options: &TranslateOptions,
) -> Result<(Vec<OverlayPage>, HashMap<u32, Vec<String>>, f32)> {
    // ── Phase 1: Extract positioned lines ────────────────────────────────────
    let mut doc = Document::from_bytes(pdf_bytes)?;
    let mut overlay_pages = extract_overlay_pages(&mut doc)?;
    drop(doc);

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

    // ── Pre-resolution: skip patterns + cache ────────────────────────────────
    // resolved: (page_num, line_idx) → pre-resolved translation text ("" for skip lines).
    // These entries are excluded from the AI batch.
    let mut resolved: HashMap<(u32, usize), String> = HashMap::new();
    // Track lines that must be completely untouched (no white rect, no text placement).
    let mut skip_line_set: std::collections::HashSet<(u32, usize)> = std::collections::HashSet::new();

    // 1. Skip patterns (compiled once; invalid regexes are silently ignored).
    //    Matched lines are fully skipped (is_skip=true, empty translation).
    let skip_regexes: Vec<regex::Regex> = options.skip_patterns.iter()
        .filter_map(|p| regex::Regex::new(p).ok())
        .collect();
    if !skip_regexes.is_empty() {
        for op in &overlay_pages {
            for (idx, line) in op.lines.iter().enumerate() {
                let text = line.text.trim();
                if !text.is_empty() && skip_regexes.iter().any(|re| re.is_match(text)) {
                    // Store "" so apply_overlay skips text placement.
                    resolved.insert((op.page_num, idx), String::new());
                    skip_line_set.insert((op.page_num, idx));
                }
            }
        }
    }

    // 2. Translation cache (lock briefly, not across .await).
    //    Cache keys are namespaced by target_lang to prevent cross-language hits
    //    when the same Arc<Mutex<TranslationCache>> is reused across translation calls.
    let mut cache_hits = 0usize;
    let mut cache_misses = 0usize;
    if let Some(cache_arc) = &options.cache {
        let mut cache = cache_arc.lock().await;
        for op in &overlay_pages {
            for (idx, line) in op.lines.iter().enumerate() {
                if resolved.contains_key(&(op.page_num, idx)) { continue; }
                let raw_key = line.text.trim();
                if raw_key.is_empty() { continue; }
                // Namespace by target_lang (NUL separator avoids collisions).
                let ns_key = format!("{}\x00{raw_key}", options.target_lang);
                if let Some(t) = cache.get(&ns_key) {
                    resolved.insert((op.page_num, idx), t.to_owned());
                    cache_hits += 1;
                } else {
                    cache_misses += 1;
                }
            }
        }
        // mutex released here — not held across the AI call below
    }
    if cache_hits + cache_misses > 0 {
        eprintln!(
            "[harumi-ai] Cache: {} hits, {} misses ({:.0}% saved)",
            cache_hits, cache_misses,
            cache_hits as f64 / (cache_hits + cache_misses) as f64 * 100.0
        );
    }

    // 3. is_skip: skip-pattern lines, HeaderFooter, and math lines.
    //    All marked lines get is_skip=true so apply_overlay omits white rects AND
    //    text placement, leaving the original PDF content completely untouched.
    for op in &mut overlay_pages {
        for (idx, line) in op.lines.iter_mut().enumerate() {
            // Skip-pattern lines (step 1) are already in skip_line_set.
            if skip_line_set.contains(&(op.page_num, idx)) {
                line.is_skip = true;
                continue;
            }
            // Cache-hit lines (step 2): not is_skip — they need white rect + translated text.
            if resolved.contains_key(&(op.page_num, idx)) { continue; }

            let is_header_footer_skip = options.skip_header_footer
                && line.region_role == LayoutRegionRole::HeaderFooter;
            let is_math_skip = options.auto_skip_math
                && text_is_primarily_math(line.text.trim());

            if is_math_skip {
                // Sanitize before logging: replace control chars to prevent terminal injection.
                let safe_text: String = line.text.chars().take(40)
                    .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
                    .collect();
                eprintln!("[harumi-ai] auto_skip_math: p{} {:?}", op.page_num, safe_text);
            }

            if is_header_footer_skip || is_math_skip {
                line.is_skip = true;
                resolved.insert((op.page_num, idx), String::new());
            }
        }
    }

    // ── Phase 2: Translate (only non-resolved blocks) ────────────────────────
    // Build filtered page_contents; pages where every block is resolved are
    // omitted entirely so we don't send empty batches to the AI.
    let page_contents: Vec<extractor::PageContent> = overlay_pages.iter()
        .filter_map(|op| {
            let blocks: Vec<extractor::Block> = op.lines.iter().enumerate()
                .filter(|(idx, _)| !resolved.contains_key(&(op.page_num, *idx)))
                .map(|(id, line)| extractor::Block {
                    id,
                    block_type: "paragraph".to_owned(),
                    text: line.text.clone(),
                })
                .collect();
            if blocks.is_empty() { None }
            else { Some(extractor::PageContent { page_num: op.page_num, size: (0.0, 0.0), blocks }) }
        })
        .collect();

    // ai_results: page_num → { line_idx → translated_text }
    let mut ai_results: HashMap<u32, HashMap<usize, String>> = HashMap::new();

    if !page_contents.is_empty() {
        let translator = Arc::clone(&options.translator);
        let target_lang = options.target_lang.clone();
        let source_lang = options.source_lang.clone();
        let batch_size = options.pages_per_batch;
        let batches: Vec<Vec<extractor::PageContent>> = page_contents
            .chunks(batch_size)
            .map(<[_]>::to_vec)
            .collect();
        let total_pages = overlay_pages.len() as u32;
        let done_pages = Arc::new(AtomicU32::new(0));

        let results: Vec<(u32, Vec<(usize, String)>)> = stream::iter(batches)
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

                    // Return (id, text) pairs to preserve line-index information.
                    let out: Vec<(u32, Vec<(usize, String)>)> = batch.iter()
                        .zip(page_block_lists.iter().chain(std::iter::repeat(&vec![])))
                        .map(|(orig, t_blocks)| {
                            let pairs: Vec<(usize, String)> = t_blocks.iter()
                                .filter_map(|tb| {
                                    orig.blocks.iter().find(|b| b.id == tb.id)
                                        .map(|b| (b.id, tb.text.clone()))
                                })
                                .collect();
                            (orig.page_num, pairs)
                        })
                        .collect();

                    Ok::<Vec<(u32, Vec<(usize, String)>)>, Error>(out)
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

        for (page_num, pairs) in results {
            ai_results.insert(page_num, pairs.into_iter().collect());
        }
    }

    // Store AI translations back into the cache (brief lock, not across .await).
    // Keys are namespaced by target_lang (same scheme as the lookup above).
    if let Some(cache_arc) = &options.cache {
        let mut cache = cache_arc.lock().await;
        for op in &overlay_pages {
            if let Some(id_map) = ai_results.get(&op.page_num) {
                for (id, translated) in id_map {
                    if let Some(line) = op.lines.get(*id) {
                        let raw_key = line.text.trim();
                        let ns_key = format!("{}\x00{raw_key}", options.target_lang);
                        cache.insert(ns_key, translated.clone());
                    }
                }
            }
        }
    }

    // ── Merge: build positional Vec<String> per page ─────────────────────────
    // Priority: resolved (skip/cache) → AI result → empty string (with warning).
    // Empty string means: no white rect, no text placement (original preserved).
    // Previously this fell back to original source text, which silently embedded
    // untranslated content in the output PDF when the AI dropped a block.
    let mut page_translations: HashMap<u32, Vec<String>> = HashMap::new();
    for op in &overlay_pages {
        let ai_map = ai_results.get(&op.page_num);
        let texts: Vec<String> = (0..op.lines.len())
            .map(|idx| {
                if let Some(t) = resolved.get(&(op.page_num, idx)) {
                    t.clone()
                } else if let Some(t) = ai_map.and_then(|m| m.get(&idx)) {
                    t.clone()
                } else {
                    // AI dropped this block. Use empty string so the original is
                    // left uncovered rather than silently embedding source text.
                    eprintln!(
                        "[harumi-ai] AI dropped block p{}:{idx} — original left in place",
                        op.page_num
                    );
                    String::new()
                }
            })
            .collect();
        page_translations.insert(op.page_num, texts);
    }

    Ok((overlay_pages, page_translations, global_body_fs))
}

// ── Main entry points ─────────────────────────────────────────────────────────

/// Full overlay translation returning structured [`TranslateOutput`] (v0.2.0+).
pub async fn translate_pdf_overlay_full(pdf_bytes: &[u8], options: TranslateOptions) -> Result<TranslateOutput> {
    let (overlay_pages, mut page_translations, global_body_fs) =
        extract_and_translate(pdf_bytes, &options).await?;

    let face = Face::parse(&options.font, 0)
        .map_err(|e| Error::FontParse(e.to_string()))?;

    let fallback_faces: Vec<Face<'_>> = options.font_fallbacks.iter()
        .filter_map(|b| Face::parse(b, 0).ok())
        .collect();
    let all_faces: Vec<&Face<'_>> = std::iter::once(&face)
        .chain(fallback_faces.iter())
        .collect();

    let min_fs = options.overflow.min_font_size();
    let translator = Arc::clone(&options.translator);
    let target_lang = options.target_lang.clone();
    let source_lang = options.source_lang.clone();

    // ── Phase 3: Multi-pass AI correction loop ────────────────────────────────
    let correction_rounds = if options.max_correction_rounds > 0 {
        run_correction_loop(
            &overlay_pages,
            &mut page_translations,
            &face,
            min_fs,
            global_body_fs,
            &translator,
            &target_lang,
            source_lang.as_deref(),
            options.max_correction_rounds,
        ).await?
    } else {
        vec![]
    };

    // ── Phase 4: Apply overlay to original PDF ────────────────────────────────
    let pdf_bytes_out = apply_overlay(
        pdf_bytes,
        &overlay_pages,
        &page_translations,
        &options,
        &face,
        &all_faces,
        global_body_fs,
        min_fs,
    )?;

    // ── Phase 5: Compute per-page quality summaries ───────────────────────────
    let page_reports = compute_page_quality(
        &overlay_pages,
        &page_translations,
        &face,
        global_body_fs,
        min_fs,
    );

    let debug_out = if options.debug.overlay_pdf || options.debug.correction_history {
        let debug_overlay = if options.debug.overlay_pdf {
            match build_debug_overlay_pdf(pdf_bytes, &overlay_pages, &options) {
                Ok(b) => Some(b),
                Err(e) => {
                    eprintln!("[harumi-ai] debug overlay PDF failed: {e}");
                    None
                }
            }
        } else {
            None
        };
        let history = if options.debug.correction_history { correction_rounds.clone() } else { vec![] };
        Some(crate::output::DebugArtifacts {
            layout_report_json: None,
            collision_report_json: None,
            debug_overlay_pdf: debug_overlay,
            correction_history: history,
        })
    } else {
        None
    };

    Ok(TranslateOutput {
        pdf_bytes: pdf_bytes_out,
        quality: TranslateQuality {
            pages: page_reports,
            overall: crate::quality::QualityResult::Pass, // re-evaluated in translate_pdf
            correction_rounds: correction_rounds.len(),
            mode_used: crate::TranslationMode::Overlay,
            fallback_reason: None,
        },
        debug: debug_out,
    })
}

/// Compute a simple per-page quality summary from overlay line placements.
fn compute_page_quality(
    overlay_pages: &[OverlayPage],
    page_translations: &HashMap<u32, Vec<String>>,
    face: &Face<'_>,
    global_body_fs: f32,
    min_fs: f32,
) -> Vec<PageQualityReport> {
    let mut reports = Vec::new();
    for overlay_page in overlay_pages {
        let page_num = overlay_page.page_num;
        let Some(translations) = page_translations.get(&page_num) else { continue };

        let mut boxes: Vec<harumi::PlacedBox> = Vec::new();
        let mut overflow_count = 0usize;
        let mut shrunk_count = 0usize;

        for (line, trans_text) in overlay_page.lines.iter().zip(translations.iter()) {
            let text = trans_text.trim();
            if text.is_empty() { continue; }
            let max_fs_cap = line.line_height * 0.85;
            let fs = if line.font_size > 0.0 { line.font_size } else { global_body_fs };
            let desired = if line.is_heading { (fs * 1.4).min(max_fs_cap) } else { fs.min(max_fs_cap) };
            let avail_w = (line.region_usable_right - line.x).max(50.0);
            let actual_fs = fit_font_size(text, face, desired, avail_w, min_fs);
            let text_w = measure_text_width(text, face, desired);
            if text_w > avail_w * 1.05 || actual_fs < desired * 0.9 {
                overflow_count += 1;
            }
            if actual_fs < desired * 0.99 { shrunk_count += 1; }
            let placed_w = text_w.min(avail_w * 1.5);
            boxes.push(harumi::PlacedBox::new([line.x, line.y, placed_w, line.line_height]));
        }

        let collisions = harumi::detect_collisions(&boxes);
        let worst_overlap_area = collisions.iter().map(|c| c.overlap_area).fold(0.0_f32, f32::max);
        let worst_overlap_rect = collisions
            .iter()
            .max_by(|a, b| a.overlap_area.partial_cmp(&b.overlap_area).unwrap_or(std::cmp::Ordering::Equal))
            .map(|c| c.overlap_rect);

        // PageFitSummary is #[non_exhaustive]; construct via from_plans and then
        // overwrite the fields with our overlay-computed values.
        let mut summary = harumi::PageFitSummary::from_plans(&[]);
        summary.overflow_count = overflow_count;
        summary.collision_count = collisions.len();
        summary.shrunk_count = shrunk_count;
        summary.worst_overlap_area = worst_overlap_area;
        summary.worst_overlap_rect = worst_overlap_rect;
        reports.push(PageQualityReport { page_num, summary });
    }
    reports
}

/// Generate a debug overlay PDF with colored boxes for source, placed, and collision rects.
fn build_debug_overlay_pdf(
    pdf_bytes: &[u8],
    overlay_pages: &[OverlayPage],
    _options: &TranslateOptions,
) -> Result<Vec<u8>> {
    use harumi::{PlacedBox, detect_collisions};

    let mut doc = harumi::Document::from_bytes(pdf_bytes)?;
    let blue: harumi::Color = harumi::Color::Rgb([0.2, 0.6, 1.0]);
    let red: harumi::Color = harumi::Color::Rgb([1.0, 0.1, 0.1]);

    for overlay_page in overlay_pages {
        let page_num = overlay_page.page_num;
        let mut page = doc.page(page_num)?;
        // Draw source bbox (blue outlines).
        for line in &overlay_page.lines {
            let r = [line.x, line.y, line.right - line.x, line.line_height];
            if r[2] > 0.0 && r[3] > 0.0 {
                page.add_rect_stroke(r, blue, 0.5, 1.0)?;
            }
        }
        // Compute placed boxes and draw collision rects.
        let boxes: Vec<PlacedBox> = overlay_page.lines.iter().map(|line| {
            PlacedBox::new([line.x, line.y, (line.right - line.x).max(1.0), line.line_height])
        }).collect();
        let collisions = detect_collisions(&boxes);
        let mut seen = std::collections::HashSet::new();
        for col in &collisions {
            let key = (col.index_a, col.index_b);
            if seen.insert(key) {
                let r = col.overlap_rect;
                if r[2] > 0.0 && r[3] > 0.0 {
                    page.add_rect_stroke(r, red, 1.0, 1.0)?;
                }
            }
        }
    }
    doc.save_to_bytes().map_err(Into::into)
}

/// Phase 4 helper: apply the overlay to the original PDF and return PDF bytes.
#[allow(clippy::too_many_arguments)]
fn apply_overlay(
    pdf_bytes: &[u8],
    overlay_pages: &[OverlayPage],
    page_translations: &HashMap<u32, Vec<String>>,
    options: &TranslateOptions,
    face: &Face<'_>,
    all_faces: &[&Face<'_>],
    global_body_fs: f32,
    min_fs: f32,
) -> Result<Vec<u8>> {

    // ── Phase 4: Apply overlay to original PDF ────────────────────────────────
    let mut doc = Document::from_bytes(pdf_bytes)?;
    // Embed primary font; fallback fonts are embedded lazily on first use.
    let primary_font = doc.embed_font(&options.font)?;
    let mut font_handles: Vec<Option<FontHandle>> =
        std::iter::once(Some(primary_font))
            .chain(std::iter::repeat_n(None, options.font_fallbacks.len()))
            .collect();

    let cover_color = options.cover_color.unwrap_or([1.0, 1.0, 1.0]);

    // Compute descender depth from the actual font (once, reused per line).
    let descender_ratio = (-face.descender() as f32 / face.units_per_em() as f32)
        .clamp(0.05, 0.35);

    for overlay_page in overlay_pages {
        let page_num = overlay_page.page_num;
        let translated_texts = page_translations.get(&page_num);

        // First pass: cover rectangles over original text.
        // Lines with is_skip = true are left untouched (no coverage, no translation).
        for &rect in &overlay_page.invisible_rects {
            doc.page(page_num)?.add_rect(rect, cover_color, 1.0)?;
        }
        for line in &overlay_page.lines {
            if line.is_skip { continue; }
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
                // Use region_usable_right (from extract_layout_regions) for a more
                // precise column-right boundary than the heuristic col_right.
                let avail_w = (line.region_usable_right - line.x).max(50.0);
                let scaled = fit_font_size(text, face, desired, avail_w, min_fs).max(desired * 0.95);
                // Apply overflow strategy: truncate if still too wide.
                let display_text: std::borrow::Cow<str> = match &options.overflow {
                    OverflowStrategy::Truncate { .. }
                        if measure_text_width(text, face, scaled) > avail_w * 1.05 =>
                    {
                        truncate_to_fit(text, face, scaled, avail_w).into()
                    }
                    _ => text.into(),
                };
                // Synthetic bold for headings and originally-bold lines.
                let bold = line.is_heading || line.is_bold;

                // Split text into font-specific runs and render each sub-run.
                let runs = split_by_font(&display_text, all_faces);
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

// ── Bilingual mode ────────────────────────────────────────────────────────────

/// Produce a bilingual PDF where each original page is followed by its
/// translated version.
///
/// Output page order: `[orig_1, trans_1, orig_2, trans_2, …, orig_n, trans_n]`.
pub async fn translate_pdf_bilingual_full(
    pdf_bytes: &[u8],
    options: TranslateOptions,
) -> Result<crate::output::TranslateOutput> {
    use crate::output::{TranslateOutput, TranslateQuality};
    use crate::pdf_translator::TranslationMode;

    // Step 1: Translate with Overlay to get translated bytes + quality data.
    let mut overlay_opts = options;
    overlay_opts.mode = TranslationMode::Overlay;
    let translated = translate_pdf_overlay_full(pdf_bytes, overlay_opts).await?;

    // Step 2: Load original and translated documents, then interleave pages.
    // After merge_from: pages 1..=n are original, pages n+1..=2n are translated.
    let mut combined = Document::from_bytes(pdf_bytes)?;
    let trans_doc = Document::from_bytes(&translated.pdf_bytes)?;
    let n = combined.page_count();

    combined.merge_from(trans_doc)?;

    // Reorder to [orig_1, trans_1, orig_2, trans_2, ..., orig_n, trans_n].
    let order: Vec<u32> = (1..=n).flat_map(|i| [i, i + n]).collect();
    combined.reorder_pages(&order)?;

    Ok(TranslateOutput {
        pdf_bytes: combined.save_to_bytes()?,
        quality: TranslateQuality {
            pages: translated.quality.pages,
            overall: translated.quality.overall,
            correction_rounds: translated.quality.correction_rounds,
            mode_used: TranslationMode::Bilingual,
            fallback_reason: None,
        },
        debug: None,
    })
}
