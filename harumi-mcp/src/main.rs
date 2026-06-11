use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::io::{self, BufRead};
use std::path::{Component, Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

const MCP_VERSION: &str = "2024-11-25";
const MAX_PDF_SIZE: u64 = 100_000_000; // 100 MB
const MAX_FONT_SIZE: u64 = 25_000_000; // 25 MB
const MAX_COORDINATE: f32 = 100_000.0;
const MAX_FONT_SIZE_PTS: f32 = 10_000.0;
const CJK_FONT_RECOMMENDATION: &str = "for CJK translation use: Simplified Chinese=NotoSansCJKsc-Regular.ttf, \
     Traditional Chinese=NotoSansCJKtc-Regular.ttf, Japanese/Chinese mixed=NotoSansCJKjp-Regular.ttf";

macro_rules! get_required_string {
    ($params:expr, $key:expr) => {
        match $params.get($key).and_then(|p| p.as_str()) {
            Some(p) if !p.is_empty() => p,
            _ => return error_response("INVALID_PARAMS", &format!("{} cannot be empty", $key)),
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplaceMode {
    Resubset,
    Preserve,
    NewFont,
    Wrap,
}

impl ReplaceMode {
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "resubset" => Ok(ReplaceMode::Resubset),
            "preserve" => Ok(ReplaceMode::Preserve),
            "new_font" => Ok(ReplaceMode::NewFont),
            "wrap" => Ok(ReplaceMode::Wrap),
            invalid => Err(format!(
                "Invalid mode '{}': must be 'resubset', 'preserve', 'new_font', or 'wrap'",
                invalid
            )),
        }
    }

    fn requires_font(&self) -> bool {
        matches!(
            self,
            ReplaceMode::Resubset | ReplaceMode::NewFont | ReplaceMode::Wrap
        )
    }

    fn requires_font_embedding(&self) -> bool {
        matches!(self, ReplaceMode::NewFont)
    }

    fn requires_font_validation(&self) -> bool {
        matches!(
            self,
            ReplaceMode::Resubset | ReplaceMode::Wrap | ReplaceMode::NewFont
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct Tool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: serde_json::Value,
}

fn error_response(code: &str, message: &str) -> serde_json::Value {
    json!({
        "error": message,
        "code": code
    })
}

fn success_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "success": true,
        "result": data
    })
}

fn replace_mode_name(mode: ReplaceMode) -> &'static str {
    match mode {
        ReplaceMode::Resubset => "resubset",
        ReplaceMode::Preserve => "preserve",
        ReplaceMode::NewFont => "new_font",
        ReplaceMode::Wrap => "wrap",
    }
}

fn replace_save_error(output_path: &str, mode: ReplaceMode, detail: &str) -> serde_json::Value {
    if matches!(mode, ReplaceMode::Resubset | ReplaceMode::Wrap)
        && detail.contains("CIDToGIDMap=Identity")
    {
        return error_response(
            "UNSUPPORTED_FONT_MAP",
            &format!(
                "Cannot replace text with mode '{}' for this PDF: {}. \
                 The PDF uses a non-Identity CIDToGIDMap, which harumi cannot resubset yet; \
                 retry with mode 'new_font' and a Unicode TTF font.",
                replace_mode_name(mode),
                detail
            ),
        );
    }

    error_response(
        "FILE_WRITE_ERROR",
        &format!("Cannot write PDF to '{}': {}", output_path, detail),
    )
}

fn validate_path(path: &str) -> Result<PathBuf, serde_json::Value> {
    if path.is_empty() {
        return Err(error_response("INVALID_PARAMS", "Path cannot be empty"));
    }

    let p = Path::new(path);

    if p.is_absolute() {
        return Err(error_response(
            "INVALID_PARAMS",
            "Absolute paths are not allowed; provide a relative path",
        ));
    }

    if p.components().any(|c| c == Component::ParentDir) {
        return Err(error_response(
            "INVALID_PARAMS",
            "Path traversal (..) is not allowed",
        ));
    }

    // Check for suspicious patterns
    if path.contains("~") || path.contains("$") {
        return Err(error_response(
            "INVALID_PARAMS",
            "Path contains suspicious characters",
        ));
    }

    Ok(p.to_path_buf())
}

fn read_file_with_limit(path: &str, max_size: u64) -> Result<Vec<u8>, serde_json::Value> {
    validate_path(path)?;

    let metadata = std::fs::metadata(path).map_err(|e| {
        error_response(
            "FILE_READ_ERROR",
            &format!("Cannot access file '{}': {}", path, e),
        )
    })?;

    if metadata.len() > max_size {
        return Err(error_response(
            "FILE_READ_ERROR",
            &format!("File exceeds maximum size ({} bytes)", max_size),
        ));
    }

    std::fs::read(path).map_err(|e| {
        error_response(
            "FILE_READ_ERROR",
            &format!("Cannot read file '{}': {}", path, e),
        )
    })
}

fn validate_output_path(path: &str) -> Result<PathBuf, serde_json::Value> {
    if path.is_empty() {
        return Err(error_response(
            "INVALID_PARAMS",
            "output_path cannot be empty",
        ));
    }

    let p = Path::new(path);

    if p.is_absolute() {
        return Err(error_response(
            "INVALID_PARAMS",
            "Absolute output paths are not allowed",
        ));
    }

    if p.components().any(|c| c == Component::ParentDir) {
        return Err(error_response(
            "INVALID_PARAMS",
            "Path traversal in output_path is not allowed",
        ));
    }

    Ok(p.to_path_buf())
}

fn validate_page_in_bounds(
    page: u32,
    page_count: u32,
    _page_name: &str,
) -> Result<(), serde_json::Value> {
    if page > page_count {
        return Err(error_response(
            "PAGE_OUT_OF_BOUNDS",
            &format!(
                "Page {} exceeds document length ({} pages)",
                page, page_count
            ),
        ));
    }
    Ok(())
}

