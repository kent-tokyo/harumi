use std::{env, fs, path::Path, time::Instant};

use harumi::Document;

const MARKERS: [&str; 11] = [
    "四半期レポート",
    "組版契約",
    "混在スタイル",
    "売上",
    "¥12,345,678",
    "顧客数",
    "1,234",
    "地域",
    "東京・大阪・福岡",
    "長大セル",
    "明細",
];

fn usage() -> ! {
    eprintln!(
        "usage: harumi-report-generation-check <harumi-flow|harumi-html|printpdf|genpdf> <font.ttf> <output.pdf>"
    );
    std::process::exit(2);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let backend = args.next().unwrap_or_else(|| usage());
    let font_path = args.next().unwrap_or_else(|| usage());
    let output_path = args.next().unwrap_or_else(|| usage());
    let font = fs::read(&font_path)?;
    let started = Instant::now();
    let generation_started = Instant::now();

    let bytes = match backend.as_str() {
        "harumi-flow" => generate_harumi(&font)?,
        "harumi-html" => generate_harumi_html(&font)?,
        "printpdf" => generate_printpdf(&font)?,
        "genpdf" => generate_genpdf(Path::new(&font_path), Path::new(&output_path))?,
        _ => usage(),
    };
    let generation_elapsed_ms = generation_started.elapsed().as_secs_f64() * 1000.0;

    if backend != "genpdf" {
        fs::write(&output_path, &bytes)?;
    }
    verify_output(&backend, &bytes)?;
    let writeback_path = verify_quality_and_write_back(&bytes, &font, Path::new(&output_path))?;
    if let Some(metrics_path) = env::var_os("HARUMI_METRICS_PATH") {
        let metrics = format!(
            "{{\n  \"generation_elapsed_ms\": {:.3},\n  \"runner_elapsed_ms\": {:.3},\n  \"peak_rss_bytes\": null,\n  \"peak_rss_scope\": \"standalone prebuilt report runner process\"\n}}\n",
            generation_elapsed_ms,
            started.elapsed().as_secs_f64() * 1000.0
        );
        fs::write(metrics_path, metrics)?;
    }
    println!(
        "generated, extracted, quality-checked, and wrote back {backend}: {output_path} -> {}",
        writeback_path.display()
    );
    Ok(())
}

fn generate_harumi(font: &[u8]) -> harumi::Result<Vec<u8>> {
    use harumi::{FlowDocument, FlowOptions};

    let mut doc = FlowDocument::new(font.to_vec(), FlowOptions::default())?;
    doc.push_heading("四半期レポート", 1)?;
    doc.push_paragraph(
        "段落組版契約: CJK/Latin mixed paragraph with enough text to exercise deterministic wrapping.",
    )?;
    doc.push_paragraph_styled(&[
        harumi::InlineSpan::plain("混在スタイル: "),
        harumi::InlineSpan::bold("bold"),
        harumi::InlineSpan::italic(" italic"),
    ])?;
    doc.push_key_value_table(&[
        ("売上", "¥12,345,678"),
        ("顧客数", "1,234"),
        ("地域", "東京・大阪・福岡"),
        ("長大セル", "CJK/Latin long cell content for wrapping"),
    ])?;
    doc.push_page_break()?;
    doc.push_heading("明細", 2)?;
    doc.push_paragraph("ページ分割後も帳票本文を抽出できることを確認します。")?;
    doc.render()
}

fn generate_harumi_html(font: &[u8]) -> harumi::Result<Vec<u8>> {
    use harumi::{HtmlRenderOptions, render_html_to_pdf};

    let html = r#"
        <h1>四半期レポート</h1>
        <p>段落組版契約: CJK/Latin mixed paragraph with enough text to exercise deterministic wrapping.</p>
        <p><strong>混在スタイル</strong>: bold and inline text.</p>
        <table>
          <tr><th>売上</th><td>¥12,345,678</td></tr>
          <tr><th>顧客数</th><td>1,234</td></tr>
          <tr><th>地域</th><td>東京・大阪・福岡</td></tr>
          <tr><th>長大セル</th><td>CJK/Latin long cell content for wrapping</td></tr>
        </table>
        <div class="page-break"></div>
        <h2>明細</h2>
        <p>HTMLからのページ分割後も本文を抽出できることを確認します。</p>
    "#;
    render_html_to_pdf(
        html,
        HtmlRenderOptions {
            font_bytes: font.to_vec(),
            ..HtmlRenderOptions::default()
        },
    )
}

