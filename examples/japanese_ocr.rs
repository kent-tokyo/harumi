//! End-to-end example: inject invisible Japanese OCR text into a scanned PDF.
//!
//! Usage:
//!   cargo run --example japanese_ocr -- scanned.pdf NotoSansCJKjp-Regular.ttf output.pdf
//!
//! The output PDF looks identical to the input but text is selectable and
//! searchable in any PDF viewer.

use harumi::{Document, TextRun};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: {} <input.pdf> <font.ttf> <output.pdf>", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let font_path = &args[2];
    let output_path = &args[3];

    // 1. Load existing PDF (scanned image, no text layer).
    let mut doc = Document::from_file(input_path)?;
    println!("Loaded: {} ({} page(s))", input_path, doc.page_count());

    // 2. Embed font — subsetting and CMap generation happen automatically at save().
    let font_bytes = std::fs::read(font_path)?;
    let font = doc.embed_font(&font_bytes)?;

    // 3. Overlay invisible OCR text on page 1.
    //    In a real pipeline, coordinates and text come from Tesseract / hOCR output.
    //    Use `harumi::ocr::hocr_y_to_pdf()` to convert pixel coords if needed.
    doc.page(1)?.add_invisible_text_runs(&[
        TextRun {
            text: "晴海ライブラリ".into(),
            font,
            x: 72.0,
            y: 750.0,
            font_size: 14.0,
            render_mode: 3,
            color: harumi::Color::Rgb([0.0; 3]),
        },
        TextRun {
            text: "日本語のPDF検索".into(),
            font,
            x: 72.0,
            y: 720.0,
            font_size: 12.0,
            render_mode: 3,
            color: harumi::Color::Rgb([0.0; 3]),
        },
        TextRun {
            text: "純Rust製・CJK対応".into(),
            font,
            x: 72.0,
            y: 690.0,
            font_size: 12.0,
            render_mode: 3,
            color: harumi::Color::Rgb([0.0; 3]),
        },
    ])?;

    // 4. Save — the original PDF structure is preserved.
    doc.save(output_path)?;
    println!("Saved:  {}", output_path);
    println!("Open the output in Preview or Acrobat and press Cmd+A to select the text.");

    Ok(())
}
