# harumi

**既存PDFへ座標付きのCJKテキストを書き戻す、Pure RustのPDFライブラリ。**

harumiは既存PDFから文字と座標を抽出し、翻訳文・置換文・OCR検索レイヤーを
書き戻します。CJKフォント、ToUnicode CMap、サブセット化、レイアウト診断を
自動処理します。OCRエンジンやPDFビューアではありません。

現在のリリースは **v1.23.6** です。

[![crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![ライセンス](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

[English](README.md) | [한국어](README_kr.md) | [中文](README_zh.md)

## 向いている用途

- 既存PDFのページ内容を保ちながら本文を置換・翻訳する
- 日本語・中国語・韓国語のフォントを埋め込む
- スキャンPDFにOCR検索テキストを追加する
- WASM、Lambda、TauriでPDF自動処理を行う

画面表示には `pdfium-render`、新規帳票には `printpdf`/`genpdf`、低レベルの
オブジェクト編集には `lopdf` が適しています。役割の比較は
[PDF ecosystem contract](docs/PDF_ECOSYSTEM.md) を参照してください。

## クイックスタート

```toml
[dependencies]
harumi = "1"
```

日本語・中国語・韓国語には、サブセット化できるTrueType版のNoto Sans CJKを
使います。

```rust
use harumi::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::from_file("input.pdf")?;
    let font = doc.embed_font(include_bytes!("NotoSansCJKjp-Regular.ttf"))?;
    doc.page(1)?.add_invisible_text(
        "検索可能な日本語テキスト", font, [72.0, 700.0], 12.0,
    )?;
    doc.save("output.pdf")?;
    Ok(())
}
```

## 主な機能

| 用途 | API・feature |
|---|---|
| 文字と座標の抽出 | `extract_text_runs`, `extract_text_chunks` |
| 既存文字の置換 | `replace_text`, `replace_text_preserve_font`, `replace_text_resubset` |
| OCR検索レイヤー | `add_invisible_text`, `add_invisible_text_runs` |
| ページ操作 | 結合、分割、回転、並べ替え、しおり、フォーム、メタデータ |
| 図形・画像 | `draw`、`image` feature |
| 帳票・HTML | `flow`、`html` feature |
| 一括抽出 | JSON/Markdown、ページ単位の警告 |

詳細は[機能一覧](docs/FEATURES.md)、[コード例](docs/EXAMPLES.md)、
[API概要](docs/API.md)を参照してください。

## 翻訳・スキャンPDF

[`harumi-ai`](harumi-ai/README.md) はデジタルPDFとOCR JSONを翻訳するための
追加ライブラリです。翻訳後の領域をオーバーレイまたは置換し、はみ出しや衝突を
警告します。画素単位の一致は保証しません。

```toml
[dependencies]
harumi-ai = "0.10"
```

## MCPサーバー

[`harumi-mcp`](harumi-mcp/README.md) は抽出、置換、OCRレイヤー、HTML変換、結合、
回転、ページ情報をMCPクライアントへ提供します。

```bash
cargo install harumi-mcp
harumi-mcp
```

## 制約

- 完全なUnicode改行、RTL、縦書き、ブラウザ相当のCSSは未完了または限定対応です。
- OpenType CFFのサブセット化には対応していません。TrueTypeを使用してください。
- PDFページの表示・ラスター化は別のrendererが必要です。
- 複雑な段組み、回転、縦書き、画像上の文字は保存後に目視確認してください。

## ドキュメントと開発

- [ドキュメント一覧](docs/README.md)
- [ロードマップ](ROADMAP.md)
- [変更履歴](CHANGELOG.md)

IssueとPRを歓迎します。ライセンスはMIT OR Apache-2.0です。
