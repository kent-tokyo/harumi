//! HTML-to-PDF renderer backed by [`FlowDocument`].
//!
//! Enabled by the `html` feature flag (implies `flow`).
//!
//! # Supported HTML elements
//!
//! | Element | Mapping |
//! |---------|---------|
//! | `<h1>`–`<h6>` | Heading at the corresponding level |
//! | `<p>` | Body paragraph |
//! | `<table><tr><th/td>` | Two-column key/value table |
//! | `<ul><li>` | Bulleted list |
//! | `<ol><li>` | Numbered list |
//! | `<br>` | (ignored; use paragraph breaks instead) |
//! | `style="page-break-after: always"` / `class="page-break"` | Page break |
//! | `<div>`, `<section>`, `<article>`, … | Block container; children are processed |
//! | `<strong>`, `<em>`, … | Text content extracted; styling ignored in v1 |
//! | `<head>`, `<script>`, `<style>`, … | Skipped entirely |

use scraper::{ElementRef, Html};

use crate::{Error, Result};

use super::{FlowDocument, FlowOptions, InlineSpan, Margins};

/// Options for [`render_html_to_pdf`].
pub struct HtmlRenderOptions {
    /// Raw TTF/OTF font bytes (required). CJK fonts such as NotoSansCJK are supported.
    pub font_bytes: Vec<u8>,
    /// Page width and height in PDF points. Default: A4 (595 × 842).
    pub page_size: (f32, f32),
    /// Page margins. Default: [`Margins::a4_standard`] (20 mm on all sides).
    pub margins: Margins,
    /// Body text font size in PDF points. Default: 11.0.
    pub body_font_size: f32,
    /// Line height multiplier relative to font size. Default: 1.4.
    pub line_height_factor: f32,
    /// Maximum number of pages that may be generated.
    ///
    /// Prevents DoS from very large HTML inputs. Default: 2000.
    pub max_pages: u32,
}

impl Default for HtmlRenderOptions {
    fn default() -> Self {
        HtmlRenderOptions {
            font_bytes: Vec::new(),
            page_size: (595.0, 842.0),
            margins: Margins::a4_standard(),
            body_font_size: 11.0,
            line_height_factor: 1.4,
            max_pages: 2000,
        }
    }
}

/// Renders an HTML string to PDF bytes.
///
/// The HTML is parsed and mapped to [`FlowDocument`] block elements.
/// Only a document-oriented subset of HTML is supported; see the module docs
/// for the complete element mapping.
///
/// `options.font_bytes` must be non-empty; all other fields have sensible defaults.
///
/// # Errors
/// Returns [`Error::InvalidInput`] if `font_bytes` is empty or `max_pages` is exceeded.
/// Other errors propagate from font embedding or PDF writing.
pub fn render_html_to_pdf(html: &str, options: HtmlRenderOptions) -> Result<Vec<u8>> {
    if options.font_bytes.is_empty() {
        return Err(Error::InvalidInput(
            "HtmlRenderOptions.font_bytes must be set to a valid TTF/OTF font".into(),
        ));
    }

    let flow_opts = FlowOptions {
        page_size: options.page_size,
        margins: options.margins,
        body_font_size: options.body_font_size,
        line_height_factor: options.line_height_factor,
        max_pages: options.max_pages,
        ..FlowOptions::default()
    };

    let mut flow = FlowDocument::new(options.font_bytes, flow_opts)?;

    let document = Html::parse_document(html);
    // Walk the tree iteratively to avoid stack overflows from deeply nested HTML.
    walk_iterative(document.root_element(), &mut flow)?;

    flow.render()
}

// ── Iterative tree walker ─────────────────────────────────────────────────────

/// Iterative depth-first traversal of the element tree.
///
/// Using an explicit stack instead of recursion prevents stack overflows when
/// processing deeply nested HTML (e.g. `<div><div><div>…</div></div></div>`).
fn walk_iterative<'a>(root: ElementRef<'a>, flow: &mut FlowDocument) -> Result<()> {
    let mut stack: Vec<ElementRef<'a>> = vec![root];

    while let Some(elem) = stack.pop() {
        process_one(elem, flow, &mut stack)?;
    }

    Ok(())
}

