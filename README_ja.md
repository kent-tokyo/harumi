# harumi

**既存PDFの抽出座標と推定領域を使って、CJK/多言語テキストを書き戻す Pure Rust PDFエンジン。**

harumi は既存PDFからテキスト位置を抽出し、翻訳・置換した結果を
推定したレイアウト領域へ書き戻します。Overlay モードでは既存のページ内容を
下地として保持しますが、視覚的同一性は保証しません。複雑な段組み、回転・縦書き、
画像上の文字を含むPDFでは品質レポートと目視確認が必要です。
CIDフォント、ToUnicode CMap、フォントサブセット、テキストフィット、
レイアウト衝突検出はすべて自動処理されます。

現在のリリースは **v1.23.6** です。変更履歴は
[CHANGELOG.md](CHANGELOG.md)、対応範囲と検証状況は
[PDF ecosystem comparison contract](docs/PDF_ECOSYSTEM.md) を参照してください。

## harumiが選ばれる領域

入力が既存PDFで、ページをできるだけ維持したまま本文を変更したい場合に
harumiを選んでください。日本語・中国語・韓国語のテキスト置換、PDF翻訳、
OCR検索レイヤー追加、座標付き領域への書き戻しを、高レイヤーAPIで扱えます。
低レイヤーのオブジェクト操作と、新規PDFの組版生成の間を埋めるライブラリです。

| 主な目的 | 最初に検討するライブラリ | harumiの位置づけ |
|---|---|---|
| ビューア表示・対話的編集 | `pdfium-render` | 任意のrenderer/oracle。標準ビルドにnative runtimeを持たず、CJK書き戻し・抽出・品質診断を担当 |
| 新規帳票・複雑な組版 | `genpdf` / `printpdf` | Flow/HTMLでCJK/Latin混在、表、テンプレート、改ページを測定可能に処理 |
| PDF内部構造の直接操作 | `lopdf` | オブジェクトモデルの上に、ページ・文字・フォント・注釈・save/reload操作を提供 |
| 大量抽出・Markdown/JSON | `unpdf` / `pdf_oxide` | 座標付きCJK抽出と、翻訳・書き戻しへ渡せる中間処理を提供 |
| 軽量な低レイヤーwriter | `pdf-writer` | CJKフォント・ToUnicode・既存PDF編集・文書自動化を担当 |

すべてのライブラリを置き換えるものではありません。既存PDFの保持、CJK文字の
正確性、位置に基づく置換、Pure Rust/WASMの配布が要件なら、harumiが候補になります。
詳細は[領域別比較ロードマップ](docs/PDF_ECOSYSTEM.md)を参照してください。

