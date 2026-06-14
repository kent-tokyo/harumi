# harumi

> **HARUMI** — **H**igh-level **A**PI for **R**ust-native **U**nicode **M**anipulation and **I**njection

**纯 Rust 实现的 PDF 操作库 — 文本叠加、内容提取、页面操作、图形绘制一站搞定。**  
完整支持中文/日文/韩文（CJK）字体。零 C 依赖。原生 WASM 支持。

[![Crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Demo](https://img.shields.io/badge/demo-live-brightgreen)](https://kent-tokyo.github.io/harumi/)

[English](README.md) | [日本語](README_ja.md) | [한국어](README_kr.md)

**[在浏览器中试用 Demo →](https://kent-tokyo.github.io/harumi/)** — 注释编辑器（文字・矩形・直线・自由笔）完全通过 WASM 在浏览器中运行

### 🔌 作为 MCP 服务器可用

从 Claude Code、Cursor 或 Continue IDE 直接使用 harumi 的 PDF 工具：

```bash
# 构建 MCP 服务器（纯 Rust，无运行时依赖）
cargo build -p harumi-mcp

# IDE 配置中可用的工具：
# - pdf_extract_text: 带位置的文本提取
# - pdf_extract_all_pages: 提取所有页面的带位置文本
# - pdf_replace_text: 保持版面进行文本替换/翻译
# - pdf_add_invisible_text: OCR 可搜索层
# - pdf_html_to_pdf: HTML→PDF 转换
# - pdf_merge: PDF 合并
# - pdf_page_info: 获取页面信息
```

PDF 翻译流程：先用 `pdf_extract_all_pages` 提取所有页面的文本片段，翻译后再用
`pdf_replace_text` 在保留原版面的前提下替换文本。如果 PDF 因非 Identity
`CIDToGIDMap` 无法重新子集化，请指定 Unicode TTF 并使用 `mode: "new_font"`。
`harumi-ai` CLI 在保留原版面时默认使用 `overlay` mode；只有需要重新生成文档时才指定 `new`。

在 [smithery.ai](https://smithery.ai) 或 [mcp.so](https://mcp.so) 上注册中。

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

字体子集化、CID 编码、ToUnicode CMap 生成、GID 重新编号——全部自动完成，对调用者完全透明。

---

## 你能得到什么

| 挑战 | harumi 的答案 |
|---|---|
| CJK 字体子集化复杂 | 一次 `embed_font()` 调用——只包含实际使用的字形，GID 正确重新编号；剥离 GSUB/GPOS/可变字体表，兼容 macOS Preview 和 PSPDFKit |
| 不想破坏原有 PDF 结构 | 仅追加；harumi 从不修改原始对象图 |
| 需要在 WASM / Lambda / 交叉编译环境运行 | 纯 Rust——零 C/C++ 依赖 |
| 需要将 OCR 文本写入指定坐标 | `add_invisible_text` / 批量版 `add_invisible_text_runs` |
| 需要在 PDF 上盖章或加水印 | `add_text(color)` 以任意 RGB 颜色叠加可见文本 |
| 需要根据页面尺寸定位文本 | `page.size()` 读取 MediaBox |
| 需要 Tauri / WASM 内存输出 | `save_to_bytes()` 直接返回 `Vec<u8>` |
| 需要绘制高亮矩形或线段 | `add_rect` / `add_line`（`draw` feature，无额外依赖） |
| 需要绘制边框矩形或多边形（标注箭头） | `add_rect_stroke` / `add_polygon`（`draw` feature） |
| 需要在矩形内自动换行显示文本 | `add_text_box`（无需 feature gate） |
| 需要嵌入 JPEG / PNG 图像 | `add_image` / `add_image_with_opacity`（`image` feature） |
| 需要保留 PNG 透明度（签名、水印） | 透明背景 PNG 通过 PDF SMask 自动处理，无白色背景 |
| 需要旋转、删除或重新排序页面 | `rotate_page` / `remove_page` / `insert_blank_page` / `reorder_pages`（无需 feature gate） |
| 需要将两个 PDF 合并为一个 | `merge_from` 将另一个文档的所有页面追加到末尾；保留内容和字体 |
| 需要从零开始创建 PDF（无现有文件） | `Document::new(size)` 创建一个空白单页 PDF；使用 `insert_blank_page` 添加更多页面 |
| 需要将 PDF 拆分为单独的文件 | `extract_pages` 以任意顺序返回包含指定页面的新 `Document` |
| 需要从现有 PDF 中提取文本位置信息 | `extract_text_runs` 可解码 CID 字体和标准简单字体（Type1、TrueType、WinAnsi 等） |
| 需要读写 PDF 元数据（标题、作者等） | `doc.metadata()` 读取 `/Info`；`doc.set_metadata(&meta)` 写入 |
| 需要在现有 PDF 中替换文本（新字体） | `page.replace_text(old, new, font)` 就地重写；返回匹配数 `usize`；自动字体切换和宽度补偿 |
| 需要用原有字体替换文本 | `page.replace_text_preserve_font(old, new)` — 无需 `FontHandle`；返回匹配数；字形验证在调用时立即执行 |
| 需要在不修改文档的情况下预检替换 | `page.can_replace_text(old, new)` — 只读扫描；返回匹配数或 `Err(FontCharNotMapped)` |
| 需要绘制椭圆或圆形 | `add_ellipse(rect, color, opacity, filled, stroke_width)`（`draw` feature） |
| 需要同时填充和描边 | `add_ellipse` / `add_polygon` / `add_path` 中 `filled=true` 且 `stroke_width>0`，使用 PDF `B` 算子 |
| 需要统一的开/闭路径 API | `add_path(points, closed, color, filled, stroke_width, opacity)`（`draw` feature） |
| 需要旋转文字（水印、斜向印章） | `add_text_with_rotation(text, font, pos, size, color, opacity, degrees)` |
| 需要跨多个 `Tj` 算子的文本替换 | `replace_text` / `replace_text_preserve_font` — 支持跨算子匹配 |
| 需要从扫描版 PDF 中提取嵌入图像 | `extract_page_image` 返回 JPEG 或 PNG 字节（`image` feature）；仅限扫描版 PDF |
| 需要在 PDF 中添加可点击的 URL 链接 | `add_link_url([x, y, w, h], url)` — 不可见 URI 注释；在任意查看器中点击即可打开链接 |
| 需要内部导航链接（目录） | `add_link_internal([x, y, w, h], target_page)` — 跳转到同一文档内的指定页面 |
| 需要书签/文档大纲 | `add_bookmark(title, page, y)` — 平铺式 PDF 大纲条目；CJK 标题自动存储为 UTF-16BE |
| 需要在每一页添加带页码的页眉/页脚 | `FlowOptions { header: Some(hf), footer: Some(hf), .. }` 配合 `HeaderFooter`（`flow` feature）；渲染时自动替换 `{{page}}` / `{{total}}` |
| 需要标题自动生成书签 | `FlowOptions { auto_bookmarks: true, .. }`（默认启用）— 每次 `push_heading` 自动创建书签条目 |
| 需要加载密码保护的 PDF | `Document::from_file_with_password(path, pw)` / `from_bytes_with_password(bytes, pw)` — 同时支持用户密码和所有者密码 |
| 需要保存加密 PDF | `doc.set_encryption(user_pw, owner_pw)` — 在 `save()` 时使用 128-bit RC4 加密 |
| 需要检测 PDF 是否原来已加密 | `doc.is_encrypted()` — 成功解密后仍返回 `true` |
| 需要为文字添加高亮/下划线/删除线 | `add_highlight` / `add_underline` / `add_strikeout` / `add_squiggly`（带 QuadPoints 的 PDF 标记注释） |
| 需要为页面添加便利贴注释 | `add_sticky_note([x, y], "注释内容")` — 支持 Unicode 的 Text 注释 |
| 需要读取 PDF 表单字段值 | `doc.form_fields()` — 返回 `Vec<FormField>`（名称、类型、当前值） |
| 需要以程序方式填写 PDF 表单 | `doc.fill_form(&[("字段名", "值")])` — 自动设置 NeedAppearances |
| 需要操作页面裁切框和印刷框 | `page.crop_box()` / `set_crop_box(rect)` / `trim_box()` / `bleed_box()` — 所有框类型均以 `[x,y,w,h]` 格式处理 |
| 需要缩放页面内容（例如 A4 → A3） | `page.scale_page_content(sx, sy)` 在现有内容前插入 `cm` 矩阵；`resize_page_with_content(w, h)` 一次性完成缩放和 MediaBox 调整（v1.4+） |
| 需要将另一个 PDF 叠加到当前 PDF 上（印章合成） | `doc.overlay_from(other)` 将 `other` 的每页作为 Form XObject 叠加到 `self` 的对应页上；字体、图像和透明度均保留（v1.4+） |
| 需要删除所有书签/目录 | `doc.clear_outline()` 同时删除待写入的书签和已加载 PDF 中的 `/Outlines` 树（v1.4+） |
| 需要在 PDF 中附加文件 | `doc.attach_file(name, data, mime)` 将任意文件嵌入为 EmbeddedFiles 附件（FlateDecode 压缩、按名称排序）；`doc.list_attachments()` 返回 `Vec<AttachmentInfo>`（v1.4+） |
| 需要验证 PDF 数字签名 | `doc.verify_signatures(&pdf_bytes)` — 提取签名元数据（签名者、时间戳、字段名）；密码学验证待做（`digital-signature` feature） |
| 需要为 PDF 创建和签署数字签名 | `doc.add_signature_field(page, rect, options)` + `doc.sign_document(context, field_name)` — 需要 `digital-signature` feature；创建签名字段，生成 RSA PKCS#1 v1.5 签名；完整 PDF 嵌入计划于 v1.2.1 |

---

## 为什么这个空白一直存在

JavaScript 有 [`pdf-lib`](https://pdf-lib.js.org/)，它可以透明地处理字体子集化、CMap 生成和文本层合成。而 Rust 的现有工具让你只能在以下方案中选择：

- **`lopdf`** — 低级别的二进制操作；需要按照 PDF 规范手动组装 CID 字体对象
- **`printpdf`** — 仅支持创建新 PDF；无法修改现有 PDF
- **`pdfium-render`** — 依赖 C++ 绑定，在 WASM、交叉编译和 Lambda 环境中构建失败

`harumi` 填补了这一空白。

---

## 与同类工具对比

| 功能 | **harumi** | pdf-lib (JS) | printpdf (Rust) | lopdf (Rust) | pdfium-render (Rust) |
|---|:---:|:---:|:---:|:---:|:---:|
| 纯 Rust（无 C/C++ 依赖） | Yes | N/A | Yes | Yes | No |
| WASM / 跨平台 | Yes | Yes | Yes | Yes | Partial |
| 向已有 PDF 添加 CJK 文本 | Yes | Yes | No | No | Yes |
| 文本提取 | Yes | Partial | No | Partial | Yes |
| 文本替换（含子集扩展） | Yes | No | No | No | No |
| 页面操作 | Yes | Yes | Partial | Yes | Yes |
| 图形绘制 | Yes | Yes | Yes | No | Yes |
| 流式文档 / 自动分页 | Yes | No | No | No | No |
| HTML → PDF | Yes | No | No | No | No |
| 内联粗体/斜体/颜色 | Yes (synthetic) | No | No | No | Yes |
| 加密（读取） | Yes | Yes | No | Partial | Yes |
| 加密（写入） | Yes (RC4-128) | Yes | No | No | Yes |

---

## 快速开始

```toml
[dependencies]
harumi = "0.7"
```

### 获取 CJK 字体

用于日文、中文和韩文 PDF 处理，请从 Google Fonts（免费，OFL 许可）下载 **NotoSansCJK 字体**：

```bash
# 日文
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKjp-Regular.ttf

# 简体中文
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKsc-Regular.ttf

# 繁体中文
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKtc-Regular.ttf

# 韩文
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKkr-Regular.ttf
```

**其他来源：**
- **Google Fonts 网站**：https://fonts.google.com（搜索 "Noto Sans CJK"）
- **Adobe Fonts**：https://fonts.adobe.com（订阅版本）
- **系统字体**：使用 `fc-list | grep -i noto` 检查是否已安装

### 不可见 OCR 文本层

```rust
use harumi::{Document, TextRun};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::from_file("scanned.pdf")?;

    // 嵌入字体 — 子集化、CMap 生成和 GID 重新编号在 save() 时自动完成
    let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;

    // 在第 1 页叠加不可见的 OCR 文本
    doc.page(1)?.add_invisible_text(
        "这里是 OCR 识别的中文文本",
        font,
        [100.0, 250.0],
        12.0,
    )?;

    doc.save("searchable_chinese.pdf")?;
    Ok(())
}
```

### 可见文本叠加

```rust
let (w, h) = doc.page(1)?.size()?;
doc.page(1)?.add_text("机密文件", font, [w / 2.0 - 30.0, h / 2.0], 24.0, [0.8, 0.0, 0.0])?;
```

### 内存输出

```rust
// 适用于 Tauri 命令、WASM 或任何内存流水线
let pdf_bytes: Vec<u8> = doc.save_to_bytes()?;
```

### 多行文本框（无需 feature gate）

```rust
// 在指定矩形内自动换行（Latin 按词换行，CJK 任意位置换行）
doc.page(1)?.add_text_box(
    "这是一段需要在窄框内自动换行的长文本。",
    font,
    [72.0, 400.0, 200.0, 120.0], // [x, y, 宽, 高]
    12.0,
    [0.0, 0.0, 0.0],              // 黑色
    0.0,                          // 0.0 = 使用 font_size * 1.2 作为行高
)?;
```

### 页面操作

```rust
// 将所有页面顺时针旋转 90°
for page_num in 1..=doc.page_count() {
    doc.rotate_page(page_num, 90)?;
}

// 删除空白封面页
doc.remove_page(1)?;

// 在第 1 页之前插入空白 A4 标题页
doc.insert_blank_page(0, (595.0, 842.0))?;

// 将 3 页文档的页面顺序反转
doc.reorder_pages(&[3, 2, 1])?;

doc.save("output.pdf")?;
```

### 合并 PDF

```rust
let mut base = Document::from_file("a.pdf")?;
let appendix = Document::from_file("b.pdf")?;
base.merge_from(appendix)?;
base.save("merged.pdf")?;
```

保留的内容：所有页面内容、嵌入字体、图像、资源。  
不保留的内容：书签/大纲、AcroForm、`/Info` 元数据（作者、创建日期等）。

> **前提条件**：`other` 不能有未刷新的待处理操作（刚加载完毕，或在 `save_to_bytes()` 后重新加载的状态）。

### 创建空白 PDF

```rust
let mut doc = Document::new((595.0, 842.0))?;   // 空白 A4
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;
doc.page(1)?.add_text("Hello, world!", font, [72.0, 700.0], 24.0, [0.0, 0.0, 0.0])?;
doc.save("output.pdf")?;
```

### 提取页面

```rust
let doc = Document::from_file("large.pdf")?;
let mut excerpt = doc.extract_pages(&[3, 5, 7])?;  // 按此顺序提取第 3、5、7 页
excerpt.save("excerpt.pdf")?;
```

### 从现有 PDF 提取文本

```rust
let doc = Document::from_file("existing.pdf")?;
let runs = doc.extract_text_runs(1)?;
for fragment in &runs {
    println!("{:?} at ({:.1}, {:.1})", fragment.text, fragment.x, fragment.y);
}
```

不仅适用于 harumi 生成的 PDF（Identity-H CID 字体），也适用于任意现有 PDF。支持标准简单字体（Type1、TrueType）及 WinAnsiEncoding、MacRomanEncoding、StandardEncoding 或 `/Differences` 编码字典。

### 替换现有 PDF 中的文本

```rust
let mut doc = Document::from_file("contract.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansJP-Regular.ttf"))?;
doc.page(1)?.replace_text("Hello", "こんにちは", font)?;
doc.save("translated.pdf")?;
```

支持匹配同一字体上下文（同一 `Tf` / `BT`~`ET` 块）内连续 `Tj`/`TJ` 算子中的跨算子文本。位置算子（`Td`、`Tm`）之间的情况不匹配。

### 使用原始嵌入字体替换文本

当您没有字体文件，但替换文本的字形已包含在 PDF 中时使用：

```rust
let mut doc = Document::from_file("contract.pdf")?;
// 无需 FontHandle — 直接复用该位置已有的字体
doc.page(1)?.replace_text_preserve_font("Draft", "Final")?;
doc.save("final.pdf")?;
```

若替换文本中有字符不在嵌入字体的子集中，`save()` 将返回 `Error::FontCharNotMapped`。此时可回退到 `replace_text` 并显式指定字体：

```rust
if doc.page(1)?.replace_text_preserve_font("Draft", replacement).is_ok() {
    // 字形在子集中 — 无需额外字体
} else {
    let font = doc.embed_font(include_bytes!("font.ttf"))?;
    doc.page(1)?.replace_text("Draft", replacement, font)?;
}
doc.save("output.pdf")?;
```

### 预检：不修改文档确认可替换性

```rust
let mut doc = Document::from_file("contract.pdf")?;
match doc.page(1)?.can_replace_text("Draft", "Final") {
    Ok(0) => println!("第 1 页未找到 'Draft'"),
    Ok(n) => println!("找到 {n} 处，字形可用"),
    Err(e) => println!("字形缺失：{e}"),
}
```

### 带字体子集扩展的文本替换

当新文本包含原始字体子集中不存在的字符时，使用 `replace_text_resubset`。传入原始（未子集化的）TTF/OTF 字节，harumi 会扩展子集，重新编码所有内容流，并在一次 `save()` 调用中完成替换。

```rust
let font_bytes = include_bytes!("NotoSansCJK-Regular.ttf");
let mut doc = Document::from_file("contract.pdf")?;
let n = doc.page(1)?.replace_text_resubset("Hello", "中文字", font_bytes)?;
doc.save("output.pdf")?;
```

> 需要提供原始未子集化的字体文件。仅支持使用 `CIDToGIDMap /Identity` 的 CIDFontType2（harumi 嵌入格式）。
> 其他工具生成的 PDF 可能使用非 Identity `CIDToGIDMap`；这种情况下请使用新嵌入字体的
> `replace_text`，或 MCP `pdf_replace_text` 的 `mode: "new_font"`。

### 读写 PDF 元数据

```rust
use harumi::{Document, PdfMetadata};

let mut doc = Document::from_file("report.pdf")?;

// 读取元数据
let meta = doc.metadata()?;
println!("标题: {:?}", meta.title);

// 写入元数据（None 字段不会写入 /Info）
doc.set_metadata(&PdfMetadata {
    title: Some("2026 年度报告".into()),
    author: Some("Harumi Team".into()),
    subject: None,
    keywords: None,
    creator: None,
})?;
doc.save("report_with_meta.pdf")?;
```

### 绘制图形（`draw` feature）

```toml
harumi = { version = "0.5", features = ["draw"] }
```

```rust
// 黄色填充矩形（x, y, 宽, 高，单位：PDF 点）
doc.page(1)?.add_rect([72.0, 690.0, 200.0, 14.0], [1.0, 1.0, 0.0], 0.4)?;

// 蓝色边框矩形（仅描边，不填充）
doc.page(1)?.add_rect_stroke([72.0, 400.0, 200.0, 100.0], [0.0, 0.0, 1.0], 1.5, 1.0)?;

// 填充三角形（标注箭头尖端）— 最后一个参数为 stroke_width（0.0 = 无描边）
doc.page(1)?.add_polygon(
    &[[100.0, 500.0], [150.0, 600.0], [200.0, 500.0]],
    [1.0, 0.5, 0.0], 1.0, true, 0.0,
)?;

// 黑色下划线
doc.page(1)?.add_line([72.0, 600.0], [300.0, 600.0], [0.0, 0.0, 0.0], 1.5, 1.0)?;
```

### 嵌入图像（`image` feature）

```toml
harumi = { version = "0.5", features = ["image"] }
```

```rust
let jpeg = std::fs::read("stamp.jpg")?;
// 支持 JPEG（直接嵌入，无需解码）和 PNG
doc.page(1)?.add_image(&jpeg, [72.0, 500.0, 100.0, 100.0])?;

// 带透明度（0.0 = 完全透明，1.0 = 不透明）
doc.page(1)?.add_image_with_opacity(&jpeg, [72.0, 400.0, 100.0, 100.0], 0.75)?;
```

### 从扫描版 PDF 中提取嵌入图像（`image` feature）

适用于 OCR 工作流：读取扫描版 PDF → 提取光栅图像 → 执行 OCR → 将不可见文本层写回。

```rust
use harumi::{Document, PageImageFormat};

let doc = Document::from_file("scanned.pdf")?;
let img = doc.extract_page_image(1)?;

match img.format {
    PageImageFormat::Jpeg => std::fs::write("page1.jpg", &img.bytes)?,
    PageImageFormat::Png  => std::fs::write("page1.png", &img.bytes)?,
}
println!("{}×{} 像素", img.width, img.height);
```

> **仅限扫描版 PDF。** 此 API 提取已有的 Image XObject，不对页面进行光栅化。文本型和矢量型 PDF 不含 Image XObject，调用时将返回 `Error::InvalidInput`。

### 自动分页结构化文档生成（`flow` feature）

```toml
harumi = { version = "0.5", features = ["flow"] }
```

```rust
use harumi::{FlowDocument, FlowOptions};

let font = include_bytes!("NotoSansCJK-Regular.ttf");
let mut doc = FlowDocument::new(font.as_ref(), FlowOptions::default())?;

doc.push_heading("年度报告", 1)?;
doc.push_paragraph("本文档汇总了本期业绩情况。")?;
doc.push_key_value_table(&[
    ("营业收入", "100万元"),
    ("费用", "80万元"),
    ("利润", "20万元"),
])?;
doc.push_list(&["进入3个新市场", "发布2款新产品"], false)?;

// 内容超出页面时自动插入分页。
// 调用 push_page_break() 可在任意位置手动分页。

let pdf_bytes = doc.render()?;
```

完整支持中文/日文/韩文——传入 CJK TTF 字体后，文本可在任意字符处自动换行。

### 带页码的页眉/页脚（`flow` feature）

```rust
use harumi::{FlowDocument, FlowOptions, HeaderFooter};

let opts = FlowOptions {
    // 每页左侧显示 "harumi docs"，右侧显示 "v0.5"
    header: Some(HeaderFooter {
        left:  Some("harumi docs".into()),
        right: Some("v0.5".into()),
        ..Default::default()
    }),
    // 居中显示 "1 / 3" 页码
    footer: Some(HeaderFooter::page_number()),
    // push_heading() 自动创建书签条目（默认：true）
    auto_bookmarks: true,
    ..Default::default()
};

let mut doc = FlowDocument::new(font, opts)?;
doc.push_heading("第一章", 1)?;
doc.push_paragraph("正文内容。")?;
let pdf_bytes = doc.render()?;
```

### FlowDocument 内联文本样式（`flow` feature）

```rust
use harumi::{FlowDocument, FlowOptions, InlineSpan};

let mut doc = FlowDocument::new(font_bytes, FlowOptions::default())?;
doc.push_paragraph_styled(&[
    InlineSpan::plain("普通文本，"),
    InlineSpan::bold("粗体文本，"),
    InlineSpan::italic("斜体文本，"),
    InlineSpan::colored("红色文本。", [0.8, 0.0, 0.0]),
])?;
```

粗体和斜体为**合成效果**，无需单独的粗体/斜体字体文件。

### 标注注释（高亮、下划线、删除线、波浪线）

```rust
// 黄色高亮
doc.page(1)?.add_highlight([72.0, 690.0, 200.0, 14.0], [1.0, 1.0, 0.0])?;

// 红色下划线
doc.page(1)?.add_underline([72.0, 640.0, 200.0, 12.0], [1.0, 0.0, 0.0])?;

// 删除线
doc.page(1)?.add_strikeout([72.0, 590.0, 200.0, 12.0], [0.0, 0.0, 0.0])?;

// 波浪下划线
doc.page(1)?.add_squiggly([72.0, 540.0, 200.0, 12.0], [0.0, 0.6, 0.2])?;

// 便签注释
doc.page(1)?.add_sticky_note([500.0, 700.0], "请审查此部分")?;
doc.save("annotated.pdf")?;
```

### 密码保护 PDF

```rust
// 加载加密 PDF
let mut doc = Document::from_file_with_password("protected.pdf", "secret")?;
assert!(doc.is_encrypted());

// 密码错误返回 Error::WrongPassword
match Document::from_bytes_with_password(&bytes, "wrong") {
    Err(harumi::Error::WrongPassword) => println!("密码错误"),
    _ => {}
}

// 加密保存
let mut doc = Document::new((595.0, 842.0))?;
doc.set_encryption("userpass", "ownerpass")?;
doc.save("protected_output.pdf")?;
```

### AcroForm：读取和填写表单字段

```rust
// 读取所有表单字段
let mut doc = Document::from_file("form.pdf")?;
for field in doc.form_fields()? {
    println!("{}: {:?} = {:?}", field.name, field.field_type, field.value);
}

// 按名称填写字段
let updated = doc.fill_form(&[
    ("FullName",   "张三"),
    ("Agree",      "yes"),       // 复选框 → /Yes
    ("Department", "Engineering"),
])?;
println!("已更新 {updated} 个字段");
doc.save("filled_form.pdf")?;
```

### 页面框（印刷工作流）

```rust
// 读写 CropBox（可见区域裁剪）
let cb = doc.page(1)?.crop_box()?;   // Option<[f32;4]>

doc.page(1)?.set_crop_box([10.0, 10.0, 575.0, 822.0])?;   // [x,y,w,h]
doc.page(1)?.set_trim_box([0.0, 0.0, 595.0, 842.0])?;
doc.page(1)?.set_bleed_box([0.0, 0.0, 601.0, 848.0])?;
doc.save("print_ready.pdf")?;
```

### 链接注释

```rust
// 可点击的 URL 区域（x, y, 宽, 高）
doc.page(1)?.add_link_url([72.0, 40.0, 200.0, 18.0], "https://example.com")?;

// 内部链接：点击该区域跳转到同一文档的第 3 页
doc.page(1)?.add_link_internal([72.0, 700.0, 150.0, 18.0], 3)?;
```

### 书签/文档大纲

```rust
// 在 PDF 查看器中构建书签面板。
// 非 ASCII 标题（CJK、带重音的拉丁字母……）自动编码为 UTF-16BE。
doc.add_bookmark("第一章",      1, 800.0)?;   // 标题、页码（从 1 开始）、y 坐标
doc.add_bookmark("第2章 概要",  2, 800.0)?;
doc.save("report.pdf")?;
```

### HTML → PDF 转换（`html` feature）

```toml
harumi = { version = "0.5", features = ["html"] }
```

```rust
use harumi::{render_html_to_pdf, HtmlRenderOptions};

let font = include_bytes!("NotoSansCJK-Regular.ttf").to_vec();
let html = r#"
    <h1>年度报告</h1>
    <p>引言段落。</p>
    <table>
      <tr><th>营业收入</th><td>100万元</td></tr>
      <tr><th>利润</th><td>20万元</td></tr>
    </table>
    <h2>亮点</h2>
    <ul><li>进入3个新市场</li><li>发布2款新产品</li></ul>
    <div style="page-break-after: always"></div>
    <h1>第二页</h1>
"#;

let pdf_bytes = render_html_to_pdf(html, HtmlRenderOptions {
    font_bytes: font,
    ..HtmlRenderOptions::default()
})?;
```

支持的元素：`<h1>`–`<h6>`、`<p>`、`<table>/<tr>/<th>/<td>`、`<ul>/<ol>/<li>`、`<div>/<section>/<article>`（块容器）。  
分页：`style="page-break-after: always"` 或 `class="page-break"`。  
跳过：`<script>`、`<style>`、`<head>`。  
深度嵌套 HTML 不会导致栈溢出（迭代式解析器，已通过 5000 层 `<div>` 嵌套验证）。

---

## API 概览

```rust
let mut doc = Document::from_file("path/to/file.pdf")?;
let mut doc = Document::from_bytes(&bytes)?;

let font: FontHandle = doc.embed_font(ttf_bytes)?;
let (width, height) = doc.page(1)?.size()?;

doc.page(1)?.add_invisible_text(text, font, [x, y], size)?;
doc.page(1)?.add_text(text, font, [x, y], size, [r, g, b])?;
doc.page(1)?.add_invisible_text_runs(&[
    TextRun { text: "第一行".into(), font, x: 72.0, y: 700.0, font_size: 11.0, render_mode: 3, color: [0.0; 3] },
    TextRun { text: "第二行".into(), font, x: 72.0, y: 685.0, font_size: 11.0, render_mode: 3, color: [0.0; 3] },
])?;

// 页面结构（无需 feature gate）
doc.page_count()                          // u32
doc.rotate_page(n, degrees)?;             // 90 的倍数；可累积
doc.remove_page(n)?;                      // 不能删除最后一页
doc.insert_blank_page(after, (w, h))?;    // after=0 表示在开头插入
doc.reorder_pages(&[new_order...])?;      // 使用从 1 开始的旧页码
doc.extract_pages(&[n1, n2, ...])?;       // 返回仅含指定页面的新 Document

// 从零创建
Document::new((w, h))?;                   // 空白单页 PDF

// 合并文档（other 不能有待处理操作）
doc.merge_from(other)?;             // 将 other 的所有页面追加到末尾

doc.save("output.pdf")?;
doc.save_to_bytes()?;   // 内存版本

// 从现有 PDF 提取文本（CID 字体 + 标准简单字体）
let runs: Vec<TextFragment> = doc.extract_text_runs(page_number)?;

// PDF 元数据（/Info 字典）
let meta: PdfMetadata = doc.metadata()?;
doc.set_metadata(&PdfMetadata { title: Some("...".into()), ..Default::default() })?;

// 替换现有内容流中的文本（单算子匹配）；返回匹配数
let n: usize = doc.page(1)?.replace_text(old_text, new_text, font)?;
// 使用原有嵌入字体替换文本；立即字形验证；返回匹配数
let n: usize = doc.page(1)?.replace_text_preserve_font(old_text, new_text)?;
// 只读扫描：返回匹配数或 Err(FontCharNotMapped)
let n: usize = doc.page(1)?.can_replace_text(old_text, new_text)?;

// 链接注释（无需 feature gate）
doc.page(1)?.add_link_url([x, y, w, h], "https://example.com")?;   // URL 链接
doc.page(1)?.add_link_internal([x, y, w, h], target_page)?;         // 文档内部链接

// 文档大纲/书签（无需 feature gate）
doc.add_bookmark("章节标题", page, y)?;  // 追加一个平铺大纲条目
```

### 坐标系

坐标以 **PDF 点**（1pt = 1/72 英寸）为单位，原点在页面**左下角**。如需转换 OCR 像素坐标：

```toml
harumi = { version = "0.5", features = ["ocr"] }
```

### 功能标志

| 标志 | 启用的功能 | 额外依赖 |
|---|---|---|
| *(默认)* | 文本叠加、字体嵌入、`add_text_box`、`add_text_box_aligned`、`add_text_with_opacity`、`add_text_box_with_opacity` | lopdf, ttf-parser |
| `draw` | `add_rect`, `add_line`, `add_rect_stroke`, `add_polygon`, `add_polyline`, `add_ellipse` — 图形绘制 | 无 |
| `image` | `add_image`, `add_image_with_opacity` — JPEG/PNG 图像嵌入；`extract_page_image` — 从扫描版 PDF 中提取嵌入图像（自动启用 `draw`） | `png` crate（纯 Rust） |
| `ocr` | `ocr::hocr_y_to_pdf`、`ocr::hocr_x_to_pdf`、`ocr::pixel_size_to_pt` — Tesseract 坐标转换工具 | 无 |
| `flow` | `FlowDocument` 推送式文档构建器，自动分页（`push_heading`、`push_paragraph`、`push_key_value_table`、`push_list`、`push_page_break`、`render`）；`HeaderFooter` 支持每页页眉/页脚，可使用 `{{page}}`/`{{total}}` 占位符；`auto_bookmarks` 从标题自动生成大纲 | 无 |
| `html` | `render_html_to_pdf` — HTML→PDF 转换（h1–h6、p、table、ul/ol、分页；自动启用 `flow`）；内置纯 Rust HTML 令牌化器 | 无 |

```rust
let pdf_y = harumi::ocr::hocr_y_to_pdf(pixel_y, page_height_pts, image_dpi);
let pdf_x = harumi::ocr::hocr_x_to_pdf(pixel_x, image_dpi);
let pt    = harumi::ocr::pixel_size_to_pt(pixel_size, image_dpi);
```

---

## 支持的字体格式

| 字体格式 | 支持状态 |
|---|---|
| TrueType (`.ttf`) | ✅ 完全支持 — 纯 Rust 子集化引擎 |
| TTC 字体集合 | ✅ 完全支持 — 通过 `embed_font_at(bytes, face_index)` 指定面索引 |
| OpenType CFF (`.otf`) | ⚠️ 接受（不支持子集化） — 按原样嵌入 |

推荐使用 [Noto Sans CJK](https://github.com/notofonts/noto-cjk) 的 **TrueType** 版本（已端到端验证）：

```
NotoSansCJKsc-Regular.ttf  （简体中文）
NotoSansCJKtc-Regular.ttf  （繁体中文）
NotoSansCJKjp-Regular.ttf  （日语）
NotoSansCJKkr-Regular.ttf  （韩语）
```

> **OTF 说明**：harumi 接受 `.otf` 文件并通过 `FontFile3 /OpenType` 嵌入，但**不支持 CFF 字体子集化** — 字体内所有字形都会被嵌入，导致 PDF 文件较大。为优化大小，请使用上述 TTF 变体。

---

## 内部实现

```
harumi
├── lopdf v0.40          — 解析和修改现有 PDF 对象图
├── ttf-parser           — 字体元数据读取（bbox、units_per_em、ascender）
└── [内置 TTF 子集化器]  — 纯 Rust TrueType 子集化引擎（无外部依赖）
```

字体处理流程：

1. 收集已使用字符 → 建立 Unicode 码点集合
2. 通过字体 `cmap` 表将码点映射为原始 GID（ttf-parser）
3. 使用内置引擎仅对已使用字形进行 TTF 子集化（GID **重新编号为 0..N**）
4. 将 `gid_to_char` 和字形宽度从原始 GID **重新映射到新 GID**（防止乱码）
5. 构建 CID 字体对象图：`Type0 → CIDFontType2 → FontDescriptor → FontFile2`
6. 生成 `/ToUnicode` CMap 流（使查看器能够复制/搜索文本）
7. 向页面 `/Contents` 数组追加新内容流

子集化采用**延迟执行**：`embed_font()` 仅存储原始 TTF 字节；`save()` 时收集所有页面已使用字符，每个字体只执行一次处理。

### 依赖最小化

harumi 致力于实现**零外部运行时依赖**（PDF 核心处理除外）。

- **TrueType 子集化** — 内置纯 Rust 实现（v1.1+）；支持 TTF + TTC（集合）、递归复合字形解析
- **字体解析** — ttf-parser（专业用途，无推移依赖）
- **图像解码** — `png` crate（可选，feature 门控）
- **加密** — getrandom（仅 OS 熵；AES-256 加密密钥生成需要）

**直接依赖数**: 3 个（getrandom、lopdf、ttf-parser，加上可选 `png`）  
**推移依赖（默认构建）**: 约 8 个（仅 lopdf 内部实用程序）

---

## 名称由来

晴海（はるみ / Harumi）— *晴*（晴空）＋ *海*（大海）。表面平静，底层暗流涌动。

## 开发路线图

| 版本 | 范围 |
|---|---|
| **v0.1** | TrueType 字体，不可见/可见文本，批量放置，`page.size()`，`save_to_bytes()`，GID 重映射，接受 OTF |
| **v0.2** | `draw` feature（`add_rect`、`add_line`），`image` feature（`add_image`、PNG SMask 透明度），页面操作（`rotate_page`、`remove_page`、`insert_blank_page`、`reorder_pages`） |
| **v0.3** | `add_text_box`、`add_rect_stroke`、`add_polygon`、`add_ellipse`、`add_path`；`add_text_with_rotation`；安全加固；`merge_from`；`Document::new`；`extract_pages` |
| **v0.4** | `extract_text_runs`（CID + 标准字体），PDF 元数据读写，`replace_text`（Tj/TJ 重写、跨算子匹配、宽度补偿、保留字体模式），`flow` feature（`FlowDocument`、CJK 自动分页），`html` feature，`extract_page_image` |
| **v0.5** | `add_link_url`、`add_link_internal` — 可点击 PDF 链接注释；`add_bookmark` — 含 CJK UTF-16BE 标题的文档大纲/书签；`HeaderFooter` + `{{page}}`/`{{total}}`；安全修复 |
| **v0.6** | 加密 PDF 读取（`from_file_with_password` / `is_encrypted` / `Error::WrongPassword`）；标记注释（高亮、下划线、删除线、便利贴）；AcroForm `form_fields()` / `fill_form()`；AGL 表格 +116 条目；Identity-H 文字提取回退 |
| **v0.7** *（当前）* | `set_encryption` — 写入密码保护 PDF；`add_squiggly` — 波浪下划线注释；页面框全类型支持（裁切框、修边框、出血框、媒体框读写） |
| **v0.8** | FlowDocument 内联样式（`InlineSpan` 粗体/斜体/颜色合成效果）；`replace_text_resubset` — 含子集扩展的文本替换；MCP `pdf_replace_text` 保持版面的翻译流程和非 Identity `CIDToGIDMap` 诊断；`cargo semver-checks` CI |

---

## 贡献

欢迎在 [github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi) 提交 Issue 和 PR。

代码库中最复杂的部分是 `src/font/embed.rs`（CID 字体对象图构建）。如果您在特定 PDF 查看器中发现渲染问题，请在 Issue 中注明查看器名称和版本。

---

## 许可证

MIT OR Apache-2.0
