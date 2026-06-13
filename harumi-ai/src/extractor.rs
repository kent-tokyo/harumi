use harumi::{ChunkType, Document};
use serde::{Deserialize, Serialize};
use crate::{Error, Result};

// ── Internal data types ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct PageContent {
    pub page_num: u32,
    pub size: (f32, f32),
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Block {
    pub id: usize,
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
}

// ── JSON wire types ─────────────────────────────────────────────────────────

#[derive(Serialize)]
struct PageInBatch<'a> {
    page: u32,
    blocks: &'a [Block],
}

#[derive(Serialize)]
struct BatchJson<'a> {
    pages: Vec<PageInBatch<'a>>,
}

#[derive(Deserialize)]
struct TranslatedBatchJson {
    pages: Vec<TranslatedPageJson>,
}

#[derive(Deserialize)]
struct TranslatedPageJson {
    blocks: Vec<TranslatedBlock>,
}

#[derive(Deserialize)]
pub(crate) struct TranslatedBlock {
    pub id: usize,
    pub text: String,
}

// ── Extraction ───────────────────────────────────────────────────────────────

/// Extract structured content from every page of `doc`.
/// Pages with no visible text are omitted.
pub(crate) fn extract_pages(doc: &mut Document) -> Result<Vec<PageContent>> {
    let page_count = doc.page_count();
    let mut pages = Vec::new();

    for page_num in 1..=page_count {
        let size = {
            let ph = doc.page(page_num)?;
            let mb = ph.media_box()?;
            (mb[2], mb[3])
        };

        let chunks = doc.extract_text_chunks(page_num)?;
        let blocks: Vec<Block> = chunks
            .into_iter()
            .enumerate()
            .filter(|(_, c)| !c.text.trim().is_empty())
            .map(|(id, chunk)| Block {
                id,
                block_type: chunk_type_to_str(&chunk.chunk_type),
                text: chunk.text,
            })
            .collect();

        if !blocks.is_empty() {
            pages.push(PageContent { page_num, size, blocks });
        }
    }
    Ok(pages)
}

// ── JSON helpers ─────────────────────────────────────────────────────────────

/// Serialise one or more pages into the JSON string sent to the LLM.
///
/// Always uses the `{"pages": [...]}` envelope so the same format works for
/// both single-page and multi-page (cross-context) requests.
pub(crate) fn pages_to_json(pages: &[PageContent]) -> Result<String> {
    let batch = BatchJson {
        pages: pages
            .iter()
            .map(|p| PageInBatch { page: p.page_num, blocks: &p.blocks })
            .collect(),
    };
    serde_json::to_string(&batch)
        .map_err(|e| Error::Translator(format!("failed to serialize pages: {e}")))
}

/// Deserialise the LLM response into per-page translated block lists.
///
/// Returns one `Vec<TranslatedBlock>` per page, in the same order as the input.
/// Strips markdown code fences before parsing.
pub(crate) fn json_to_translated_pages(raw: &str) -> Result<Vec<Vec<TranslatedBlock>>> {
    let s = strip_code_fence(raw);
    serde_json::from_str::<TranslatedBatchJson>(s)
        .map(|b| b.pages.into_iter().map(|p| p.blocks).collect())
        .map_err(|e| {
            Error::Translator(format!(
                "LLM response is not valid JSON: {e}. Raw: {raw}"
            ))
        })
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn chunk_type_to_str(ct: &ChunkType) -> String {
    match ct {
        ChunkType::Heading(n) => format!("h{n}"),
        ChunkType::Paragraph => "paragraph".to_owned(),
        _ => "paragraph".to_owned(),
    }
}

fn strip_code_fence(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix("```json").unwrap_or(s);
    let s = s.strip_prefix("```").unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s);
    s.trim()
}
