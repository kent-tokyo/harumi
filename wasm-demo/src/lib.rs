use harumi::Document;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn stamp_pdf(pdf: &[u8], font: &[u8], text: &str) -> Result<Vec<u8>, JsValue> {
    let mut doc = Document::from_bytes(pdf).map_err(err)?;
    let fh = doc.embed_font(font).map_err(err)?;

    for p in 1..=doc.page_count() {
        let pos = doc
            .page(p)
            .map_err(err)?
            .size()
            .ok()
            .map(|(w, h)| [w / 2.0 - 40.0, h / 2.0])
            .unwrap_or([200.0, 400.0]);

        doc.page(p)
            .map_err(err)?
            .add_text(text, fh, pos, 24.0, [0.8, 0.0, 0.0])
            .map_err(err)?;
    }

    doc.save_to_bytes().map_err(err)
}

#[wasm_bindgen]
pub fn ocr_layer(pdf: &[u8], font: &[u8], text: &str, x: f32, y: f32) -> Result<Vec<u8>, JsValue> {
    let mut doc = Document::from_bytes(pdf).map_err(err)?;
    let fh = doc.embed_font(font).map_err(err)?;

    for p in 1..=doc.page_count() {
        doc.page(p)
            .map_err(err)?
            .add_invisible_text(text, fh, [x, y], 12.0)
            .map_err(err)?;
    }

    doc.save_to_bytes().map_err(err)
}

#[wasm_bindgen]
pub fn page_count(pdf: &[u8]) -> Result<u32, JsValue> {
    let doc = Document::from_bytes(pdf).map_err(err)?;
    Ok(doc.page_count())
}

/// Apply a list of annotations to a PDF.
///
/// `annotations_json` is a JSON array of annotation objects:
/// - `{"type":"text",  "page":1, "x":100, "y":200, "text":"Hello", "size":14, "r":1,"g":0,"b":0}`
/// - `{"type":"rect",  "page":1, "x":50,  "y":100, "w":200, "h":80, "r":1,"g":1,"b":0, "opacity":0.3}`
/// - `{"type":"line",  "page":1, "x1":10, "y1":20, "x2":300, "y2":20, "r":0,"g":0,"b":1, "width":2, "opacity":1}`
///
/// All coordinates are in PDF points (origin: bottom-left).
/// Pages are 1-indexed.
#[wasm_bindgen]
pub fn apply_annotations(pdf: &[u8], font: &[u8], annotations_json: &str) -> Result<Vec<u8>, JsValue> {
    let annotations: Vec<Annotation> =
        serde_json::from_str(annotations_json).map_err(|e| err(e))?;

    if annotations.is_empty() {
        return Ok(pdf.to_vec());
    }

    let mut doc = Document::from_bytes(pdf).map_err(err)?;
    let needs_font = annotations.iter().any(|a| matches!(a, Annotation::Text { .. }));
    let fh = if needs_font {
        Some(doc.embed_font(font).map_err(err)?)
    } else {
        None
    };

    for ann in &annotations {
        match ann {
            Annotation::Text { page, x, y, text, size, r, g, b } => {
                let fh = fh.unwrap();
                doc.page(*page)
                    .map_err(err)?
                    .add_text(text, fh, [*x, *y], *size, [*r, *g, *b])
                    .map_err(err)?;
            }
            Annotation::Rect { page, x, y, w, h, r, g, b, opacity } => {
                doc.page(*page)
                    .map_err(err)?
                    .add_rect([*x, *y, *w, *h], [*r, *g, *b], *opacity)
                    .map_err(err)?;
            }
            Annotation::Line { page, x1, y1, x2, y2, r, g, b, width, opacity } => {
                doc.page(*page)
                    .map_err(err)?
                    .add_line([*x1, *y1], [*x2, *y2], [*r, *g, *b], *width, *opacity)
                    .map_err(err)?;
            }
            Annotation::Pen { page, points, r, g, b, width, opacity } => {
                doc.page(*page)
                    .map_err(err)?
                    .add_polyline(points, [*r, *g, *b], *width, *opacity)
                    .map_err(err)?;
            }
        }
    }

    doc.save_to_bytes().map_err(err)
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Annotation {
    Text {
        page: u32,
        x: f32,
        y: f32,
        text: String,
        size: f32,
        r: f32,
        g: f32,
        b: f32,
    },
    Rect {
        page: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        g: f32,
        b: f32,
        opacity: f32,
    },
    Line {
        page: u32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        r: f32,
        g: f32,
        b: f32,
        width: f32,
        opacity: f32,
    },
    Pen {
        page: u32,
        points: Vec<[f32; 2]>,
        r: f32,
        g: f32,
        b: f32,
        width: f32,
        opacity: f32,
    },
}

fn err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}
