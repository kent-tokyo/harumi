# harumi-mcp

**MCP server for harumi PDF library** — Pure Rust, zero C/C++ dependencies.

Expose harumi's PDF manipulation capabilities to Claude Code, Cursor, and Continue IDE via the Model Context Protocol (MCP).

## Installation

### From crates.io (published binary)

```bash
cargo install harumi-mcp
```

Then configure in your IDE (Claude Code, Cursor, Continue) to use `harumi-mcp` as a tool server.

### From source

```bash
cargo build -p harumi-mcp --release
./target/release/harumi-mcp
```

## Tools

### 1. `pdf_extract_text`

Extract text with x,y positions from a single PDF page.

**Input:**
- `pdf_path` (string, required): Path to the PDF file
- `page` (integer, required): 1-indexed page number (must be >= 1)

**Output:**
```json
{
  "success": true,
  "result": {
    "fragments": [
      {
        "text": "Hello",
        "x": 72.0,
        "y": 700.0,
        "width": 30.5,
        "height": 12.0,
        "font_size": 12.0,
        "font_name": "NotoSansCJKjp-Regular"
      },
      {
        "text": "World",
        "x": 150.0,
        "y": 700.0,
        "width": 35.2,
        "height": 12.0,
        "font_size": 12.0,
        "font_name": "NotoSansCJKjp-Regular"
      }
    ]
  }
}
```

**Error Codes:**
- `INVALID_PARAMS` — Missing/invalid parameters
- `FILE_READ_ERROR` — Cannot read PDF file
- `INVALID_PDF` — PDF is malformed
- `NO_PAGES` — PDF contains no pages
- `PAGE_OUT_OF_BOUNDS` — Page number exceeds document length
- `EXTRACTION_ERROR` — Cannot extract text from page

### 2. `pdf_extract_all_pages`

Extract text from all pages at once (recommended for multi-page translation).

**Input:**
- `pdf_path` (string, required): Path to the PDF file

**Output:**
```json
{
  "success": true,
  "result": {
    "pages": [
      {
        "page": 1,
        "fragments": [
          {
            "text": "...",
            "x": 72.0,
            "y": 700.0,
            "width": 30.5,
            "height": 12.0,
            "font_size": 12.0,
            "font_name": "NotoSansCJKjp-Regular"
          },
          ...
        ]
      },
      {"page": 2, "fragments": [...]}
    ],
    "page_count": 2,
    "extracted_pages": 2,
    "warnings": ["Page 3: Cannot extract (corrupted)"]
  }
}
```

**Note:** Continues on errors; returns `warnings` for pages that failed.

### 3. `pdf_extract_text_structured`

Extract text with semantic structure (headings vs. paragraphs). Groups text by font size heuristics (H1-H6 detection).

**Input:**
- `pdf_path` (string, required): Path to the PDF file
- `page` (integer, required): 1-indexed page number
- `markdown` (boolean, optional): Output as Markdown. Default: false

**Output (chunks mode):**
```json
{
  "success": true,
  "result": {
    "chunks": [
      {
        "text": "Main Title",
        "type": "h1",
        "bbox": [50, 750, 150, 20],
        "avg_font_size": 18
      },
      {
        "text": "Body paragraph...",
        "type": "paragraph",
        "bbox": [50, 700, 400, 50],
        "avg_font_size": 12
      }
    ]
  }
}
```

**Output (markdown mode):**
```json
{
  "success": true,
  "result": {
    "markdown": "# Main Title\n\nBody paragraph..."
  }
}
```

### 4. `pdf_replace_text`

Replace text in a PDF while preserving layout. **Core translation tool.**

**Input:**
- `pdf_path` (string, required): Path to input PDF
- `output_path` (string, required): Path to output PDF
- `replacements` (array, required): Array of `{old_text, new_text}` pairs. Array cannot be empty.
- `font_path` (string, required): Path to TTF font file (e.g., NotoSansCJK-Regular.ttf)
- `pages` (array, optional): List of 1-indexed page numbers. Default: all pages
- `mode` (string, optional): One of:
  - `resubset` (default) — Rebuild font subset to support new characters
  - `preserve` — Keep existing font (fails if character not present)
  - `new_font` — Switch to new font entirely
  - `wrap` — Like `resubset` but wraps long replacement text to multiple lines
- `line_height` (number, optional): Vertical spacing for wrapped lines (e.g., `14.4` for 12pt font). Default: `font_size × 1.2`. Only used with `mode: "wrap"`.
- `strict` (boolean, optional): If `true`, fail without writing `output_path` when any replacement error occurs. Default: `false`.

