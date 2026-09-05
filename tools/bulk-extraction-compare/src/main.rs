use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Instant,
};

use rayon::prelude::*;
use serde::Serialize;

type CompareError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Serialize)]
struct BackendReport {
    corpus: &'static str,
    version: u8,
    backend: &'static str,
    parallelism: usize,
    elapsed_ms: u128,
    peak_memory_bytes: Option<u64>,
    inputs: Vec<InputReport>,
}

#[derive(Debug, Serialize)]
struct InputReport {
    id: String,
    pdf_path: String,
    page_count: usize,
    text_marker_recall: f32,
    expected_markers: usize,
    found_markers: usize,
    coordinate_coverage: f32,
    coordinate_records: usize,
    markdown_block_count: usize,
    image_count: usize,
    elapsed_ms: u128,
    failure_classes: Vec<String>,
}

fn usage() -> ! {
    eprintln!(
        "usage: harumi-bulk-extraction-compare <unpdf|pdf-oxide> <corpus-dir> <output.json> [parallelism]"
    );
    std::process::exit(2);
}

fn main() -> Result<(), CompareError> {
    let mut args = env::args().skip(1);
    let backend = args.next().unwrap_or_else(|| usage());
    let corpus_dir = PathBuf::from(args.next().unwrap_or_else(|| usage()));
    let output_path = PathBuf::from(args.next().unwrap_or_else(|| usage()));
    let parallelism = args
        .next()
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(1)
        .max(1);
    let input_paths = corpus_paths(&corpus_dir)?;
    let started = Instant::now();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(parallelism)
        .build()?;
    let inputs = pool.install(|| match backend.as_str() {
        "unpdf" => input_paths
            .par_iter()
            .map(|path| measure_unpdf(path))
            .collect::<Result<Vec<_>, _>>(),
        "pdf-oxide" => input_paths
            .par_iter()
            .map(|path| measure_pdf_oxide(path))
            .collect::<Result<Vec<_>, _>>(),
        _ => usage(),
    })?;
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let report = BackendReport {
        corpus: "bulk-extraction-corpus-v1",
        version: 1,
        backend: match backend.as_str() {
            "unpdf" => "unpdf-0.17.0",
            "pdf-oxide" => "pdf_oxide-0.3.77",
            _ => unreachable!(),
        },
        parallelism,
        elapsed_ms: started.elapsed().as_millis(),
        peak_memory_bytes: peak_memory_bytes(),
        inputs,
    };
    fs::write(&output_path, serde_json::to_vec_pretty(&report)?)?;
    println!("wrote {}", output_path.display());
    Ok(())
}

fn corpus_paths(dir: &Path) -> Result<Vec<PathBuf>, CompareError> {
    let mut paths = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "pdf"))
        .collect::<Vec<_>>();
    paths.sort();
    if paths.len() != 5 {
        return Err(format!(
            "expected 5 corpus PDFs in {}, found {}",
            dir.display(),
            paths.len()
        )
        .into());
    }
    Ok(paths)
}

fn measure_unpdf(path: &Path) -> Result<InputReport, CompareError> {
    let started = Instant::now();
    let result = unpdf::Unpdf::new()
        .sequential()
        .with_images(true)
        .parse(path)?;
    let document = result.document();
    let markdown = result.to_markdown()?;
    let text = result.plain_text();
    let json =
        serde_json::from_str::<serde_json::Value>(&result.to_json(unpdf::JsonFormat::Compact)?)?;
    let coordinate_total = count_bbox_arrays(&json);
    let coordinate_valid = count_valid_bbox_arrays(&json);
    let image_count = document.pages.iter().map(|page| page.images.len()).sum();
    let page_count = document.page_count() as usize;
    let (expected, found) = marker_counts(path, &text);
    Ok(make_report(
        path,
        page_count,
        found,
        expected,
        coordinate_valid,
        coordinate_total,
        markdown_blocks(&markdown),
        image_count,
        started.elapsed().as_millis(),
    ))
}