/// Check for text overflow when replacing old_text with new_text.
/// Returns a warning message if overflow ratio > 1.2 (>20% growth), or None if no overflow detected.
fn check_overflow(
    old_w: f32,
    new_w: f32,
    old_text: &str,
    new_text: &str,
    page_num: u32,
) -> Option<String> {
    if old_w <= 0.001 {
        return None; // Cannot calculate ratio if old width is too small
    }
    let overflow_ratio = new_w / old_w;
    if overflow_ratio > 1.2 {
        let overflow_pt = new_w - old_w;
        Some(format!(
            "Page {}: '{}' → '{}' may overflow (+{:.1}pt, {:.0}% of original)",
            page_num,
            old_text,
            new_text,
            overflow_pt,
            overflow_ratio * 100.0
        ))
    } else {
        None
    }
}

/// Check if all characters in new_text have glyphs in the font.
/// Returns Ok(()) if all glyphs present, Err(missing_chars) if any are missing.
fn check_font_glyphs(new_text: &str, font_bytes: &[u8], font_size: f32) -> Result<(), Vec<char>> {
    if harumi::calculate_text_width(new_text, font_bytes, font_size).is_some() {
        Ok(())
    } else {
        let missing: Vec<char> = new_text
            .chars()
            .filter(|&ch| {
                harumi::calculate_text_width(&ch.to_string(), font_bytes, font_size).is_none()
            })
            .collect();
        Err(missing)
    }
}

/// Format a font support error message for user feedback.
fn format_font_error(
    missing_chars: Vec<char>,
    new_text: &str,
    page_num: u32,
    idx: usize,
) -> String {
    let preview: String = missing_chars.iter().take(5).collect();
    let detail = match missing_chars.len() {
        count if count > 1 => format!("font missing {} character(s) (e.g. '{}')", count, preview),
        1 => format!("font missing character '{}'", preview),
        _ => format!("font may not support all characters in '{}'", new_text),
    };
    format!(
        "Page {}, replacement {}: {} — {}",
        page_num, idx, detail, CJK_FONT_RECOMMENDATION
    )
}

/// Validate that new_text has glyphs in the font (for resubset/wrap modes).
/// Returns an error message if glyphs are missing, or None if all glyphs present.
fn validate_font_support(
    new_text: &str,
    font_bytes: &[u8],
    font_size: f32,
    page_num: u32,
    idx: usize,
) -> Option<String> {
    match check_font_glyphs(new_text, font_bytes, font_size) {
        Ok(()) => None,
        Err(missing) => Some(format_font_error(missing, new_text, page_num, idx)),
    }
}

fn map_runs_to_fragments(runs: Vec<harumi::TextFragment>) -> Vec<serde_json::Value> {
    runs.into_iter()
        .map(|r| {
            // Validate width/height are within reasonable bounds
            let width = r.width.abs().min(MAX_COORDINATE);
            let height = r.height.abs().min(MAX_COORDINATE);
            json!({
                "text": r.text,
                "x": r.x,
                "y": r.y,
                "width": width,
                "height": height,
                "font_size": r.font_size,
                "font_name": r.font_name
            })
        })
        .collect()
}

fn list_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "pdf_extract_text".to_string(),
            description: "Extract text with x,y positions from a PDF page. Supports CJK fonts.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pdf_path": {"type": "string", "description": "Path to the PDF file"},
                    "page": {"type": "integer", "description": "1-indexed page number"}
                },
                "required": ["pdf_path", "page"]
            }),
        },
        Tool {
            name: "pdf_extract_all_pages".to_string(),
            description: "Extract text from all pages with x,y positions and page numbers. Efficiently handles multi-page PDFs.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pdf_path": {"type": "string", "description": "Path to the PDF file"}
                },
                "required": ["pdf_path"]
            }),
        },
        Tool {
            name: "pdf_extract_text_structured".to_string(),
            description: "Extract text with semantic structure (headings vs paragraphs). Groups fragments by font size and position.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pdf_path": {"type": "string", "description": "Path to the PDF file"},
                    "page": {"type": "integer", "description": "1-indexed page number"},
                    "markdown": {"type": "boolean", "description": "Output as Markdown (optional, default: false)"}
                },
                "required": ["pdf_path", "page"]
            }),
        },
        Tool {
            name: "pdf_add_invisible_text".to_string(),
            description: "Add invisible OCR text layer to a PDF page. Text is searchable but not visible.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pdf_path": {"type": "string", "description": "Path to input PDF file"},
                    "output_path": {"type": "string", "description": "Path to output PDF file"},
                    "font_path": {"type": "string", "description": "Path to TTF font file"},
                    "page": {"type": "integer", "description": "1-indexed page number"},
                    "text": {"type": "string", "description": "Text to add (supports CJK)"},
                    "x": {"type": "number", "description": "X position in PDF points"},
                    "y": {"type": "number", "description": "Y position in PDF points"},
                    "size": {"type": "number", "description": "Font size in points"}
                },
                "required": ["pdf_path", "output_path", "font_path", "page", "text", "x", "y", "size"]
            }),
        },
        Tool {
            name: "pdf_replace_text".to_string(),
            description: "Replace text in a PDF while preserving layout via automatic font subsetting. Supports multiple replacements across pages.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pdf_path": {"type": "string", "description": "Path to input PDF file"},
                    "output_path": {"type": "string", "description": "Path to output PDF file"},
                    "replacements": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_text": {"type": "string", "description": "Text to find"},
                                "new_text": {"type": "string", "description": "Text to replace with"}
                            },
                            "required": ["old_text", "new_text"]
                        },
                        "description": "Array of text replacements"
                    },
                    "pages": {
                        "type": "array",
                        "items": {"type": "integer"},
                        "description": "List of 1-indexed page numbers to apply replacements to (optional, default: all pages)"
                    },
                    "font_path": {"type": "string", "description": "Path to TTF font file (required for all modes except 'preserve')"},
                    "mode": {
                        "type": "string",
                        "enum": ["resubset", "preserve", "new_font", "wrap"],
                        "description": "'resubset' (default): rebuild font subset with new chars; 'preserve': keep existing font (fails if chars missing); 'new_font': switch to new font; 'wrap': like resubset but wraps long text to multiple lines"
                    },
                    "strict": {
                        "type": "boolean",
                        "description": "If true, fail without writing output when any replacement error occurs. Default: false."
                    }
                },
                "required": ["pdf_path", "output_path", "replacements"]
            }),
        },
        Tool {
            name: "pdf_rotate_page".to_string(),
            description: "Rotate a PDF page by 90, 180, or 270 degrees.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pdf_path": {"type": "string", "description": "Path to input PDF file"},
                    "output_path": {"type": "string", "description": "Path to output PDF file"},
                    "page": {"type": "integer", "description": "1-indexed page number"},
                    "degrees": {
                        "type": "integer",
                        "enum": [90, 180, 270, -90, -180, -270],
                        "description": "Rotation angle in degrees"
                    }
                },
                "required": ["pdf_path", "output_path", "page", "degrees"]
            }),
        },
        Tool {
            name: "pdf_html_to_pdf".to_string(),
            description: "Convert HTML to PDF. Pure Rust, zero C dependencies. Supports CJK fonts.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "HTML source code"},
                    "output_path": {"type": "string", "description": "Output PDF path"},
                    "title": {"type": "string", "description": "Optional title for PDF metadata"}
                },
                "required": ["html", "output_path"]
            }),
        },
        Tool {
            name: "pdf_merge".to_string(),
            description: "Merge two PDF files into one, appending pages from the second to the first.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pdf1_path": {"type": "string", "description": "Path to first PDF"},
                    "pdf2_path": {"type": "string", "description": "Path to second PDF"},
                    "output_path": {"type": "string", "description": "Path to save merged PDF"}
                },
                "required": ["pdf1_path", "pdf2_path", "output_path"]
            }),
        },
        Tool {
            name: "pdf_page_info".to_string(),
            description: "Get page count and dimensions (width/height in points) of a PDF.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pdf_path": {"type": "string", "description": "Path to PDF file"}
                },
                "required": ["pdf_path"]
            }),
        },
    ]
}

