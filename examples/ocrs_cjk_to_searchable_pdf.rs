//! Convert ocrs-cjk JSON output into a searchable PDF using harumi.
//!
//! **Pipeline:**
//! ```text
//! scanned.pdf  ──►  ocrs-cjk --json  ──►  ocr.json
//!                                              │
//!              harumi (this example)  ◄────────┘
//!                        │
//!                        ▼
//!               searchable.pdf  (original image preserved, invisible text layer added)
//! ```
//!
//! Usage:
//! ```bash
//! cargo run --example ocrs_cjk_to_searchable_pdf -- \
//!   scanned.pdf ocr.json NotoSansCJKjp-Regular.ttf output.pdf
//! ```
//!
//! Try with the bundled fixture:
//! ```bash
//! cargo run --example ocrs_cjk_to_searchable_pdf -- \
//!   examples/fixtures/scanned_sample.pdf \
//!   examples/fixtures/ocrs_sample.json \
//!   /path/to/NotoSansCJKjp-Regular.ttf \
//!   searchable.pdf
//! ```
//!
//! The output PDF looks identical to the input but the recognized CJK text is
//! selectable and searchable in any PDF viewer (Acrobat, Preview, Chrome).
//! The original page is **not rasterized** — harumi uses append-only PDF editing.

use harumi::Document;
use serde::Deserialize;

// ── ocrs-cjk JSON schema ─────────────────────────────────────────────────────
// Matches the HierText-format JSON produced by `ocrs --json`.

#[derive(Deserialize)]
struct OcrsOutput {
    image_width: u32,
    image_height: u32,
    paragraphs: Vec<Paragraph>,
}

#[derive(Deserialize)]
struct Paragraph {
    lines: Vec<Line>,
}

#[derive(Deserialize)]
struct Line {
    words: Vec<Word>,
}

#[derive(Deserialize)]
struct Word {
    text: String,
    confidence: f32,
    /// 4 corners in pixel coordinates:
    /// index 0 = bottom-right, 1 = bottom-left, 2 = top-left, 3 = top-right.
    vertices: Vec<[u32; 2]>,
}

// ── main ─────────────────────────────────────────────────────────────────────

/// Minimum OCR confidence to embed. Words below this are skipped.
const MIN_CONFIDENCE: f32 = 0.50;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        eprintln!(
            "Usage: {} <input.pdf> <ocr.json> <font.ttf> <output.pdf>",
            args[0]
        );
        eprintln!();
        eprintln!("  input.pdf  — scanned PDF (any viewer-compatible PDF)");
        eprintln!("  ocr.json   — JSON produced by `ocrs --json` (ocrs-cjk)");
        eprintln!("  font.ttf   — CJK TTF/OTF (e.g. NotoSansCJKjp-Regular.ttf)");
        eprintln!("  output.pdf — searchable PDF output");
        std::process::exit(1);
    }
    let (input_pdf, ocr_json, font_ttf, output_pdf) =
        (&args[1], &args[2], &args[3], &args[4]);

    // 1. Parse ocrs-cjk JSON (one JSON file = one image = one PDF page).
    let json_bytes = std::fs::read(ocr_json)?;
    let ocrs: OcrsOutput = serde_json::from_slice(&json_bytes)?;

    if ocrs.image_width == 0 || ocrs.image_height == 0 {
        eprintln!(
            "Warning: image dimensions are 0 in {ocr_json}. \
             Run ocrs on a PNG/JPEG image (not a PDF) to get accurate dimensions."
        );
    }

    // 2. Load the PDF and embed the CJK font.
    //    embed_font() stores raw bytes only; subsetting and CMap generation
    //    happen at save() time (harumi's deferred-subsetting design).
    let mut doc = Document::from_file(input_pdf)?;
    let font_bytes = std::fs::read(font_ttf)?;
    let font = doc.embed_font(&font_bytes)?;

    // 3. Map pixel coordinates to PDF points.
    //    ocrs-cjk works in pixel space (top-left origin, Y increases downward).
    //    PDF coordinate space has bottom-left origin, Y increases upward.
    let (page_w_pt, page_h_pt) = doc.page(1)?.size()?;
    let scale_x = if ocrs.image_width > 0 {
        page_w_pt / ocrs.image_width as f32
    } else {
        1.0
    };
    let scale_y = if ocrs.image_height > 0 {
        page_h_pt / ocrs.image_height as f32
    } else {
        1.0
    };

    let mut placed = 0usize;
    let mut skipped = 0usize;

    for para in &ocrs.paragraphs {
        for line in &para.lines {
            for word in &line.words {
                if word.confidence < MIN_CONFIDENCE {
                    skipped += 1;
                    continue;
                }
                if word.text.trim().is_empty() || word.vertices.len() < 4 {
                    continue;
                }

                // Vertex layout: [0] bottom-right, [1] bottom-left,
                //                [2] top-left,     [3] top-right.
                let [x_tl_px, y_tl_px] = word.vertices[2]; // top-left in image coords
                let [_, y_bl_px] = word.vertices[1]; // bottom-left in image coords

                // Text X baseline: left edge of the word.
                let pdf_x = x_tl_px as f32 * scale_x;

                // Text Y baseline: bottom of the glyph box, flipped to PDF space.
                let pdf_y = page_h_pt - (y_bl_px as f32 * scale_y);

                // Font size: height of the bounding box in PDF points.
                let font_size =
                    ((y_bl_px as f32 - y_tl_px as f32) * scale_y).max(4.0);

                doc.page(1)?.add_invisible_text(
                    &word.text,
                    font,
                    [pdf_x, pdf_y],
                    font_size,
                )?;
                placed += 1;
            }
        }
    }

    // 4. Save — the original PDF structure is untouched (harumi is append-only).
    doc.save(output_pdf)?;
    println!(
        "Saved: {output_pdf}  \
         ({placed} words embedded, {skipped} skipped below {:.0}% confidence)",
        MIN_CONFIDENCE * 100.0
    );
    println!(
        "Press Cmd+A / Ctrl+A in a PDF viewer to verify the invisible text layer."
    );
    Ok(())
}
