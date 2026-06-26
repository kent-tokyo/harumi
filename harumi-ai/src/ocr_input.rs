// ocr_input.rs — OCR JSON → OcrRegion conversion (HierText / ocrs-cjk format)

use crate::{Error, Result};
use serde::Deserialize;

/// Minimum per-word confidence to include a word in a region.
const MIN_CONFIDENCE: f32 = 0.50;

// ── JSON schema (matches ocrs-cjk --json / HierText) ─────────────────────────

#[derive(Deserialize)]
pub(crate) struct OcrsRoot {
    pub image_width: u32,
    pub image_height: u32,
    pub paragraphs: Vec<OcrsParagraph>,
}

#[derive(Deserialize)]
pub(crate) struct OcrsParagraph {
    pub lines: Vec<OcrsLine>,
}

#[derive(Deserialize)]
pub(crate) struct OcrsLine {
    pub words: Vec<OcrsWord>,
}

#[derive(Deserialize)]
pub(crate) struct OcrsWord {
    pub text: String,
    pub confidence: f32,
    /// 4 corners: [bottom-right, bottom-left, top-left, top-right] (pixel coords).
    pub vertices: Vec<[u32; 2]>,
}

// ── Output type ───────────────────────────────────────────────────────────────

/// A paragraph-level text region derived from OCR JSON output.
///
/// Coordinates are in PDF points (`[x, y, width, height]`, origin bottom-left).
#[derive(Debug, Clone)]
pub struct OcrRegion {
    /// Concatenated text of all words in this paragraph line, space-separated.
    pub text: String,
    /// Bounding box in PDF points: `[x, y, width, height]`.
    pub bbox: [f32; 4],
    /// Mean word-level confidence (0.0–1.0).
    pub confidence: f32,
}

// ── Conversion ────────────────────────────────────────────────────────────────

/// Parse ocrs-cjk / HierText JSON and convert pixel coordinates to PDF points.
///
/// - Groups at the paragraph *line* level (each `paragraphs[i].lines[j]` → one region).
/// - Words below [`MIN_CONFIDENCE`] are excluded from both text and bbox.
/// - Lines whose surviving text is empty are dropped.
///
/// `page_w` and `page_h` are the PDF page dimensions in points (from `page.size()`).
pub fn ocr_json_to_regions(json: &[u8], page_w: f32, page_h: f32) -> Result<Vec<OcrRegion>> {
    let root: OcrsRoot = serde_json::from_slice(json)
        .map_err(|e| Error::Translator(format!("OCR JSON parse error: {e}")))?;

    let (img_w, img_h) = (root.image_width, root.image_height);
    if img_w == 0 || img_h == 0 {
        return Err(Error::Translator(
            "OCR JSON: image_width / image_height must be non-zero".into(),
        ));
    }

    let scale_x = page_w / img_w as f32;
    let scale_y = page_h / img_h as f32;

    let mut regions = Vec::new();

    for para in &root.paragraphs {
        for line in &para.lines {
            let mut words_text = Vec::new();
            let mut conf_sum = 0.0f32;
            let mut conf_count = 0u32;

            // Axis-aligned bbox across all qualifying words in this line.
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX; // image coords (top = small y)
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN; // image coords (bottom = large y)

            for word in &line.words {
                if word.confidence < MIN_CONFIDENCE
                    || word.text.trim().is_empty()
                    || word.vertices.len() < 4
                {
                    continue;
                }

                words_text.push(word.text.as_str());
                conf_sum += word.confidence;
                conf_count += 1;

                for [x, y] in &word.vertices {
                    let xf = *x as f32;
                    let yf = *y as f32;
                    if xf < min_x { min_x = xf; }
                    if xf > max_x { max_x = xf; }
                    if yf < min_y { min_y = yf; }
                    if yf > max_y { max_y = yf; }
                }
            }

            if words_text.is_empty() {
                continue;
            }

            let text = words_text.join(" ");
            let confidence = conf_sum / conf_count as f32;

            // Convert from image coords (top-left, Y down) to PDF coords (bottom-left, Y up).
            let pdf_x = min_x * scale_x;
            let pdf_y = page_h - max_y * scale_y; // bottom of line in PDF space
            let pdf_w = (max_x - min_x) * scale_x;
            let pdf_h = (max_y - min_y) * scale_y;

            regions.push(OcrRegion {
                text,
                bbox: [pdf_x, pdf_y, pdf_w.max(1.0), pdf_h.max(4.0)],
                confidence,
            });
        }
    }

    Ok(regions)
}
