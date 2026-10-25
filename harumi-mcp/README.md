# harumi-mcp

MCP server for the [harumi](https://github.com/kent-tokyo/harumi) PDF library.
It is pure Rust and has no C/C++ runtime dependency.

## Install

```bash
cargo install harumi-mcp
```

From a checkout:

```bash
cargo build -p harumi-mcp --release
./target/release/harumi-mcp
```

## Tools

All page numbers are 1-indexed. File paths must be readable by the server.

| Tool | Required input | Result |
|---|---|---|
| `pdf_extract_text` | `pdf_path`, `page` | Positioned text fragments |
| `pdf_extract_all_pages` | `pdf_path` | Positioned fragments for every page |
| `pdf_extract_text_structured` | `pdf_path`, `page` | Grouped text and layout metadata |
| `pdf_replace_text` | `pdf_path`, `output_path`, `replacements` | Output path and match counts |
| `pdf_rotate_page` | `pdf_path`, `page`, `degrees` | Rotated PDF output path |
| `pdf_add_invisible_text` | `pdf_path`, `output_path`, `font_path`, `page`, `text`, `x`, `y`, `size` | Searchable PDF output path |
| `pdf_html_to_pdf` | `html`, `output_path` | PDF output path |
| `pdf_merge` | `pdf1_path`, `pdf2_path`, `output_path` | Merged PDF output path |
| `pdf_page_info` | `pdf_path` | Page count and dimensions |

The server returns a consistent envelope:

```json
{"success": true, "result": {}}
```

Errors use `success: false` and a stable code such as `INVALID_PARAMS`,
`FILE_READ_ERROR`, `INVALID_PDF`, `PAGE_OUT_OF_BOUNDS`, or
`EXTRACTION_ERROR`.

## IDE configuration

The command used by an IDE is the installed binary:

```json
{
  "mcpServers": {
    "harumi": { "command": "harumi-mcp" }
  }
}
```

If the binary is not on `PATH`, use an absolute path. The server does not
upload PDFs or call an external PDF converter; it reads and writes paths passed
by the client.

## Font and PDF limits

- Text replacement and invisible text require a readable TrueType font file.
- CJK text is supported through harumi's CID font embedding and ToUnicode maps.
- HTML conversion supports the documented HTML/Flow subset, not arbitrary CSS.
- Extraction is best-effort for malformed PDFs, missing CMaps, complex vertical
  composition, and deeply nested Form XObjects.
- Review generated PDFs with a renderer for complex layouts.

## Build and test

Rust 1.88 or newer is required:

```bash
cargo test -p harumi-mcp
cargo clippy -p harumi-mcp -- -D warnings
```

## License

MIT OR Apache-2.0

## See also

- [harumi API guide](../docs/API.md)
- [harumi examples](../docs/EXAMPLES.md)
- [MCP specification](https://modelcontextprotocol.io/)
