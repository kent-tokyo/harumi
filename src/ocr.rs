//! Helpers for converting OCR engine output coordinates to PDF coordinates.
//!
//! Most OCR engines (Tesseract, hOCR, etc.) use a pixel coordinate system
//! with the **origin at the top-left** of the image. PDF uses an origin at
//! the **bottom-left** of the page, measured in points (1 pt = 1/72 inch).
//!
//! Enable with:
//! ```toml
//! harumi = { version = "0.1", features = ["ocr"] }
//! ```

/// Converts a pixel Y coordinate (top-left origin) to a PDF Y coordinate
/// (bottom-left origin).
///
/// # Arguments
/// * `pixel_y`        – Y coordinate from the OCR engine, in pixels from the top.
/// * `page_height_pt` – Height of the PDF page in points (e.g. 842.0 for A4).
/// * `image_dpi`      – DPI of the scanned image (e.g. 300.0).
///
/// # Example
/// ```
/// # #[cfg(feature = "ocr")]
/// let pdf_y = harumi::ocr::hocr_y_to_pdf(1200.0, 842.0, 300.0);
/// ```
pub fn hocr_y_to_pdf(pixel_y: f32, page_height_pt: f32, image_dpi: f32) -> f32 {
    let pt_per_px = 72.0 / image_dpi;
    page_height_pt - (pixel_y * pt_per_px)
}

/// Converts a pixel X coordinate to a PDF X coordinate (same origin, just unit change).
pub fn hocr_x_to_pdf(pixel_x: f32, image_dpi: f32) -> f32 {
    pixel_x * 72.0 / image_dpi
}

/// Converts a pixel font size (line height) to PDF points.
pub fn pixel_size_to_pt(pixel_size: f32, image_dpi: f32) -> f32 {
    pixel_size * 72.0 / image_dpi
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn y_conversion_top_of_page() {
        // pixel_y = 0 (top of image) → pdf_y = page_height (top in PDF coords)
        let pdf_y = hocr_y_to_pdf(0.0, 842.0, 72.0);
        assert!((pdf_y - 842.0).abs() < 0.01);
    }

    #[test]
    fn y_conversion_bottom_of_page() {
        // pixel_y = page_height_in_pixels → pdf_y ≈ 0
        let dpi = 72.0_f32;
        let page_h_pt = 842.0_f32;
        let page_h_px = page_h_pt; // at 72 dpi, 1px = 1pt
        let pdf_y = hocr_y_to_pdf(page_h_px, page_h_pt, dpi);
        assert!(pdf_y.abs() < 0.01);
    }

    #[test]
    fn x_conversion_300dpi() {
        // 300px at 300 dpi = 72pt = 1 inch
        let pdf_x = hocr_x_to_pdf(300.0, 300.0);
        assert!((pdf_x - 72.0).abs() < 0.01);
    }
}
