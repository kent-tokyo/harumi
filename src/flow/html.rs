//! HTML-to-PDF renderer backed by [`FlowDocument`].
//!
//! Enabled by the `html` feature flag (implies `flow`).
//!
//! # Supported HTML elements
//!
//! | Element | Mapping |
//! |---------|---------|
//! | `<h1>`–`<h6>` | Heading at the corresponding level |
//! | `<p>` | Body paragraph; inline `text-align` is supported |
//! | `<table><tr><th/td>` | Table cells with `colspan`/`rowspan` |
//! | `<ul><li>` | Bulleted list |
//! | `<ol><li>` | Numbered list |
//! | `<br>` | Explicit line break inside a paragraph |
//! | `style="page-break-before: always"` / `class="page-break-before"` | Page break before element |
//! | `style="page-break-after: always"` / `class="page-break"` | Page break after element |
//! | `<div>`, `<section>`, `<article>`, … | Block container; children are processed |
//! | `<strong>`, `<em>`, … | Text content extracted; styling ignored in v1 |
//! | `<head>`, `<script>`, `<style>`, … | Skipped entirely |

use crate::{Error, Result};

use super::{
    FlowDocument, FlowOptions, FlowTableCell, FlowTextAlignment, InlineSpan, Margins,
    TableCellAlignment, TableOptions,
    html_tokenizer::{HtmlNode, parse_html},
};

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
    /// Baseline offset in PDF points. Positive values move the baseline upward.
    /// Default: `0.0`.
    pub baseline_offset: f32,
    /// Default trailing spacing for block paragraphs in PDF points. Default: 6.0.
    pub paragraph_spacing: f32,
    /// Optional fallback font for body characters missing from `font_bytes`.
    pub fallback_font_bytes: Option<Vec<u8>>,
    /// Minimum paragraph lines kept at page boundaries. Default: 2.
    pub paragraph_min_lines: usize,
    /// Keep headings with the first following body line when possible.
    /// Default: `false`.
    pub keep_headings_with_next: bool,
    /// Keep image figures with the first following body line when possible.
    /// Default: `false`.
    pub keep_figures_with_next: bool,
    /// Horizontal alignment for body paragraphs. Default: left.
    pub body_alignment: FlowTextAlignment,
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
            baseline_offset: 0.0,
            paragraph_spacing: 6.0,
            fallback_font_bytes: None,
            paragraph_min_lines: 2,
            keep_headings_with_next: false,
            keep_figures_with_next: false,
            body_alignment: FlowTextAlignment::Left,
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
        baseline_offset: options.baseline_offset,
        paragraph_spacing: options.paragraph_spacing,
        fallback_font_bytes: options.fallback_font_bytes,
        paragraph_min_lines: options.paragraph_min_lines,
        keep_headings_with_next: options.keep_headings_with_next,
        keep_figures_with_next: options.keep_figures_with_next,
        body_alignment: options.body_alignment,
        max_pages: options.max_pages,
        ..FlowOptions::default()
    };

    let mut flow = FlowDocument::new(options.font_bytes, flow_opts)?;

    let document = parse_html(html);
    // Walk the tree iteratively to avoid stack overflows from deeply nested HTML.
    for child in document.children() {
        walk_iterative(child, &mut flow)?;
    }

    flow.render()
}

// ── Iterative tree walker ─────────────────────────────────────────────────────

/// Iterative depth-first traversal of the element tree.
///
/// Using an explicit stack instead of recursion prevents stack overflows when
/// processing deeply nested HTML (e.g. `<div><div><div>…</div></div></div>`).
fn walk_iterative<'a>(root: &'a HtmlNode, flow: &mut FlowDocument) -> Result<()> {
    let mut stack: Vec<&'a HtmlNode> = vec![root];

    while let Some(elem) = stack.pop() {
        process_one(elem, flow, &mut stack)?;
    }

    Ok(())
}

