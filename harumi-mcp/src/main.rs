use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{self, BufRead};

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
        None => return json!({"error": "Missing parameters"}),
    };

    let pdf_path = match params.get("pdf_path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing pdf_path"}),
    };

    let page = match params.get("page").and_then(|p| p.as_u64()) {
        Some(p) => p as u32,
        None => return json!({"error": "Missing page"}),
    };

    let pdf_bytes = match std::fs::read(pdf_path) {
        Ok(b) => b,
        Err(e) => return json!({"error": format!("Cannot read PDF: {}", e)}),
    };

    let doc = match harumi::Document::from_bytes(&pdf_bytes) {
        Ok(d) => d,
        Err(e) => return json!({"error": format!("Invalid PDF: {}", e)}),
    };

    let runs = match doc.extract_text_runs(page) {
        Ok(r) => r,
        Err(e) => return json!({"error": format!("Cannot extract text: {}", e)}),
    };

    let fragments: Vec<_> = runs
        .into_iter()
        .map(|r| json!({"text": r.text, "x": r.x, "y": r.y}))
        .collect();

    json!({"fragments": fragments})
}

fn add_invisible_text(params: Option<serde_json::Value>) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return json!({"error": "Missing parameters"}),
    };

    let pdf_path = match params.get("pdf_path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing pdf_path"}),
    };

    let output_path = match params.get("output_path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing output_path"}),
    };

    let font_path = match params.get("font_path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing font_path"}),
    };

    let page = match params.get("page").and_then(|p| p.as_u64()) {
        Some(p) => p as u32,
        None => return json!({"error": "Missing page"}),
    };

    let text = match params.get("text").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing text"}),
    };

    let x = match params.get("x").and_then(|p| p.as_f64()) {
        Some(p) => p as f32,
        None => return json!({"error": "Missing x"}),
    };

    let y = match params.get("y").and_then(|p| p.as_f64()) {
        Some(p) => p as f32,
        None => return json!({"error": "Missing y"}),
    };

    let size = match params.get("size").and_then(|p| p.as_f64()) {
        Some(p) => p as f32,
        None => return json!({"error": "Missing size"}),
    };

    let pdf_bytes = match std::fs::read(pdf_path) {
        Ok(b) => b,
        Err(e) => return json!({"error": format!("Cannot read input PDF: {}", e)}),
    };

    let font_bytes = match std::fs::read(font_path) {
        Ok(b) => b,
        Err(e) => return json!({"error": format!("Cannot read font: {}", e)}),
    };

    let mut doc = match harumi::Document::from_bytes(&pdf_bytes) {
        Ok(d) => d,
        Err(e) => return json!({"error": format!("Invalid input PDF: {}", e)}),
    };

    let font = match doc.embed_font(&font_bytes) {
        Ok(f) => f,
        Err(e) => return json!({"error": format!("Cannot embed font: {}", e)}),
    };

    match doc.page(page) {
        Ok(mut p) => {
            if let Err(e) = p.add_invisible_text(text, font, [x, y], size) {
                return json!({"error": format!("Cannot add invisible text: {}", e)});
            }
        }
        Err(e) => return json!({"error": format!("Invalid page: {}", e)}),
    }

    if let Err(e) = doc.save(output_path) {
        return json!({"error": format!("Cannot save PDF: {}", e)});
    }

    json!({
        "success": true,
        "message": format!("Invisible text added to page {}", page),
        "output_path": output_path
    })
}

fn html_to_pdf(params: Option<serde_json::Value>) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return json!({"error": "Missing parameters"}),
    };

    let html = match params.get("html").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing html"}),
    };

    let output_path = match params.get("output_path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing output_path"}),
    };

    let _title = params.get("title").and_then(|p| p.as_str()).map(|s| s.to_string());

    let options = harumi::flow::html::HtmlRenderOptions::default();

    let pdf_bytes = match harumi::flow::html::render_html_to_pdf(html, options) {
        Ok(b) => b,
        Err(e) => return json!({"error": format!("HTML rendering failed: {}", e)}),
    };

    if let Err(e) = std::fs::write(output_path, pdf_bytes) {
        return json!({"error": format!("Cannot write PDF: {}", e)});
    }

    json!({
        "success": true,
        "message": "HTML converted to PDF",
        "output_path": output_path
    })
}

