//! Generates examples/fixtures/scanned_sample.pdf — a minimal A4 PDF that
//! simulates a scanned page, for use with the ocrs_cjk_to_searchable_pdf example.
//!
//! Usage:
//!   cargo run --example gen_ocrs_fixture

use harumi::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("examples/fixtures")?;

    let mut doc = Document::new((595.28, 841.89))?; // A4 in points

    // Add a second page so the fixture exercises multi-page awareness.
    doc.insert_blank_page(2, (595.28, 841.89))?;

    doc.save("examples/fixtures/scanned_sample.pdf")?;
    println!("Written: examples/fixtures/scanned_sample.pdf");
    Ok(())
}