fn extract_text(params: Option<serde_json::Value>) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return error_response("INVALID_REQUEST", "Missing parameters"),
    };

    let pdf_path = get_required_string!(&params, "pdf_path");

    let page = match params.get("page").and_then(|p| p.as_u64()) {
        Some(p) => {
            if p == 0 {
                return error_response("INVALID_PARAMS", "Page number must be >= 1 (1-indexed)");
            }
            p as u32
        }
        None => return error_response("INVALID_PARAMS", "Missing required parameter: page"),
    };

    let pdf_bytes = match read_file_with_limit(pdf_path, MAX_PDF_SIZE) {
        Ok(b) => b,
        Err(e) => return e,
    };

    let doc = match harumi::Document::from_bytes(&pdf_bytes) {
        Ok(d) => d,
        Err(e) => return error_response("INVALID_PDF", &format!("Invalid PDF: {}", e)),
    };

    if doc.page_count() == 0 {
        return error_response("NO_PAGES", "PDF contains no pages");
    }

    if let Err(e) = validate_page_in_bounds(page, doc.page_count(), "page") {
        return e;
    }

    let runs = match doc.extract_text_runs(page) {
        Ok(r) => r,
        Err(e) => {
            return error_response(
                "EXTRACTION_ERROR",
                &format!("Cannot extract text from page {}: {}", page, e),
            );
        }
    };

    let fragments = map_runs_to_fragments(runs);

    success_response(json!({"fragments": fragments}))
}

fn extract_all_pages(params: Option<serde_json::Value>) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return error_response("INVALID_REQUEST", "Missing parameters"),
    };

    let pdf_path = get_required_string!(&params, "pdf_path");

    let pdf_bytes = match read_file_with_limit(pdf_path, MAX_PDF_SIZE) {
        Ok(b) => b,
        Err(e) => return e,
    };

    let doc = match harumi::Document::from_bytes(&pdf_bytes) {
        Ok(d) => d,
        Err(e) => return error_response("INVALID_PDF", &format!("Invalid PDF: {}", e)),
    };

    let page_count = doc.page_count();
    if page_count == 0 {
        return error_response("NO_PAGES", "PDF contains no pages");
    }

    let mut all_pages = Vec::new();
    let mut page_errors = Vec::new();

    for page_num in 1..=page_count {
        match doc.extract_text_runs(page_num) {
            Ok(runs) => {
                let fragments = map_runs_to_fragments(runs);
                all_pages.push(json!({
                    "page": page_num,
                    "fragments": fragments
                }));
            }
            Err(e) => {
                page_errors.push(format!("Page {}: {}", page_num, e));
            }
        }
    }

    let mut result = json!({
        "pages": all_pages,
        "page_count": page_count,
        "extracted_pages": all_pages.len()
    });

    if !page_errors.is_empty() {
        result["warnings"] = json!(page_errors);
    }

    success_response(result)
}