**Output:**
```json
{
  "success": true,
  "result": {
    "output_path": "output.pdf",
    "total_replacements": 5,
    "pages_processed": 3,
    "warnings": [
      "Page 1: 'Good morning' → 'Ohayou gozaimasu' may overflow (+15.3pt, 128% of original)"
    ]
  }
}
```

**Warnings:**
The `warnings` array may contain:
- **Overflow warnings** (when replacement text grows > 120% of original): Indicates potential text overflow. Consider using `mode: "wrap"` or shortening the replacement.
- **Font errors** (when font parsing fails): Fix the font file path or TTF format.
- **Missing glyph warnings** (when the chosen font lacks a replacement character): Use a font that covers the target language. For batch translation, set `strict: true` to avoid writing a partially translated PDF.
- **Other errors** (malformed parameters, file I/O): Retry with corrected inputs.

**Limitations:**
- **Line wrapping not automatic in `resubset`/`preserve`/`new_font` modes:** If replaced text is significantly longer, it will exceed line boundaries. Use `mode: "wrap"` for automatic wrapping, or review warnings.
- **Wrap mode limitations:** Works for single-font replacements in simple layouts. Does not handle multi-column PDFs, RTL text, or embedded font switches.
- **Font types:** Only CIDFontType2 (Type0 with Identity-H/V) fonts supported. See "Font Compatibility" section.
- **Font size limit:** Font files up to 25 MB are accepted. This covers common Google Fonts CJK TrueType files such as Noto Sans SC.
- **CFF fonts:** OpenType/CFF fonts not supported. Convert `.otf` to `.ttf` (see below).

**PDF Translation Workflow Guide:**

Choose the `mode` based on your translation scenario:

| Scenario | Recommended Mode | Why | Example |
|----------|------------------|-----|---------|
| **Japanese PDF → English text** | `resubset` | Existing Type0 font already has CJK glyphs; add Latin characters to the subset. Efficient—only rebuilds the font once. | Japanese PDF with CIDFont → Replace Japanese text with English |
| **English PDF → Japanese text** | `new_font` | Most English PDFs use Type1 or TrueType simple fonts, which `resubset` cannot extend. Embed a Japanese-capable font (e.g., NotoSansCJK) instead. | English PDF with Type1 font → Replace English text with Japanese |
| **Japanese PDF ↔ Chinese text** | `resubset` | Use `NotoSansCJKjp-Regular.ttf` (TrueType variant) which covers all CJK characters (JA/SC/TC) + hiragana/katakana. | Japanese PDF → Simplified Chinese translation |
| **Chinese PDF → Japanese text** | `resubset` + `wrap` | Use `NotoSansCJKjp-Regular.ttf`. Chinese text rarely uses hiragana, so JA translation is longer (>20% overflow likely) → add `mode: "wrap"` to prevent text overflow. | Simplified Chinese PDF → Replace Chinese text with Japanese (with hiragana/punctuation) |
| **Chinese PDF → English text** | `resubset` + `wrap` | Chinese PDFs use CIDFont (Type0) — `resubset` can add Latin characters. English text grows 1.5–2× longer than Chinese, so wrap is essential. | Simplified/Traditional Chinese PDF → Replace Chinese text with English translations |
| **English PDF → Chinese text** | `new_font` | English PDFs use Type1/TrueType simple fonts — `resubset` cannot extend these. Embed a CJK-capable font (NotoSansCJKsc for Simplified, NotoSansCJKtc for Traditional). | English PDF with Type1 font → Replace English text with Simplified Chinese translation |
| **French PDF → Japanese text** | `new_font` | French PDFs typically use Type1 fonts — `resubset` cannot extend these. Embed a Japanese-capable font (NotoSansCJKjp-Regular.ttf). | French PDF with Type1 font → Replace French text with Japanese |
| **Japanese PDF → French text** | `resubset` + `wrap` | Japanese PDFs use CIDFont (Type0). French is a European language with Latin Extended characters (é, è, ê, etc.) — NotoSansCJKjp supports these. French grows 1.3–1.8× longer than Japanese, requiring wrap to prevent overflow. | Japanese PDF → Replace Japanese text with French translation |
| **Text length increases significantly (>120%)** | `wrap` | Prevents text overflow by automatically wrapping long replacements across multiple lines. | Short Japanese → Long English translation that needs 2+ lines |
| **Minor text changes, same font** | `preserve` | Fastest option if all replacement characters already exist in the PDF's font. Fails if characters are missing. | "Hello" → "Hi" in an English PDF |

