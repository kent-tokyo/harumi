use harumi::Document;
use wasm_bindgen::prelude::*;

/// Stamp visible text on every page of a PDF.
///
/// Returns the modified PDF bytes.
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

/// Add an invisible OCR text layer to every page of a PDF.
///
/// Returns the modified PDF bytes.
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

/// Return the page count of a PDF.
#[wasm_bindgen]
pub fn page_count(pdf: &[u8]) -> Result<u32, JsValue> {
    let doc = Document::from_bytes(pdf).map_err(err)?;
    Ok(doc.page_count())
}

fn err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}
