use crate::{Error, Result};
use harumi::{ChunkType, Document};
use serde::{Deserialize, Serialize};

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
            pages.push(PageContent {
                page_num,
                size,
                blocks,
            });
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
            .map(|p| PageInBatch {
                page: p.page_num,
                blocks: &p.blocks,
            })
            .collect(),
    };
    serde_json::to_string(&batch)
        .map_err(|e| Error::Translator(format!("failed to serialize pages: {e}")))
}

/// Deserialise the LLM response into per-page translated block lists.
///
/// Returns one `Vec<TranslatedBlock>` per page, in the same order as the input.
/// Strips markdown code fences before parsing. On failure, attempts to repair
/// unescaped double-quotes inside `"text"` string values before retrying.
pub(crate) fn json_to_translated_pages(raw: &str) -> Result<Vec<Vec<TranslatedBlock>>> {
    let s = strip_code_fence(raw);
    if let Ok(b) = serde_json::from_str::<TranslatedBatchJson>(s) {
        return Ok(b.pages.into_iter().map(|p| p.blocks).collect());
    }
    let repaired = repair_json_strings(s);
    serde_json::from_str::<TranslatedBatchJson>(&repaired)
        .map(|b| b.pages.into_iter().map(|p| p.blocks).collect())
        .map_err(|e| Error::Translator(format!("LLM response is not valid JSON: {e}. Raw: {raw}")))
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

/// Repair unescaped double-quote characters inside JSON `"text":"..."` string values.
///
/// LLMs occasionally omit the `\"` escaping when the translated text itself
/// contains double-quotes (e.g. section references like `"8 Safety measures"`).
/// This function uses a character-level scan to detect and escape interior `"`
/// characters whose position cannot be the closing quote of the string value.
///
/// A `"` is treated as the *closing* quote when the immediately following
/// non-whitespace is `}`, `]`, or `,"` (the start of the next key). Everything
/// else is treated as an interior quote and escaped to `\"`.
fn repair_json_strings(s: &str) -> String {
    const MARKER: &str = "\"text\":\"";
    let mut result = String::with_capacity(s.len() + 64);
    let mut rest = s;

    while let Some(pos) = rest.find(MARKER) {
        result.push_str(&rest[..pos + MARKER.len()]);
        let after_open = &rest[pos + MARKER.len()..];

        let mut value = String::new();
        let mut consumed = after_open.len();
        let mut prev_backslash = false;

        for (i, c) in after_open.char_indices() {
            if prev_backslash {
                value.push('\\');
                value.push(c);
                prev_backslash = false;
                continue;
            }
            match c {
                '\\' => {
                    prev_backslash = true;
                }
                '"' => {
                    let tail = after_open[i + c.len_utf8()..].trim_start();
                    let is_close = tail.starts_with('}')
                        || tail.starts_with(']')
                        || tail.starts_with(",\"")
                        || tail.is_empty();
                    if is_close {
                        consumed = i + c.len_utf8();
                        break;
                    }
                    value.push('\\');
                    value.push('"');
                }
                _ => {
                    value.push(c);
                }
            }
        }

        result.push_str(&value);
        result.push('"');
        rest = &after_open[consumed..];
    }

    result.push_str(rest);
    result
}

#[cfg(test)]
mod repair_tests {
    use super::repair_json_strings;

    #[test]
    fn no_change_when_already_valid() {
        let s = r#"{"pages":[{"blocks":[{"id":1,"text":"hello world"}]}]}"#;
        let r = repair_json_strings(s);
        assert!(serde_json::from_str::<serde_json::Value>(&r).is_ok());
        assert!(r.contains("hello world"));
    }

    #[test]
    fn escapes_interior_quotes_in_text_value() {
        // Claude output where section name is wrapped in unescaped quotes
        let broken = r#"{"pages":[{"blocks":[{"id":9,"text":"（参照"8 防護措置"章节）"}]}]}"#;
        let repaired = repair_json_strings(broken);
        let v: serde_json::Value =
            serde_json::from_str(&repaired).expect("repaired JSON should parse");
        let text = v["pages"][0]["blocks"][0]["text"].as_str().unwrap();
        assert!(
            text.contains('"'),
            "interior quotes should be preserved in value: {text}"
        );
        assert!(text.starts_with('（'));
        assert!(text.ends_with('）'));
    }

    #[test]
    fn handles_multiple_blocks() {
        let broken = r#"{"pages":[{"blocks":[{"id":1,"text":"see "section 1""},{"id":2,"text":"plain text"}]}]}"#;
        let repaired = repair_json_strings(broken);
        let v: serde_json::Value =
            serde_json::from_str(&repaired).expect("repaired JSON should parse");
        let t1 = v["pages"][0]["blocks"][0]["text"].as_str().unwrap();
        let t2 = v["pages"][0]["blocks"][1]["text"].as_str().unwrap();
        assert!(t1.contains('"'));
        assert_eq!(t2, "plain text");
    }

    #[test]
    fn passes_through_already_escaped_quotes() {
        let valid = r#"{"pages":[{"blocks":[{"id":1,"text":"say \"hello\""}]}]}"#;
        let repaired = repair_json_strings(valid);
        let v: serde_json::Value =
            serde_json::from_str(&repaired).expect("repaired JSON should parse");
        let text = v["pages"][0]["blocks"][0]["text"].as_str().unwrap();
        assert_eq!(text, "say \"hello\"");
    }
}
