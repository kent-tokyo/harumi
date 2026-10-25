# harumi

**用纯 Rust 从现有 PDF 提取带位置的文本，并写回 CJK/多语言内容。**

当前版本为 **1.23.2**。

[English](README.md) · [日本語](README_ja.md) · [한국어](README_kr.md)

## 主要功能

- 提取、替换和翻译叠加现有 PDF 文本
- CJK 字体嵌入、子集化和 OCR 可搜索文本层
- 页面合并、拆分、旋转、排序、注释、表单和加密
- `draw` 功能的图形与虚线，`image` 功能的图像嵌入
- `flow` / `html` 功能的报告和 HTML 转 PDF
- 支持 WASM、Lambda、Tauri 和 MCP

它不是 OCR 引擎或 PDF 查看器。复杂分栏、旋转/竖排文字以及图片背景文档应使用
渲染器检查输出。

## 最短示例

```rust
use harumi::Document;

let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJKsc-Regular.ttf"))?;
doc.page(1)?.add_invisible_text("可搜索的中文", font, [72.0, 700.0], 12.0)?;
doc.save("searchable.pdf")?;
# Ok::<(), harumi::Error>(())
```

## 翻译

`harumi-ai`支持数字 PDF 和使用 OCR JSON 的扫描 PDF 翻译，并提供 LLM 翻译、区域布局
和逐页质量报告。

## 详细文档

- [API 概览](docs/API.md)
- [代码示例](docs/EXAMPLES.md)
- [功能列表](docs/FEATURES.md)
- [PDF 比较范围](docs/PDF_ECOSYSTEM.md)
- [MCP 服务器](harumi-mcp/README.md)

## 贡献与许可证

欢迎提交 Issue 和 Pull Request。MIT OR Apache-2.0。
