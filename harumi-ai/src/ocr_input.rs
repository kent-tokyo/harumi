// ocr_input.rs — OCR JSON → OcrRegion conversion (HierText / ocrs-cjk format)

use crate::{Error, Result};
use serde::Deserialize;
use std::collections::HashSet;

/// Minimum per-word confidence to include a word in a region.
const MIN_CONFIDENCE: f32 = 0.50;

// ── JSON schema (matches ocrs-cjk --json / HierText) ─────────────────────────

#[derive(Deserialize)]
pub(crate) struct OcrsRoot {
    /// Optional 1-based destination PDF page.  When absent in a multi-page payload,
    /// the object's array position determines the page.
    #[serde(default, alias = "page_num")]
    pub page: Option<u32>,
    pub image_width: u32,
    pub image_height: u32,
    pub paragraphs: Vec<OcrsParagraph>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OcrsInput {
    Single(OcrsRoot),
    Document { pages: Vec<OcrsRoot> },
    PageList(Vec<OcrsRoot>),
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

    root_to_regions(&root, page_w, page_h)
}

/// Parse either a legacy single-page OCR object, a `{"pages": [...]}` document,
/// or an array of page objects and map every OCR page to a PDF page.
///
/// Each page object may contain an explicit 1-based `page`/`page_num`; otherwise
/// its position in the payload is used. Duplicate and out-of-range pages are rejected.
pub fn ocr_json_to_page_regions(
    json: &[u8],
    page_sizes: &[(f32, f32)],
) -> Result<Vec<(u32, Vec<OcrRegion>)>> {
    let input: OcrsInput = serde_json::from_slice(json)
        .map_err(|e| Error::Translator(format!("OCR JSON parse error: {e}")))?;
    let roots = match input {
        OcrsInput::Single(root) => vec![root],
        OcrsInput::Document { pages } | OcrsInput::PageList(pages) => pages,
    };

    let mut seen = HashSet::new();
    let mut result = Vec::with_capacity(roots.len());
    for (index, root) in roots.iter().enumerate() {
        let page = root.page.unwrap_or(index as u32 + 1);
        if page == 0 || page as usize > page_sizes.len() {
            return Err(Error::Translator(format!(
                "OCR JSON page {page} is outside PDF page range 1..={}",
                page_sizes.len()
            )));
        }
        if !seen.insert(page) {
            return Err(Error::Translator(format!(
                "OCR JSON contains duplicate page {page}"
            )));
        }
        let (page_w, page_h) = page_sizes[page as usize - 1];
        result.push((page, root_to_regions(root, page_w, page_h)?));
    }
    result.sort_by_key(|(page, _)| *page);
    Ok(result)
}

fn root_to_regions(root: &OcrsRoot, page_w: f32, page_h: f32) -> Result<Vec<OcrRegion>> {
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
                    if xf < min_x {
                        min_x = xf;
                    }
                    if xf > max_x {
                        max_x = xf;
                    }
                    if yf < min_y {
                        min_y = yf;
                    }
                    if yf > max_y {
                        max_y = yf;
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn page_json(page: Option<u32>, text: &str) -> serde_json::Value {
        let mut value = serde_json::json!({
            "image_width": 100,
            "image_height": 200,
            "paragraphs": [{
                "lines": [{
                    "words": [{
                        "text": text,
                        "confidence": 0.9,
                        "vertices": [[10, 20], [30, 20], [30, 40], [10, 40]]
                    }]
                }]
            }]
        });
        if let Some(page) = page {
            value["page"] = serde_json::json!(page);
        }
        value
    }

    #[test]
    fn multipage_envelope_maps_explicit_pages() {
        let json = serde_json::json!({
            "pages": [page_json(Some(2), "two"), page_json(Some(1), "one")]
        });
        let pages = ocr_json_to_page_regions(
            json.to_string().as_bytes(),
            &[(100.0, 200.0), (200.0, 400.0)],
        )
        .unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].0, 1);
        assert_eq!(pages[0].1[0].text, "one");
        assert_eq!(pages[1].0, 2);
        assert_eq!(pages[1].1[0].text, "two");
        assert_eq!(pages[1].1[0].bbox, [20.0, 320.0, 40.0, 40.0]);
    }

    #[test]
    fn multipage_array_uses_array_order() {
        let json = serde_json::json!([page_json(None, "one"), page_json(None, "two")]);
        let pages = ocr_json_to_page_regions(
            json.to_string().as_bytes(),
            &[(100.0, 200.0), (100.0, 200.0)],
        )
        .unwrap();
        assert_eq!(pages.iter().map(|p| p.0).collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn multipage_rejects_duplicate_and_out_of_range_pages() {
        let duplicate = serde_json::json!({
            "pages": [page_json(Some(1), "one"), page_json(Some(1), "again")]
        });
        assert!(
            ocr_json_to_page_regions(
                duplicate.to_string().as_bytes(),
                &[(100.0, 200.0), (100.0, 200.0)],
            )
            .unwrap_err()
            .to_string()
            .contains("duplicate page 1")
        );

        let out_of_range = serde_json::json!({"pages": [page_json(Some(3), "three")]});
        assert!(
            ocr_json_to_page_regions(
                out_of_range.to_string().as_bytes(),
                &[(100.0, 200.0), (100.0, 200.0)],
            )
            .unwrap_err()
            .to_string()
            .contains("outside PDF page range")
        );
    }
}