fn merge_pdfs(params: Option<serde_json::Value>) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return json!({"error": "Missing parameters"}),
    };

    let pdf1_path = match params.get("pdf1_path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing pdf1_path"}),
    };

    let pdf2_path = match params.get("pdf2_path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing pdf2_path"}),
    };

    let output_path = match params.get("output_path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing output_path"}),
    };

    let pdf1_bytes = match std::fs::read(pdf1_path) {
        Ok(b) => b,
        Err(e) => return json!({"error": format!("Cannot read first PDF: {}", e)}),
    };

    let pdf2_bytes = match std::fs::read(pdf2_path) {
        Ok(b) => b,
        Err(e) => return json!({"error": format!("Cannot read second PDF: {}", e)}),
    };

    let mut doc1 = match harumi::Document::from_bytes(&pdf1_bytes) {
        Ok(d) => d,
        Err(e) => return json!({"error": format!("Invalid first PDF: {}", e)}),
    };

    let doc2 = match harumi::Document::from_bytes(&pdf2_bytes) {
        Ok(d) => d,
        Err(e) => return json!({"error": format!("Invalid second PDF: {}", e)}),
    };

    if let Err(e) = doc1.merge_from(doc2) {
        return json!({"error": format!("Cannot merge: {}", e)});
    }

    if let Err(e) = doc1.save(output_path) {
        return json!({"error": format!("Cannot save merged PDF: {}", e)});
    }

    json!({
        "success": true,
        "message": "PDFs merged successfully",
        "output_path": output_path
    })
}

fn page_info(params: Option<serde_json::Value>) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return json!({"error": "Missing parameters"}),
    };

    let pdf_path = match params.get("pdf_path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing pdf_path"}),
    };

    let pdf_bytes = match std::fs::read(pdf_path) {
        Ok(b) => b,
        Err(e) => return json!({"error": format!("Cannot read PDF: {}", e)}),
    };

    let mut doc = match harumi::Document::from_bytes(&pdf_bytes) {
        Ok(d) => d,
        Err(e) => return json!({"error": format!("Invalid PDF: {}", e)}),
    };

    let page_count = doc.page_count();

    let (width, height) = match doc.page(1) {
        Ok(p) => match p.size() {
            Ok(s) => s,
            Err(e) => return json!({"error": format!("Cannot get page size: {}", e)}),
        },
        Err(e) => return json!({"error": format!("Cannot access page 1: {}", e)}),
    };

    json!({
        "page_count": page_count,
        "page_width": width,
        "page_height": height
    })
}

fn send_response(id: Option<serde_json::Value>, result: serde_json::Value) {
    let response = if let Some(id) = id {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        })
    } else {
        json!({
            "jsonrpc": "2.0",
            "result": result
        })
    };

    println!("{}", response.to_string());
}

fn main() {
    let stdin = io::stdin();
    let reader = stdin.lock();

    // Send initialize response on startup
    send_response(
        Some(json!(1)),
        json!({
            "protocolVersion": "2024-11-25",
            "capabilities": {},
            "serverInfo": {
                "name": "harumi-mcp",
                "version": "0.1.0"
            }
        }),
    );

    // Process incoming requests line by line
    for line in reader.lines() {
        if let Ok(line) = line {
            if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(&line) {
                let result = match req.method.as_str() {
                    "tools/list" => json!({"tools": list_tools()}),
                    "tools/call" => {
                        let tool_name = req
                            .params
                            .as_ref()
                            .and_then(|p| p.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown");

                        let args = req.params.as_ref().and_then(|p| p.get("arguments")).cloned();

                        match tool_name {
                            "pdf_extract_text" => extract_text(args),
                            "pdf_add_invisible_text" => add_invisible_text(args),
                            "pdf_html_to_pdf" => html_to_pdf(args),
                            "pdf_merge" => merge_pdfs(args),
                            "pdf_page_info" => page_info(args),
                            _ => json!({"error": format!("Unknown tool: {}", tool_name)}),
                        }
                    }
                    _ => json!({"error": format!("Unknown method: {}", req.method)}),
                };

                send_response(req.id, result);
            }
        }
    }
}