fn generate_printpdf(font: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use printpdf::*;

    let mut font_warnings = Vec::new();
    let parsed = ParsedFont::from_bytes(font, 0, &mut font_warnings)
        .ok_or("printpdf could not parse the fixture font")?;
    let mut doc = PdfDocument::new("shared-report-generation");
    let font_id = doc.add_font(&parsed);

    let page1 = text_page(
        &font_id,
        &[
            "四半期レポート",
            "段落組版契約: CJK/Latin mixed paragraph with wrapping.",
            "混在スタイル: bold and inline text.",
            "売上: ¥12,345,678",
            "顧客数: 1,234",
            "地域: 東京・大阪・福岡",
        ],
        true,
    );
    let page2 = text_page(
        &font_id,
        &[
            "明細",
            "ページ分割後も帳票本文を抽出できることを確認します。",
        ],
        false,
    );
    let mut warnings = Vec::new();
    Ok(doc
        .with_pages(vec![page1, page2])
        .save(&PdfSaveOptions::default(), &mut warnings))
}

fn text_page(font_id: &printpdf::FontId, lines: &[&str], with_table: bool) -> printpdf::PdfPage {
    use printpdf::*;

    let mut ops = vec![
        Op::StartTextSection,
        Op::SetTextCursor {
            pos: Point::new(Mm(20.0), Mm(270.0)),
        },
        Op::SetLineHeight { lh: Pt(18.0) },
        Op::SetFont {
            font: PdfFontHandle::External(font_id.clone()),
            size: Pt(12.0),
        },
    ];
    for line in lines {
        ops.push(Op::ShowText {
            items: vec![TextItem::Text((*line).to_owned())],
        });
        ops.push(Op::AddLineBreak);
    }
    if with_table {
        ops.push(Op::EndTextSection);
        for (y, key, value) in [
            (212.0, "売上", "¥12,345,678"),
            (194.0, "顧客数", "1,234"),
            (176.0, "地域", "東京・大阪・福岡"),
            (158.0, "長大セル", "CJK/Latin long cell"),
        ] {
            for (x, text) in [(22.0, key), (78.0, value)] {
                ops.push(Op::StartTextSection);
                ops.push(Op::SetFont {
                    font: PdfFontHandle::External(font_id.clone()),
                    size: Pt(12.0),
                });
                ops.push(Op::SetTextCursor {
                    pos: Point::new(Mm(x), Mm(y)),
                });
                ops.push(Op::ShowText {
                    items: vec![TextItem::Text(text.to_owned())],
                });
                ops.push(Op::EndTextSection);
            }
        }
        for y in [224.0, 206.0, 188.0, 170.0, 152.0] {
            ops.push(Op::DrawLine {
                line: printpdf::Line {
                    points: vec![
                        printpdf::LinePoint {
                            p: Point::new(Mm(20.0), Mm(y)),
                            bezier: false,
                        },
                        printpdf::LinePoint {
                            p: Point::new(Mm(190.0), Mm(y)),
                            bezier: false,
                        },
                    ],
                    is_closed: false,
                },
            });
        }
        for x in [20.0, 75.0, 190.0] {
            ops.push(Op::DrawLine {
                line: printpdf::Line {
                    points: vec![
                        printpdf::LinePoint {
                            p: Point::new(Mm(x), Mm(224.0)),
                            bezier: false,
                        },
                        printpdf::LinePoint {
                            p: Point::new(Mm(x), Mm(152.0)),
                            bezier: false,
                        },
                    ],
                    is_closed: false,
                },
            });
        }
    } else {
        ops.push(Op::EndTextSection);
    }
    PdfPage::new(Mm(210.0), Mm(297.0), ops)
}

fn generate_genpdf(
    font_path: &Path,
    output_path: &Path,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let font_data = genpdf::fonts::FontData::new(fs::read(font_path)?, None)?;
    let family = genpdf::fonts::FontFamily {
        regular: font_data.clone(),
        bold: font_data.clone(),
        italic: font_data.clone(),
        bold_italic: font_data,
    };
    let mut doc = genpdf::Document::new(family);
    doc.set_title("shared-report-generation");
    doc.push(genpdf::elements::Paragraph::new("四半期レポート"));
    doc.push(genpdf::elements::Paragraph::new(
        "段落組版契約: CJK/Latin mixed paragraph with enough text to exercise deterministic wrapping.",
    ));
    doc.push(genpdf::elements::Paragraph::new(
        "混在スタイル: bold and inline text.",
    ));
    let mut table = genpdf::elements::TableLayout::new(vec![1, 2]);
    for (key, value) in [
        ("売上", "¥12,345,678"),
        ("顧客数", "1,234"),
        ("地域", "東京・大阪・福岡"),
        ("長大セル", "CJK/Latin long cell content for wrapping"),
    ] {
        table
            .row()
            .element(genpdf::elements::Paragraph::new(key))
            .element(genpdf::elements::Paragraph::new(value))
            .push()?;
    }
    doc.push(table);
    doc.push(genpdf::elements::PageBreak::new());
    doc.push(genpdf::elements::Paragraph::new("明細"));
    doc.push(genpdf::elements::Paragraph::new(
        "ページ分割後も帳票本文を抽出できることを確認します。",
    ));
    doc.render_to_file(output_path)?;
    Ok(fs::read(output_path)?)
}

