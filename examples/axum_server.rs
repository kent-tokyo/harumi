//! Axum web server example — PDF processing API using harumi.
//!
//! Shows how to build a PDF-processing web service where users upload PDFs,
//! harumi processes them in-memory, and the result is returned as bytes.
//! No temporary files needed; all I/O goes through `from_bytes` / `save_to_bytes`.
//!
//! # Usage
//!
//! ```bash
//! FONT_PATH=/path/to/NotoSansCJK.ttf cargo run --example axum_server
//! # or
//! cargo run --example axum_server -- /path/to/font.ttf
//! ```
//!
//! # Endpoints
//!
//! | Method | Path          | Description                                   |
//! |--------|---------------|-----------------------------------------------|
//! | GET    | `/health`     | Health check                                  |
//! | POST   | `/stamp`      | Stamp visible text onto PDF pages             |
//! | POST   | `/ocr-layer`  | Add invisible OCR text layer to all pages     |
//!
//! # Example requests
//!
//! ```bash
//! # Stamp "APPROVED" on all pages (red, centered)
//! curl -F pdf=@input.pdf -F text="APPROVED" \
//!      http://localhost:3000/stamp -o output.pdf
//!
//! # Stamp on page 1 only
//! curl -F pdf=@input.pdf -F text="DRAFT" -F page=1 \
//!      http://localhost:3000/stamp -o output.pdf
//!
//! # Add invisible OCR text layer (for search/copy)
//! curl -F pdf=@scanned.pdf -F text="検索可能なテキスト" -F x=100 -F y=700 \
//!      http://localhost:3000/ocr-layer -o searchable.pdf
//! ```

use std::sync::Arc;

use axum::{
    Router,
    extract::{Multipart, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use harumi::Document;

// ── App state ────────────────────────────────────────────────────────────────

/// Font bytes loaded once at startup and shared across requests.
#[derive(Clone)]
struct AppState {
    font: Arc<Vec<u8>>,
}

// ── Error handling ────────────────────────────────────────────────────────────

enum AppError {
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        let body = format!(r#"{{"error":"{}"}}"#, msg.replace('"', "\\\""));
        (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
    }
}

type AppResult<T> = Result<T, AppError>;

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let font_path = std::env::var("FONT_PATH")
        .ok()
        .or_else(|| std::env::args().nth(1))
        .unwrap_or_else(|| {
            eprintln!("Provide a font path via FONT_PATH env var or as a CLI argument.");
            eprintln!("  FONT_PATH=/path/to/NotoSansCJK.ttf cargo run --example axum_server");
            std::process::exit(1);
        });

    let font = std::fs::read(&font_path).unwrap_or_else(|e| {
        eprintln!("Cannot read font '{}': {}", font_path, e);
        std::process::exit(1);
    });

    let state = AppState { font: Arc::new(font) };

    let app = Router::new()
        .route("/health", get(health))
        .route("/stamp", post(stamp))
        .route("/ocr-layer", post(ocr_layer))
        .with_state(state);

    let addr = "0.0.0.0:3000";
    println!("harumi PDF server on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn health() -> &'static str {
    "OK"
}

/// POST /stamp
///
/// Multipart fields:
/// - `pdf`  (required) — PDF file
/// - `text` (required) — Text to stamp in red
/// - `page` (optional) — 1-indexed page number; omit to stamp all pages
async fn stamp(State(state): State<AppState>, mut multipart: Multipart) -> AppResult<Response> {
    let mut pdf_bytes: Option<Vec<u8>> = None;
    let mut text: Option<String> = None;
    let mut page_num: Option<u32> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        match field.name() {
            Some("pdf") => {
                pdf_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| AppError::Internal(e.to_string()))?
                        .to_vec(),
                );
            }
            Some("text") => {
                text = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::BadRequest(e.to_string()))?,
                );
            }
            Some("page") => {
                let s = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                page_num = Some(
                    s.trim()
                        .parse()
                        .map_err(|_| AppError::BadRequest("'page' must be a positive integer".into()))?,
                );
            }
            _ => {}
        }
    }

    let pdf_bytes = pdf_bytes.ok_or_else(|| AppError::BadRequest("missing 'pdf' field".into()))?;
    let text = text.ok_or_else(|| AppError::BadRequest("missing 'text' field".into()))?;

    let mut doc =
        Document::from_bytes(&pdf_bytes).map_err(|e| AppError::Internal(e.to_string()))?;
    let font = doc
        .embed_font(&state.font)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let pages: Vec<u32> = match page_num {
        Some(p) => vec![p],
        None => (1..=doc.page_count()).collect(),
    };

    for p in pages {
        // Get page size for centering; fall back to a fixed position if unavailable.
        // PageHandle is a temporary borrow — it is dropped before the next doc.page() call.
        let pos = doc
            .page(p)
            .map_err(|e| AppError::Internal(e.to_string()))?
            .size()
            .ok()
            .map(|(w, h)| [w / 2.0 - 40.0, h / 2.0])
            .unwrap_or([200.0, 400.0]);

        doc.page(p)
            .map_err(|e| AppError::Internal(e.to_string()))?
            .add_text(&text, font, pos, 24.0, [0.8, 0.0, 0.0])
            .map_err(|e| AppError::Internal(e.to_string()))?;
    }

    pdf_response(doc)
}

/// POST /ocr-layer
///
/// Multipart fields:
/// - `pdf`  (required) — PDF file (typically a scanned image PDF)
/// - `text` (required) — Invisible OCR text to embed on every page
/// - `x`    (optional) — X coordinate in points (default: 72)
/// - `y`    (optional) — Y coordinate in points (default: 72)
async fn ocr_layer(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    let mut pdf_bytes: Option<Vec<u8>> = None;
    let mut text: Option<String> = None;
    let mut x: f32 = 72.0;
    let mut y: f32 = 72.0;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        match field.name() {
            Some("pdf") => {
                pdf_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| AppError::Internal(e.to_string()))?
                        .to_vec(),
                );
            }
            Some("text") => {
                text = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::BadRequest(e.to_string()))?,
                );
            }
            Some("x") => {
                let s = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                x = s
                    .trim()
                    .parse()
                    .map_err(|_| AppError::BadRequest("'x' must be a number".into()))?;
            }
            Some("y") => {
                let s = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                y = s
                    .trim()
                    .parse()
                    .map_err(|_| AppError::BadRequest("'y' must be a number".into()))?;
            }
            _ => {}
        }
    }

    let pdf_bytes = pdf_bytes.ok_or_else(|| AppError::BadRequest("missing 'pdf' field".into()))?;
    let text = text.ok_or_else(|| AppError::BadRequest("missing 'text' field".into()))?;

    let mut doc =
        Document::from_bytes(&pdf_bytes).map_err(|e| AppError::Internal(e.to_string()))?;
    let font = doc
        .embed_font(&state.font)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    for p in 1..=doc.page_count() {
        doc.page(p)
            .map_err(|e| AppError::Internal(e.to_string()))?
            .add_invisible_text(&text, font, [x, y], 12.0)
            .map_err(|e| AppError::Internal(e.to_string()))?;
    }

    pdf_response(doc)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn pdf_response(mut doc: Document) -> AppResult<Response> {
    let bytes = doc
        .save_to_bytes()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(([(header::CONTENT_TYPE, "application/pdf")], bytes).into_response())
}