**Note:** `mode: "resubset"` requires the input PDF to use Type0/CIDFont (typically CJK PDFs). For English PDFs with Type1/TrueType fonts, use `mode: "new_font"` and specify a Unicode-capable font file.

**CJK Translation Notes:**
- For Japanese ↔ Chinese translation, use **`NotoSansCJKjp-Regular.ttf`** (TrueType variant, **not** `.otf`). This font covers all CJK characters (Japanese, Simplified Chinese, Traditional Chinese) plus hiragana/katakana.
- Chinese → Japanese translation produces longer text (Japanese uses hiragana for grammatical particles and inflections). Overflow likelihood is **high** — combine with `mode: "wrap"` to prevent text overflow.
- Japanese → Chinese translation typically produces shorter text (pure kanji replaces hiragana + kanji), so overflow is **unlikely**.
- If using Simplified Chinese (SC), `NotoSansCJKsc-Regular.ttf` is also fully supported. If using Traditional Chinese (TC), `NotoSansCJKtc-Regular.ttf` works.

**English ↔ Chinese Translation Notes:**
- **Chinese → English** (`mode: "resubset" + "wrap"`): Use `NotoSansCJKjp-Regular.ttf` — covers all CJK + Latin (ASCII) characters. English translations of Chinese text grow 1.5–2× longer (word spacing, articles, particles). Overflow likelihood is **high** — use `mode: "wrap"` to split across lines.
- **English → Chinese** (`mode: "new_font"`): Use `NotoSansCJKsc-Regular.ttf` (Simplified) or `NotoSansCJKtc-Regular.ttf` (Traditional) — English PDFs typically use Type1/TrueType simple fonts unsuitable for subsetting, so new font embedding is required. Chinese translations are typically shorter than English text, so overflow is **unlikely**.
- `mode: "wrap"` is not available with `mode: "new_font"`. For English→Chinese, wrap is rarely needed since Chinese text is more compact.

**English ↔ Japanese / French ↔ Japanese Translation Notes:**
- **JA → EN/FR**: Use `NotoSansCJKjp-Regular.ttf`. Japanese → European languages produces longer text (articles, grammatical particles, word spacing). Overflow likelihood is **high** — use `mode: "wrap"` to prevent text overflow.
- **EN/FR → JA**: Use `NotoSansCJKjp-Regular.ttf`. European languages → Japanese produces shorter text (1 character = 1 concept). Overflow is **unlikely**.
- **French accented characters** (é, è, ê, ë, à, â, ù, û, ô, î, ï, ç, œ, etc.) are fully supported via automatic NFC normalization. All Unicode inputs are normalized to NFC form before processing, ensuring that both composed (é = U+00E9) and decomposed (e + U+0301) forms work correctly.
- `mode: "wrap"` is not available with `mode: "new_font"`. For EN→JA / FR→JA, wrap is rarely needed since Japanese is significantly more compact.

**EN↔ZH Implementation Examples:**

**Example 1: English PDF → Simplified Chinese (mode: new_font)**
```bash
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "pdf_replace_text",
    "params": {
      "pdf_path": "english.pdf",
      "output_path": "chinese_simplified.pdf",
      "mode": "new_font",
      "font_path": "NotoSansCJKsc-Regular.ttf",
      "replacements": [
        {"old_text": "Hello", "new_text": "你好"},
        {"old_text": "Welcome", "new_text": "欢迎"}
      ]
    }
  }'
```
**Why `new_font`:** English PDFs typically use Type1 fonts, which `resubset` cannot extend. Instead, embed a complete Unicode-capable CJK font.

---

**Example 2: Simplified Chinese PDF → English (mode: wrap)**
```bash
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 2,
    "method": "pdf_replace_text",
    "params": {
      "pdf_path": "chinese.pdf",
      "output_path": "english.pdf",
      "mode": "wrap",
      "line_height": 14.4,
      "font_path": "NotoSansCJKjp-Regular.ttf",
      "replacements": [
        {"old_text": "你好", "new_text": "Hello there"},
        {"old_text": "欢迎", "new_text": "Welcome to our store"}
      ]
    }
  }'
```
**Why `mode: "wrap"`:** Chinese PDFs use CIDFont; `wrap` mode extends the font subset with Latin characters and automatically splits long English text across multiple lines to prevent overflow (English text is 1.5–2× longer than Chinese).

---