/// Process a single element. If this element is a block container, its children
/// are pushed onto `stack` in reverse order (so the first child is processed first).
fn process_one<'a>(
    elem: ElementRef<'a>,
    flow: &mut FlowDocument,
    stack: &mut Vec<ElementRef<'a>>,
) -> Result<()> {
    let tag = elem.value().name();

    match tag {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level: u8 = tag[1..].parse().unwrap_or(1);
            let text = collect_text(elem);
            if !text.trim().is_empty() {
                flow.push_heading(text.trim(), level)?;
            }
        }

        "p" => {
            let spans = collect_inline_spans(elem);
            let has_content = spans.iter().any(|s| !s.text.trim().is_empty());
            if has_content {
                flow.push_paragraph_styled(&spans)?;
            }
        }

        "table" => {
            process_table(elem, flow)?;
            // Do NOT push children — table is handled as a unit.
        }

        "ul" => {
            process_list(elem, flow, false)?;
            // Do NOT push children — list is handled as a unit.
        }

        "ol" => {
            process_list(elem, flow, true)?;
            // Do NOT push children — list is handled as a unit.
        }

        // Non-content elements — skip entirely (don't push children either).
        "head" | "script" | "style" | "meta" | "link" | "title" | "noscript" => {}

        // Block containers and everything else: push children so they are processed.
        _ => {
            // Push in reverse order so the first child is at the top of the stack.
            let children: Vec<ElementRef<'_>> = elem
                .children()
                .filter_map(ElementRef::wrap)
                .collect();
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }
    }

    // page-break-after check (applied after content, before siblings)
    if has_page_break(elem) {
        flow.push_page_break()?;
    }

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn collect_text(elem: ElementRef<'_>) -> String {
    elem.text().collect()
}

/// Collect inline styled spans from an element's children, preserving bold/italic/color.
///
/// Handles: `<strong>`, `<b>` (bold), `<em>`, `<i>` (italic),
/// `<span style="color:...">` (color), `<a href="...">` (blue link color).
/// Other inline elements fall through as plain text.
fn collect_inline_spans(elem: ElementRef<'_>) -> Vec<InlineSpan> {
    let mut spans: Vec<InlineSpan> = Vec::new();
    collect_inline_spans_inner(elem, false, false, [0.0; 3], &mut spans);
    // Trim leading/trailing whitespace from the overall collection.
    if let Some(first) = spans.first_mut() {
        let trimmed = first.text.trim_start().to_owned();
        first.text = trimmed;
    }
    if let Some(last) = spans.last_mut() {
        let trimmed = last.text.trim_end().to_owned();
        last.text = trimmed;
    }
    spans.retain(|s| !s.text.is_empty());
    spans
}

fn collect_inline_spans_inner(
    elem: ElementRef<'_>,
    parent_bold: bool,
    parent_italic: bool,
    parent_color: [f32; 3],
    out: &mut Vec<InlineSpan>,
) {
    use scraper::node::Node;

    let tag = elem.value().name();
    let bold = parent_bold || matches!(tag, "strong" | "b");
    let italic = parent_italic || matches!(tag, "em" | "i");
    let color = inherited_color(elem, tag, parent_color);

    for child in elem.children() {
        match child.value() {
            Node::Text(text) => {
                let t = text.to_string();
                if !t.is_empty() {
                    out.push(InlineSpan { text: t, bold, italic, color });
                }
            }
            Node::Element(_) => {
                if let Some(child_ref) = ElementRef::wrap(child) {
                    let child_tag = child_ref.value().name();
                    // Skip non-content elements.
                    if matches!(child_tag, "script" | "style" | "head") {
                        continue;
                    }
                    collect_inline_spans_inner(child_ref, bold, italic, color, out);
                }
            }
            _ => {}
        }
    }
}