[![harumi on crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![harumi-ai on crates.io](https://img.shields.io/crates/v/harumi-ai.svg?label=harumi-ai)](https://crates.io/crates/harumi-ai)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Demo](https://img.shields.io/badge/demo-live-brightgreen)](https://kent-tokyo.github.io/harumi/)

[English](README.md) | [中文](README_zh.md) | [한국어](README_kr.md)

**[ブラウザでデモを試す →](https://kent-tokyo.github.io/harumi/)** — テキスト・矩形・直線・フリーハンドペンのアノテーションエディタ（WASMでブラウザ完結）

**主な用途：**
- デジタルPDFの翻訳（テキスト抽出 → LLMで翻訳 → レイアウト推定に基づく書き戻し）
- スキャンPDFの翻訳（OCR JSON を入力 → 翻訳 → 元テキスト領域を隠して翻訳文をオーバーレイ）
- スキャンPDFへのOCR検索テキストレイヤー追加
- 日本語・中国語・韓国語テキストのオーバーレイ・スタンプ
- ページ操作・注釈・フォーム編集・PDF結合
- WASM・Lambda・Tauri・MCPを使ったAI文書ワークフロー

> OCRエンジンでも、PDFビューワーでもありません。
> harumi は文書自動化のための「PDF書き戻しレイヤー」です。

---

## harumi が解決すること

**Before（harumi なし）:**
PDF仕様書を読みながらCIDフォントオブジェクトを手動組み立て。CMap生成・GIDマッピング・サブセット化を数百行で自前実装。それでも文字化けと格闘。

**After（harumi あり）:**

```rust
let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;
doc.page(1)?.add_invisible_text("検索対象テキスト", font, [72.0, 700.0], 12.0)?;
doc.save("searchable.pdf")?;
```

フォントのサブセット化・CIDエンコーディング・ToUnicode CMap生成・GID再採番はすべて自動。

---

## デジタルPDFとスキャンPDFを翻訳する

**harumi-ai** はデジタルPDFとスキャンPDFを同一のAPIで翻訳します。

```rust
use harumi_ai::{InputTextSource, TranslateOptions, translate_pdf};

// デジタルPDF（デフォルト）
let opts = TranslateOptions::new("ja", my_llm, font_bytes);
let output = translate_pdf(&digital_pdf, opts).await?;

// スキャンPDF — ocrs-cjk, PaddleOCR などの OCR JSON を渡す
let ocr_json = std::fs::read("ocr.json")?;
let mut opts = TranslateOptions::new("ja", my_llm, font_bytes);
opts.input_source = InputTextSource::OcrJson(ocr_json);
let output = translate_pdf(&scanned_pdf, opts).await?;
```

OCR は ocrs-cjk・PaddleOCR・hOCR・ALTO など HierText フォーマットに対応する任意のツールから取得できます。

**スキャンPDFの始め方：**

```bash
# 1. CJK OCR CLI をインストール
cargo install ocrs-cjk-cli

# 2. OCR 実行（テキスト・バウンディングボックス・信頼度を出力）
ocrs scanned.pdf --json -o ocr.json

# 3. harumi-ai で翻訳・書き戻し
# （TranslateOptions に InputTextSource::OcrJson を設定）
```

### OCR 検索テキストレイヤー（翻訳なし）

翻訳せずに不可視の検索テキストレイヤーを追加する場合（RAGインデックスや検索用）は、OCR出力を直接 `add_invisible_text` に渡します：

```bash
cargo run --example ocrs_cjk_to_searchable_pdf -- \
  examples/fixtures/scanned_sample.pdf \
  examples/fixtures/ocrs_sample.json \
  /path/to/NotoSansCJKjp-Regular.ttf \
  searchable.pdf
```

### AI による OCR 補正

OCR エンジンは字形が似たCJK文字を誤認識することがあります。LLM で誤りを補正しつつ、元のバウンディングボックスを保持できます。

**このモードの核心：AI は text だけ補正する。バウンディングボックスは動かさない。**

> **OCR補正 ≠ 翻訳。** 翻訳 write-back は region レベルの設計が必要です。翻訳パスには `harumi-ai` を使ってください。

詳細は `examples/fixtures/` 内の `ocrs_sample_raw.json`、`ocrs_sample_corrected.json`、
`ai_correction_report.json` を参照してください。

harumi は OCR エンジンではありません。翻訳パスには、LLM の上に構築された `harumi-ai` を使います。

---

## 得られるもの

**[全機能一覧 →](docs/FEATURES.md)**

| 課題 | harumi の答え |
|---|---|
| CJKフォントのサブセット化が難しい | `embed_font()` 1回で完結。使用文字だけ自動的に間引き、GIDも正しく再採番 |
| 既存ページを保持したい | Overlay は新しい内容を追記し、置換モードは対象のコンテンツストリームだけを書き換える |
| WASM / Lambda / クロスコンパイル環境でビルドしたい | 純Rust。C依存ゼロ |
| OCRテキストを座標付きで埋め込みたい | `add_invisible_text` / バッチ版 `add_invisible_text_runs` |
| 既存PDFのテキストを置換したい | `replace_text` / `replace_text_preserve_font` / `replace_text_resubset` |
| レイアウト保持翻訳の品質ゲートが欲しい | `extract_layout_regions` → `plan_text_for_regions` → `assess_page_layout_quality` |

比較表・詳細機能説明は[英語 README](README.md) を参照してください。

新規帳票・レポート生成は `printpdf` / `genpdf`、または harumi の
`FlowDocument` の責務です。一方、既存 PDF の翻訳・テキスト置換・座標付き
追記・ページ再配置は harumi の既存 PDF 書き戻し API の責務です。比較用の
固定 fixture は [`docs/PDF_ECOSYSTEM.md`](docs/PDF_ECOSYSTEM.md) にあります。

---

## MCPサーバー（harumi-mcp）

Claude Code・Cursor・Continue IDE から harumi のPDF操作ツールを直接利用できます：

```bash
cargo build -p harumi-mcp
# 利用可能なツール: pdf_extract_text, pdf_extract_all_pages, pdf_replace_text,
#                   pdf_add_invisible_text, pdf_html_to_pdf, pdf_merge, pdf_page_info
```

[smithery.ai](https://smithery.ai) または [mcp.so](https://mcp.so) に登録するとワンクリックでインストールできます。

---

## クイックスタート

```toml
[dependencies]
harumi = "1"
```

### CJKフォントの取得

日本語・中国語・韓国語の処理には **NotoSansCJK** フォント（Google Fonts、無料・OFLライセンス）を使います：

```bash
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKjp-Regular.ttf
```

または [fonts.google.com](https://fonts.google.com) で「Noto Sans CJK」を検索。

### 不可視のOCRテキストレイヤー

```rust
let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJKjp-Regular.ttf"))?;

doc.page(1)?.add_invisible_text(
    "ここにOCRで読み取った日本語テキスト",
    font,
    [100.0, 250.0], // x, y（PDFポイント、原点：左下）
    12.0,
)?;

doc.save("searchable.pdf")?;
```

### 可視テキストのオーバーレイ

```rust
// ページ中央に赤いスタンプ
let (w, h) = doc.page(1)?.size()?;
doc.page(1)?.add_text(
    "機密",
    font,
    [w / 2.0 - 20.0, h / 2.0],
    24.0,
    [0.8, 0.0, 0.0], // 赤
)?;
```

**[コード例一覧 →](docs/EXAMPLES.md)** — ページ操作、PDF結合、HTML→PDF、注釈、フォーム、図形、画像、FlowDocument

**[API リファレンス →](docs/API.md)** — 座標系、フィーチャーフラグ、対応フォント、内部設計

ドキュメントの一覧は [docs/README.md](docs/README.md) を参照してください。

---

## Contributing

[github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi) でIssueやPRを歓迎します。

---

## License

MIT OR Apache-2.0