**Example 3: English PDF → Japanese (mode: new_font)**
```bash
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 3,
    "method": "pdf_replace_text",
    "params": {
      "pdf_path": "english_manual.pdf",
      "output_path": "japanese_manual.pdf",
      "mode": "new_font",
      "font_path": "NotoSansCJKjp-Regular.ttf",
      "replacements": [
        {"old_text": "Installation", "new_text": "インストール"},
        {"old_text": "Configuration", "new_text": "設定"},
        {"old_text": "Error", "new_text": "エラー"}
      ]
    }
  }'
```
**Why `new_font`:** English PDFs typically use Type1 fonts, which `resubset` cannot extend. Embed NotoSansCJKjp (Japanese font) to replace English text with Japanese.

---

**Example 4: Japanese PDF → French (mode: resubset + wrap)**
```bash
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 4,
    "method": "pdf_replace_text",
    "params": {
      "pdf_path": "japanese_manual.pdf",
      "output_path": "french_manual.pdf",
      "mode": "wrap",
      "line_height": 14.4,
      "font_path": "NotoSansCJKjp-Regular.ttf",
      "replacements": [
        {"old_text": "インストール", "new_text": "Installation"},
        {"old_text": "設定", "new_text": "Configuration"},
        {"old_text": "エラー", "new_text": "Erreur"}
      ]
    }
  }'
```
**Why `mode: "wrap"`:** Japanese PDFs use CIDFont; `wrap` mode extends the font subset with Latin Extended characters (including accented characters: é, è, ê, à, â, ç, etc.). French text grows ~1.5× longer than Japanese, requiring wrap to prevent overflow.

---

**Font Compatibility:**

| Font Type | Support | Solution |
|-----------|---------|----------|
| TrueType (.ttf) | ✅ Full | Use directly |
| OpenType CFF (.otf) | ❌ Not supported | Convert with `fonttools`: `fontTools varLib.instancer font.otf -o font.ttf` |
| Type1 | ❌ Not supported | Use TTF variant if available |

**Example: Convert OTF to TTF**
```bash
# Install fonttools (if not already installed)
pip install fonttools

# Convert
python -m fontTools.varLib.instancer Noto Sans CJK JP.otf -o "Noto Sans CJK JP.ttf"
```

**How to Get Fonts for CJK Translation:**

For Japanese, Chinese, and Korean PDF translation, download **NotoSansCJK** fonts from Google Fonts (free, OFL licensed):

```bash
# Japanese
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKjp-Regular.ttf

# Simplified Chinese
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKsc-Regular.ttf

# Traditional Chinese
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKtc-Regular.ttf

# Korean
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKkr-Regular.ttf
```

**Alternative Sources:**
- **Google Fonts Website**: https://fonts.google.com (search "Noto Sans CJK")
- **Adobe Fonts**: https://fonts.adobe.com (subscription-based, includes variable fonts)
- **Local Font Manager**: Use `fc-list` to check if your system already has CJK fonts installed

```bash
# Check installed fonts
fc-list | grep -i "noto\|dejavu\|liberation"
```

All fonts provided should be in **TrueType (.ttf) format**. OpenType CFF (.otf) fonts require conversion (see example above).

### 5. `pdf_rotate_page`

Rotate a PDF page by 90/180/270 degrees.

**Input:**
- `pdf_path` (string, required): Path to input PDF
- `output_path` (string, required): Path to output PDF
- `page` (integer, required): 1-indexed page number
- `degrees` (integer, required): One of: 90, 180, 270, -90, -180, -270

**Output:**
```json
{
  "success": true,
  "result": {
    "output_path": "output.pdf",
    "page": 1,
    "degrees": 90
  }
}
```

### 6. `pdf_add_invisible_text`

Add searchable (invisible) OCR text layer to a PDF page.

**Input:**
- `pdf_path` (string, required): Path to input PDF
- `output_path` (string, required): Path to output PDF
- `font_path` (string, required): Path to TTF font file
- `page` (integer, required): 1-indexed page number
- `text` (string, required): Text to add (cannot be empty, supports CJK)
- `x` (number, required): X position in PDF points (must be >= 0)
- `y` (number, required): Y position in PDF points (must be >= 0)
- `size` (number, required): Font size in points (must be > 0 and <= 1000)

**Output:**
```json
{
  "success": true,
  "result": {
    "output_path": "output.pdf",
    "page": 1,
    "text_length": 50
  }
}
```

### 7. `pdf_html_to_pdf`

Convert HTML to PDF. Supports headings, paragraphs, tables, lists, CJK fonts, page breaks.

**Input:**
- `html` (string, required): HTML source code (max 50 MB)
- `output_path` (string, required): Output PDF path
- `title` (string, optional): PDF metadata title (currently unused)

