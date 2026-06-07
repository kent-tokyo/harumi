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

Extract text with x,y positions from a PDF page.

**Input:**
- `pdf_path` (string): Path to the PDF file
- `page` (integer): 1-indexed page number

**Output:**
```json
{
  "fragments": [
    {"text": "Hello", "x": 72.0, "y": 700.0},
    {"text": "World", "x": 150.0, "y": 700.0}
  ]
}
```

### 2. `pdf_add_invisible_text`

Add searchable (invisible) OCR text layer to a PDF page. Typical use: overlay OCR results on scanned PDFs.

**Input:**
- `pdf_path` (string): Path to input PDF
- `output_path` (string): Path to save output PDF
- `font_path` (string): Path to TTF font file (e.g., NotoSansCJK-Regular.ttf)
- `page` (integer): 1-indexed page number
- `text` (string): Text to add (supports CJK)
- `x` (number): X position in PDF points
- `y` (number): Y position in PDF points
- `size` (number): Font size in points

**Output:**
```json
{
  "success": true,
  "message": "Invisible text added to page 1",
  "output_path": "output.pdf"
}
```

### 3. `pdf_html_to_pdf`

Convert HTML to PDF. Supports headings, paragraphs, tables, lists, CJK fonts, page breaks.

**Input:**
- `html` (string): HTML source code
- `output_path` (string): Output PDF path
- `title` (string, optional): PDF metadata title

**Output:**
```json
{
  "success": true,
  "message": "HTML converted to PDF",
  "output_path": "output.pdf"
}
```

### 4. `pdf_merge`

Merge two PDF files, appending pages from the second to the first.

**Input:**
- `pdf1_path` (string): Path to first PDF
- `pdf2_path` (string): Path to second PDF
- `output_path` (string): Path to save merged PDF

**Output:**
```json
{
  "success": true,
  "message": "PDFs merged successfully",
  "output_path": "merged.pdf"
}
```

### 5. `pdf_page_info`

Get page count and dimensions (width/height in PDF points) of a PDF.

**Input:**
- `pdf_path` (string): Path to PDF file

**Output:**
```json
{
  "page_count": 10,
  "page_width": 595.0,
  "page_height": 842.0
}
```

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
✅ **Text Extraction** — extract with precise x,y positions  
✅ **OCR Integration** — add invisible text layers for scanned PDFs  
✅ **HTML to PDF** — convert HTML directly to PDF with CJK support  
✅ **PDF Merge** — combine multiple PDFs  
✅ **Zero Setup** — binary distribution, no Python/Node runtime required  

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
