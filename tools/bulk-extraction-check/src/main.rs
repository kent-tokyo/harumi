use std::{env, fs, path::Path, time::Instant};

use harumi::{Document, Result as HarumiResult};
use serde::Serialize;

const RED_PNG: &[u8] = include_bytes!("../../../tests/fixtures/red_1x1.png");

#[derive(Debug, Serialize)]
struct CorpusReport {
    corpus: &'static str,
    version: u8,
    tool: &'static str,
    inputs: Vec<InputReport>,
}

#[derive(Debug, Serialize)]
struct InputReport {
    id: &'static str,
    pdf_path: String,
    page_count: u32,
    text_marker_recall: f32,
    expected_markers: usize,
    found_markers: usize,
    fragment_count: usize,
    coordinate_coverage: f32,
    markdown_block_count: usize,
    image_count: usize,
    invisible_fragment_count: usize,
    elapsed_ms: u128,
}

fn usage() -> ! {
    eprintln!(
        "usage: harumi-bulk-extraction-check <font.ttf> <output.json>\n\nWrites five generated corpus PDFs next to output.json and a separate metrics report."
    );
    std::process::exit(2);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let font_path = args.next().unwrap_or_else(|| usage());
    let report_path = args.next().unwrap_or_else(|| usage());
    let font = fs::read(font_path)?;
    let report_path = Path::new(&report_path);
    if let Some(parent) = report_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let output_dir = report_path.parent().unwrap_or_else(|| Path::new("."));

    let cases = [
        ("cjk-text", generate_cjk_text(&font)?),
        ("one-glyph-per-tj", generate_one_glyph_per_tj(&font)?),
        ("two-column-report", generate_two_column_report(&font)?),
        ("scanned-page-ocr-json", generate_scan_with_ocr(&font)?),
        ("generated-report", generate_generated_report(&font)?),
    ];

    let mut inputs = Vec::with_capacity(cases.len());
    for (id, bytes) in cases {
        let pdf_path = output_dir.join(format!("bulk-{id}.pdf"));
        fs::write(&pdf_path, &bytes)?;
        inputs.push(measure_case(id, &bytes, pdf_path)?);
    }

    let report = CorpusReport {
        corpus: "bulk-extraction-corpus-v1",
        version: 1,
        tool: "harumi",
        inputs,
    };
    fs::write(report_path, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "wrote {} and {} corpus PDFs",
        report_path.display(),
        report.inputs.len()
    );
    Ok(())
}

fn generate_cjk_text(font: &[u8]) -> HarumiResult<Vec<u8>> {
    let mut doc = Document::new((595.0, 842.0))?;
    let handle = doc.embed_font(font)?;
    doc.page(1)?
        .add_text("四半期レポート", handle, [72.0, 750.0], 24.0, [0.0; 3])?;
    doc.page(1)?.add_text(
        "日本語本文を抽出する固定corpusです。",
        handle,
        [72.0, 700.0],
        12.0,
        [0.0; 3],
    )?;
    doc.insert_blank_page(1, (595.0, 842.0))?;
    doc.page(2)?
        .add_text("明細", handle, [72.0, 750.0], 18.0, [0.0; 3])?;
    doc.page(2)?.add_text(
        "ページ境界をまたぐ抽出を確認します。",
        handle,
        [72.0, 700.0],
        12.0,
        [0.0; 3],
    )?;
    doc.save_to_bytes()
}

fn generate_one_glyph_per_tj(font: &[u8]) -> HarumiResult<Vec<u8>> {
    let mut doc = Document::new((595.0, 842.0))?;
    let handle = doc.embed_font(font)?;
    for (index, ch) in "一文字TjのCJK連結".chars().enumerate() {
        doc.page(1)?.add_text(
            &ch.to_string(),
            handle,
            [72.0 + index as f32 * 18.0, 700.0],
            12.0,
            [0.0; 3],
        )?;
    }
    doc.save_to_bytes()
}