fn measure_pdf_oxide(path: &Path) -> Result<InputReport, CompareError> {
    let started = Instant::now();
    let document = pdf_oxide::PdfDocument::open(path)?;
    let page_count = document.page_count()?;
    let mut text = String::new();
    let mut markdown_block_count = 0;
    let mut coordinate_total = 0usize;
    let mut coordinate_valid = 0usize;
    let mut image_count = 0usize;
    for page in 0..page_count {
        text.push_str(&document.extract_text(page)?);
        let markdown = document.to_markdown(page, &Default::default())?;
        markdown_block_count += markdown_blocks(&markdown);
        for line in document.extract_text_lines(page)? {
            coordinate_total += 1;
            if line.bbox.x.is_finite()
                && line.bbox.y.is_finite()
                && line.bbox.width > 0.0
                && line.bbox.height > 0.0
            {
                coordinate_valid += 1;
            }
        }
        image_count += document.extract_images(page)?.len();
    }
    let (expected, found) = marker_counts(path, &text);
    Ok(make_report(
        path,
        page_count,
        found,
        expected,
        coordinate_valid,
        coordinate_total,
        markdown_block_count,
        image_count,
        started.elapsed().as_millis(),
    ))
}

fn make_report(
    path: &Path,
    page_count: usize,
    found: usize,
    expected: usize,
    coordinate_valid: usize,
    coordinate_total: usize,
    markdown_block_count: usize,
    image_count: usize,
    elapsed_ms: u128,
) -> InputReport {
    let id = input_id(path);
    InputReport {
        failure_classes: failure_classes(&id, found, expected, coordinate_total, image_count),
        id,
        pdf_path: path.display().to_string(),
        page_count,
        text_marker_recall: ratio(found, expected),
        expected_markers: expected,
        found_markers: found,
        coordinate_coverage: ratio(coordinate_valid, coordinate_total),
        coordinate_records: coordinate_total,
        markdown_block_count,
        image_count,
        elapsed_ms,
    }
}

fn input_id(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .strip_prefix("bulk-")
        .unwrap_or_default()
        .to_owned()
}

fn marker_counts(path: &Path, text: &str) -> (usize, usize) {
    let markers: &[&str] = match input_id(path).as_str() {
        "cjk-text" => &["四半期レポート", "日本語本文", "明細"],
        "one-glyph-per-tj" => &["一文字TjのCJK連結"],
        "two-column-report" => &["左段落1", "右段落1", "売上", "¥12,345,678"],
        "scanned-page-ocr-json" => &["Visible label", "OCR layer"],
        "generated-report" => &["四半期レポート", "売上", "¥12,345,678", "明細"],
        _ => &[],
    };
    (
        markers.len(),
        markers
            .iter()
            .filter(|marker| text.contains(**marker))
            .count(),
    )
}

fn ratio(found: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        found as f32 / total as f32
    }
}

fn failure_classes(
    id: &str,
    found: usize,
    expected: usize,
    coordinate_total: usize,
    image_count: usize,
) -> Vec<String> {
    let mut failures = Vec::new();
    if found < expected {
        failures.push("text_missing".to_owned());
    }
    if id == "one-glyph-per-tj" && found < expected {
        failures.push("fragment_joining_changed".to_owned());
    }
    if coordinate_total == 0 {
        failures.push("coordinates_missing".to_owned());
    }
    if id == "scanned-page-ocr-json" && image_count == 0 {
        failures.push("image_missing".to_owned());
    }
    failures
}

#[cfg(target_os = "macos")]
fn peak_memory_bytes() -> Option<u64> {
    use std::mem::MaybeUninit;

    let mut usage = MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: getrusage initializes the provided rusage struct.
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    (result == 0).then(|| unsafe { usage.assume_init().ru_maxrss as u64 })
}

#[cfg(target_os = "linux")]
fn peak_memory_bytes() -> Option<u64> {
    use std::mem::MaybeUninit;

    let mut usage = MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: getrusage initializes the provided rusage struct.
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    (result == 0).then(|| unsafe { usage.assume_init().ru_maxrss as u64 * 1024 })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn peak_memory_bytes() -> Option<u64> {
    None
}

fn markdown_blocks(markdown: &str) -> usize {
    markdown
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .count()
}

fn count_bbox_arrays(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(map) => {
            usize::from(map.get("bbox").is_some())
                + map.values().map(count_bbox_arrays).sum::<usize>()
        }
        serde_json::Value::Array(items) => items.iter().map(count_bbox_arrays).sum(),
        _ => 0,
    }
}

fn count_valid_bbox_arrays(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(map) => {
            let current = map.get("bbox").filter(|bbox| {
                bbox.as_array().is_some_and(|items| {
                    items.len() >= 4 && items.iter().all(serde_json::Value::is_number)
                })
            });
            usize::from(current.is_some())
                + map.values().map(count_valid_bbox_arrays).sum::<usize>()
        }
        serde_json::Value::Array(items) => items.iter().map(count_valid_bbox_arrays).sum(),
        _ => 0,
    }
}