fn extract_text_structured(params: Option<serde_json::Value>) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return error_response("INVALID_REQUEST", "Missing parameters"),
    };

    let pdf_path = get_required_string!(&params, "pdf_path");

    let page = match params.get("page").and_then(|p| p.as_u64()) {
        Some(p) => {
            if p == 0 {
                return error_response("INVALID_PARAMS", "Page number must be >= 1 (1-indexed)");
            }
            p as u32
        }
        None => return error_response("INVALID_PARAMS", "Missing required parameter: page"),
    };

    let as_markdown = params
        .get("markdown")
        .and_then(|p| p.as_bool())
        .unwrap_or(false);

    let pdf_bytes = match read_file_with_limit(pdf_path, MAX_PDF_SIZE) {
        Ok(b) => b,
        Err(e) => return e,
    };

    let doc = match harumi::Document::from_bytes(&pdf_bytes) {
        Ok(d) => d,
        Err(e) => return error_response("INVALID_PDF", &format!("Invalid PDF: {}", e)),
    };

    if doc.page_count() == 0 {
        return error_response("NO_PAGES", "PDF contains no pages");
    }

    if let Err(e) = validate_page_in_bounds(page, doc.page_count(), "page") {
        return e;
    }

    if as_markdown {
        match doc.extract_as_markdown(page) {
            Ok(markdown) => success_response(json!({"markdown": markdown})),
            Err(e) => error_response(
                "EXTRACTION_ERROR",
                &format!("Cannot extract markdown from page {}: {}", page, e),
            ),
        }
    } else {
        match doc.extract_text_chunks(page) {
            Ok(chunks) => {
                let chunk_data: Vec<_> = chunks
                    .into_iter()
                    .map(|chunk| {
                        let chunk_type_str = match &chunk.chunk_type {
                            harumi::ChunkType::Heading(level) => format!("h{}", level),
                            harumi::ChunkType::Paragraph => "paragraph".to_string(),
                            _ => "unknown".to_string(),
                        };
                        json!({
                            "text": chunk.text,
                            "type": chunk_type_str,
                            "bbox": chunk.bbox,
                            "avg_font_size": chunk.avg_font_size
                        })
                    })
                    .collect();
                success_response(json!({"chunks": chunk_data}))
            }
            Err(e) => error_response(
                "EXTRACTION_ERROR",
                &format!("Cannot extract chunks from page {}: {}", page, e),
            ),
        }
    }
}

fn replace_text(params: Option<serde_json::Value>) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return error_response("INVALID_REQUEST", "Missing parameters"),
    };

    let pdf_path = get_required_string!(&params, "pdf_path");
    let output_path_str = get_required_string!(&params, "output_path");

    if let Err(e) = validate_output_path(output_path_str) {
        return e;
    }

    let replacements = match params.get("replacements").and_then(|p| p.as_array()) {
        Some(r) => r.clone(),
        None => {
            return error_response("INVALID_PARAMS", "Missing required parameter: replacements");
        }
    };

    if replacements.is_empty() {
        return error_response("INVALID_PARAMS", "replacements array cannot be empty");
    }

    let mode_str = params
        .get("mode")
        .and_then(|p| p.as_str())
        .unwrap_or("resubset");
    let mode = match ReplaceMode::from_str(mode_str) {
        Ok(m) => m,
        Err(e) => return error_response("INVALID_PARAMS", &e),
    };

    let line_height = params
        .get("line_height")
        .and_then(|p| p.as_f64())
        .unwrap_or(0.0) as f32;
    if !line_height.is_finite() || line_height < 0.0 {
        return error_response(
            "INVALID_PARAMS",
            "line_height must be a non-negative finite number",
        );
    }

    let strict = params
        .get("strict")
        .and_then(|p| p.as_bool())
        .unwrap_or(false);

    let pdf_bytes = match read_file_with_limit(pdf_path, MAX_PDF_SIZE) {
        Ok(b) => b,
        Err(e) => return e,
    };

    // font_path is required for all modes except 'preserve'
    let font_bytes: Vec<u8> = if mode.requires_font() {
        let font_path = get_required_string!(&params, "font_path");
        match read_file_with_limit(font_path, MAX_FONT_SIZE) {
            Ok(b) => b,
            Err(e) => return e,
        }
    } else {
        Vec::new()
    };

    let mut doc = match harumi::Document::from_bytes(&pdf_bytes) {
        Ok(d) => d,
        Err(e) => return error_response("INVALID_PDF", &format!("Invalid PDF: {}", e)),
    };

    let doc_page_count = doc.page_count();
    if doc_page_count == 0 {
        return error_response("NO_PAGES", "PDF contains no pages");
    }

    let pages: Vec<u32> = match params.get("pages").and_then(|p| p.as_array()) {
        Some(pages_arr) => {
            let mut pages_list = Vec::new();
            for (idx, p) in pages_arr.iter().enumerate() {
                match p.as_u64() {
                    Some(page_num) => {
                        if page_num == 0 {
                            return error_response(
                                "INVALID_PARAMS",
                                &format!(
                                    "Invalid page number at index {}: must be >= 1 (1-indexed)",
                                    idx
                                ),
                            );
                        }
                        if let Err(e) =
                            validate_page_in_bounds(page_num as u32, doc_page_count, "pages")
                        {
                            return e;
                        }
                        pages_list.push(page_num as u32);
                    }
                    None => {
                        return error_response(
                            "INVALID_PARAMS",
                            &format!("pages[{}] is not a valid integer", idx),
                        );
                    }
                }
            }
            if pages_list.is_empty() {
                (1..=doc_page_count).collect()
            } else {
                pages_list
            }
        }
        None => (1..=doc_page_count).collect(),
    };

    let pages_count = pages.len();
    let mut total_replacements = 0;
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let font = if mode.requires_font_embedding() {
        match doc.embed_font(&font_bytes) {
            Ok(f) => Some(f),
            Err(e) => {
                return error_response("FONT_ERROR", &format!("Cannot embed font: {}", e));
            }
        }
    } else {
        None
    };

    for page_num in &pages {
        // Extract fragments to determine actual font sizes for overflow calculation
        let page_fragments = match doc.extract_text_runs(*page_num) {
            Ok(frags) => frags,
            Err(_) => Vec::new(), // If extraction fails, continue with empty fragments
        };
        let fragment_map: HashMap<&str, &harumi::TextFragment> = page_fragments
            .iter()
            .map(|f| (f.text.as_str(), f))
            .collect();

        match doc.page(*page_num) {
            Ok(mut page) => {
                // Cache text width calculations to avoid redundant TTF parsing when same new_text
                // with same font_size appears multiple times in replacements on the same page.
                let mut width_cache: HashMap<(String, u32), Option<f32>> = HashMap::new();

                for (idx, replacement) in replacements.iter().enumerate() {
                    let old_text = match replacement.get("old_text").and_then(|p| p.as_str()) {
                        Some(t) => {
                            if t.is_empty() {
                                errors
                                    .push(format!("Replacement {}: old_text cannot be empty", idx));
                                continue;
                            }
                            // Normalize to NFC to ensure consistent matching
                            t.nfc().collect::<String>()
                        }
                        None => {
                            errors.push(format!("Replacement {}: missing old_text field", idx));
                            continue;
                        }
                    };

                    let new_text = match replacement.get("new_text").and_then(|p| p.as_str()) {
                        Some(t) => {
                            if t.is_empty() {
                                errors
                                    .push(format!("Replacement {}: new_text cannot be empty", idx));
                                continue;
                            }
                            // Normalize to NFC to support accented characters (é, è, ê, etc.)
                            t.nfc().collect::<String>()
                        }
                        None => {
                            errors.push(format!("Replacement {}: missing new_text field", idx));
                            continue;
                        }
                    };

                    let fragment = fragment_map.get(old_text.as_str());
                    let actual_font_size = fragment.map(|f| f.font_size).unwrap_or(12.0_f32);

                    if let Some(fragment) = fragment {
                        // Calculate new text width once per replacement (reuse for both overflow check and validation).
                        // Cache by (text, font_size) to avoid redundant TTF parsing when same text appears multiple times.
                        let font_size_bits = actual_font_size.to_bits();
                        let cache_key = (new_text.clone(), font_size_bits);
                        let new_text_width = if !font_bytes.is_empty() {
                            *width_cache.entry(cache_key).or_insert_with(|| {
                                harumi::calculate_text_width(
                                    &new_text,
                                    &font_bytes,
                                    actual_font_size,
                                )
                            })
                        } else {
                            None
                        };

                        if let Some(new_w) = new_text_width {
                            if let Some(warning) = check_overflow(
                                fragment.width,
                                new_w,
                                &old_text,
                                &new_text,
                                *page_num,
                            ) {
                                warnings.push(warning);
                            }
                        }

                        // Pre-check only when the text was extracted on this page. Without this guard,
                        // multi-page translations validate every replacement on every page and produce
                        // noisy missing-glyph warnings for text that is not present on that page.
                        if mode.requires_font_validation() && !font_bytes.is_empty() {
                            if let Some(error) = validate_font_support(
                                &new_text,
                                &font_bytes,
                                actual_font_size,
                                *page_num,
                                idx,
                            ) {
                                errors.push(error);
                                continue;
                            }
                        }
                    }

                    let result = match mode {
                        ReplaceMode::Resubset => {
                            page.replace_text_resubset(&old_text, &new_text, &font_bytes)
                        }
                        ReplaceMode::Preserve => {
                            page.replace_text_preserve_font(&old_text, &new_text)
                        }
                        ReplaceMode::NewFont => match font {
                            Some(f) => page.replace_text(&old_text, &new_text, f),
                            None => {
                                errors.push("Font not embedded".to_string());
                                continue;
                            }
                        },
                        ReplaceMode::Wrap => {
                            // Wrap mode: use line_height, default to 1.2x font_size if not provided
                            let effective_line_height = if line_height > 0.0 {
                                line_height
                            } else {
                                actual_font_size * 1.2
                            };
                            page.replace_text_resubset_with_wrap(
                                &old_text,
                                &new_text,
                                &font_bytes,
                                effective_line_height,
                            )
                        }
                    };

                    match result {
                        Ok(count) => total_replacements += count,
                        Err(e) => {
                            errors.push(format!("Page {}, replacement {}: {}", page_num, idx, e))
                        }
                    }
                }
            }
            Err(e) => {
                errors.push(format!("Cannot access page {}: {}", page_num, e));
            }
        }
    }

    if strict && !errors.is_empty() {
        return error_response(
            "REPLACEMENT_ERROR",
            &format!(
                "Replacement failed with {} error(s); output was not written. First error: {}",
                errors.len(),
                errors[0]
            ),
        );
    }

    if let Err(e) = doc.save(output_path_str) {
        return replace_save_error(output_path_str, mode, &e.to_string());
    }

    let mut result = json!({
        "total_replacements": total_replacements,
        "pages_processed": pages_count,
        "output_path": output_path_str
    });

    // Combine errors and overflow warnings into a single warnings array
    let mut all_warnings = warnings;
    all_warnings.extend(errors);
    if !all_warnings.is_empty() {
        result["warnings"] = json!(all_warnings);
    }

    success_response(result)
}