fn verify_output(backend: &str, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let doc = Document::from_bytes(bytes)?;
    if doc.page_count() != 2 {
        return Err(format!("{backend}: expected 2 pages, got {}", doc.page_count()).into());
    }
    let text: String = (1..=doc.page_count())
        .flat_map(|page| doc.extract_text_runs(page).unwrap_or_default())
        .map(|run| run.text)
        .collect();
    let mut marker_offset = 0usize;
    for marker in MARKERS {
        let Some(relative) = text[marker_offset..].find(marker) else {
            if text.is_empty() {
                dump_extraction_inputs(bytes, backend)?;
            }
            return Err(format!(
                "{backend}: missing or out-of-order marker {marker:?} in {text:?}"
            )
            .into());
        };
        marker_offset += relative + marker.len();
    }
    if !bytes.windows(b"/FontFile".len()).any(|w| w == b"/FontFile") {
        return Err(format!("{backend}: no embedded font evidence").into());
    }
    Ok(())
}

fn verify_quality_and_write_back(
    bytes: &[u8],
    font_bytes: &[u8],
    output_path: &Path,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let mut doc = Document::from_bytes(bytes)?;
    let mut overflow_count = 0usize;
    let mut overflow_details = Vec::new();
    for page in 1..=doc.page_count() {
        let (page_width, page_height) = doc.page(page)?.size()?;
        for fragment in doc.extract_text_runs(page)? {
            if fragment.x < 0.0
                || fragment.y < 0.0
                || fragment.x + fragment.width > page_width + 1.0
                || fragment.y + fragment.height > page_height + 1.0
            {
                overflow_count += 1;
                overflow_details.push((
                    page,
                    fragment.text,
                    fragment.x,
                    fragment.y,
                    fragment.width,
                    fragment.height,
                    page_width,
                    page_height,
                ));
            }
        }
    }
    if overflow_count != 0 {
        return Err(format!(
            "quality check found {overflow_count} page-boundary overflows: {overflow_details:?}"
        )
        .into());
    }

    let font = doc.embed_font(font_bytes)?;
    doc.page(1)?
        .add_text("harumi-writeback", font, [36.0, 36.0], 10.0, [0.0; 3])?;
    let writeback = doc.save_to_bytes()?;
    let mut writeback_path = output_path.to_path_buf();
    let file_name = output_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("report.pdf");
    writeback_path.set_file_name(format!("{file_name}.harumi-writeback.pdf"));
    fs::write(&writeback_path, &writeback)?;
    let reloaded = Document::from_bytes(&writeback)?;
    let text = reloaded.extract_text(1)?;
    if !text.contains("harumi-writeback") {
        return Err(format!("write-back marker was not extractable after save: {text:?}").into());
    }
    Ok(writeback_path)
}

fn dump_extraction_inputs(bytes: &[u8], backend: &str) -> Result<(), Box<dyn std::error::Error>> {
    let pdf = harumi::lopdf::Document::load_mem(bytes)?;
    eprintln!("{backend}: extraction diagnostics");
    for (page, page_id) in pdf.get_pages() {
        eprintln!(
            "page {page}: content={:?}",
            String::from_utf8_lossy(&pdf.get_page_content(page_id)?)
        );
        for (name, font) in pdf.get_page_fonts(page_id)? {
            let subtype = font
                .get(b"Subtype")
                .ok()
                .and_then(|object| object.as_name().ok())
                .map(String::from_utf8_lossy);
            let to_unicode = font.get(b"ToUnicode").ok();
            eprintln!(
                "font {:?}: subtype={subtype:?} to_unicode={to_unicode:?}",
                String::from_utf8_lossy(&name)
            );
            eprintln!("descendant raw: {:?}", font.get(b"DescendantFonts").ok());
            if let Some(harumi::lopdf::Object::Reference(id)) = to_unicode {
                if let Ok(object) = pdf.get_object(*id) {
                    if let Ok(stream) = object.as_stream() {
                        let mut stream = stream.clone();
                        let _ = stream.decompress();
                        eprintln!("ToUnicode: {}", String::from_utf8_lossy(&stream.content));
                    }
                }
            }
            if let Ok(harumi::lopdf::Object::Array(descendants)) = font.get(b"DescendantFonts") {
                if let Some(harumi::lopdf::Object::Reference(id)) = descendants.first() {
                    eprintln!("descendant: {:?}", pdf.get_object(*id).ok());
                }
            }
        }
    }
    Ok(())
}
