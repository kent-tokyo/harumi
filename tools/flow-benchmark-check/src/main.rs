use std::{env, fs, time::Instant};

use harumi::{Document, FlowDocument, FlowOptions};

fn usage() -> ! {
    eprintln!("usage: harumi-flow-benchmark-check <font.ttf> <pages> <output.json>");
    std::process::exit(2);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let font_path = args.next().unwrap_or_else(|| usage());
    let pages: usize = args.next().unwrap_or_else(|| usage()).parse()?;
    let output_path = args.next().unwrap_or_else(|| usage());
    if pages == 0 {
        return Err("pages must be positive".into());
    }

    let font = fs::read(font_path)?;
    let mut warmup = FlowDocument::new(font.clone(), FlowOptions::default())?;
    let warmup_started = Instant::now();
    for page in 0..3 {
        warmup.push_paragraph(&format!("Warm-up paragraph {}.", page + 1))?;
        if page < 2 {
            warmup.push_page_break()?;
        }
    }
    let warmup_bytes = warmup.render()?;
    let warmup_elapsed_ms = warmup_started.elapsed().as_secs_f64() * 1000.0;

    let started = Instant::now();
    let mut document = FlowDocument::new(
        font,
        FlowOptions {
            max_pages: (pages as u32).saturating_add(2),
            ..FlowOptions::default()
        },
    )?;
    for page in 0..pages {
        document.push_heading(&format!("Benchmark page {}", page + 1), 2)?;
        document.push_paragraph(
            "Deterministic mixed CJK/Latin benchmark paragraph for pagination and font embedding.",
        )?;
        if page + 1 < pages {
            document.push_page_break()?;
        }
    }
    let bytes = document.render()?;
    let actual_pages = Document::from_bytes(&bytes)?.page_count();
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let metrics = format!(
        "{{\n  \"requested_pages\": {},\n  \"actual_pages\": {},\n  \"warmup_pages\": 3,\n  \"warmup_elapsed_ms\": {:.3},\n  \"elapsed_ms\": {:.3},\n  \"pdf_bytes\": {},\n  \"peak_rss_bytes\": null,\n  \"peak_rss_scope\": \"standalone prebuilt benchmark process\",\n  \"warmup_pdf_bytes\": {}\n}}\n",
        actual_pages,
        pages,
        warmup_elapsed_ms,
        elapsed_ms,
        bytes.len(),
        warmup_bytes.len()
    );
    fs::write(output_path, metrics)?;
    Ok(())
}