fn rotate_page(params: Option<serde_json::Value>) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return error_response("INVALID_REQUEST", "Missing parameters"),
    };

    let pdf_path = get_required_string!(&params, "pdf_path");
    let output_path_str = get_required_string!(&params, "output_path");

    if let Err(e) = validate_output_path(output_path_str) {
        return e;
    }

    let page = match params.get("page").and_then(|p| p.as_u64()) {
        Some(p) => {
            if p == 0 {
                return error_response("INVALID_PARAMS", "Page number must be >= 1 (1-indexed)");
            }
            p as u32
        }
        None => return error_response("INVALID_PARAMS", "Missing required parameter: page"),
    };

    let degrees = match params.get("degrees").and_then(|p| p.as_i64()) {
        Some(d) => d as i32,
        None => return error_response("INVALID_PARAMS", "Missing required parameter: degrees"),
    };

    if ![90, 180, 270, -90, -180, -270].contains(&degrees) {
        return error_response(
            "INVALID_PARAMS",
            &format!(
                "Invalid rotation angle: {}. Must be 90, 180, 270, -90, -180, or -270",
                degrees
            ),
        );
    }

    let pdf_bytes = match read_file_with_limit(pdf_path, MAX_PDF_SIZE) {
        Ok(b) => b,
        Err(e) => return e,
    };

    let mut doc = match harumi::Document::from_bytes(&pdf_bytes) {
        Ok(d) => d,
        Err(e) => return error_response("INVALID_PDF", &format!("Invalid PDF: {}", e)),
    };

    if doc.page_count() == 0 {
        return error_response("NO_PAGES", "PDF contains no pages");
    }

    if let Err(e) = validate_page_in_bounds(page, doc.page_count(), "page") {
        return e;
    }

    if let Err(e) = doc.rotate_page(page, degrees) {
        return error_response(
            "ROTATION_ERROR",
            &format!("Cannot rotate page {}: {}", page, e),
        );
    }

    if let Err(e) = doc.save(output_path_str) {
        return error_response(
            "FILE_WRITE_ERROR",
            &format!("Cannot write PDF to '{}': {}", output_path_str, e),
        );
    }

    success_response(json!({
        "output_path": output_path_str,
        "page": page,
        "degrees": degrees
    }))
}

