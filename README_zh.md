# harumi

**纯 Rust 实现的 CJK PDF 回写引擎，基于提取坐标和推定区域重新放置文本。**

harumi 从现有 PDF 中提取带位置的文本，让你翻译或替换后，
将结果写回推定的版面区域。Overlay 模式会保留原页面内容，但不保证像素级一致；
复杂分栏、旋转或竖排文字以及图片背景仍需结合质量报告进行人工检查。
CID 字体、CMap、Unicode 映射、字体子集化、文本适配和版面碰撞检测全部自动完成。

[![harumi on crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![harumi-ai on crates.io](https://img.shields.io/crates/v/harumi-ai.svg?label=harumi-ai)](https://crates.io/crates/harumi-ai)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Demo](https://img.shields.io/badge/demo-live-brightgreen)](https://kent-tokyo.github.io/harumi/)

[English](README.md) | [日本語](README_ja.md) | [한국어](README_kr.md)

**[在浏览器中试用 Demo →](https://kent-tokyo.github.io/harumi/)** — 注释编辑器（文字・矩形・直线・自由笔）完全通过 WASM 在浏览器中运行

**主要用途：**
- 数字 PDF 翻译（提取文本 → LLM 翻译 → 基于版面推定进行回写）
- 扫描 PDF 翻译（传入 OCR JSON → 翻译 → 遮盖原文 → 叠加译文）
- 在扫描 PDF 上叠加 OCR 可搜索文本层
- 中文/日文/韩文文本叠加与盖章
- 页面操作、注释、表单编辑、PDF 合并
- 基于 WASM、Lambda、Tauri 或 MCP 的 AI 文档工作流

> 不是 OCR 引擎，不是 PDF 查看器。  
> harumi 是文档自动化的「PDF 回写层」。

---

## harumi 解决了什么

**使用前（没有 harumi）：**  
对照 PDF 规范手动组装 CID 字体对象，自行实现 CMap 生成、GID 映射和字体子集化，写几百行代码，还要与乱码问题反复较劲。

**使用后（有了 harumi）：**

```rust
let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;
doc.page(1)?.add_invisible_text("可搜索的文本", font, [72.0, 700.0], 12.0)?;
doc.save("searchable.pdf")?;
```

字体子集化、CID 编码、ToUnicode CMap 生成、GID 重新编号——全部自动完成。

---

## 翻译数字 PDF 和扫描 PDF

**harumi-ai** 通过单一入口翻译数字 PDF 和扫描 PDF：

```rust
use harumi_ai::{InputTextSource, TranslateOptions, translate_pdf};

// 数字 PDF（默认）
let opts = TranslateOptions::new("zh", my_llm, font_bytes);
let output = translate_pdf(&digital_pdf, opts).await?;

// 扫描 PDF — 传入 ocrs-cjk、PaddleOCR 等工具的 OCR JSON
let ocr_json = std::fs::read("ocr.json")?;
let mut opts = TranslateOptions::new("zh", my_llm, font_bytes);
opts.input_source = InputTextSource::OcrJson(ocr_json);
let output = translate_pdf(&scanned_pdf, opts).await?;
```

OCR 可来自 ocrs-cjk、PaddleOCR、hOCR、ALTO 或任何生成 HierText 格式输出的工具。

**扫描 PDF 快速入门：**

```bash
# 1. 安装 CJK OCR CLI
cargo install ocrs-cjk-cli

# 2. 运行 OCR（输出文本 + 边界框 + 置信度）
ocrs scanned.pdf --json -o ocr.json

# 3. 通过 harumi-ai 翻译并回写
# （在 TranslateOptions 中设置 InputTextSource::OcrJson）
```

### 可搜索文本层（不翻译）

如需在不翻译的情况下添加不可见的可搜索文本层（适用于 RAG 索引和 PDF 搜索），
可直接将 OCR 输出传给 `add_invisible_text`：

```bash
cargo run --example ocrs_cjk_to_searchable_pdf -- \
  examples/fixtures/scanned_sample.pdf \
  examples/fixtures/ocrs_sample.json \
  /path/to/NotoSansCJKjp-Regular.ttf \
  searchable.pdf
```

### AI 辅助 OCR 纠错

OCR 引擎有时会误识字形相近的 CJK 字符。LLM 可以在保留原始边界框的前提下纠正这些错误。

**此模式的关键约束：AI 只纠正文字，边界框不得移动。**

> **OCR 纠错 ≠ 翻译。** 翻译回写需要区域级布局设计，请使用 `harumi-ai`。

详见 `examples/fixtures/ocrs_sample_raw.json`、`ocrs_sample_corrected.json`、`ai_correction_report.json`。

harumi 不是 OCR 引擎。翻译路径请使用构建在任意 LLM 之上的 `harumi-ai`。

---

## 你能得到什么

**[完整功能列表 →](docs/FEATURES.md)**

| 需求 | harumi 的解决方案 |
|---|---|
| CJK 字体子集化复杂 | 一次 `embed_font()` 调用——仅包含已用字形，GID 正确重新编号 |
| 需要保留现有页面内容 | Overlay 追加新内容；替换模式只改写目标内容流 |
| 需要在 WASM / Lambda 中运行 | 纯 Rust——零 C/C++ 依赖 |
| 需要带坐标的 OCR 文本 | `add_invisible_text` / 批量版 `add_invisible_text_runs` |
| 需要替换现有 PDF 中的文本 | `replace_text` / `replace_text_preserve_font` / `replace_text_resubset` |
| 需要版面保持翻译的质量门控 | `extract_layout_regions` → `plan_text_for_regions` → `assess_page_layout_quality` |

对比表和详细功能说明请参阅[英文 README](README.md)。

---

## MCP 服务器（harumi-mcp）

从 Claude Code、Cursor 或 Continue IDE 直接使用 harumi 的 PDF 工具：

```bash
cargo build -p harumi-mcp
# 可用工具: pdf_extract_text, pdf_extract_all_pages, pdf_replace_text,
#           pdf_add_invisible_text, pdf_html_to_pdf, pdf_merge, pdf_page_info
```

在 [smithery.ai](https://smithery.ai) 或 [mcp.so](https://mcp.so) 上注册即可一键安装。

---

## 快速开始

```toml
[dependencies]
harumi = "1"
```

### 获取 CJK 字体

处理中文/日文/韩文请下载 **NotoSansCJK** 字体（Google Fonts，免费，OFL 许可证）：

```bash
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKsc-Regular.ttf
```

或在 [fonts.google.com](https://fonts.google.com) 搜索「Noto Sans CJK」。

### 不可见 OCR 文本层

```rust
let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;

doc.page(1)?.add_invisible_text(
    "可搜索的中文文本",
    font,
    [100.0, 250.0], // x, y（PDF 点，原点：左下角）
    12.0,
)?;

doc.save("searchable.pdf")?;
```

### 可见文本叠加

```rust
let (w, h) = doc.page(1)?.size()?;
doc.page(1)?.add_text(
    "机密",
    font,
    [w / 2.0 - 20.0, h / 2.0],
    24.0,
    [0.8, 0.0, 0.0], // 红色
)?;
```

**[更多示例 →](docs/EXAMPLES.md)** — 页面操作、PDF 合并、HTML→PDF、注释、表单、图形、图像、FlowDocument

**[API 参考 →](docs/API.md)** — 坐标系、功能标志、支持的字体、内部实现

---

## Contributing

欢迎在 [github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi) 提交 Issue 和 PR。

---

## License

MIT OR Apache-2.0