/// Process a single element. If this element is a block container, its children
/// are pushed onto `stack` in reverse order (so the first child is processed first).
fn process_one<'a>(
    elem: &'a HtmlNode,
    flow: &mut FlowDocument,
    stack: &mut Vec<&'a HtmlNode>,
) -> Result<()> {
    let tag = match elem.tag_name() {
        Some(t) => t,
        None => return Ok(()), // Skip text nodes
    };

    if has_page_break_before(elem) {
        flow.push_page_break()?;
    }

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
                let alignment = parse_css_text_alignment(elem.attr("style").as_deref())
                    .unwrap_or_else(|| flow.default_body_alignment());
                flow.push_paragraph_styled_with_alignment(&spans, alignment)?;
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
            let children: Vec<&HtmlNode> = elem.children().collect();
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

fn collect_text(elem: &HtmlNode) -> String {
    elem.text_content()
}

/// Collect inline styled spans from an element's children, preserving bold/italic/color.
///
/// Handles: `<strong>`, `<b>` (bold), `<em>`, `<i>` (italic),
/// `<span style="color:...">` (color), `<a href="...">` (blue link color).
/// Other inline elements fall through as plain text.
fn collect_inline_spans(elem: &HtmlNode) -> Vec<InlineSpan> {
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
    elem: &HtmlNode,
    parent_bold: bool,
    parent_italic: bool,
    parent_color: [f32; 3],
    out: &mut Vec<InlineSpan>,
) {
    let tag = match elem.tag_name() {
        Some(t) => t,
        None => {
            // Text node
            if let Some(text) = elem.as_text()
                && !text.is_empty()
            {
                out.push(InlineSpan {
                    text: text.to_string(),
                    bold: parent_bold,
                    italic: parent_italic,
                    color: parent_color.into(),
                });
            }
            return;
        }
    };

    let bold = parent_bold || matches!(tag, "strong" | "b");
    let italic = parent_italic || matches!(tag, "em" | "i");
    let color = inherited_color(elem, tag, parent_color);

    if tag == "br" {
        out.push(InlineSpan {
            text: "\n".to_owned(),
            bold: parent_bold,
            italic: parent_italic,
            color: parent_color.into(),
        });
        return;
    }

    for child in elem.children() {
        let child_tag = child.tag_name();
        // Skip non-content elements.
        if let Some(ct) = child_tag
            && matches!(ct, "script" | "style" | "head")
        {
            continue;
        }
        collect_inline_spans_inner(child, bold, italic, color, out);
    }
}

/// Resolve the effective color for an element, inheriting from parent if not overridden.
fn inherited_color(elem: &HtmlNode, tag: &str, parent_color: [f32; 3]) -> [f32; 3] {
    // <a> defaults to a blue link color.
    if tag == "a" {
        return [0.0, 0.0, 0.8];
    }
    // Look for inline style="color: ...".
    if let Some(style) = elem.attr("style")
        && let Some(c) = parse_css_color(&style)
    {
        return c;
    }
    parent_color
}

fn parse_css_text_alignment(style: Option<&str>) -> Option<FlowTextAlignment> {
    let style = style?.to_ascii_lowercase();
    let start = style.find("text-align:")? + "text-align:".len();
    match style[start..].split(';').next().unwrap_or_default().trim() {
        "left" => Some(FlowTextAlignment::Left),
        "center" => Some(FlowTextAlignment::Center),
        "right" => Some(FlowTextAlignment::Right),
        _ => None,
    }
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

fn has_page_break(elem: &HtmlNode) -> bool {
    let style = elem.attr("style").unwrap_or_default();
    let class = elem.attr("class").unwrap_or_default();
    style.contains("page-break-after: always")
        || style.contains("page-break-after:always")
        || class.split_whitespace().any(|c| c == "page-break")
}

fn has_page_break_before(elem: &HtmlNode) -> bool {
    let style = elem.attr("style").unwrap_or_default().to_ascii_lowercase();
    let class = elem.attr("class").unwrap_or_default();
    style.contains("page-break-before: always")
        || style.contains("page-break-before:always")
        || class.split_whitespace().any(|c| c == "page-break-before")
}

/// Collects `<tr>` elements that are direct or `<tbody>`/`<thead>`/`<tfoot>`-wrapped
/// children of `table` — without descending into nested `<table>` elements.
fn table_rows(table: &HtmlNode) -> Vec<&HtmlNode> {
    let mut rows = Vec::new();
    for child in table.children() {
        match child.tag_name() {
            Some("tr") => rows.push(child),
            Some("tbody") | Some("thead") | Some("tfoot") => {
                // One level of tbody/thead/tfoot wrapping — stop here.
                for tr in child.children() {
                    if tr.tag_name() == Some("tr") {
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

fn process_table(table: &HtmlNode, flow: &mut FlowDocument) -> Result<()> {
    let mut rows: Vec<Vec<FlowTableCell>> = Vec::new();

    for tr in table_rows(table) {
        // Collect only direct <th>/<td> children of this <tr>.
        let cells: Vec<FlowTableCell> = tr
            .children()
            .filter(|e| matches!(e.tag_name(), Some("th") | Some("td")))
            .map(|e| {
                let colspan = parse_span_attribute(e, "colspan")?;
                let rowspan = parse_span_attribute(e, "rowspan")?;
                let mut cell = FlowTableCell::new(collect_text(e).trim().to_owned())
                    .with_colspan(colspan)
                    .with_rowspan(rowspan);
                if let Some(alignment) = parse_css_text_alignment(e.attr("style").as_deref()) {
                    cell = cell.with_alignment(match alignment {
                        FlowTextAlignment::Left => TableCellAlignment::Left,
                        FlowTextAlignment::Center => TableCellAlignment::Center,
                        FlowTextAlignment::Right => TableCellAlignment::Right,
                    });
                }
                if let Some(padding) = parse_css_padding(e.attr("style").as_deref()) {
                    cell = cell.with_padding(padding);
                }
                Ok(cell)
            })
            .collect::<Result<Vec<_>>>()?;
        if !cells.is_empty() {
            rows.push(cells);
        }
    }

    if rows.is_empty() {
        return Ok(());
    }

    flow.push_table_cells(&rows, TableOptions::default())
}

fn parse_span_attribute(elem: &HtmlNode, name: &str) -> Result<usize> {
    elem.attr(name)
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| Error::InvalidInput(format!("HTML {name} must be a positive integer")))
        })
        .transpose()
        .map(|value| value.unwrap_or(1))
}

fn parse_css_padding(style: Option<&str>) -> Option<f32> {
    let style = style?.to_ascii_lowercase();
    let start = style.find("padding:")? + "padding:".len();
    let value = style[start..].split(';').next()?.trim();
    let (number, scale) = value
        .strip_suffix("pt")
        .map(|value| (value.trim(), 1.0))
        .or_else(|| value.strip_suffix("px").map(|value| (value.trim(), 0.75)))?;
    let padding = number.parse::<f32>().ok()? * scale;
    (padding.is_finite() && padding >= 0.0).then_some(padding)
}

fn process_list(list: &HtmlNode, flow: &mut FlowDocument, ordered: bool) -> Result<()> {
    // Only collect direct <li> children to avoid duplicating text from nested lists.
    let items: Vec<String> = list
        .children()
        .filter(|e| e.tag_name() == Some("li"))
        .map(|li| collect_text(li).trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    if items.is_empty() {
        return Ok(());
    }

    let items_ref: Vec<&str> = items.iter().map(String::as_str).collect();
    flow.push_list(&items_ref, ordered)
}
