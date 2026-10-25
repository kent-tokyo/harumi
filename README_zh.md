# harumi

**用 Pure Rust 将带坐标的 CJK 文本写回现有 PDF。**

harumi 从现有 PDF 提取文本和坐标，再写回译文、替换文本或 OCR 搜索层。它会自动
处理 CJK 字体、ToUnicode CMap、字体子集和布局诊断。它不是 OCR 引擎，也不是 PDF 查看器。

当前版本为 **v1.23.6**。

[English](README.md) | [日本語](README_ja.md) | [한국어](README_kr.md)

## 适用场景

- 尽量保留原页面，同时翻译或替换 PDF 文本
- 嵌入中文、日文、韩文字体
- 为扫描 PDF 添加 OCR 搜索文本
- 在 WASM、Lambda、Tauri 中自动处理 PDF

屏幕渲染可使用 `pdfium-render`，新报告可使用 `printpdf`/`genpdf`，底层对象编辑可
使用 `lopdf`。各库的职责边界见 [PDF ecosystem contract](docs/PDF_ECOSYSTEM.md)。

## 快速开始

```toml
[dependencies]
harumi = "1"
```

请使用支持子集化的 TrueType 版 Noto Sans CJK。

```rust
use harumi::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::from_file("input.pdf")?;
    let font = doc.embed_font(include_bytes!("NotoSansCJKsc-Regular.ttf"))?;
    doc.page(1)?.add_invisible_text(
        "可搜索的中文文本", font, [72.0, 700.0], 12.0,
    )?;
    doc.save("output.pdf")?;
    Ok(())
}
```

## 主要功能

| 用途 | API 或 feature |
|---|---|
| 提取文本和坐标 | `extract_text_runs`、`extract_text_chunks` |
| 替换现有文本 | `replace_text`、`replace_text_preserve_font`、`replace_text_resubset` |
| OCR 搜索层 | `add_invisible_text`、`add_invisible_text_runs` |
| 页面操作 | 合并、拆分、旋转、排序、书签、表单、元数据 |
| 图形与图像 | `draw`、`image` feature |
| 报告与 HTML | `flow`、`html` feature |

详情请看[功能列表](docs/FEATURES.md)、[代码示例](docs/EXAMPLES.md)和
[API 概览](docs/API.md)。

## 翻译与扫描 PDF

[`harumi-ai`](harumi-ai/README.md) 可翻译数字 PDF 和 OCR JSON。它可以覆盖或替换翻译
区域，并报告溢出与重叠。它不保证像素级一致。

## MCP 服务器

[`harumi-mcp`](harumi-mcp/README.md) 向 MCP 客户端提供提取、替换、OCR 层、HTML、合并、
旋转和页面信息工具。

```bash
cargo install harumi-mcp
harumi-mcp
```

## 限制

- 完整 Unicode 换行、RTL、竖排和浏览器级 CSS 尚未完成或仅有限支持。
- 不支持 OpenType CFF 子集化，请使用 TrueType 字体。
- PDF 页面渲染需要单独的 renderer。
- 复杂分栏、旋转、竖排或图像文字应在保存后检查。

文档索引见 [docs/README.md](docs/README.md)，变更见 [CHANGELOG.md](CHANGELOG.md)，
优先级见 [ROADMAP.md](ROADMAP.md)。许可证为 MIT OR Apache-2.0。
