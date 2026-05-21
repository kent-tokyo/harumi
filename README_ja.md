# harumi

**テキスト注入・抽出、ページ操作、図形描画まで — 純Rust製PDF操作ライブラリ。**  
日本語・中国語・韓国語（CJK）フォント完全対応。C依存ゼロ。WASM対応。

[![Crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

[English](README.md) | [中文](README_zh.md) | [한국어](README_kr.md)

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

フォントのサブセット化・CIDエンコーディング・ToUnicode CMap生成・GID再採番はすべて自動。ライブラリが隠蔽します。

---

## 得られるもの

| 課題 | harumi の答え |
|---|---|
| CJKフォントのサブセット化が難しい | `embed_font()` 1回で完結。使用文字だけ自動的に間引き、GIDも正しく再採番 |
| 既存PDFの構造を壊したくない | 追記のみ。元のオブジェクトグラフには触れない |
| WASM / Lambda / クロスコンパイル環境でビルドしたい | 純Rust。C依存ゼロ |
| OCRテキストを座標付きで埋め込みたい | `add_invisible_text` / バッチ版 `add_invisible_text_runs` |
| PDFに透かし・スタンプを押したい | `add_text(color)` で任意のRGB色で可視テキストを重ねる |
| ページサイズに合わせて座標を決めたい | `page.size()` で MediaBox を取得 |
| Tauri / WASM でメモリ上でPDFを扱いたい | `save_to_bytes()` で `Vec<u8>` を直接取得 |
| ハイライト矩形・線を描画したい | `add_rect` / `add_line`（`draw` feature、追加依存なし） |
| テキストボックス枠線や吹き出し多角形を描画したい | `add_rect_stroke` / `add_polygon`（`draw` feature） |
| テキストを折り返して矩形内に流し込みたい | `add_text_box`（feature gate 不要） |
| JPEG・PNG画像を埋め込みたい | `add_image` / `add_image_with_opacity`（`image` feature） |
| PNG の透明度を保持したい（署名・透かし） | 透明背景PNGはPDF SMaskで自動処理 — 白背景なし |
| ページを回転・削除・並び替えたい | `rotate_page` / `remove_page` / `insert_blank_page` / `reorder_pages`（feature フラグ不要） |
| 2つのPDFを1つに結合したい | `merge_from` でもう一方のドキュメントの全ページを末尾に追加。コンテンツとフォントは保持される |
| 既存ファイルなしでPDFをゼロから作成したい | `Document::new(size)` で白紙の1ページPDFを作成。ページ追加は `insert_blank_page` |
| PDFを個別ファイルに分割したい | `extract_pages` で指定ページのみを含む新しい `Document` を任意の順序で取得 |
| 既存PDFからテキストの位置情報を取り出したい | `extract_text_runs` でCIDフォントと標準シンプルフォント（Type1、TrueType、WinAnsiなど）をデコード |
| PDFのメタデータ（タイトル・著者など）を読み書きしたい | `doc.metadata()` で `/Info` を読み込み、`doc.set_metadata(&meta)` で書き込む |
| 既存PDFのテキストを検索・置換したい（新フォント） | `page.replace_text(old, new, font)` でコンテントストリームをその場で書き換え。マッチ件数を `usize` で返す。フォント切替・幅補正も自動 |
| 既存フォントで文字を置換したい | `page.replace_text_preserve_font(old, new)` — `FontHandle` 不要。マッチ件数を返す。グリフ検証はコール時に即時実行 |
| 変更なしで置換可能か事前確認したい | `page.can_replace_text(old, new)` — 読み取り専用スキャン。マッチ件数または `Err(FontCharNotMapped)` を返す |
| 楕円・円を描画したい | `add_ellipse(rect, color, opacity, filled, stroke_width)`（`draw` feature） |
| 塗りと枠線を同時に描画したい | `add_ellipse` / `add_polygon` / `add_path` で `filled=true` かつ `stroke_width>0` — PDF `B` 演算子を使用 |
| 開放・閉鎖パスを統一APIで描画したい | `add_path(points, closed, color, filled, stroke_width, opacity)`（`draw` feature） |
| テキストを回転させたい（透かし・斜めスタンプ） | `add_text_with_rotation(text, font, pos, size, color, opacity, degrees)` |
| 複数の `Tj` 演算子にまたがるテキストを置換したい | `replace_text` / `replace_text_preserve_font` — クロス演算子マッチングに対応 |

---

## なぜ今まで存在しなかったか

JavaScriptには [`pdf-lib`](https://pdf-lib.js.org/) があり、フォントのサブセット化・CMap生成・テキストレイヤー合成を透過的に処理してくれます。Rustの既存ツールではそれができません：

- **`lopdf`** — 低レイヤのバイナリ操作。CIDフォントオブジェクトをPDF仕様書を読みながら手動で組み立てる必要がある
- **`printpdf`** — 新規PDF作成専用。既存PDFの編集は不可
- **`pdfium-render`** — C++バインディングを必要とし、WASM・クロスコンパイル・AWS Lambda環境でビルドが通らない

`harumi` はその空白を埋めます。

---

## クイックスタート

```toml
[dependencies]
harumi = "0.3"
```

### 不可視のOCRテキストレイヤー

```rust
use harumi::{Document, TextRun};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::from_file("scanned.pdf")?;

    // フォントを埋め込む（サブセット化・CMap生成・GID再採番はsave()時に自動処理）
    let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;

    // 1ページ目にOCRテキストを「不可視」レイヤーとして重ねる
    doc.page(1)?.add_invisible_text(
        "ここにOCRで読み取った日本語テキスト",
        font,
        [100.0, 250.0], // X, Y 座標（PDF座標系：左下原点、単位はポイント）
        12.0,
    )?;

    // 保存（既存のPDF構造は壊れない）
    doc.save("searchable_japanese.pdf")?;
    Ok(())
}
```

### 可視テキストの重ね合わせ

```rust
// ページサイズを取得して中央にスタンプを押す
let (w, h) = doc.page(1)?.size()?;
doc.page(1)?.add_text(
    "社外秘",
    font,
    [w / 2.0 - 30.0, h / 2.0],
    24.0,
    [0.8, 0.0, 0.0], // 赤（RGB、0.0〜1.0）
)?;
```

### メモリ上での出力

```rust
// Tauriコマンド・WASM・インメモリパイプライン向け
let pdf_bytes: Vec<u8> = doc.save_to_bytes()?;
```

### 多行テキストボックス（feature gate 不要）

```rust
// 単語境界（Latin）または任意位置（CJK）で折り返し、ボックス下端でクリップ
doc.page(1)?.add_text_box(
    "折り返しが必要な長い日本語テキストをここに入れます。",
    font,
    [72.0, 400.0, 200.0, 120.0], // [x, y, width, height]
    12.0,
    [0.0, 0.0, 0.0],              // 黒
    0.0,                          // 0.0 = font_size * 1.2 を行間に使用
)?;
```

### ページ操作

```rust
// 全ページを時計回りに90°回転
for page_num in 1..=doc.page_count() {
    doc.rotate_page(page_num, 90)?;
}

// 空白の表紙ページを削除
doc.remove_page(1)?;

// 1ページ目の前に白紙のA4タイトルページを挿入
doc.insert_blank_page(0, (595.0, 842.0))?;

// 3ページのドキュメントのページ順を逆にする
doc.reorder_pages(&[3, 2, 1])?;

doc.save("output.pdf")?;
```

### PDFの結合

```rust
let mut base = Document::from_file("a.pdf")?;
let appendix = Document::from_file("b.pdf")?;
base.merge_from(appendix)?;
base.save("merged.pdf")?;
```

保持されるもの：全ページのコンテンツ・埋め込みフォント・画像・リソース。  
保持されないもの：アウトライン/ブックマーク、AcroForm、`/Info` メタデータ（著者・作成日など）。

> **前提条件**：`other` にフラッシュされていない保留中の操作がないこと（新規読み込み直後、または `save_to_bytes()` 後に再読み込みした状態）。

### 白紙PDFの作成

```rust
let mut doc = Document::new((595.0, 842.0))?;   // 白紙A4
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;
doc.page(1)?.add_text("Hello, world!", font, [72.0, 700.0], 24.0, [0.0, 0.0, 0.0])?;
doc.save("output.pdf")?;
```

### ページの抽出

```rust
let doc = Document::from_file("large.pdf")?;
let mut excerpt = doc.extract_pages(&[3, 5, 7])?;  // 3・5・7ページ目をこの順で取得
excerpt.save("excerpt.pdf")?;
```

### 既存PDFからテキストを抽出

```rust
let doc = Document::from_file("existing.pdf")?;
let runs = doc.extract_text_runs(1)?;
for fragment in &runs {
    println!("{:?} at ({:.1}, {:.1})", fragment.text, fragment.x, fragment.y);
}
```

harumi が出力したPDF（Identity-H CIDフォント）だけでなく、任意の既存PDFにも対応。Type1・TrueTypeなど標準シンプルフォント（WinAnsiEncoding・MacRomanEncoding・StandardEncoding・`/Differences` 辞書）も解析できます。

### 既存PDFのテキスト置換

```rust
let mut doc = Document::from_file("contract.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansJP-Regular.ttf"))?;
// マッチ件数を返す（0 = 見つからなかった）
let n = doc.page(1)?.replace_text("Hello", "こんにちは", font)?;
doc.save("translated.pdf")?;
```

同一フォントコンテキスト内（同一 `Tf` / `BT`〜`ET` ブロック）の連続する `Tj`/`TJ` 演算子にまたがるテキストにもマッチします（クロス演算子マッチング）。位置変更演算子（`Td`, `Tm`）を挟む場合は対象外です。

### 元フォントをそのまま使ってテキストを置換

グリフ検証はコール時に即時実行されます — `save()` まで待ちません：

```rust
let mut doc = Document::from_file("contract.pdf")?;
match doc.page(1)?.replace_text_preserve_font("Draft", replacement) {
    Ok(n) if n > 0 => { /* n 件置換キュー済み — 追加フォント不要 */ }
    Ok(_) => { /* old_text が見つからなかった */ }
    Err(_) => {
        // グリフがサブセットにない — 明示フォントでフォールバック
        let font = doc.embed_font(include_bytes!("font.ttf"))?;
        doc.page(1)?.replace_text("Draft", replacement, font)?;
    }
}
doc.save("output.pdf")?;
```

### 変更せずに置換可能か確認する

```rust
let mut doc = Document::from_file("contract.pdf")?;
match doc.page(1)?.can_replace_text("Draft", "Final") {
    Ok(0) => println!("'Draft' は1ページ目に見つかりませんでした"),
    Ok(n) => println!("{n} 件見つかりました。グリフも問題なし"),
    Err(e) => println!("グリフが欠けています: {e}"),
}
```

### PDFメタデータの読み書き

```rust
use harumi::{Document, PdfMetadata};

let mut doc = Document::from_file("report.pdf")?;

// メタデータを読み込む
let meta = doc.metadata()?;
println!("タイトル: {:?}", meta.title);

// メタデータを書き込む（None フィールドは /Info に書かれない）
doc.set_metadata(&PdfMetadata {
    title: Some("年次報告書 2026".into()),
    author: Some("Harumi Team".into()),
    subject: None,
    keywords: None,
    creator: None,
})?;
doc.save("report_with_meta.pdf")?;
```

### 図形描画（`draw` feature）

```toml
harumi = { version = "0.3", features = ["draw"] }
```

```rust
// 黄色塗り矩形（x, y, width, height、PDFポイント単位）
doc.page(1)?.add_rect([72.0, 690.0, 200.0, 14.0], [1.0, 1.0, 0.0], 0.4)?;

// 青い枠線のみの矩形（塗りなし）
doc.page(1)?.add_rect_stroke([72.0, 400.0, 200.0, 100.0], [0.0, 0.0, 1.0], 1.5, 1.0)?;

// 塗り三角形（吹き出しの矢印先端）— 末尾引数は stroke_width（0.0 = 枠線なし）
doc.page(1)?.add_polygon(
    &[[100.0, 500.0], [150.0, 600.0], [200.0, 500.0]],
    [1.0, 0.5, 0.0], 1.0, true, 0.0,
)?;

// 塗り + 枠線同時描画（PDF `B` 演算子）
doc.page(1)?.add_polygon(
    &[[100.0, 500.0], [150.0, 600.0], [200.0, 500.0]],
    [0.0, 0.6, 1.0], 1.0, true, 2.0,
)?;

// 黒い下線
doc.page(1)?.add_line([72.0, 600.0], [300.0, 600.0], [0.0, 0.0, 0.0], 1.5, 1.0)?;

// 半透明の青い楕円（バウンディングボックス: x, y, width, height）
doc.page(1)?.add_ellipse([200.0, 300.0, 150.0, 100.0], [0.0, 0.4, 1.0], 0.7, true, 0.0)?;

// 円の輪郭のみ（2pt 枠線）
doc.page(1)?.add_ellipse([100.0, 100.0, 80.0, 80.0], [1.0, 0.0, 0.0], 1.0, false, 2.0)?;

// 開放パス（多角形：閉じない）
doc.page(1)?.add_path(
    &[[100.0, 500.0], [150.0, 600.0], [200.0, 500.0]],
    false, [0.2, 0.8, 0.2], false, 1.5, 1.0,
)?;

// 45° 回転ウォーターマーク
let font = doc.embed_font(include_bytes!("NotoSansCJK.ttf"))?;
let (w, h) = doc.page(1)?.size()?;
doc.page(1)?.add_text_with_rotation(
    "社外秘",
    font,
    [w / 2.0, h / 2.0],
    72.0,
    [0.8, 0.0, 0.0],  // 赤
    0.3,              // 30% 不透明度
    45.0,             // 反時計回りに45°
)?;
```

### 画像埋め込み（`image` feature）

```toml
harumi = { version = "0.3", features = ["image"] }
```

```rust
let jpeg = std::fs::read("stamp.jpg")?;
// JPEG（再エンコードなし）とPNGに対応
doc.page(1)?.add_image(&jpeg, [72.0, 500.0, 100.0, 100.0])?;

// 透明度付き（0.0 = 完全透明、1.0 = 不透明）
doc.page(1)?.add_image_with_opacity(&jpeg, [72.0, 400.0, 100.0, 100.0], 0.75)?;

// アルファチャンネル付きPNG — 透明領域はPDF SMaskで処理、白背景なし
let sig_png = std::fs::read("signature.png")?;
doc.page(1)?.add_image(&sig_png, [72.0, 300.0, 200.0, 80.0])?;
```

---

## API

```rust
// 読み込み
let mut doc = Document::from_file("path/to/file.pdf")?;
let mut doc = Document::from_bytes(&bytes)?;

// フォント埋め込み（1ファイルにつき1回。ハンドルを複数ページで使い回せる）
let font: FontHandle = doc.embed_font(ttf_bytes)?;

// ページサイズ（PDFポイント、幅 × 高さ）
let (width, height) = doc.page(1)?.size()?;

// 不可視テキスト（OCRテキストレイヤー用）
doc.page(1)?.add_invisible_text(text, font, [x, y], size)?;

// 可視テキスト（透かし・スタンプ・注釈用）
doc.page(1)?.add_text(text, font, [x, y], size, [r, g, b])?;

// バッチ配置（サブセット処理が1回にまとまるため効率的）
doc.page(1)?.add_invisible_text_runs(&[
    TextRun { text: "1行目テキスト".into(), font, x: 72.0, y: 700.0, font_size: 11.0, render_mode: 3, color: [0.0; 3] },
    TextRun { text: "2行目テキスト".into(), font, x: 72.0, y: 685.0, font_size: 11.0, render_mode: 3, color: [0.0; 3] },
])?;

// ページ構造（feature フラグ不要）
doc.page_count()                          // u32
doc.rotate_page(n, degrees)?;             // 90の倍数; 累積する
doc.remove_page(n)?;                      // 最後の1ページは削除不可
doc.insert_blank_page(after, (w, h))?;    // after=0 で先頭に挿入
doc.reorder_pages(&[new_order...])?;      // 1始まりの旧ページ番号を指定
doc.extract_pages(&[n1, n2, ...])?;       // 指定ページのみの新しい Document

// ゼロから作成
Document::new((w, h))?;                   // 白紙の1ページPDF

// PDF結合（other に保留中の操作がないこと）
doc.merge_from(other)?;             // other の全ページを末尾に追加

// 保存
doc.save("output.pdf")?;
doc.save_to_bytes()?;   // インメモリ版

// 既存PDFからテキストを抽出（CIDフォント＋標準シンプルフォント対応）
let runs: Vec<TextFragment> = doc.extract_text_runs(page_number)?;

// PDFメタデータ（/Info 辞書）
let meta: PdfMetadata = doc.metadata()?;
doc.set_metadata(&PdfMetadata { title: Some("...".into()), ..Default::default() })?;

// 既存コンテントストリームのテキスト置換（シングルオペレータマッチング）
doc.page(1)?.replace_text(old_text, new_text, font)?;
```

### 座標系について

座標は **PDFポイント**（1pt = 1/72インチ）で、原点はページ**左下**です。Tesseract / hOCR など左上原点のピクセル座標を使う場合は `ocr` featureのヘルパーを使ってください：

```toml
harumi = { version = "0.3", features = ["ocr"] }
```

### Feature flags

| フラグ | 有効になる機能 | 追加依存 |
|---|---|---|
| *(デフォルト)* | テキスト重ね合わせ・フォント埋め込み・`add_text_box`・`add_text_box_aligned`・`add_text_with_opacity`・`add_text_box_with_opacity` | lopdf, allsorts, ttf-parser |
| `draw` | `add_rect`, `add_line`, `add_rect_stroke`, `add_polygon`, `add_polyline`, `add_ellipse` — 図形描画 | なし |
| `image` | `add_image`, `add_image_with_opacity` — JPEG/PNG（`draw` を有効化） | `image` クレート |
| `ocr` | `ocr::hocr_y_to_pdf`・`ocr::hocr_x_to_pdf`・`ocr::pixel_size_to_pt` — Tesseract 座標変換ヘルパー | なし |

```rust
let pdf_y = harumi::ocr::hocr_y_to_pdf(pixel_y, page_height_pts, image_dpi);
let pdf_x = harumi::ocr::hocr_x_to_pdf(pixel_x, image_dpi);
let pt    = harumi::ocr::pixel_size_to_pt(pixel_size, image_dpi);
```

---

## 対応フォント

| フォント形式 | 対応状況 |
|---|---|
| TrueType (`.ttf`) | 対応・動作確認済み |
| OpenType CFF (`.otf`) | 受け付けるが allsorts 依存（後述） |
| TTC コレクション | 対応済み（index 0 を使用） |

日本語・中国語・韓国語には [Noto Sans CJK](https://github.com/notofonts/noto-cjk) の **TrueType** バリアントを推奨します（E2E動作確認済み）：

```
NotoSansCJKjp-Regular.ttf  （日本語）
NotoSansCJKsc-Regular.ttf  （簡体字）
NotoSansCJKtc-Regular.ttf  （繁体字）
NotoSansCJKkr-Regular.ttf  （韓国語）
```

> **OTFについて**: harumi は `.otf` ファイルを受け付け、`FontFile3 /OpenType` として埋め込みます。ただし allsorts v0.17 が一部の CFF バリアント（CFF2可変フォントなど）をサブセット化できない場合があり、その場合は `save()` 時に `FontParse` エラーになります。確実に動作させるには TTF バリアントをご利用ください。

---

## 内部実装

```
harumi
├── lopdf v0.40          — 既存PDFのオブジェクトグラフ解析・編集
├── allsorts v0.17+      — TrueTypeフォントサブセット化（Prince組版ソフトで実績あり）
└── ttf-parser           — フォントメタデータ取得（bbox、units_per_em、ascender）
```

フォントパイプラインの流れ：

1. 使用文字を収集 → Unicode コードポイントのセットを作成
2. フォントの `cmap` テーブルで コードポイント → 元のグリフID（GID）にマッピング
3. allsorts で使用グリフのみにTTFをサブセット化（GIDは **0..N に再採番**）
4. `gid_to_char` とアドバンス幅を元GID → 新GIDに **再マッピング**（文字化け防止）
5. PDFのCIDフォントオブジェクトグラフを構築: `Type0 → CIDFontType2 → FontDescriptor → FontFile2`
6. `/ToUnicode` CMAPストリームを生成（ビューアでのテキスト選択・検索を可能にする）
7. ページの `/Contents` 配列に新しいコンテントストリームを追記

サブセット化は**遅延実行**：`embed_font()` は生のTTFバイト列を保持し、`save()` 時に全ページの使用文字を収集し、フォントごとに1回だけ処理します。

---

## 名前について

晴海（はるみ）— *晴*（晴れた空）＋ *海*（海）。表面は穏やか、内部には多くの処理が走っている。

## ロードマップ

| バージョン | スコープ |
|---|---|
| **v0.1** | TrueType、不可視・可視テキスト、バッチ配置、`page.size()`、`save_to_bytes()`、GIDバグ修正、OTF受け付け |
| **v0.2** | `draw` feature（`add_rect`、`add_line`）、`image` feature（`add_image`、`add_image_with_opacity`）、CFF2早期エラー、TTCマジック検出、MediaBox親チェーン走査 |
| **v0.3** | `add_text_box`・`add_rect_stroke`・`add_polygon`; セキュリティ強化（NaNガード・二重保存防止・間接Contents配列対応・JPEGマーカー修正・PNG整数オーバーフロー修正） |
| **v0.4** | PNG真の透明度（SMask）— 透明背景PNGが白背景なしで正しく描画される |
| **v0.5** | `add_text_with_opacity`、`add_text_box_aligned`（VerticalAlign）、`add_polyline`、`add_text_box_with_opacity` — **完了** |
| **v0.6** | ページ操作 — `rotate_page`、`remove_page`、`insert_blank_page`、`reorder_pages` — **完了** |
| **v0.7** | `merge_from`（PDF結合）、`remove_page` 正確性・オーファンオブジェクト修正 — **完了** |
| **v0.8** | `Document::new`（白紙PDFの作成）、`extract_pages`（ページ分割） — **完了** |
| **v0.9** | `extract_text_runs`（CIDフォント＋標準シンプルフォント対応）、PDFメタデータ読み書き（`metadata()`、`set_metadata()`、`PdfMetadata`） — **完了** |
| **v0.10** | `replace_text` — コンテントストリームの真のテキスト置換：Tj/TJ書き換え、フォント自動切替、Td幅補正 — **完了** |
| **Next（v0.11以降）** | `#[non_exhaustive]` on Error、MSRV宣言、WASM CI、crates.io公開 |

---

## コントリビュート

[github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi) でIssue・PRを歓迎します。

最も複雑なコードは `src/font/embed.rs`（CIDフォントオブジェクトグラフの構築）です。特定のPDFビューアでの描画バグを報告する場合は、ビューア名とバージョンを明記してください。

---

## ライセンス

MIT OR Apache-2.0
