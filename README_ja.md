# harumi

> **HARUMI** — **H**igh-level **A**PI for **R**ust-native **U**nicode **M**anipulation and **I**njection

**テキスト注入・抽出、ページ操作、図形描画まで — 純Rust製PDF操作ライブラリ。**  
日本語・中国語・韓国語（CJK）フォント完全対応。C依存ゼロ。WASM対応。

[![Crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Demo](https://img.shields.io/badge/demo-live-brightgreen)](https://kent-tokyo.github.io/harumi/)

[English](README.md) | [中文](README_zh.md) | [한국어](README_kr.md)

**[ブラウザでデモを試す →](https://kent-tokyo.github.io/harumi/)** — テキスト・矩形・直線・フリーハンドペンのアノテーションエディタ（WASMでブラウザ完結）

### 🔌 MCP サーバーとして利用可能

Claude Code・Cursor・Continue IDE から harumi のPDF操作ツールを直接利用できます：

```bash
# MCPサーバーをビルド（純Rust、ランタイム依存なし）
cargo build -p harumi-mcp

# IDE設定で以下のツールを利用可能に：
# - pdf_extract_text: テキスト位置付き抽出
# - pdf_extract_all_pages: 全ページのテキスト位置付き抽出
# - pdf_replace_text: レイアウトを保ったテキスト置換・翻訳
# - pdf_add_invisible_text: OCR検索レイヤー追加
# - pdf_html_to_pdf: HTML→PDF変換
# - pdf_merge: PDF結合
# - pdf_page_info: ページ情報取得
```

PDF翻訳では `pdf_extract_all_pages` で全ページの断片を抽出し、翻訳後に
`pdf_replace_text` で既存レイアウトを保ったまま置換します。非Identity
`CIDToGIDMap` のため再サブセット化できないPDFでは、Unicode TTFを指定して
`mode: "new_font"` を使います。
`harumi-ai` の CLI は、既存レイアウトを保ちたい場合の既定が `overlay` mode です。
新規レイアウトで作り直したい場合だけ `new` を指定してください。
Overlay mode は `detect_text_columns` で複数段組を検出し、訳文を原文の正確な
ベースライン Y に配置します（フォントの実ディセンダー量で白矩形を補正済み）。

[smithery.ai](https://smithery.ai) または [mcp.so](https://mcp.so) に登録予定。

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
| CJKフォントのサブセット化が難しい | `embed_font()` 1回で完結。使用文字だけ自動的に間引き、GIDも正しく再採番。GSUB/GPOS/可変フォントテーブルを除去してmacOS Preview・PSPDFKit互換 |
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
| 既存PDFからテキストの位置情報を取り出したい | `extract_text_runs` でCIDフォントと標準シンプルフォント（Type1、TrueType、Type3、WinAnsiなど）をデコード |
| PDFのメタデータ（タイトル・著者など）を読み書きしたい | `doc.metadata()` で `/Info` を読み込み、`doc.set_metadata(&meta)` で書き込む |
| 既存PDFのテキストを検索・置換したい（新フォント） | `page.replace_text(old, new, font)` でコンテントストリームをその場で書き換え。マッチ件数を `usize` で返す。フォント切替・幅補正も自動 |
| 既存フォントで文字を置換したい | `page.replace_text_preserve_font(old, new)` — `FontHandle` 不要。マッチ件数を返す。グリフ検証はコール時に即時実行 |
| 変更なしで置換可能か事前確認したい | `page.can_replace_text(old, new)` — 読み取り専用スキャン。マッチ件数または `Err(FontCharNotMapped)` を返す |
| 楕円・円を描画したい | `add_ellipse(rect, color, opacity, filled, stroke_width)`（`draw` feature） |
| 塗りと枠線を同時に描画したい | `add_ellipse` / `add_polygon` / `add_path` で `filled=true` かつ `stroke_width>0` — PDF `B` 演算子を使用 |
| 開放・閉鎖パスを統一APIで描画したい | `add_path(points, closed, color, filled, stroke_width, opacity)`（`draw` feature） |
| テキストを回転させたい（透かし・斜めスタンプ） | `add_text_with_rotation(text, font, pos, size, color, opacity, degrees)` |
| 複数の `Tj` 演算子またはフォントランにまたがるテキストを置換したい | `replace_text` / `replace_text_preserve_font` — クロス演算子 **および** クロス `Tf` マッチングに対応 |
| スキャン PDF から埋め込み画像を取り出したい | `extract_page_image` で JPEG または PNG バイト列を取得（`image` feature）。スキャン PDF 専用 |
| PDF にクリッカブルな URL リンクを付けたい | `add_link_url([x, y, w, h], url)` — 不可視 URI アノテーション。任意のビューアでクリックすると URL が開く |
| PDF 内ページへの内部ナビゲーションリンクが必要 | `add_link_internal([x, y, w, h], target_page)` — 同一ドキュメント内の指定ページへジャンプ |
| ブックマーク（ナビゲーションアウトライン）が必要 | `add_bookmark(title, page, y)` — フラットな PDF アウトラインエントリ。CJK タイトルは UTF-16BE で自動エンコード |
| 全ページにページ番号付きヘッダ/フッタを付けたい | `FlowOptions { header: Some(hf), footer: Some(hf), .. }` に `HeaderFooter` を指定（`flow` feature）。`{{page}}`/`{{total}}` をレンダリング時に展開 |
| 見出しからアウトラインエントリを自動生成したい | `FlowOptions { auto_bookmarks: true, .. }`（デフォルト） — `push_heading` のたびにブックマークが作成される |
| パスワード付き PDF を読み込みたい | `Document::from_file_with_password(path, pw)` / `from_bytes_with_password(bytes, pw)` — ユーザー・オーナーパスワード両対応 |
| PDF をパスワード保護して保存したい | `doc.set_encryption(user_pw, owner_pw)` — `save()` 時に 128-bit RC4 で暗号化 |
| PDF が暗号化されていたか確認したい | `doc.is_encrypted()` — 復号後も `true` を返す |
| テキストをハイライト・下線・取り消し線で強調したい | `add_highlight` / `add_underline` / `add_strikeout` / `add_squiggly` — QuadPoints 付き PDF マークアップ注釈 |
| ページに付箋コメントを貼り付けたい | `add_sticky_note([x, y], "テキスト")` — Unicode 対応の Text 注釈 |
| PDF フォームのフィールド値を読み取りたい | `doc.form_fields()` — `Vec<FormField>` を返す（名前・種別・現在値） |
| PDF フォームをプログラムから記入したい | `doc.fill_form(&[("フィールド名", "値")])` — NeedAppearances を自動設定 |
| ページのクロップボックスや印刷用ボックスを操作したい | `page.crop_box()` / `set_crop_box(rect)` / `trim_box()` / `bleed_box()` — 全ボックス種別に対応（`[x,y,w,h]` 形式） |
| ページのコンテンツをスケールしたい（例: A4 → A3） | `page.scale_page_content(sx, sy)` で既存コンテンツの先頭に `cm` 行列を挿入；`resize_page_with_content(w, h)` でスケール＋MediaBox 変更を一度に実行（v1.4+） |
| 別の PDF を現在の PDF に重ねたい（スタンプ合成） | `doc.overlay_from(other)` で `other` の各ページを `self` の対応ページに Form XObject として重ね書き；フォント・画像・透過度も保持（v1.4+） |
| ブックマーク（目次）をすべて削除したい | `doc.clear_outline()` でペンディング中のブックマークと読み込み済み PDF の `/Outlines` ツリーを一括削除（v1.4+） |
| PDF にファイルを添付したい | `doc.attach_file(name, data, mime)` で任意ファイルを EmbeddedFiles として添付（FlateDecode 圧縮・名前順ソート済み）；`doc.list_attachments()` → `Vec<AttachmentInfo>`（v1.4+） |
| 抽出テキストから太字・斜体・フォント名を取得したい | `TextFragment::is_bold`・`is_italic`・`font_family`・`base_font` — PostScript `/BaseFont` 名から解析（v1.4.1+） |
| 抽出テキストから段組レイアウトを検出したい | `detect_text_columns(&frags, page_width)` — X密度ヒストグラムで空白ギャップを検出し `Vec<ColumnZone>` を返す（v1.4.1+） |
| 抽出テキストを行・段落単位でグループ化したい | `group_text_fragments(&frags, GroupingStrategy::Paragraph)` — 隣接フラグメントを `TextGroup` へ統合。`Paragraph` は段落境界まで結合、`Line` は同一行のみ結合。翻訳モデルへの入力品質向上に活用（v1.5+） |
| フォントが特定の文字をカバーするか確認したい | `font_covers_char(font_bytes, ch) -> bool` — ttf-parser 経由で cmap を検索。フォールバックフォント選択に利用（v1.5+） |
| テーブル形式 PDF からセル単位でテキストを取得したい | `extract_table_cells(&frags, page_width, page_height)` — `detect_text_columns` で列を、Y座標クラスタリングで行を検出し `Vec<TableCell>` を返す。各セルに `row`/`col`（0始まり）・`text`・バウンディングボックスを付与。罫線なし PDF はヒューリスティック（v1.5+） |
| CMYKカラーを使いたい（印刷ワークフロー） | `Color::Cmyk([c, m, y, k])` — 統一された `Color` enum。`Color::Rgb()` は `From<[f32; 3]>` で互換性あり（v1.0+、破壊的変更） |
| PDF の電子署名を検証したい | `doc.verify_signatures(&pdf_bytes)` — 全署名データを抽出（署名者・タイムスタンプ・フィールド名）、RSA PKCS#1 v1.5 暗号学的検証を実行、`is_valid: bool` 付き `SignatureInfo` を返す（`digital-signature` feature、v1.2.2+） |
| PDF に電子署名を付与したい | `doc.add_signature_field(page, rect, options)` + `SigningContext::from_cert_and_key(cert, key)` + `doc.sign_document(context, field_name)` → 署名済み PDF バイト — PKCS#7 DER構造、SHA-256 + RSA署名、ByteRange per spec 対応、v1.2.2+ 完全実装（`digital-signature` feature） |
| TextFragment がどの PDF 演算子から生成されたか追跡したい | `TextFragment.source_stream` / `source_op_start` / `source_op_end` — 元の `Tj`/`TJ` キーワードの Content Stream 内バイトオフセット（v1.5.15+） |
| 1文字ずつ Tj で描かれた PDF のテキストを置換したい | `page.replace_text_fragments(&frags, new_text, font)` — ソース演算子を `() Tj` で無効化し、最初のフラグメント位置に `new_text` を配置。PScript5/Distiller・Type3 レイアウトで `replace_text()` が一致しない場合の解決策（v1.5.15+） |
| Tm 行列の水平スケールを取得したい | `TextFragment.tm_x_scale: Option<f32>` — Tm 行列の √(a²+b²)。`font_size=1` + 大スケール Tm を使う PDF でも正しい視覚幅・列オフセットを算出できる（v1.6.0+） |
| 帳票 PDF の InPlace 翻訳でラベル列と値列の位置を安定して取得したい | `TextFragment.tm_lm_x / tm_lm_y: Option<f32>` — テキストラインマトリクス（T_lm）の座標。`x`（グリフアドバンス累積）や `tm_origin_x`（Tm 固定）とは異なり、毎 `Td` ごとにリセットされる「行アンカー」。前行のテキスト幅に関係なく常にクリーンな列位置を返す（v1.7.0+） |
| 翻訳用にソースフラグメント付きのテーブルセルを抽出したい | `extract_table_cells` が返す `TableCell` に `fragments: Vec<TextFragment>` と `bbox() -> [f32; 4]` を追加。`&cell.fragments` + `cell.bbox()` をそのまま `replace_fragments_fit_to_bbox` や `replace_text_fragments_batch_opts` に渡せる（v1.8.0+） |
| バッチ翻訳でセルごとにフォントサイズ・幅を指定したい | `page.replace_text_fragments_batch_opts(entries, font)` — `BatchEntry` ごとに独自の `FragmentReplaceOpts`（font_size・max_width・shrink_to_fit・color）を指定できる 1 パスバッチ置換（v1.8.0+） |
| 翻訳文を元セルの bbox に収めて配置したい | `page.replace_fragments_fit_to_bbox(&cell.fragments, text, font, cell.bbox(), FitOptions::default())` — 元テキストを抑制し、セル幅に shrink-to-fit した翻訳文を配置する（v1.8.0+） |
| テキストの描画幅を事前に計測したい | `doc.measure_text(text, font, font_size) -> Result<f32>` — 登録済みフォントの TTF メトリクスを使って PDF ポイント単位のアドバンス幅を返す（v1.9.0+） |
| テキストを描画する前に矩形へのレイアウトを計画したい | `doc.fit_text_to_box(text, font, rect, font_size, opts) -> Result<FitResult>` — ドキュメントを変更せず折り返し・縮小計画だけを行う。行リスト・実効フォントサイズ・`used_rect`・`overflow_horizontal`/`overflow_vertical` フラグを返す。`OverflowPolicy` で 4 ポリシー: `Shrink`・`WrapThenShrink`・`Truncate`・`Report`（v1.9.0+） |
| 配置予定テキストボックス同士の衝突を検出したい | `detect_collisions(boxes: &[PlacedBox]) -> Vec<Collision>` — O(n²) の AABB 重複検出。各 `Collision` に `index_a`・`index_b`・`overlap_rect`・`overlap_area` を持つ。`fit_text_to_box` の `used_rect` と組み合わせ、コンテントストリーム変更前に衝突をプリフライト検査できる（v1.9.0+） |
| 置換テキストに元グリフ幅ではなく列全体の幅を使いたい | `extract_layout_regions(&frags, page_w, page_h, opts) -> Vec<LayoutRegion>` — 各セルを `source_bbox`（グリフ境界）と `usable_rect`（利用可能領域）の両方付きで返す。`usable_rect.width` は次の列の開始位置まで伸びるため、翻訳文は元ラベル幅ではなく実際の列幅を使えるようになる（v1.10.0+） |
| レイアウトセルへの翻訳文配置をまとめて計画したい | `doc.plan_text_for_regions(regions, replacements, font, opts) -> Result<Vec<RegionFitPlan>>` — 各置換文を `region.usable_rect` に `fit_text_to_box` で収め、領域間の衝突を検出し、`RegionFitPlan { region, fit, collisions }` を返す。レイアウト・フィット・衝突検出を1回のパスで完結（v1.10.0+） |
| 帳票 PDF 翻訳でベースラインを保持し列幅を安全に扱いたい | `doc.plan_text_for_regions_with_policy(regions, replacements, font, options)` — `RegionTextFitOptions` の `BaselinePolicy::PreserveSourceBaseline` で元ベースラインを維持し、`WidthPolicy::SourceLineWidth` でラベル列が値列に侵食しないよう制御。`RegionTextFitOptions::for_role(&region.role)` でロール別デフォルト取得、`&[]` で全領域に自動適用（v1.11.0+） |
| 領域がラベル・値・見出し・本文のどれかを知りたい | `LayoutRegion::role: LayoutRegionRole` — `LeftLabel` / `RightValue` / `ParagraphBody` / `SectionHeading` / `HeaderFooter` / `Unknown`。`extract_layout_regions` が列位置・行シブリング・ページ端近接から自動割り当て（v1.11.0+） |
| 衝突の構造的関係（同行・隣接行・ヘッダーフッター）を分類したい | `classify_collisions(regions, collisions) -> Vec<ClassifiedCollision>` — 各 `Collision` に `CollisionKind`（`SameRegion`・`SameRow`・`AdjacentRow`・`SameColumn`・`HeaderFooter`・`Unknown`）、各領域の `LayoutRegionRole`、および `CollisionSeverity`（`Minor`・`Moderate`・`Major`）を付加。`plan_text_for_regions*` の `RegionFitPlan.collisions` は `Vec<ClassifiedCollision>` を直接返す（v1.12.0+、severity は v1.14.0+） |
| PlacedBox のサイズだけで衝突の深刻度を評価したい（LayoutRegion なし） | `collision_severity(overlap_area, box_a_area, box_b_area) -> CollisionSeverity` — 独立した深刻度計算関数。LayoutRegion を使わない呼び出し元向け。box_area が 0 の場合は絶対 pt² 閾値にフォールバック（v1.14.0+） |
| フォーム・表形式 PDF からラベル/値のペアを抽出したい | `extract_label_value_pairs(regions) -> Vec<LabelValuePair>` — 各 `LayoutRegionRole::LeftLabel` 領域と同一行の `RightValue` 兄弟を対にする。密集した SDS/帳票 PDF の検出や翻訳コンテキスト構築に有用（v1.14.0+） |
| テキスト配置の結果（縮小・オーバーフロー・切り詰め）を知りたい | `FitResult::status: PlacementStatus` — `Ok`（調整なしで収まった）、`Shrunk`（フォント縮小・下限以上）、`ShrunkToMin`（`min_font_size` 下限到達・溢れる場合あり）、`Overflow`（`OverflowPolicy::Report` 時のオーバーフロー）、`Truncated`（`OverflowPolicy::Truncate` または `Report + max_lines` での行切り捨て）。フラグを個別確認せず一つのシグナルとして使用可能（v1.13.0+） |
| 翻訳レイアウトのページ単位品質ゲートが欲しい | `PageFitSummary::from_plans(plans) -> PageFitSummary` — `RegionFitPlan` バッチの集計。フィールド: `overflow_count`・`collision_count`・`shrunk_count`・`worst_overlap_area`・`worst_overlap_rect`。最終 PDF 書き出し前の品質判定に使用（v1.13.0+） |
| PDF 上でレイアウト衝突や配置をビジュアルデバッグしたい | `page.add_fit_debug_overlay(&plans, DebugOverlayOptions::default())` — 色付きストローク矩形を描画（青=ソース bbox、緑=配置テキスト、赤=衝突重複）。`DebugOverlayOptions` で色と線幅を設定。NaN/無効座標は自動スキップ（`draw` feature、v1.13.0+） |
| テキスト配置時に微小な幅差を文字間隔で吸収したい | `add_text_styled_with_char_spacing(text, font, pos, size, color, bold, italic, char_spacing)` — PDF `Tc` 演算子; 負の `char_spacing` でフォントサイズを維持したまま文字幅を縮小 (v1.15.0+) |
| ページ上の画像ボックスを取得したい（オーバーレイ回避用） | `doc.page_image_bboxes(page)` — 軸平行 Image XObject の `[x, y, width, height]` を返す; `image` feature 不要 (v1.15.0+) |

---

## 類似ツールとの比較

| 機能 | **harumi** | pdf-lib (JS) | printpdf (Rust) | lopdf (Rust) | pdfium-render (Rust) |
|---|:---:|:---:|:---:|:---:|:---:|
| 純Rust — C/C++依存なし | Yes | N/A | Yes | Yes | No (C++ PDFium) |
| WASM / クロスプラットフォーム | Yes | Yes | Yes | Yes | Partial (complex setup) |
| 既存PDFへのCJKテキスト追加 | Yes | Yes | No (new PDFs only) | No (manual) | Yes |
| テキスト抽出 | Yes (CID + simple) | Partial (basic) | No | Partial (basic) | Yes 完全 |
| テキスト置換（再サブセット対応） | Yes | No | No | No | No |
| ページ操作 | Yes | Yes | Partial (limited) | Yes (low-level) | Yes |
| 図形描画 | Yes | Yes | Yes | No (manual) | Yes |
| フロードキュメント / 自動ページング | Yes | No | No | No | No |
| HTML → PDF | Yes | No | No | No | No |
| インライン太字・斜体・色 | Yes (synthetic) | No | No | No | Yes |
| 暗号化（読み込み） | Yes (RC4) | Yes | No | Partial | Yes |
| 暗号化（書き込み） | Yes (RC4-128) | Yes | No | No | Yes |
| マークアップ注釈 | Yes | Partial (basic) | No | No | Yes |
| CMYKカラー対応 | Yes (v1.0+) | Yes | Yes | No | Yes |
| デジタル署名作成 | Yes (v1.2.2+) | No | No | No | No |
| デジタル署名検証 | Yes (v1.2.2+) | Partial (basic) | No | No | Yes |

> Yes = 対応  Partial = 部分対応  No = 非対応  N/A = 言語レベルの機能

---

## モダン Rust PDF ライブラリとの比較

| 機能 | **harumi** | unpdf | pdf_oxide | justpdf-core |
|---|:---:|:---:|:---:|:---:|
| **目的** | 読み書き両対応 | 読み込みのみ | フル機能 | フル機能 |
| **主な用途** | 既存PDFへの日本語テキスト重ねがけ | PDF → Markdown/テキスト抽出 | マルチ言語バインディング対応 | 包括的なPDFエンジン |
| 純Rust（C/C++依存なし） | Yes | Yes | 可能性高 | Yes |
| WASM対応 | Yes（CI確認済み） | Yes | Yes | 未記載 |
| **テキスト抽出** |
| — CIDフォント（ToUnicode CMap） | Yes | Yes ⭐ | Yes | Yes |
| — シンプルフォント（Type1/TrueType/Type3） | Yes | Yes | Yes | Yes |
| — Form XObject 再帰 | Yes（v1.5+） | Yes ⭐ | Yes | 不明 |
| — グラフィック状態継承 | Yes（v1.5+） | Yes ⭐ | Yes | 不明 |
| — `uni<XXXX>` グリフ名 | No（v1.4で対応予定） | Yes ⭐ | 不明 | 不明 |
| — 読み取り順 / XY-Cut | No | Yes ⭐ | Yes | 不明 |
| — RTL / BiDi対応 | No | Yes ⭐ | 不明 | 不明 |
| **テキスト書き込み** |
| — CJKフォント埋め込み | Yes ⭐ | N/A | 部分対応 | Yes |
| — フォントサブセット化 | Yes ⭐（deferred） | N/A | 不明 | Yes |
| — Identity-H / Identity-V | Yes ⭐ | N/A | 不明 | Yes |
| — Type0 CID生成 | Yes ⭐ | N/A | 不明 | Yes |
| **ページ操作** | Yes | No | Yes | Yes |
| **図形・画像描画** | Yes | No | Yes（部分） | Yes |
| **暗号化（読み込み）** | Yes（RC4） | Yes（RC4） | Yes | Yes（RC4, AES） |
| **暗号化（書き込み）** | Yes（RC4-128, AES-256） | No | Yes | Yes（RC4, AES-256） |
| **デジタル署名** | 部分対応（メタデータ） | No | Yes | Yes（PKCS#7/CMS） |
| **PDF/A準拠** | 予定中（v1.3） | No | Yes（検証） | Yes（検証） |
| **パフォーマンス重視** | 正確性 | 速度（特化） | 速度（PyMuPDF比5倍） | 包括性 |
| **マルチ言語バインディング** | WASMのみ | なし | Python/JS/Go/C#/Java等7言語 | C FFI のみ |

**主な違い：**
- **harumi** — 既存PDFへの日本語テキスト*書き込み*に特化。Deferred subsetting戦略が明確。WASM対応を保証
- **unpdf** — PDFの*読み込み*とMarkdown/テキスト抽出に特化。CJK抽出品質が高い（XY-Cut、RTL、Form XObject対応）
- **pdf_oxide** — マルチ言語バインディング対応の汎用PDFエンジン。ゼロコピートークナイズで5倍高速。Rustコアに複数言語バインディング
- **justpdf-core** — 包括的なPDFエンジン。レガシーPDF互換性のため地域別CIDシステム（Japan1/GB1/CNS1/Korea1）対応

**推奨用途：**
- **harumi** — 既存PDFにCJKテキストを重ねがけする場合（OCRレイヤー、スタンプ、透かし）
- **unpdf** — CJK PDFからテキストを抽出し文字化けを防ぐ場合
- **pdf_oxide** — マルチ言語サポートと高速抽出が必要な場合
- **justpdf-core** — CJK特化ではない包括的なPDFエンジンが必要な場合

⭐ = このカテゴリでの独自の強み

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
harumi = "1.5"
```

### CJK フォント入手方法

日本語・中国語・韓国語の PDF 処理には、**NotoSansCJK フォント**（Google Fonts、OFL ライセンス、無料）をダウンロードしてください：

```bash
# 日本語
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKjp-Regular.ttf

# 簡体字中国語
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKsc-Regular.ttf

# 繁体字中国語
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKtc-Regular.ttf

# 韓国語
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKkr-Regular.ttf
```

**その他の入手元:**
- **Google Fonts ウェブサイト**: https://fonts.google.com（検索: "Noto Sans CJK"）
- **Adobe Fonts**: https://fonts.adobe.com（サブスクリプション版）
- **システムフォント**: `fc-list | grep -i noto` で既にインストール済みか確認

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
for frag in &runs {
    println!(
        "{:?} at ({:.1}, {:.1}) font={} color={:?} invisible={}",
        frag.text, frag.x, frag.y, frag.font_name, frag.color, frag.invisible,
    );
}
```

各 `TextFragment` が持つフィールド：`text`、`x`/`y`（PDF ポイント座標）、`width`、`font_size`、**`font_name`**（PDF リソース名。例：`"HR0"`）、**`color`**（RGB フィル `[f32; 3]`）、**`invisible`**（OCR `Tr 3` テキストの場合 `true`）、**`is_bold`**・**`is_italic`**・**`font_family`**・**`base_font`**（PostScript `/BaseFont` 名から解析）。

harumi が出力したPDF（Identity-H CIDフォント）だけでなく、任意の既存PDFにも対応。Type1・TrueTypeなど標準シンプルフォント（WinAnsiEncoding・MacRomanEncoding・StandardEncoding・`/Differences` 辞書）も解析できます。

### 既存PDFのテキスト置換

```rust
let mut doc = Document::from_file("contract.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansJP-Regular.ttf"))?;
// マッチ件数を返す（0 = 見つからなかった）
let n = doc.page(1)?.replace_text("Hello", "こんにちは", font)?;
doc.save("translated.pdf")?;
```

同一 BT/ET ブロック内の連続する `Tj`/`TJ` 演算子にまたがるテキストにもマッチします（クロス演算子マッチング）。さらに、`Tf` フォント切替演算子をまたぐマッチにも対応しています（クロス `Tf` マッチング）。日本語 PDF では1つの視覚的な行が複数のフォントラン（本文漢字を `F1`、括弧文字を `F2`）に分かれていることが多く、このようなケースも正しくマッチします。垂直方向の `Td` または `Tm`（新しい視覚的な行）を挟む場合は対象外です。

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

### フォントサブセット拡張付きテキスト置換

新テキストに元のサブセットにない文字が含まれる場合、`replace_text_resubset` を使います。元の（未サブセット）TTF/OTFバイトを渡すと、harumi がサブセットを拡張し、全コンテントストリームを再エンコードして1回の `save()` で置換を完了します。

```rust
let font_bytes = include_bytes!("NotoSansJP-Regular.ttf");
let mut doc = Document::from_file("contract.pdf")?;

// replace_text_preserve_font はここで FontCharNotMapped を返す
let n = doc.page(1)?.replace_text_resubset("Hello", "日本語", font_bytes)?;
doc.save("output.pdf")?;
```

中国語・韓国語・アラビア語など任意の言語に対応しています（フォントがその文字を含む場合）。

> **注意**: PDFに埋め込まれたサブセットではなく、元の未サブセットフォントファイルが必要です。
> CIDToGIDMap=Identity のCIDFontType2フォント（harumi が埋め込むもの）のみ対応。
> 他ツール生成のPDFでは非Identity `CIDToGIDMap` が使われることがあります。その場合は新規埋め込みフォントで
> `replace_text` を使うか、MCP `pdf_replace_text` の `mode: "new_font"` を使ってください。

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
harumi = { version = "1", features = ["draw"] }
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
harumi = { version = "1", features = ["image"] }
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

### スキャン PDF から埋め込み画像を取り出す（`image` feature）

OCR ワークフロー向け：スキャン PDF を読み込み → 画像を取り出す → OCR を実行 → 不可視テキストレイヤーを書き戻す。

```rust
use harumi::{Document, PageImageFormat};

let doc = Document::from_file("scanned.pdf")?;
let img = doc.extract_page_image(1)?;

match img.format {
    PageImageFormat::Jpeg => std::fs::write("page1.jpg", &img.bytes)?,
    PageImageFormat::Png  => std::fs::write("page1.png", &img.bytes)?,
}
println!("{}×{} ピクセル", img.width, img.height);
```

> **スキャン PDF 専用。** 既存の Image XObject を取り出す機能であり、ページをラスタ化するわけではありません。テキスト PDF・ベクター PDF には Image XObject がないため `Error::InvalidInput` を返します。

### 自動改ページ付き構造化ドキュメントの生成（`flow` feature）

```toml
harumi = { version = "1", features = ["flow"] }
```

```rust
use harumi::{FlowDocument, FlowOptions};

let font = include_bytes!("NotoSansCJK-Regular.ttf");
let mut doc = FlowDocument::new(font.as_ref(), FlowOptions::default())?;

doc.push_heading("年次報告書", 1)?;
doc.push_paragraph("この文書は当期の業績をまとめたものです。")?;
doc.push_key_value_table(&[
    ("売上高", "100万円"),
    ("費用", "80万円"),
    ("利益", "20万円"),
])?;
doc.push_list(&["3つの新市場に進出", "2つの新製品を発売"], false)?;

// コンテンツがページに収まらない場合は自動改ページ。
// push_page_break() で任意の位置に手動改ページを挿入できる。

let pdf_bytes = doc.render()?;
```

日本語・中国語・韓国語もそのまま利用可能。CJKフォントを渡すと任意の文字位置で折り返します。

### FlowDocument でのインラインテキストスタイル（`flow` feature）

段落内に太字・斜体・色を混在させることができます:

```rust
use harumi::{FlowDocument, FlowOptions, InlineSpan};

let mut doc = FlowDocument::new(font_bytes, FlowOptions::default())?;
doc.push_paragraph_styled(&[
    InlineSpan::plain("通常テキスト、"),
    InlineSpan::bold("太字テキスト、"),
    InlineSpan::italic("斜体テキスト、"),
    InlineSpan::colored("赤色テキスト。", [0.8, 0.0, 0.0]),
])?;
let pdf = doc.render()?;
```

太字と斜体は**合成効果**（fill+stroke と12°せん断）で実現するため、別途太字・斜体フォントは不要です。

### ページ番号付きヘッダ/フッタ（`flow` feature）

```rust
use harumi::{FlowDocument, FlowOptions, HeaderFooter};

let opts = FlowOptions {
    // 全ページの左に "harumi docs"、右に "v0.5" を表示
    header: Some(HeaderFooter {
        left:  Some("harumi docs".into()),
        right: Some("v0.5".into()),
        ..Default::default()
    }),
    // 中央に "1 / 3" 形式のページカウンタ
    footer: Some(HeaderFooter::page_number()),
    // push_heading() が自動的にブックマークエントリを作成（デフォルト: true）
    auto_bookmarks: true,
    ..Default::default()
};

let mut doc = FlowDocument::new(font, opts)?;
doc.push_heading("第1章", 1)?;
doc.push_paragraph("本文テキスト。")?;
let pdf_bytes = doc.render()?;
```

### リンクアノテーション

```rust
// クリッカブルな URL 領域（x, y, 幅, 高さ）
doc.page(1)?.add_link_url([72.0, 40.0, 200.0, 18.0], "https://example.com")?;

// 内部リンク：該当領域をクリックすると同一ドキュメントの3ページ目へジャンプ
doc.page(1)?.add_link_internal([72.0, 700.0, 150.0, 18.0], 3)?;
```

### マークアップ注釈（ハイライト、下線、取り消し線、スクイグリー）

```rust
// 黄色のハイライト
doc.page(1)?.add_highlight([72.0, 690.0, 200.0, 14.0], [1.0, 1.0, 0.0])?;

// 赤い下線
doc.page(1)?.add_underline([72.0, 640.0, 200.0, 12.0], [1.0, 0.0, 0.0])?;

// 取り消し線
doc.page(1)?.add_strikeout([72.0, 590.0, 200.0, 12.0], [0.0, 0.0, 0.0])?;

// スクイグリー（波線）下線
doc.page(1)?.add_squiggly([72.0, 540.0, 200.0, 12.0], [0.0, 0.6, 0.2])?;

// スティッキーノートコメント
doc.page(1)?.add_sticky_note([500.0, 700.0], "この部分を確認")?;
doc.save("annotated.pdf")?;
```

### パスワード保護PDF

```rust
// 暗号化されたPDFを読み込む
let mut doc = Document::from_file_with_password("protected.pdf", "secret")?;
assert!(doc.is_encrypted());

// 誤ったパスワードは Error::WrongPassword を返す
match Document::from_bytes_with_password(&bytes, "wrong") {
    Err(harumi::Error::WrongPassword) => println!("パスワードが違います"),
    _ => {}
}

// パスワード保護して保存
let mut doc = Document::new((595.0, 842.0))?;
doc.set_encryption("userpass", "ownerpass")?;
doc.save("protected_output.pdf")?;
```

### AcroForm: フォームフィールドの読み取りと入力

```rust
// 全フォームフィールドを読む
let mut doc = Document::from_file("form.pdf")?;
for field in doc.form_fields()? {
    println!("{}: {:?} = {:?}", field.name, field.field_type, field.value);
}

// フィールド名で値を入力
let updated = doc.fill_form(&[
    ("FullName",   "田中 花子"),
    ("Agree",      "yes"),       // チェックボックス → /Yes
    ("Department", "Engineering"),
])?;
println!("{updated} フィールドを更新");
doc.save("filled_form.pdf")?;
```

### ページボックス（印刷ワークフロー）

```rust
// CropBox（表示領域クリップ）の読み書き
let cb = doc.page(1)?.crop_box()?;   // Option<[f32;4]>

doc.page(1)?.set_crop_box([10.0, 10.0, 575.0, 822.0])?;   // [x,y,w,h]
doc.page(1)?.set_trim_box([0.0, 0.0, 595.0, 842.0])?;
doc.page(1)?.set_bleed_box([0.0, 0.0, 601.0, 848.0])?;
doc.save("print_ready.pdf")?;
```

### ブックマーク（ドキュメントアウトライン）

```rust
// PDF ビューアのブックマークパネルを構築する。
// ASCII 以外のタイトル（CJK、アクセント付きラテン文字など）は UTF-16BE で自動エンコード。
doc.add_bookmark("第1章",    1, 800.0)?;   // タイトル、ページ（1始まり）、Y座標
doc.add_bookmark("第2章 概要", 2, 800.0)?;
doc.save("report.pdf")?;
```

### HTML→PDF変換（`html` feature）

```toml
harumi = { version = "1", features = ["html"] }
```

```rust
use harumi::{render_html_to_pdf, HtmlRenderOptions};

let font = include_bytes!("NotoSansCJK-Regular.ttf").to_vec();
let html = r#"
    <h1>年次報告書</h1>
    <p>はじめに。</p>
    <table>
      <tr><th>売上高</th><td>100万円</td></tr>
      <tr><th>利益</th><td>20万円</td></tr>
    </table>
    <h2>ハイライト</h2>
    <ul><li>3つの新市場に進出</li><li>2つの新製品を発売</li></ul>
    <div style="page-break-after: always"></div>
    <h1>2ページ目</h1>
"#;

let pdf_bytes = render_html_to_pdf(html, HtmlRenderOptions {
    font_bytes: font,
    ..HtmlRenderOptions::default()
})?;
```

対応要素：`<h1>`–`<h6>`、`<p>`、`<table>/<tr>/<th>/<td>`、`<ul>/<ol>/<li>`、`<div>/<section>/<article>`（ブロックコンテナ）。  
改ページ：`style="page-break-after: always"` または `class="page-break"`。  
スキップ：`<script>`、`<style>`、`<head>`。  
インラインスタイル: `<strong>`/`<b>`（太字）、`<em>`/`<i>`（斜体）、`<span style="color: #RRGGBB">`（色指定）、`<a href>`（青色リンク）。  
深いネスト構造もスタックオーバーフローなし（反復型パーサ、5000段ネストで検証済み）。

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

// 既存コンテントストリームのテキスト置換（シングルオペレータマッチング）。マッチ件数を返す
let n: usize = doc.page(1)?.replace_text(old_text, new_text, font)?;
// 元の埋め込みフォントで置換。グリフ検証は即時実行。マッチ件数を返す
let n: usize = doc.page(1)?.replace_text_preserve_font(old_text, new_text)?;
// 読み取り専用スキャン：マッチ件数または Err(FontCharNotMapped) を返す
let n: usize = doc.page(1)?.can_replace_text(old_text, new_text)?;

// リンクアノテーション（feature フラグ不要）
doc.page(1)?.add_link_url([x, y, w, h], "https://example.com")?;   // URL リンク
doc.page(1)?.add_link_internal([x, y, w, h], target_page)?;         // ドキュメント内リンク

// ドキュメントアウトライン / ブックマーク（feature フラグ不要）
doc.add_bookmark("セクションタイトル", page, y)?;  // フラットなアウトラインエントリを追加
```

### 座標系について

座標は **PDFポイント**（1pt = 1/72インチ）で、原点はページ**左下**です。Tesseract / hOCR など左上原点のピクセル座標を使う場合は `ocr` featureのヘルパーを使ってください：

```toml
harumi = { version = "1", features = ["ocr"] }
```

### Feature flags

| フラグ | 有効になる機能 | 追加依存 |
|---|---|---|
| *(デフォルト)* | テキスト重ね合わせ・フォント埋め込み・`add_text_box`・`add_text_box_aligned`・`add_text_with_opacity`・`add_text_box_with_opacity` | lopdf, ttf-parser |
| `draw` | `add_rect`, `add_line`, `add_rect_stroke`, `add_polygon`, `add_polyline`, `add_ellipse` — 図形描画 | なし |
| `image` | `add_image`, `add_image_with_opacity` — JPEG/PNG 画像埋め込み；`extract_page_image` — スキャン PDF から画像を取り出す（`draw` を有効化） | `png` クレート（純Rust） |
| `ocr` | `ocr::hocr_y_to_pdf`・`ocr::hocr_x_to_pdf`・`ocr::pixel_size_to_pt` — Tesseract 座標変換ヘルパー | なし |
| `flow` | `FlowDocument` push 型ビルダー・自動改ページ（`push_heading`・`push_paragraph`・`push_key_value_table`・`push_list`・`push_page_break`・`render`）；`HeaderFooter` によるページごとのヘッダ/フッタ（`{{page}}`/`{{total}}` 展開）；見出しから自動ブックマーク生成（`auto_bookmarks`） | なし |
| `html` | `render_html_to_pdf` — HTML→PDF変換（h1–h6・p・table・ul/ol・改ページ。`flow` を有効化）；内部に純Rust HTMLトークナイザを実装 | なし |

```rust
let pdf_y = harumi::ocr::hocr_y_to_pdf(pixel_y, page_height_pts, image_dpi);
let pdf_x = harumi::ocr::hocr_x_to_pdf(pixel_x, image_dpi);
let pt    = harumi::ocr::pixel_size_to_pt(pixel_size, image_dpi);
```

---

## 対応フォント

| フォント形式 | 対応状況 |
|---|---|
| TrueType (`.ttf`) | ✅ 完全対応 — 純Rust サブセット化エンジン |
| TTC コレクション | ✅ 完全対応 — `embed_font_at(bytes, face_index)` で面指定可能 |
| OpenType CFF (`.otf`) | ⚠️ 受け付けるが（サブセット化なし） — そのまま埋め込み |

日本語・中国語・韓国語には [Noto Sans CJK](https://github.com/notofonts/noto-cjk) の **TrueType** バリアントを推奨します（E2E動作確認済み）：

```
NotoSansCJKjp-Regular.ttf  （日本語）
NotoSansCJKsc-Regular.ttf  （簡体字）
NotoSansCJKtc-Regular.ttf  （繁体字）
NotoSansCJKkr-Regular.ttf  （韓国語）
```

> **OTFについて**: harumi は `.otf` ファイルを受け付け、`FontFile3 /OpenType` として埋め込みますが、**CFF フォントはサブセット化できません** — フォント内の全グリフが埋め込まれるため PDF サイズが大きくなります。サイズ最適化のため、上記の TTF バリアントをご利用ください。

---

## 内部実装

```
harumi
├── lopdf v0.40          — 既存PDFのオブジェクトグラフ解析・編集
├── ttf-parser           — フォントメタデータ取得（bbox、units_per_em、ascender）
└── [内製 TTF サブセッタ] — 純Rust TrueType サブセット化エンジン（外部クレート不要）
```

フォントパイプラインの流れ：

1. 使用文字を収集 → Unicode コードポイントのセットを作成
2. フォントの `cmap` テーブルで コードポイント → 元のグリフID（GID）にマッピング
3. 内製エンジンで使用グリフのみにTTFをサブセット化（GIDは **0..N に再採番**）
4. `gid_to_char` とアドバンス幅を元GID → 新GIDに **再マッピング**（文字化け防止）
5. PDFのCIDフォントオブジェクトグラフを構築: `Type0 → CIDFontType2 → FontDescriptor → FontFile2`
6. `/ToUnicode` CMAPストリームを生成（ビューアでのテキスト選択・検索を可能にする）
7. ページの `/Contents` 配列に新しいコンテントストリームを追記

サブセット化は**遅延実行**：`embed_font()` は生のTTFバイト列を保持し、`save()` 時に全ページの使用文字を収集し、フォントごとに1回だけ処理します。

### 依存関係の最小化

harumi は **外部ランタイム依存ゼロ**（コア PDF 処理以外）を目指しています。

- **TrueType サブセット化** — 内製の純Rust実装（v1.1+）；TTF + TTC（コレクション）対応、再帰的コンポジットグリフ解決
- **フォント解析** — ttf-parser（専門用途、推移的依存なし）
- **画像デコード** — `png` クレート（オプション、feature ゲート済み）
- **暗号化** — getrandom（OS エントロピー専用；AES-256 暗号化キー生成に必須）

**直接依存数**: 3個（getrandom、lopdf、ttf-parser、オプション `png`）  
**推移的依存（デフォルトビルド）**: 約8個（lopdf の内部ユーティリティのみ）

---

## 名前について

晴海（はるみ）— *晴*（晴れた空）＋ *海*（海）。表面は穏やか、内部には多くの処理が走っている。

## ロードマップ

| バージョン | スコープ |
|---|---|
| **v0.1** | TrueType フォント、不可視・可視テキスト、バッチ配置、`page.size()`、`save_to_bytes()`、GID 再採番、OTF 受け付け |
| **v0.2** | `draw` feature（`add_rect`、`add_line`）、`image` feature（`add_image`、PNG SMask 透明度）、ページ操作（`rotate_page`、`remove_page`、`insert_blank_page`、`reorder_pages`） |
| **v0.3** | `add_text_box`・`add_rect_stroke`・`add_polygon`・`add_ellipse`・`add_path`；`add_text_with_rotation`；セキュリティ強化；`merge_from`；`Document::new`；`extract_pages` |
| **v0.4** | `extract_text_runs`（CIDフォント＋標準シンプルフォント）、PDF メタデータ読み書き、`replace_text`（Tj/TJ 書き換え・クロスオペレータマッチング・幅補正・フォント保持モード）、`flow` feature（`FlowDocument`、CJK 自動改ページ）、`html` feature、`extract_page_image` |
| **v0.5** | `add_link_url`・`add_link_internal` — クリッカブルな PDF リンクアノテーション；`add_bookmark` — CJK UTF-16BE タイトル対応のドキュメントアウトライン；`HeaderFooter` + `{{page}}`/`{{total}}` の `FlowDocument` 対応；見出しからの `auto_bookmarks`；セキュリティ修正 |
| **v0.6** | 暗号化 PDF 読み込み（`from_file_with_password` / `is_encrypted` / `Error::WrongPassword`）；マークアップ注釈（ハイライト・下線・取り消し線・付箋）；AcroForm `form_fields()` / `fill_form()`；AGL テーブル +116 エントリ（中欧文字・合字・euro）；Identity-H テキスト抽出フォールバック |
| **v0.7** *(current)* | `set_encryption` — パスワード保護付き PDF の書き出し；`add_squiggly` — 波線下線注釈；ページボックス全種対応（`crop_box`・`trim_box`・`bleed_box`・`media_box` 読み書き） |
| **v0.8** | FlowDocument インラインスタイル（太字/イタリック/カラースパン）；`replace_text_resubset` — サブセット拡張付きテキスト置換；MCP `pdf_replace_text` のレイアウト保持翻訳ワークフローと非Identity `CIDToGIDMap` 診断；HTML インラインスタイル対応（`<strong>`・`<em>`・`<span style="color">`・`<a href>`） |
| **v1.4.1** | `TextFragment` フォント属性（`is_bold`・`is_italic`・`font_family`・`base_font`）；`detect_text_columns` + `ColumnZone` による段組レイアウト推定 |
| **v1.4.2** | `harumi-ai` Overlay モード精度向上：per-line 白矩形サイジング（高さ・幅・ディセンダー被覆）、ベースライン Y 座標の正確な配置、`detect_text_columns` による複数段組対応、太字による見出し検出、NaN 安全な読み取り順ソート |
| **v1.5.0** | `group_text_fragments` — `TextFragment` を行/段落単位の `TextGroup` へ統合；`font_covers_char` — cmap カバレッジ判定；Form XObject 再帰テキスト抽出（`Do` 演算子対応）；複数 CS 間グラフィック状態継承；`harumi-ai`: `OverflowStrategy`（Shrink/Truncate）、`font_fallbacks` 複数フォント描画、`on_progress` コールバック；`extract_table_cells` — テーブル行/列検出（ヒューリスティック） |
| **v1.5.1** | `harumi-ai` v0.1.0 初回 crates.io 公開；InPlace デバッグログの CJK バイトスライスパニック修正；Clippy lint 修正（`repeat_n`、`div_ceil`） |
| **v1.5.2** | 祖先 Pages ノードからのフォント継承修正（`collect_fonts_inner` 親チェーン走査）；LLM 出力のエスケープなし JSON 修復；デフォルト `max_tokens` 4096→16000 引き上げ |
| **v1.5.3** | 継承 `/Resources` 経由の Form XObject 発見を修正（Chrome/Skia PDF — `extract_text_from_xobjects` 親チェーン走査）；`replace_text()` が Form XObject コンテンツストリームも書き換えるよう対応し `harumi-ai` InPlace モードが Chrome/Skia PDF で動作 |
| **v1.5.4** | Type3 フォントのテキスト抽出対応（`collect_font_dict_entries` に `/Subtype /Type3` を追加）；Type3 フォントのみを使う Chrome/Skia PDF での翻訳出力ゼロ問題を修正 |
| **v1.5.5** | Overlay CTM 座標変換を修正 — `parse_content_stream` が `q`/`Q`/`cm` を追跡し `TextFragment` 座標をページ空間へ変換；Chrome/Skia PDF（スケール + Y 反転 CTM）での Overlay テキスト配置が正確に |
| **v1.5.6** | Chrome/Skia PDF 3件のバグ修正：(1) `overlay_from` が既存ページコンテンツを `q`/`Q` で包み、不均衡な `cm` 演算子を分離；(2) `extract_text_from_xobjects` が `ParseCarryState.do_ctm_map` で per-`Do` CTM を追跡し、最後の累積 CTM ではなく各 XObject 固有の CTM を適用；(3) `diagnose_match_failure` に cross-BT スキャンを追加し、BT/ET をまたぐ Type3 フォントテキストを `"type3-char-per-tj"` として報告 |
| **v1.5.8** | 翻訳ビジュアルバグ 3件修正：(1) `finalize()` が `append_to_contents()` 前に `wrap_page_contents_in_q_q()` を呼び出し、既存コンテンツの不均衡 `cm` が追加ストリームに影響しないよう分離；(2) `ctm_stack` を `ParseCarryState` に移動し複数 `Contents` 配列ストリーム間で正しく継続（Form XObject の per-XObject CTM 追跡）；(3) Chrome/Skia Type3 PDF 向け cross-BT `Tj` 置換 — `find_cross_bt_matches()` + `CrossBtMatch` で BT/ET をまたぐテキストを検出し `replace_text()` が動作するように |
| **v1.5.9** | `ReplaceOptions { normalize_whitespace }` + `replace_text_opts()` — `old_text` から空白を除去してマッチングし、harumi-ai のスペース結合フラグメント（`"T h e F r e e"` など）が Chrome/Skia Type3 PDF に一致するように（harumi-ai の grouping ロジック変更不要）；`TextFragment.space_advance` — フラグメントのフォントサイズでのスペースグリフ advance width を追加、単語間隔か文字間隔かの判定に使用でき `"10M+"` → `"1 0 M +"` の問題を解消 |
| **v1.5.10** | cross-BT マッチカウントが常にゼロになるバグ修正：`count_matches_in_raw_streams()` に cross-BT カウントパスを追加し `replace_text_opts(normalize_whitespace: true)` が Chrome/Skia Type3 PDF で実際に置換をキューに積むように；`find_cross_bt_matches()` のフォント追跡修正 — `BT` での `cur_font.clear()` と `Tf` の `in_bt` ガードを除去し、フォントが `BT`/`ET` をまたいで正しく持続するように（PDF 仕様準拠） |
| **v1.5.11** | 伝統的な日本語 PDF（GHS SDS 等）の InPlace 一致率向上：同一視覚行上の各文字を `Tm` で配置するパターン（日本語 PDF 生成ツールで一般的）で cross-op・cross-Tf マッチが機能するように。`collect_char_segments` と `collect_cross_tf_segments` が垂直 Tm（y 変化量 ≥ 1 pt）のみフラッシュ；`Tm` とテキスト状態演算子（`Tc`/`Tw`/`Tz`/`TL`/`Ts`）を中間演算子ホワイトリストに追加し `rewrite_content_stream` で抑制 |
| **v1.5.12** | AES-256 暗号化 PDF のサイレントストリームスキップ修正：`page_content_streams()` と `decode_form_xobject()` が `decompress()` 失敗時に `stream.content` にフォールバック（lopdf が `load_with_password` 時に解凍済みの場合に対応）、ページあたり 13→40+ フラグメント改善；`ExtractionWarning`/`WarningKind` + `extract_text_runs_verbose()` 診断 API；`TextFragment.tf_font_size` + `TextFragment.tm_y_scale` 新フィールド；ゼロ advance width フォールバック（0.5em/文字） |
| **v1.5.13** | XObject フォント解決バグ修正：`xobject_fonts()` がページレベルのフォントをベースとして使用するよう変更。PScript5/Distiller 製 PDF で Form XObject が `/Resources` を持つが `/Font` サブエントリを持たない場合（フォントがページ側に定義される典型的な構造）のテキスト全ドロップを修正 |
| **v1.5.14** | クロスストリーム BT/ET 状態の引き継ぎ：`in_bt`・現在のフォント・テキスト位置を `ParseCarryState` に移動し、ストリーム境界をまたいで保持。Distiller 製 PDF が単一の BT…ET ブロックを複数の `/Contents` 配列ストリームに分割するケース（前ストリームで BT が閉じられず後続ストリームで裸の `Tj` が 48 個破棄されるなど）を修正 |
| **v1.5.15** | `TextFragment.source_stream` / `source_op_start` / `source_op_end` — 各フラグメントを生成した `Tj`/`TJ` のバイトオフセットで追跡；`PageHandle::replace_text_fragments(fragments, new_text, font)` — ソース演算子を `() Tj` で抑制し訳文を配置。1文字単位 Tj の PScript5/Distiller・Type3 PDF の InPlace 翻訳が可能に |
| **v1.5.16** | `TextFragment.source_xobject: Option<(u32, u16)>` — Form XObject ソース参照；`replace_text_fragments_opts` と `FragmentReplaceOpts`（`font_size`・`max_width`・`y_offset`・`color`）；XObject ストリーム書き換えサポート |
| **v1.5.17** | `text_fragment_bounds` — アセンダー/ディセンダー推定付き集計バウンディングボックス；`FragmentReplaceOpts.shrink_to_fit` + `min_font_size` |
| **v1.5.18** | `replace_text_fragments_batch` — シングルパスバッチ置換（バイトオフセットずれなし）；`dry_run` プリフライト；`FragmentReplaceFailureReason` + `can_suppress_fragment` |
| **v1.5.19** | Td のみ BT ブロックで `tm_origin_x/y` が `Some(0.0)` を誤報告していたバグを修正 |
| **v1.6.0** | `TextFragment::tm_x_scale` — Tm 行列の X スケール；非均一 Tm での字送り幅・TJ カーニングを修正 |
| **v1.7.0** | `TextFragment.tm_lm_x / tm_lm_y` — テキストラインマトリクス座標（Td でリセット）；帳票翻訳の安定した列/行アンカー |
| **v1.8.0** | `TableCell.fragments` + `bbox()`；`replace_text_fragments_batch_opts` with `BatchEntry`；`replace_fragments_fit_to_bbox` |
| **v1.9.0** | `measure_text`；`fit_text_to_box` + `FitResult` + `BoxFitOptions` / `OverflowPolicy`；`detect_collisions` + `PlacedBox` + `Collision` |
| **v1.10.0** | `extract_layout_regions` + `LayoutRegion`（`source_bbox` + `usable_rect`）；`plan_text_for_regions` → `Vec<RegionFitPlan>` |
| **v1.11.0** | `LayoutRegionRole`；`BaselinePolicy` + `WidthPolicy` + `RegionTextFitOptions`；`plan_text_for_regions_with_policy`；HeaderFooter 近接判定の NaN ガード |
| **v1.12.0** | `CollisionKind` + `ClassifiedCollision` + `classify_collisions` — 構造的衝突分類；`RegionFitPlan.collisions` を `Vec<ClassifiedCollision>` に変更 |
| **v1.13.0** | `Collision::overlap_area`（pt² 重大度フィールド）；`PlacementStatus` enum + `FitResult::status`；`PageFitSummary::from_plans`；`add_fit_debug_overlay` + `DebugOverlayOptions`（`draw` feature）；バグ修正: Report+max_lines Truncated ステータス、WrapThenShrink 下限フォントでの溢れ、Truncate rh=0 誤 Ok、NaN 座標ガード |
| **v1.14.0** | `CollisionSeverity`（Minor/Moderate/Major）+ `ClassifiedCollision::severity` フィールド（source_bbox 面積比で自動計算）；`collision_severity()` スタンドアロン関数；`LabelValuePair` + `extract_label_value_pairs()` — LeftLabel/RightValue 領域のペア抽出（密集帳票/SDS PDF 検出用） |

---

## コントリビュート

[github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi) でIssue・PRを歓迎します。

最も複雑なコードは `src/font/embed.rs`（CIDフォントオブジェクトグラフの構築）です。特定のPDFビューアでの描画バグを報告する場合は、ビューア名とバージョンを明記してください。

---

## ライセンス

MIT OR Apache-2.0