fn add_invisible_text(params: Option<serde_json::Value>) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return error_response("INVALID_REQUEST", "Missing parameters"),
    };

    let pdf_path = get_required_string!(&params, "pdf_path");
    let output_path_str = get_required_string!(&params, "output_path");
    let font_path = get_required_string!(&params, "font_path");

    if let Err(e) = validate_output_path(output_path_str) {
        return e;
    }

    let page = match params.get("page").and_then(|p| p.as_u64()) {
        Some(p) => {
            if p == 0 {
                return error_response("INVALID_PARAMS", "Page number must be >= 1 (1-indexed)");
            }
            p as u32
        }
        None => return error_response("INVALID_PARAMS", "Missing required parameter: page"),
    };

    let text = match params.get("text").and_then(|p| p.as_str()) {
        Some(p) => {
            if p.is_empty() {
                return error_response("INVALID_PARAMS", "text cannot be empty");
            }
            p
        }
        None => return error_response("INVALID_PARAMS", "Missing required parameter: text"),
    };

    let x = match params.get("x").and_then(|p| p.as_f64()) {
        Some(p) => {
            if p < 0.0 {
                return error_response("INVALID_PARAMS", "x coordinate must be >= 0");
            }
            p as f32
        }
        None => return error_response("INVALID_PARAMS", "Missing required parameter: x"),
    };

    let y = match params.get("y").and_then(|p| p.as_f64()) {
        Some(p) => {
            if p < 0.0 {
                return error_response("INVALID_PARAMS", "y coordinate must be >= 0");
            }
            p as f32
        }
        None => return error_response("INVALID_PARAMS", "Missing required parameter: y"),
    };

    let size = match params.get("size").and_then(|p| p.as_f64()) {
        Some(p) => {
            if p <= 0.0 || p > 1000.0 {
                return error_response(
                    "INVALID_PARAMS",
                    "font size must be > 0 and <= 1000 points",
                );
            }
            p as f32
        }
        None => return error_response("INVALID_PARAMS", "Missing required parameter: size"),
    };

    if x > MAX_COORDINATE || y > MAX_COORDINATE || size > MAX_FONT_SIZE_PTS {
        return error_response(
            "INVALID_PARAMS",
            &format!(
                "Coordinates/size exceed maximum bounds (x: {}, y: {}, MAX_COORD: {}, size: {}, MAX_SIZE: {})",
                x, y, MAX_COORDINATE, size, MAX_FONT_SIZE_PTS
            ),
        );
    }

    let pdf_bytes = match read_file_with_limit(pdf_path, MAX_PDF_SIZE) {
        Ok(b) => b,
        Err(e) => return e,
    };

    let font_bytes = match read_file_with_limit(font_path, MAX_FONT_SIZE) {
        Ok(b) => b,
        Err(e) => return e,
    };

    let mut doc = match harumi::Document::from_bytes(&pdf_bytes) {
        Ok(d) => d,
        Err(e) => return error_response("INVALID_PDF", &format!("Invalid PDF: {}", e)),
    };

    if doc.page_count() == 0 {
        return error_response("NO_PAGES", "PDF contains no pages");
    }

    if let Err(e) = validate_page_in_bounds(page, doc.page_count(), "page") {
        return e;
    }

    let font = match doc.embed_font(&font_bytes) {
        Ok(f) => f,
        Err(e) => return error_response("FONT_ERROR", &format!("Cannot embed font: {}", e)),
    };

    match doc.page(page) {
        Ok(mut p) => {
            if let Err(e) = p.add_invisible_text(text, font, [x, y], size) {
                return error_response(
                    "TEXT_ERROR",
                    &format!("Cannot add invisible text to page {}: {}", page, e),
                );
            }
        }
        Err(e) => {
            return error_response("PAGE_ERROR", &format!("Cannot access page {}: {}", page, e));
        }
    }

    if let Err(e) = doc.save(output_path_str) {
        return error_response(
            "FILE_WRITE_ERROR",
            &format!("Cannot write PDF to '{}': {}", output_path_str, e),
        );
    }

    success_response(json!({
        "output_path": output_path_str,
        "page": page,
        "text_length": text.len()
    }))
}

fn html_to_pdf(params: Option<serde_json::Value>) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return error_response("INVALID_REQUEST", "Missing parameters"),
    };

    let html = match params.get("html").and_then(|p| p.as_str()) {
        Some(p) => {
            if p.is_empty() {
                return error_response("INVALID_PARAMS", "html cannot be empty");
            }
            p
        }
        None => return error_response("INVALID_PARAMS", "Missing required parameter: html"),
    };

    if html.len() > 50_000_000 {
        return error_response(
            "INVALID_PARAMS",
            "HTML content exceeds maximum size (50 MB)",
        );
    }

    let output_path_str = get_required_string!(&params, "output_path");

    let output_path_buf = match validate_output_path(output_path_str) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let options = harumi::flow::html::HtmlRenderOptions::default();

    let pdf_bytes = match harumi::flow::html::render_html_to_pdf(html, options) {
        Ok(b) => b,
        Err(e) => return error_response("RENDER_ERROR", &format!("HTML rendering failed: {}", e)),
    };

    if let Err(e) = std::fs::write(&output_path_buf, pdf_bytes) {
        return error_response("FILE_WRITE_ERROR", &format!("Cannot write PDF: {}", e));
    }

    success_response(json!({
        "output_path": output_path_str,
        "html_length": html.len()
    }))
}