fn generate_two_column_report(font: &[u8]) -> HarumiResult<Vec<u8>> {
    let mut doc = Document::new((595.0, 842.0))?;
    let handle = doc.embed_font(font)?;
    for (row, (left, right)) in [
        ("左段落1", "右段落1"),
        ("左段落2", "右段落2"),
        ("売上", "¥12,345,678"),
    ]
    .into_iter()
    .enumerate()
    {
        let y = 740.0 - row as f32 * 32.0;
        doc.page(1)?
            .add_text(left, handle, [72.0, y], 12.0, [0.0; 3])?;
        doc.page(1)?
            .add_text(right, handle, [330.0, y], 12.0, [0.0; 3])?;
    }
    doc.save_to_bytes()
}

fn generate_scan_with_ocr(font: &[u8]) -> HarumiResult<Vec<u8>> {
    let mut doc = Document::new((595.0, 842.0))?;
    let handle = doc.embed_font(font)?;
    doc.page(1)?
        .add_image(RED_PNG, [72.0, 500.0, 120.0, 120.0])?;
    doc.page(1)?
        .add_text("Visible label", handle, [72.0, 470.0], 12.0, [0.0; 3])?;
    doc.page(1)?
        .add_invisible_text("OCR layer", handle, [72.0, 470.0], 12.0)?;
    doc.save_to_bytes()
}

fn generate_generated_report(font: &[u8]) -> HarumiResult<Vec<u8>> {
    use harumi::{FlowDocument, FlowOptions};

    let mut doc = FlowDocument::new(font.to_vec(), FlowOptions::default())?;
    doc.push_heading("四半期レポート", 1)?;
    doc.push_paragraph("同一帳票比較用の固定フィクスチャです。")?;
    doc.push_key_value_table(&[
        ("売上", "¥12,345,678"),
        ("顧客数", "1,234"),
        ("地域", "東京・大阪・福岡"),
    ])?;
    doc.push_page_break()?;
    doc.push_heading("明細", 2)?;
    doc.push_paragraph("ページ分割後も帳票本文を抽出できることを確認します。")?;
    doc.render()
}

fn measure_case(
    id: &'static str,
    bytes: &[u8],
    pdf_path: std::path::PathBuf,
) -> Result<InputReport, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let doc = Document::from_bytes(bytes)?;
    let expected = match id {
        "cjk-text" => ["四半期レポート", "日本語本文", "明細"].as_slice(),
        "one-glyph-per-tj" => ["一文字TjのCJK連結"].as_slice(),
        "two-column-report" => ["左段落1", "右段落1", "売上", "¥12,345,678"].as_slice(),
        "scanned-page-ocr-json" => ["Visible label", "OCR layer"].as_slice(),
        "generated-report" => ["四半期レポート", "売上", "¥12,345,678", "明細"].as_slice(),
        _ => return Err(format!("unknown corpus case: {id}").into()),
    };
    let mut fragments = Vec::new();
    let mut markdown_block_count = 0;
    let mut image_count = 0;
    let mut invisible_fragment_count = 0;
    for page in 1..=doc.page_count() {
        let page_fragments = doc.extract_text_runs(page)?;
        invisible_fragment_count += page_fragments
            .iter()
            .filter(|fragment| fragment.invisible)
            .count();
        fragments.extend(page_fragments);
        let markdown = doc.extract_as_markdown(page)?;
        markdown_block_count += markdown
            .split("\n\n")
            .filter(|block| !block.trim().is_empty())
            .count();
        image_count += doc
            .extract_page_images(page)
            .map(|images| images.len())
            .unwrap_or(0);
    }
    let text: String = fragments
        .iter()
        .map(|fragment| fragment.text.as_str())
        .collect();
    let found_markers = expected
        .iter()
        .filter(|marker| text.contains(**marker))
        .count();
    let coordinate_coverage = if fragments.is_empty() {
        0.0
    } else {
        fragments
            .iter()
            .filter(|fragment| {
                fragment.x.is_finite()
                    && fragment.y.is_finite()
                    && fragment.width > 0.0
                    && fragment.height > 0.0
            })
            .count() as f32
            / fragments.len() as f32
    };
    Ok(InputReport {
        id,
        pdf_path: pdf_path.display().to_string(),
        page_count: doc.page_count(),
        text_marker_recall: found_markers as f32 / expected.len() as f32,
        expected_markers: expected.len(),
        found_markers,
        fragment_count: fragments.len(),
        coordinate_coverage,
        markdown_block_count,
        image_count,
        invisible_fragment_count,
        elapsed_ms: started.elapsed().as_millis(),
    })
}