**Output:**
```json
{
  "success": true,
  "result": {
    "output_path": "output.pdf",
    "html_length": 5000
  }
}
```

### 8. `pdf_merge`

Merge two PDF files, appending pages from the second to the first.

**Input:**
- `pdf1_path` (string, required): Path to first PDF
- `pdf2_path` (string, required): Path to second PDF
- `output_path` (string, required): Path to save merged PDF

**Output:**
```json
{
  "success": true,
  "result": {
    "output_path": "merged.pdf",
    "first_pdf_pages": 10,
    "second_pdf_pages": 5,
    "merged_page_count": 15
  }
}
```

### 9. `pdf_page_info`

Get page count and dimensions of all pages in a PDF.

**Input:**
- `pdf_path` (string, required): Path to PDF file

**Output:**
```json
{
  "success": true,
  "result": {
    "page_count": 10,
    "pages": [
      {"page": 1, "width": 595.0, "height": 842.0},
      {"page": 2, "width": 595.0, "height": 842.0}
    ]
  }
}
```

**Note:** Now returns per-page dimensions. Handles PDFs with variable page sizes and empty PDFs gracefully.

## Error Response Format

All errors follow a consistent format:

```json
{
  "error": "Descriptive error message",
  "code": "ERROR_CODE"
}
```

**Common error codes:**
- `INVALID_REQUEST` — Malformed request
- `INVALID_PARAMS` — Missing or invalid parameters
- `FILE_READ_ERROR` — Cannot read input file
- `FILE_WRITE_ERROR` — Cannot write output file
- `INVALID_PDF` — PDF is malformed or corrupted
- `NO_PAGES` — PDF has zero pages
- `PAGE_OUT_OF_BOUNDS` — Page number exceeds document
- `EXTRACTION_ERROR` — Text extraction failed
- `RENDER_ERROR` — HTML rendering failed
- `FONT_ERROR` — Font embedding failed

## Usage in IDE

### Claude Code

Add to `.claude/mcp-servers.json` or Claude Code settings:

```json
{
  "servers": {
    "harumi-mcp": {
      "command": "harumi-mcp"
    }
  }
}
```

### Cursor

Add to `cursor_config.json`:

```json
{
  "mcp_servers": {
    "harumi-mcp": {
      "command": "harumi-mcp"
    }
  }
}
```

### Continue

Add to `~/.continue/config.json`:

```json
{
  "mcpServers": {
    "harumi-mcp": {
      "command": "harumi-mcp"
    }
  }
}
```

## Features

✅ **Pure Rust** — no C/C++ dependencies, works in WASM, Lambda, cross-compile environments  
✅ **CJK Support** — full support for Chinese, Japanese, Korean fonts  
✅ **Text Extraction** — extract with precise x,y positions from single/all pages  
✅ **Structured Extraction** — semantic heading/paragraph detection  
✅ **Translation Ready** — `pdf_replace_text` with automatic layout preservation  
✅ **OCR Integration** — add invisible text layers for scanned PDFs  
✅ **HTML to PDF** — convert HTML directly to PDF with CJK support  
✅ **PDF Merge** — combine multiple PDFs  
✅ **Robust Error Handling** — descriptive error messages with error codes  
✅ **Zero Setup** — binary distribution, no Python/Node runtime required  

## Known Limitations

### Text Replacement & Layout

- **Line wrapping not automatic:** When replacing text with longer translations, content may exceed line boundaries. **Workaround:** Review output PDF visually or keep translation lengths within ±20% of original.
- **Single-line compensation:** Width compensation works within a line only. Does not reflow to next line.

### Font Support

- **CIDFontType2 only:** Only Type0 CIDFonts with Identity-H/V encoding supported for `replace_text_resubset()`
- **TrueType required:** Only `.ttf` (glyf-based) fonts supported. **No CFF/OpenType.**
- **Recommendation:** Use NotoSans/NotoSerif families (available in both OTF and TTF)

### Text Extraction

- **Unicode combining marks:** Decomposed characters (e.g., é as e + acute) may be extracted as separate fragments
- **RTL text:** Arabic/Hebrew text position coordinates may be semantically incorrect

## Building from Source

```bash
# Requires Rust 1.88+
cargo build -p harumi-mcp --release

# Run
./target/release/harumi-mcp
```

## License

MIT OR Apache-2.0

## See Also

- [harumi](https://github.com/kent-tokyo/harumi) — The underlying PDF library
- [MCP Specification](https://modelcontextprotocol.io/)