fn merge_pdfs(params: Option<serde_json::Value>) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return error_response("INVALID_REQUEST", "Missing parameters"),
    };

    let pdf1_path = get_required_string!(&params, "pdf1_path");
    let pdf2_path = get_required_string!(&params, "pdf2_path");
    let output_path_str = get_required_string!(&params, "output_path");

    if let Err(e) = validate_output_path(output_path_str) {
        return e;
    }

    let pdf1_bytes = match read_file_with_limit(pdf1_path, MAX_PDF_SIZE) {
        Ok(b) => b,
        Err(e) => return e,
    };

    let pdf2_bytes = match read_file_with_limit(pdf2_path, MAX_PDF_SIZE) {
        Ok(b) => b,
        Err(e) => return e,
    };

    let mut doc1 = match harumi::Document::from_bytes(&pdf1_bytes) {
        Ok(d) => d,
        Err(e) => {
            return error_response(
                "INVALID_PDF",
                &format!("Invalid PDF ({}): {}", pdf1_path, e),
            );
        }
    };

    let doc2 = match harumi::Document::from_bytes(&pdf2_bytes) {
        Ok(d) => d,
        Err(e) => {
            return error_response(
                "INVALID_PDF",
                &format!("Invalid PDF ({}): {}", pdf2_path, e),
            );
        }
    };

    let doc1_page_count = doc1.page_count();
    let doc2_page_count = doc2.page_count();

    if doc1_page_count == 0 {
        return error_response("NO_PAGES", "First PDF contains no pages");
    }

    if doc2_page_count == 0 {
        return error_response("NO_PAGES", "Second PDF contains no pages");
    }

    if let Err(e) = doc1.merge_from(doc2) {
        return error_response("MERGE_ERROR", &format!("Cannot merge PDFs: {}", e));
    }

    if let Err(e) = doc1.save(output_path_str) {
        return error_response(
            "FILE_WRITE_ERROR",
            &format!("Cannot write PDF to '{}': {}", output_path_str, e),
        );
    }

    success_response(json!({
        "output_path": output_path_str,
        "first_pdf_pages": doc1_page_count,
        "second_pdf_pages": doc2_page_count,
        "merged_page_count": doc1_page_count + doc2_page_count
    }))
}

fn page_info(params: Option<serde_json::Value>) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return error_response("INVALID_REQUEST", "Missing parameters"),
    };

    let pdf_path = get_required_string!(&params, "pdf_path");

    let pdf_bytes = match read_file_with_limit(pdf_path, MAX_PDF_SIZE) {
        Ok(b) => b,
        Err(e) => return e,
    };

    let mut doc = match harumi::Document::from_bytes(&pdf_bytes) {
        Ok(d) => d,
        Err(e) => return error_response("INVALID_PDF", &format!("Invalid PDF: {}", e)),
    };

    let page_count = doc.page_count();

    if page_count == 0 {
        return success_response(json!({
            "page_count": 0,
            "pages": []
        }));
    }

    let mut pages = Vec::new();
    for page_num in 1..=page_count {
        match doc.page(page_num) {
            Ok(p) => match p.size() {
                Ok((width, height)) => {
                    pages.push(json!({
                        "page": page_num,
                        "width": width,
                        "height": height
                    }));
                }
                Err(e) => {
                    pages.push(json!({
                        "page": page_num,
                        "error": format!("Cannot get page size: {}", e)
                    }));
                }
            },
            Err(e) => {
                pages.push(json!({
                    "page": page_num,
                    "error": format!("Cannot access page: {}", e)
                }));
            }
        }
    }

    success_response(json!({
        "page_count": page_count,
        "pages": pages
    }))
}

fn send_response(id: Option<serde_json::Value>, result: serde_json::Value) {
    let response = json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(serde_json::Value::Null),
        "result": result
    });

    println!("{}", response);
}

