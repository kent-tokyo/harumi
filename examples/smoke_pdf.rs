//! Smoke-test harumi against a real-world PDF file.
//!
//! Usage:
//!   cargo run --example smoke_pdf -- <input.pdf> <font.ttf> [output_dir]
//!
//! What it tests:
//!   1. Load the PDF and print basic metadata.
//!   2. Overlay invisible text on every page.
//!   3. Overlay a visible "TESTED" stamp on page 1.
//!   4. Query page.size() for all pages.
//!   5. Save and reload; verify page count is preserved.
//!
//! Exit code 0 = all checks passed.

use harumi::{Document, TextRun};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <input.pdf> <font.ttf> [output_dir]", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let font_path = &args[2];
    let output_dir = args.get(3).map(|s| s.as_str()).unwrap_or(".");

    println!("=== harumi smoke test ===");
    println!("Input:  {}", input_path);
    println!("Font:   {}", font_path);

    // --- 1. Load ---
    let mut doc = Document::from_file(input_path)?;
    let n = doc.page_count();
    println!("Pages:  {}", n);
    assert!(n > 0, "document has no pages");

    // --- 2. Font embed ---
    let font_bytes = std::fs::read(font_path)?;
    let font = doc.embed_font(&font_bytes)?;
    println!("Font registered (handle {:?})", font);

    // --- 3. Query page sizes ---
    println!("\nPage sizes:");
    for i in 1..=n {
        match doc.page(i)?.size() {
            Ok((w, h)) => println!("  page {}: {:.1} × {:.1} pt", i, w, h),
            Err(e) => println!("  page {}: size() error: {} (inherited MediaBox?)", i, e),
        }
    }

    // --- 4. Invisible text on every page ---
    for i in 1..=n {
        let text = format!("harumi smoke test — page {}", i);
        doc.page(i)?.add_invisible_text(&text, font, [72.0, 60.0], 10.0)?;
    }

    // --- 5. Visible stamp on page 1 ---
    let stamp_text = "TESTED";
    if let Ok((w, h)) = doc.page(1)?.size() {
        doc.page(1)?.add_text(stamp_text, font, [w / 2.0 - 30.0, h / 2.0], 28.0, [0.0, 0.5, 0.0])?;
        println!("\nStamp '{}' placed at ({:.0}, {:.0})", stamp_text, w / 2.0 - 30.0, h / 2.0);
    } else {
        doc.page(1)?.add_text(stamp_text, font, [200.0, 400.0], 28.0, [0.0, 0.5, 0.0])?;
        println!("\nStamp '{}' placed at (200, 400) — size() unavailable", stamp_text);
    }

    // --- 6. Batch run on page 1 ---
    doc.page(1)?.add_invisible_text_runs(&[
        TextRun { text: "バッチテスト行1".into(), font, x: 72.0, y: 40.0, font_size: 10.0, render_mode: 3, color: [0.0; 3] },
        TextRun { text: "batch test line 2".into(), font, x: 72.0, y: 26.0, font_size: 10.0, render_mode: 3, color: [0.0; 3] },
    ])?;

    // --- 7. Save ---
    let stem = std::path::Path::new(input_path)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let output_path = format!("{}/{}_harumi_smoke.pdf", output_dir, stem);

    doc.save(&output_path)?;
    println!("\nSaved: {}", output_path);

    // --- 8. Reload and verify ---
    let reloaded = Document::from_file(&output_path)?;
    assert_eq!(reloaded.page_count(), n, "page count must be preserved");
    println!("Reload OK: {} pages", reloaded.page_count());

    println!("\nAll checks passed.");
    Ok(())
}
