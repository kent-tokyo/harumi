# harumi-mcp

MCP server for harumi PDF operations. It is Pure Rust and has no C/C++ runtime
dependency. Use it from Claude Code, Cursor, Continue, or another MCP client.

## Install and run

Install the published binary:

```bash
cargo install harumi-mcp
harumi-mcp
```

Or build from this repository:

```bash
cargo build -p harumi-mcp --release
./target/release/harumi-mcp
```

The server communicates over stdio. Configure the command in your MCP client:

```json
{
  "mcpServers": {
    "harumi": { "command": "harumi-mcp" }
  }
}
```

## Tools

All paths are local filesystem paths. Page numbers are 1-based.

| Tool | Purpose | Main inputs |
|---|---|---|
| `pdf_extract_text` | Extract positioned text from one page | `pdf_path`, `page` |
| `pdf_extract_all_pages` | Extract positioned text from the whole PDF | `pdf_path` |
| `pdf_extract_text_structured` | Return grouped text and layout information | `pdf_path`, `page` |
| `pdf_replace_text` | Replace text in an existing PDF | `pdf_path`, `output_path`, `old_text`, `new_text` |
| `pdf_rotate_page` | Rotate one page | `pdf_path`, `output_path`, `page`, `degrees` |
| `pdf_add_invisible_text` | Add an OCR/search layer | `pdf_path`, `output_path`, `page`, `text`, `x`, `y`, `font_path` |
| `pdf_html_to_pdf` | Convert supported HTML to PDF | `html`, `output_path`, `font_path` |
| `pdf_merge` | Merge PDFs in order | `input_paths`, `output_path` |
| `pdf_page_info` | Read page count and dimensions | `pdf_path` |

The exact schema is defined by the server implementation and may evolve with the
crate. Prefer the MCP client's tool schema over copied examples in this document.

## Response and errors

Successful responses contain `success: true` and a `result` object. Failures contain
`success: false`, an error code, and a message. Common codes are:

`INVALID_PARAMS`, `FILE_READ_ERROR`, `INVALID_PDF`, `NO_PAGES`,
`PAGE_OUT_OF_BOUNDS`, and `EXTRACTION_ERROR`.

The server does not run OCR or an LLM. Supply OCR text and coordinates from an
external tool, or use `harumi-ai` for translation workflows.

## Client configuration

For Claude Code, add the server to the MCP configuration with `harumi-mcp` as the
command. Cursor and Continue use the same command in their MCP server settings.
The published registry entries are also available through
[Smithery](https://smithery.ai) and [mcp.so](https://mcp.so) when listed there.

## Limitations

- Complex, rotated, vertical, or image-backed pages need visual review.
- `pdf_replace_text` depends on the source PDF's encodings and font coverage.
- HTML support is intentionally smaller than a browser's CSS implementation.
- Input and output paths must be accessible to the server process.

## Development

```bash
cargo test -p harumi-mcp
cargo clippy -p harumi-mcp -- -D warnings
```

The minimum supported Rust version is Rust 1.88. See the root
[`README.md`](../README.md), [`docs/API.md`](../docs/API.md), and
[`docs/EXAMPLES.md`](../docs/EXAMPLES.md) for library details.

## License

MIT OR Apache-2.0