fn main() {
    let stdin = io::stdin();
    let reader = stdin.lock();

    for line in reader.lines().map_while(Result::ok) {
        if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(&line) {
            let result = match req.method.as_str() {
                "initialize" => success_response(json!({
                    "protocolVersion": MCP_VERSION,
                    "capabilities": {},
                    "serverInfo": {
                        "name": "harumi-mcp",
                        "version": "0.1.0"
                    }
                })),
                "tools/list" => success_response(json!({"tools": list_tools()})),
                "tools/call" => {
                    let tool_name = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");

                    let args = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("arguments"))
                        .cloned();

                    match tool_name {
                        "pdf_extract_text" => extract_text(args),
                        "pdf_extract_all_pages" => extract_all_pages(args),
                        "pdf_extract_text_structured" => extract_text_structured(args),
                        "pdf_replace_text" => replace_text(args),
                        "pdf_rotate_page" => rotate_page(args),
                        "pdf_add_invisible_text" => add_invisible_text(args),
                        "pdf_html_to_pdf" => html_to_pdf(args),
                        "pdf_merge" => merge_pdfs(args),
                        "pdf_page_info" => page_info(args),
                        _ => {
                            error_response("UNKNOWN_TOOL", &format!("Unknown tool: {}", tool_name))
                        }
                    }
                }
                _ => error_response("UNKNOWN_METHOD", &format!("Unknown method: {}", req.method)),
            };

            send_response(req.id, result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_response_shape() {
        let err = error_response("TEST_ERROR", "Test message");
        assert_eq!(err["error"], "Test message");
        assert_eq!(err["code"], "TEST_ERROR");
    }

    #[test]
    fn test_success_response_shape() {
        let data = json!({"foo": "bar"});
        let result = success_response(data);
        assert_eq!(result["success"], true);
        assert_eq!(result["result"]["foo"], "bar");
    }

    #[test]
    fn test_replace_save_error_non_identity_font_map() {
        let result = replace_save_error(
            "out.pdf",
            ReplaceMode::Wrap,
            "invalid input: replace_text_resubset only supports CIDToGIDMap=Identity",
        );

        assert_eq!(result["code"], "UNSUPPORTED_FONT_MAP");
        assert!(result["error"].as_str().unwrap().contains("new_font"));
    }

    #[test]
    fn test_replace_save_error_generic_write_error() {
        let result = replace_save_error("out.pdf", ReplaceMode::NewFont, "permission denied");

        assert_eq!(result["code"], "FILE_WRITE_ERROR");
        assert!(result["error"].as_str().unwrap().contains("out.pdf"));
    }

    #[test]
    fn test_font_size_limit_allows_common_cjk_ttf() {
        // Google Fonts' NotoSansSC variable TTF is about 17.8 MB.
        assert!(MAX_FONT_SIZE >= 17_800_000);
    }

    #[test]
    fn test_replace_text_schema_exposes_strict_mode() {
        let tools = list_tools();
        let replace_tool = tools
            .iter()
            .find(|tool| tool.name == "pdf_replace_text")
            .expect("pdf_replace_text tool should be listed");

        assert_eq!(
            replace_tool.input_schema["properties"]["strict"]["type"],
            "boolean"
        );
    }

    #[test]
    fn test_extract_text_missing_params() {
        let result = extract_text(None);
        assert_eq!(result["code"], "INVALID_REQUEST");
    }

    #[test]
    fn test_extract_text_zero_page() {
        let params = json!({
            "pdf_path": "/tmp/test.pdf",
            "page": 0
        });
        let result = extract_text(Some(params));
        assert_eq!(result["code"], "INVALID_PARAMS");
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("Page number must be >= 1")
        );
    }

    #[test]
    fn test_extract_text_empty_path() {
        let params = json!({
            "pdf_path": "",
            "page": 1
        });
        let result = extract_text(Some(params));
        assert_eq!(result["code"], "INVALID_PARAMS");
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("cannot be empty")
        );
    }

    #[test]
    fn test_extract_text_nonexistent_file() {
        let params = json!({
            "pdf_path": "nonexistent/path/test.pdf",
            "page": 1
        });
        let result = extract_text(Some(params));
        assert_eq!(result["code"], "FILE_READ_ERROR");
    }

    #[test]
    fn test_replace_text_empty_replacements() {
        let params = json!({
            "pdf_path": "test.pdf",
            "output_path": "out.pdf",
            "replacements": [],
            "font_path": "font.ttf"
        });
        let result = replace_text(Some(params));
        assert_eq!(result["code"], "INVALID_PARAMS");
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("cannot be empty")
        );
    }

    #[test]
    fn test_replace_text_invalid_mode() {
        let params = json!({
            "pdf_path": "test.pdf",
            "output_path": "out.pdf",
            "replacements": [{"old_text": "a", "new_text": "b"}],
            "font_path": "font.ttf",
            "mode": "invalid"
        });
        let result = replace_text(Some(params));
        assert_eq!(result["code"], "INVALID_PARAMS");
        assert!(result["error"].as_str().unwrap().contains("Invalid mode"));
    }

    #[test]
    fn test_replace_text_pages_non_integer() {
        // This test requires a valid PDF file to reach the pages validation
        // For now, we just verify the error handling path exists (implicit in code review)
        // A full integration test would need a real PDF file
    }

    #[test]
    fn test_rotate_page_invalid_degrees() {
        let params = json!({
            "pdf_path": "test.pdf",
            "output_path": "out.pdf",
            "page": 1,
            "degrees": 45
        });
        let result = rotate_page(Some(params));
        assert_eq!(result["code"], "INVALID_PARAMS");
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("Invalid rotation angle")
        );
    }

    #[test]
    fn test_add_invisible_text_zero_size() {
        let params = json!({
            "pdf_path": "test.pdf",
            "output_path": "out.pdf",
            "font_path": "font.ttf",
            "page": 1,
            "text": "test",
            "x": 10.0,
            "y": 10.0,
            "size": 0
        });
        let result = add_invisible_text(Some(params));
        assert_eq!(result["code"], "INVALID_PARAMS");
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("font size must be > 0")
        );
    }

    #[test]
    fn test_add_invisible_text_negative_xy() {
        let params = json!({
            "pdf_path": "test.pdf",
            "output_path": "out.pdf",
            "font_path": "font.ttf",
            "page": 1,
            "text": "test",
            "x": -1.0,
            "y": 10.0,
            "size": 12.0
        });
        let result = add_invisible_text(Some(params));
        assert_eq!(result["code"], "INVALID_PARAMS");
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("coordinate must be >= 0")
        );
    }

    #[test]
    fn test_html_to_pdf_empty_html() {
        let params = json!({
            "html": "",
            "output_path": "out.pdf"
        });
        let result = html_to_pdf(Some(params));
        assert_eq!(result["code"], "INVALID_PARAMS");
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("cannot be empty")
        );
    }

    #[test]
    fn test_html_to_pdf_empty_output_path() {
        let params = json!({
            "html": "<p>Test</p>",
            "output_path": ""
        });
        let result = html_to_pdf(Some(params));
        assert_eq!(result["code"], "INVALID_PARAMS");
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("cannot be empty")
        );
    }

    #[test]
    fn test_merge_pdfs_empty_paths() {
        let params = json!({
            "pdf1_path": "",
            "pdf2_path": "pdf2.pdf",
            "output_path": "out.pdf"
        });
        let result = merge_pdfs(Some(params));
        assert_eq!(result["code"], "INVALID_PARAMS");
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("cannot be empty")
        );
    }

    #[test]
    fn test_page_info_empty_path() {
        let params = json!({
            "pdf_path": ""
        });
        let result = page_info(Some(params));
        assert_eq!(result["code"], "INVALID_PARAMS");
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("cannot be empty")
        );
    }

    #[test]
    fn test_page_info_nonexistent_file() {
        let params = json!({
            "pdf_path": "nonexistent/file.pdf"
        });
        let result = page_info(Some(params));
        assert_eq!(result["code"], "FILE_READ_ERROR");
    }
}
