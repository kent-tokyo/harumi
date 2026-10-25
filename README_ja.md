# harumi

**既存PDFのテキストを位置情報付きで抽出し、CJK・多言語テキストを書き戻すPure Rustライブラリ。**

現行版は **1.23.4** です。

[English](README.md) · [中文](README_zh.md) · [한국어](README_kr.md)

## できること

- 既存PDFのテキスト抽出・置換・翻訳オーバーレイ
- CJKフォントの埋め込み、サブセット化、OCR検索レイヤー追加
- ページの結合・分割・回転・並べ替え、注釈、フォーム、暗号化
- `draw`機能による図形・破線描画、`image`機能による画像埋め込み
- `flow` / `html`機能による帳票・HTMLからのPDF生成
- `svg`機能による限定されたSVG図形のベクターPDF化（CSS色、`rgb()`、`metadata`に対応）
- WASM、Lambda、Tauri、MCPでの利用

OCRエンジンやPDFビューアではありません。複雑な段組み、回転・縦書き、画像背景の
文字では、生成結果をレンダラーで確認してください。

## 最短の例

```rust
use harumi::Document;

let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJKjp-Regular.ttf"))?;
doc.page(1)?.add_invisible_text("検索可能な日本語", font, [72.0, 700.0], 12.0)?;
doc.save("searchable.pdf")?;
# Ok::<(), harumi::Error>(())
```

## 翻訳

`harumi-ai`は、デジタルPDFのテキスト抽出と、OCR JSONを使うスキャンPDFの翻訳に対応します。
LLM翻訳、領域計画、ページ単位の品質レポートを提供します。

## 詳細

- [API概要](docs/API.md)
- [コード例](docs/EXAMPLES.md)
- [機能一覧](docs/FEATURES.md)
- [PDFライブラリ比較の境界](docs/PDF_ECOSYSTEM.md)
- [MCPサーバー](harumi-mcp/README.md)

## 貢献・ライセンス

IssueとPull Requestを歓迎します。MIT OR Apache-2.0。