/// Resolve the effective color for an element, inheriting from parent if not overridden.
fn inherited_color(elem: ElementRef<'_>, tag: &str, parent_color: [f32; 3]) -> [f32; 3] {
    // <a> defaults to a blue link color.
    if tag == "a" {
        return [0.0, 0.0, 0.8];
    }
    // Look for inline style="color: ...".
    if let Some(style) = elem.value().attr("style") {
        if let Some(c) = parse_css_color(style) {
            return c;
        }
    }
    parent_color
}

/// Parse `color: #RRGGBB`, `color: #RGB`, or `color: rgb(r, g, b)` from a CSS style string.
/// Returns `None` if no parseable color is found.
fn parse_css_color(style: &str) -> Option<[f32; 3]> {
    // Find "color:" in style string.
    let lower = style.to_ascii_lowercase();
    let start = lower.find("color:")? + 6;
    let value = lower[start..].trim_start();

    if let Some(hex) = value.strip_prefix('#') {
        let hex = hex.split(|c: char| !c.is_ascii_hexdigit()).next()?;
        return match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
                Some([r, g, b])
            }
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()? as f32 / 255.0;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()? as f32 / 255.0;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()? as f32 / 255.0;
                Some([r, g, b])
            }
            _ => None,
        };
    }
    if let Some(inner) = value.strip_prefix("rgb(") {
        let inner = inner.split(')').next()?;
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse::<f32>().ok()? / 255.0;
            let g = parts[1].trim().parse::<f32>().ok()? / 255.0;
            let b = parts[2].trim().parse::<f32>().ok()? / 255.0;
            return Some([r, g, b]);
        }
    }
    None
}

fn has_page_break(elem: ElementRef<'_>) -> bool {
    let style = elem.value().attr("style").unwrap_or("");
    let class = elem.value().attr("class").unwrap_or("");
    style.contains("page-break-after: always")
        || style.contains("page-break-after:always")
        || class.split_whitespace().any(|c| c == "page-break")
}

/// Collects `<tr>` elements that are direct or `<tbody>`/`<thead>`/`<tfoot>`-wrapped
/// children of `table` — without descending into nested `<table>` elements.
fn table_rows(table: ElementRef<'_>) -> Vec<ElementRef<'_>> {
    let mut rows = Vec::new();
    for child in table.children().filter_map(ElementRef::wrap) {
        match child.value().name() {
            "tr" => rows.push(child),
            "tbody" | "thead" | "tfoot" => {
                // One level of tbody/thead/tfoot wrapping — stop here.
                for tr in child.children().filter_map(ElementRef::wrap) {
                    if tr.value().name() == "tr" {
                        rows.push(tr);
                    }
                }
            }
            // Nested <table> or other elements — skip.
            _ => {}
        }
    }
    rows
}

fn process_table(table: ElementRef<'_>, flow: &mut FlowDocument) -> Result<()> {
    let mut rows: Vec<(String, String)> = Vec::new();

    for tr in table_rows(table) {
        // Collect only direct <th>/<td> children of this <tr>.
        let cells: Vec<String> = tr
            .children()
            .filter_map(ElementRef::wrap)
            .filter(|e| matches!(e.value().name(), "th" | "td"))
            .map(|e| collect_text(e).trim().to_owned())
            .collect();

        match cells.len() {
            0 => {}
            1 => rows.push((cells[0].clone(), String::new())),
            _ => rows.push((cells[0].clone(), cells[1].clone())),
        }
    }

    if rows.is_empty() {
        return Ok(());
    }

    let rows_ref: Vec<(&str, &str)> =
        rows.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    flow.push_key_value_table(&rows_ref)
}

fn process_list(list: ElementRef<'_>, flow: &mut FlowDocument, ordered: bool) -> Result<()> {
    // Only collect direct <li> children to avoid duplicating text from nested lists.
    let items: Vec<String> = list
        .children()
        .filter_map(ElementRef::wrap)
        .filter(|e| e.value().name() == "li")
        .map(|li| collect_text(li).trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    if items.is_empty() {
        return Ok(());
    }

    let items_ref: Vec<&str> = items.iter().map(String::as_str).collect();
    flow.push_list(&items_ref, ordered)
}
