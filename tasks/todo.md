# harumi TODO

## Phase 0 — プロジェクト基盤 (完了)

- [x] `cargo new harumi --lib` でクレートを初期化
- [x] `Cargo.toml` に lopdf 0.40 / allsorts 0.17 / ttf-parser 0.25 / thiserror 2 を追加（`MIT OR Apache-2.0`）
- [x] `src/error.rs` — `HarumiError` enum の定義
- [x] `src/document.rs` — `Document::from_file` / `from_bytes` の骨格
- [x] インテグレーションテスト: PDFを読み込んでそのまま保存 → ページ数が保たれることを確認

## Phase 1 — フォントパイプライン (完了)

- [x] `src/font/mod.rs` — `FontHandle`（不透明な u32）と `FontKind` enum
- [x] `src/font/subset.rs`
  - [x] フォント種別判定（先頭4バイト: `0x00010000` / `true` → TrueType, `OTTO` → CFF）
  - [x] CFF を受け入れ（`UnsupportedFontKind` 廃止 → allsorts に委ねる）
  - [x] ttf-parser で Unicode → GID マッピング（allsorts より API がシンプル）
  - [x] allsorts `SubsetProfile::Pdf` + `CmapTarget::Unicode` で TTF サブセット生成
  - [x] **GIDリマッピング修正**: allsorts が 0..N に再採番した新GIDを gid_to_char / gid_to_advance に反映
  - [x] gids.len() > u16::MAX のオーバーフロー防御
  - [x] 戻り値: `SubsetResult { bytes, gid_to_char, gid_to_advance, units_per_em, font_kind }`
- [x] `src/font/cmap.rs` — ToUnicode CMap ストリーム生成
  - [x] `beginbfchar` ブロック（最大100エントリ/ブロック）
  - [x] UTF-16BE エンコーディング対応（サロゲートペアも含む）
- [x] `src/font/embed.rs` — lopdf オブジェクトグラフ構築
  - [x] `FontDescriptor` オブジェクト
  - [x] `FontFile2` ストリーム（TTF サブセット）+ `FontFile3` ストリーム（CFF/OTF サブセット）
  - [x] `CIDFontType2` オブジェクト（`/W` 幅配列、`/CIDToGIDMap /Identity`）
  - [x] `Type0` フォントオブジェクト（`/Identity-H` エンコーディング）
  - [x] ToUnicode ストリームの紐付け
  - [x] `build_widths_array` の `unwrap()` を `unwrap_or(units_per_em)` に変更（防御的）

## Phase 2 — コンテントストリーム注入 (完了)

- [x] `src/content/text.rs` — PDFテキスト演算子の構築
  - [x] `BT / ET`, `Tf`, `3 Tr`（不可視レンダーモード）, `Tj`
  - [x] GID → 2バイト16進文字列変換（Identity-H エンコーディング）
- [x] `PageHandle::add_invisible_text` の実装
  - [x] `/Contents` が単一参照・配列・未存在の3ケースを処理
  - [x] 新しいコンテントストリームを `/Contents` 配列に追記
  - [x] `/Resources /Font` 辞書にフォントを登録（間接参照・インライン両対応）
- [x] テキスト追加を `save()` 時に遅延処理（全ページのフォントを1回のサブセットにまとめる）

## Phase 3 — 仕上げ (完了)

- [x] `PageHandle::add_invisible_text_runs` — バッチAPI（`TextRun` スライス受け取り）
- [x] `#[cfg(feature = "ocr")]` — hOCR座標変換ヘルパー `hocr_y_to_pdf` / `hocr_x_to_pdf` / `pixel_size_to_pt`
- [x] `examples/japanese_ocr.rs` — CLAUDE.md のターゲットAPIと一致するデモ
- [x] `add_text(text, font, pos, size, color)` — 可視テキスト（Tr 0 + RGB）
- [x] `page.size() -> (f32, f32)` — MediaBox からページサイズ取得（親チェーン走査対応）
- [x] `save_to_bytes() -> Result<Vec<u8>>` — Tauri / WASM 向けインメモリ出力
- [x] docs.rs 用の rustdoc コメント整備（`src/lib.rs`, `src/document.rs`）
- [x] `finalize()` の `embedded.get().unwrap()` を `.ok_or(Error::InvalidFont(...))?` に変更
- [x] `has_cff2_table()` — CFF2 可変フォントを早期検出して明確なエラーを返す
- [x] `ttcf` マジックバイト検出を追加（ttf-parser / allsorts は index=0 で TTC を自動処理）

## Phase 4 — draw / image feature (完了)

- [x] `Cargo.toml`: `draw` / `image` feature 追加、`image` クレートを optional dep に
- [x] `src/error.rs`: `ImageDecode(String)` エラー追加（`image` feature 限定）
- [x] `src/draw/mod.rs`: `DrawOp` enum、`ExtGStateRegistry`（opacity 重複排除）
- [x] `src/draw/shapes.rs`: `rect_stream()`、`line_stream()` — 純粋な PDF オペレータ
- [x] `src/draw/image.rs`: `PreparedImage`、`embed_xobject()`、`image_stream()`、JPEG SOF マーカーパーサ
- [x] `src/document.rs`: `PendingOp` enum 導入、`finalize()` 拡張、`add_rect` / `add_line` / `add_image` / `add_image_with_opacity` 追加、ExtGState / XObject リソース登録ヘルパー
- [x] `tests/draw_smoke.rs`: 8件の統合テスト（矩形・線・JPEG・PNG・透明度・混合）
- [x] `tests/fixtures/red_1x1.jpg` / `red_1x1.png`: テスト用フィクスチャ
- [x] README 4言語（英・日・中・韓）を draw/image feature で更新

## Phase 5 — add_text_box + add_rect_stroke + add_polygon (完了)

- [x] `src/draw/mod.rs`: `DrawOp::RectStroke`, `DrawOp::Polygon` バリアント追加
- [x] `src/draw/shapes.rs`: `rect_stroke_stream()`、`polygon_stream()` 追加（ユニットテスト 4 件含む）
- [x] `src/document.rs`: `is_cjk()`、`glyph_advance_pt()`、`wrap_paragraph()` private helper
- [x] `src/document.rs`: `add_text_box()`（feature gate なし — ttf_parser で advance 幅を計測し greedy 折り返し）
- [x] `src/document.rs`: `add_rect_stroke()`、`add_polygon()`（draw feature）、`finalize()` match 拡張
- [x] `tests/draw_smoke.rs`: 3件の統合テスト追加（rect_stroke・polygon・text_box 折り返し）

## Phase 6 — セキュリティ・バグ修正 (完了)

- [x] `src/error.rs`: `Error::InvalidInput(String)` エラーバリアント追加
- [x] C-1: `size()` — MediaBox 配列の要素数 < 4 でパニックしていた → 境界チェック追加
- [x] C-2: `finalize()` — 二重 `save()` ガード（`Document.finalized: bool` フィールド＋開始時チェック＋終了時セット）
- [x] C-3: `prepare()` — PNG の `w * h * 3` が u32 オーバーフロー → u64 演算 + 200MP 上限
- [x] M-1: `append_to_contents()` — 間接 Contents 配列（InDesign 形式）への対応（参照先が Array か Stream かを判定）
- [x] M-2: `parse_jpeg_dims()` — スタンドアロン JPEG マーカー（RST0–RST7, EOI 等）が length フィールドを持たないのに読んでいた → `i += 2; continue`
- [x] M-3: `add_text_box()` — `rect[2] <= 0.0 || rect[3] <= 0.0` のアーリーリターン
- [x] Mi-2: 全公開 API に NaN/Infinity ガード（`check_finite()` private helper）
- [x] C-1 (第2回): `prepare()` — `image::load_from_memory` の前に `ImageReader::into_dimensions()` で寸法チェック → OOM 防止
- [x] M-1 (第2回): `add_invisible_text_runs` — ループ内に `check_finite` ガード追加（x/y/font_size/color が未チェックだった）
- [x] M-2 (第2回): `add_text_box` — `rect`/`font_size`/`color`/`line_height` に `check_finite` ガード追加
- [x] Mi-1 (第2回): `src/font/subset.rs` — `gids.contains()` の O(N²) 線形探索を `HashSet<u16>` に置き換え
- [x] Mi-2 (第2回): `src/font/subset.rs` line 107 — 最大グリフ数ガードのコメントを明確化
- [x] Mi-3 (第2回): `parse_jpeg_dims()` — JPEG 0xFF パディングバイト対応（JPEG spec §B.1.1.2）、ユニットテスト 2 件追加

## テスト（Phase 7 完了時点 — 56件）

| スイート | ファイル | 件数 |
|---|---|---|
| ユニット | `src/` 各モジュール内 `#[cfg(test)]` | 16 |
| インテグレーション | `tests/integration.rs` | 7 |
| スモーク | `tests/smoke.rs` | 8 |
| E2E | `tests/e2e_noto_jp.rs` | 6 |
| draw/image/テキストボックス/SMask | `tests/draw_smoke.rs` | 13 |
| JPEG パーサ | `src/draw/image.rs` 内 `#[cfg(test)]` | 2 |
| ドキュメンテーション | `src/lib.rs`, `src/document.rs` | 4 |

- [x] CMap 生成（BMP文字・サロゲートペア）
- [x] コンテントストリーム演算子（`3 Tr`・hex エンコーディング）
- [x] 図形ストリーム（`rect_stream` / `line_stream` / `rect_stroke_stream` / `polygon_stream`）
- [x] フォント種別判定（TTF / TTC / CFF / 不明）
- [x] PDF 構造バリアント（Contents欠け・配列・間接 Resources・MediaBox 継承）
- [x] draw/image API（矩形・線・JPEG・PNG・透明度・枠線矩形・多角形）
- [x] `add_text_box` 折り返し（narrow box → 複数 BT/ET ブロック）
- [x] 手動確認: Chrome で Cmd+A → 不可視テキスト3行・可視テキスト1行がすべて選択されることを確認済み（Preview は CIDFontType2+Identity-H の選択が苦手なため Chrome / Acrobat で検証）

## ドキュメント

- [x] README.md (英語) — Phase 5/6 追記済み
- [x] README_ja.md (日本語) — Phase 5/6 追記済み
- [x] README_zh.md (中国語) — Phase 5/6 追記済み
- [x] README_kr.md (韓国語) — Phase 5/6 追記済み
- [x] CHANGELOG.md

## Phase 7 — PNG SMask / True Transparency (完了)

- [x] `src/draw/image.rs`: `ImageData::RgbWithAlpha { rgb, alpha }` バリアント追加
- [x] `prepare()`: alpha チャンネル検出 — `has_alpha` で分岐し白背景合成を廃止
- [x] `embed_xobject()`: `RgbWithAlpha` アーム — DeviceGray SMask サブオブジェクト作成 → `/SMask` 参照付きメイン画像
- [x] `tests/fixtures/red_semitransparent_1x1.png`: 1x1 RGBA (255,0,0,128) フィクスチャ追加
- [x] `tests/draw_smoke.rs`: `smoke_png_alpha_creates_smask` / `smoke_png_opaque_has_no_smask` 追加

## Phase 8 — テキスト opacity / VerticalAlign / add_polyline / crates.io 準備 (完了)

- [x] `src/document.rs`: `PendingText` に `opacity: f32` フィールド追加
- [x] `src/content/text.rs`: `text_stream` に `gs_name: Option<&str>` 追加（opacity 適用）
- [x] `src/document.rs`: `finalize()` Pass 3 — Text アームで opacity < 1.0 のとき ExtGState 登録
- [x] `src/document.rs`: `add_text_with_opacity` 追加（`draw` feature、ExtGState /ca）
- [x] `src/document.rs`: `add_text_box_with_opacity` 追加（`draw` feature）
- [x] `src/document.rs`: `VerticalAlign { Top, Center, Bottom }` enum 追加（`src/lib.rs` から pub 再エクスポート）
- [x] `src/document.rs`: `add_text_box_aligned(align: VerticalAlign)` 追加、`add_text_box` は Top に委譲
- [x] `src/draw/mod.rs`: `DrawOp::Polyline` バリアント追加
- [x] `src/draw/shapes.rs`: `polyline_stream()` 追加（パスを閉じずに S でストローク）
- [x] `src/document.rs`: `add_polyline` 追加（`draw` feature）
- [x] `src/document.rs`: `embed_font` の doc comment に重複挙動を明記
- [x] `src/document.rs`: `document_is_send` テスト追加（`Document: Send` を型検査）
- [x] `tests/integration.rs`: `roundtrip_save_reload_preserves_page_count` 追加
- [x] `tests/draw_smoke.rs`: `smoke_text_with_opacity` / `smoke_text_box_center_align` / `smoke_polyline_three_segments` 追加
- [x] `src/content/text.rs`: `text_stream_with_gs_emits_gs_op` ユニットテスト追加
- [x] `src/draw/shapes.rs`: `polyline_no_closepath` / `polyline_fewer_than_2_points_is_empty` ユニットテスト追加

## テスト（全フェーズ合計 — 64件）

| スイート | ファイル | 件数 |
|---|---|---|
| ユニット | `src/` 各モジュール内 `#[cfg(test)]` | 22 |
| インテグレーション | `tests/integration.rs` | 8 |
| スモーク | `tests/smoke.rs` | 8 |
| E2E | `tests/e2e_noto_jp.rs` | 6 |
| draw/image/テキストボックス/SMask/Phase8 | `tests/draw_smoke.rs` | 16 |
| ドキュメンテーション | `src/lib.rs`, `src/document.rs` | 4 |

## Phase 9 — ページ操作 API (完了)

- [x] `Document::rotate_page(number: u32, degrees: i32) -> Result<()>` — /Rotate に degrees を加算（90 の倍数のみ）; 負数 = CCW; 値を累積; i64 演算でオーバーフロー回避
- [x] `Document::remove_page(number: u32) -> Result<()>` — ページを削除; ページ存在チェック → 最小件数チェック → /Parent を更新
- [x] `Document::insert_blank_page(after: u32, size: (f32, f32)) -> Result<()>` — after=0 で先頭挿入、after=page_count で末尾追加; size は check_finite + 正値チェック
- [x] `Document::reorder_pages(new_order: &[u32]) -> Result<()>` — 1-indexed; 長さ・範囲・重複を検証; ネスト /Pages ツリーの /Parent を更新
- [x] Private helper: `root_pages_id(doc: &lopdf::Document) -> Result<ObjectId>`
- [x] 上記 4 メソッドはすべて `save()` 後に呼ぶと `InvalidInput` を返す
- [x] `Document::page()` に `finalized` ガードを追加（`save()` 後の呼び出しで即 `InvalidInput`）

### バグ修正（フェーズ 9 内レビューで発見）

- [x] `rotate_page`: `Object::Real` の /Rotate を `match` で分岐（`as_i64()` は `Object::Integer` のみ対応のため `Real(270.0)` が 0 扱いになっていた）
- [x] `rotate_page`: i64 演算に切り替えてオーバーフロー回避（クラフト入力で debug ビルドがパニックしていた）
- [x] `insert_blank_page`: `size` パラメータに `check_finite` + `> 0.0` チェックを追加（NaN/Inf/ゼロ/負値 → `InvalidInput`）

### ドックテスト追加

- [x] `no_run` 例を追加: `page_count()`, `save_to_bytes()`, `save_to_writer()`, `add_text_box()`, `add_text_box_aligned()`, `add_rect()`, `add_line()`

### テスト追加 (`tests/page_ops.rs` — 新規ファイル、27 件)

- [x] rotate: 90°・累積・負値・Real /Rotate・ゼロ・無効 degrees・ページ未発見
- [x] remove: 中間・先頭・末尾・1ページのみ・未発見
- [x] insert_blank: 先頭・末尾・中間・page_count 超えエラー・NaN/ゼロ/負値 size・挿入後テキスト追加
- [x] reorder: 逆順・左ローテーション・長さ不一致・重複・範囲外・ゼロエントリ
- [x] finalized ガード: 4 op すべて + page()

## Phase 10 — merge_from + remove_page バグ修正 (完了)

### 新機能: Document::merge_from(other: Document)

- [x] `other` の全ページを `self` の末尾に追記する
- [x] 実装 (`src/document.rs`):
  1. finalized ガード + `other.pending.is_empty()` チェック
  2. `other.inner.renumber_objects_with(self.inner.max_id + 1)` で ID 衝突回避
  3. `self.inner.objects.extend(other_inner.objects)` — 全オブジェクトをマージ
  4. `self.inner.max_id = other_inner.max_id` — max_id を更新
  5. other の各ページの /Parent を self の pages_root に更新
  6. /Kids を self_pages + other_pages で再構築し /Count を更新
- [x] 事前条件: `other.pending` が空であること（freshly loaded か save_to_bytes() 済み）
- [x] 非保存対象: Outlines/Bookmarks、AcroForm、/Info メタデータ

### バグ修正: remove_page の正確性 + サイズ改善

- [x] **正確性バグ**: 削除済みページの pending ops が残っていた問題を修正
  - `self.pending.retain(|p| p.page_id != target_id)` で解決
- [x] **サイズバグ**: 削除済みページの dict オブジェクトがオーファンとして残っていた問題を修正
  - `self.inner.objects.remove(&target_id)` で解決
  - 注意: /Contents ストリームと /Resources は意図的に残す（他ページで共有の可能性）

### テスト追加 (`tests/page_ops.rs` — 7 件追加、合計 34 件)

- [x] `smoke_merge_appends_pages`: 1ページ + 3ページ = 4ページ
- [x] `smoke_merge_preserves_content`: マージ後も MediaBox の高さが保持される
- [x] `smoke_merge_then_add_text`: マージ後のページに self のフォントでテキストを追加
- [x] `smoke_merge_rejects_pending`: other に pending ops があるとエラー
- [x] `smoke_merge_rejects_finalized`: self が finalized（pending ops あり）だとエラー
- [x] `smoke_merge_two_then_two`: 2ページ + 2ページ = 4ページ
- [x] `regression_remove_page_pending_cleared`: ページ 2 にテキスト追加 → ページ 2 を削除 → save が成功する

## テスト（Phase 10 完了時点 — 84件、デフォルト features）

| スイート | ファイル | 件数 |
|---|---|---|
| ユニット | `src/` 各モジュール内 `#[cfg(test)]` | 12 |
| インテグレーション | `tests/integration.rs` | 8 |
| スモーク | `tests/smoke.rs` | 8 |
| E2E | `tests/e2e_noto_jp.rs` | 6 |
| draw/image/テキストボックス/SMask/Phase8 | `tests/draw_smoke.rs` | 2 (feature なしで実行可能なもの; 全4件中2件は feature フラグ必要) |
| ページ操作 | `tests/page_ops.rs` | 34 |
| ドキュメンテーション | `src/lib.rs`, `src/document.rs` | 14 |

## Phase 11 — Document::new + extract_pages (完了)

- [x] `Document::new(size: (f32, f32)) -> Result<Self>` — 白紙の1ページPDFをゼロから作成。size はPDFポイント単位（A4 = 595×842、Letter = 612×792）。ゼロ・負値・NaN は `InvalidInput` を返す
- [x] `Document::extract_pages(&self, page_numbers: &[u32]) -> Result<Document>` — 指定ページ（1始まり、呼び出し元が指定した順序）だけを含む新しい `Document` を返す。ページのコンテンツ・フォント・画像を保持。Outlines / AcroForm / /Names / /PageLabels / /OpenAction / /StructTreeRoot を除去。空配列・重複は `InvalidInput`、範囲外は `PageNotFound`。`self` を変更しない
- [x] `tests/document_new.rs` — 新規テスト 6 件
- [x] `tests/page_ops.rs` — 新規テスト 8 件追加（合計テスト数 98 件）

## テスト（全フェーズ合計 — 98件）

| スイート | ファイル | 件数 |
|---|---|---|
| ユニット | `src/` 各モジュール内 `#[cfg(test)]` | 12 |
| インテグレーション | `tests/integration.rs` | 8 |
| スモーク | `tests/smoke.rs` | 8 |
| E2E | `tests/e2e_noto_jp.rs` | 6 |
| draw/image/テキストボックス/SMask/Phase8 | `tests/draw_smoke.rs` | 2 (feature なしで実行可能なもの) |
| ページ操作 | `tests/page_ops.rs` | 42 (Phase 9/10 の 34 件 + Phase 11 の 8 件) |
| Document::new | `tests/document_new.rs` | 6 |
| ドキュメンテーション | `src/lib.rs`, `src/document.rs` | 14 |

## Phase 12 — extract_text_runs 拡張 + PdfMetadata + publish 準備 (完了)

### extract_text_runs: 標準エンコーディングサポート (`src/extract.rs` 全面改修)

- [x] `FontInfo` に `bytes_per_char: u8` フィールド追加（CID=2, simple=1）
- [x] `Token::LitStr(Vec<u8>)` バリアント追加 — `(...)` リテラル文字列のキャプチャ
- [x] `parse_literal_string` 実装（`\n \r \t \\ \( \)` と8進 `\ddd` エスケープ対応）
- [x] `parse_to_unicode_cmap` に `beginbfrange` 対応追加（連続範囲・明示リスト両形式）
- [x] 3種の標準エンコーディングテーブル埋め込み（`const [Option<char>; 256]`）: `WIN_ANSI_ENCODING` / `MAC_ROMAN_ENCODING` / `STANDARD_ENCODING`
- [x] AGL サブセット（~200エントリ）の静的ソート済みスライス + `glyph_name_to_char()` バイナリサーチ
- [x] `collect_fonts_inner` に Subtype 分岐追加（Type0 → CID / Type1・MMType1・TrueType → simple font）
- [x] `collect_simple_font`: /ToUnicode → エンコーディングテーブルのフォールバックチェーン
- [x] `build_encoding_map`: /Encoding が Name の場合はテーブル直接、Dictionary の場合は /BaseEncoding + /Differences 適用
- [x] `apply_differences`: `/Differences` 配列のグリフ名を AGL で解決して map を上書き
- [x] `collect_simple_font_widths`: /FirstChar + /Widths[] から WidthRun を生成
- [x] `decode_chars_to_fragment`（旧 `decode_gids_to_fragment`）に bytes_per_char 分岐を追加
- [x] `Tj` / `TJ` アームで `Token::LitStr` を `Token::HexStr` と同等に処理
- [x] 統合テスト 4 件追加（`tests/extract_text.rs`）: hex 文字列・リテラル文字列・TJ 配列・ToUnicode なしのエンコーディングフォールバック

### セキュリティ・バグ修正（コードレビュー起因）

- [x] `parse_bfrange_line`: `lo > hi` の場合に早期 return（従来は誤ったマッピングを1件挿入）
- [x] `decode_chars_to_fragment`: 未マップグリフで `?` 使用によりフラグメント全体をドロップしていた → `continue` に変更（1グリフだけスキップ）
- [x] 誤検知: `set_metadata` の `StringFormat::Literal` は lopdf が `(` `)` `\` を自動エスケープするため実際には問題なし（confirmed）

### PDF メタデータ読み書き (`src/document.rs`)

- [x] `PdfMetadata` 構造体追加（`title`, `author`, `subject`, `keywords`, `creator`: 全 `Option<String>`）— `Debug, Clone, Default, PartialEq` derive
- [x] `lopdf_string_to_rust()` ヘルパー: UTF-16BE（BOM `\xFE\xFF`）とraw UTF-8 / Latin-1 に対応
- [x] `Document::metadata()` — /Info を trailer から読み取り。/Info なしは `Ok(PdfMetadata::default())`
- [x] `Document::set_metadata(&PdfMetadata)` — 常に新しい /Info dict を追加してトレーラー参照を更新
- [x] `src/lib.rs` に `PdfMetadata` を再エクスポート
- [x] `tests/metadata.rs` 新規（5件）: no-/Info default, 完全ラウンドトリップ, 部分書き込み, 上書き, 既存 /Info 読み取り

### crates.io publish 準備

- [x] `Error` enum に `#[non_exhaustive]` 追加
- [x] `UnsupportedFontKind` のドキュメントコメントから "v0.1" 表記を削除
- [x] `Cargo.toml`: `version = "0.2.0"`, `rust-version = "1.88"`, `readme = "README.md"`, `description` を更新
- [x] README 4言語（英・日・中・韓）に extract_text_runs 拡張と PdfMetadata API を追記
- [x] `CHANGELOG.md` の `[Unreleased]` を確認・補完
- [x] `cargo publish --dry-run` で確認済み（コミット後に実行）

## テスト（Phase 12 完了時点 — 121件）

| スイート | ファイル | 件数 |
|---|---|---|
| ユニット | `src/` 各モジュール内 `#[cfg(test)]` | 27 |
| インテグレーション | `tests/integration.rs` | 6 |
| スモーク | `tests/smoke.rs` | 8 |
| E2E | `tests/e2e_noto_jp.rs` | 2 |
| draw/image/テキストボックス/SMask/opacity | `tests/draw_smoke.rs` | 8 |
| ページ操作 | `tests/page_ops.rs` | 42 |
| Document::new | `tests/document_new.rs` | 6 |
| テキスト抽出 | `tests/extract_text.rs` | 11 |
| メタデータ | `tests/metadata.rs` | 5 |
| ドキュメンテーション | `src/lib.rs`, `src/document.rs` | 14 |

## Phase 13 — replace_text: コンテントストリームの真のテキスト置換 (完了)

- [x] `src/extract.rs`: `FontInfo`, `WidthRun`, `collect_fonts`, `page_content_streams`, `decode_hex_bytes`, `is_pdf_whitespace`, `is_pdf_delimiter`, `parse_literal_string` を `pub(crate)` に変更（ロジック変更なし）
- [x] `src/replace.rs` (新規ファイル):
  - [x] `TextReplaceOp { font, old_text, new_text }` — `document.rs` から渡されるキュー項目
  - [x] `ResolvedReplacement { old_text, new_text, new_pdf_font_name, char_to_gid, gid_to_advance, units_per_em }` — フォント埋め込み後に解決された置換情報
  - [x] `rewrite_content_stream(bytes, replacements, existing_fonts) -> (Vec<u8>, HashSet<Vec<u8>>)` — コアストリーム書き換えロジック。バイトオフセット追跡付きトークナイザで Tj/TJ 演算子を処理
  - [x] `rewrite_page_streams(doc, page_id, resolved) -> (Vec<u8>, HashSet<Vec<u8>>)` — ページの全ストリームを結合して書き換え
  - [x] TJ 配列スプリット: TJ 配列内の一致要素を個別 Tj として切り出し、前後の配列要素は個別 Tj で再出力、カーニング数値は `Td` として吸収
  - [x] 幅補正: `(orig_width - new_width) 0 Td` で後続テキストのドリフトを防止
  - [x] 使用フォント追跡: 実際に置換に使われたフォント名の `HashSet` を返し、選択的リソース登録を可能にする
  - [x] ユニットテスト 5 件（push_number・decode_existing・emit_replacement・no_match_is_verbatim・tj_split）
- [x] `src/document.rs`:
  - [x] `PendingOp::Replace(TextReplaceOp)` バリアント追加
  - [x] `EmbedState` に `gid_to_advance: BTreeMap<u16, u16>`, `units_per_em: u16` フィールド追加
  - [x] フォント命名を `F{idx}` → `HR{idx}` に変更（既存 PDF のフォント名と衝突しないようにするため）
  - [x] Pass 1: `PendingOp::Replace` の `new_text` からも文字を収集
  - [x] Pass 2 と Pass 3 の間に新しい置換パスを追加: `collect_fonts` → `ResolvedReplacement` リスト作成 → `rewrite_page_streams` → Contents を書き換え → 実際に使ったフォントのみをリソースに登録
  - [x] Pass 3: `PendingOp::Replace` はスキップ（置換パスで処理済み）
  - [x] `PageHandle::replace_text(old_text, new_text, font) -> Result<()>` 公開メソッド追加
- [x] `src/lib.rs`: `mod replace;` 追加
- [x] `tests/replace.rs` (新規ファイル): 統合テスト 4 件
  - [x] `replace_text_latin_present_in_output` — "Hello" → "World" のラウンドトリップ
  - [x] `replace_text_no_match_is_noop` — 対象テキストなし → ファイル無変更
  - [x] `replace_text_cjk` — "日本語" → "英語" の CJK 置換
  - [x] `replace_multiple_on_same_page` — 同一ページで複数の独立した置換

### バグ修正（Phase 13 実装中に発見）

- [x] **フォント名衝突バグ**: 埋め込みフォントを `F0`, `F1` と命名すると既存 PDF のフォントリソースを上書きし、既存テキストが文字化けしていた → `HR{idx}` プレフィックスに変更
- [x] **非選択的リソース登録バグ**: 置換マッチがなくても `add_font_to_resources` を呼び出して既存フォントリソースを上書きしていた → `rewrite_content_stream` が実際に使ったフォント名の `HashSet` を返す設計に変更し、`HashSet` に含まれるフォントのみ登録
- [x] **`subset.gid_to_advance` の二重 move エラー**: `EmbedParams` に渡した後で `EmbedState` にも使用 → `saved_gid_to_advance = subset.gid_to_advance.clone()` で解決
- [x] **`FontInfo` の private フィールドが replace.rs のユニットテストで参照できない**: `dw`, `w_runs`, `WidthRun`, `start_gid`, `widths` を `pub(crate)` に変更

## テスト（Phase 13 完了時点 — 139件）

| スイート | ファイル | 件数 |
|---|---|---|
| ユニット | `src/` 各モジュール内 `#[cfg(test)]` | 32 |
| インテグレーション | `tests/integration.rs` | 6 |
| スモーク | `tests/smoke.rs` | 8 |
| E2E | `tests/e2e_noto_jp.rs` | 2 |
| draw/image/テキストボックス/SMask/opacity | `tests/draw_smoke.rs` | 8 |
| ページ操作 | `tests/page_ops.rs` | 42 |
| Document::new | `tests/document_new.rs` | 6 |
| テキスト抽出 | `tests/extract_text.rs` | 11 |
| メタデータ | `tests/metadata.rs` | 5 |
| テキスト置換 | `tests/replace.rs` | 4 |
| ドキュメンテーション | `src/lib.rs`, `src/document.rs` | 15 |

## Phase 14 — TextFragment 拡張 + replace_text count + can_replace_text + add_ellipse (完了)

### TextFragment フィールド追加 (`src/extract.rs`)

- [x] `TextFragment::font_name: String` — その位置での PDF リソース名（例: `"HR0"`, `"F1"`）
- [x] `TextFragment::color: [f32; 3]` — その位置での RGB 塗りつぶし色（`rg`/`g` 演算子を追跡; デフォルト黒）
- [x] `TextFragment::invisible: bool` — テキストレンダーモードが 3（`Tr 3`）なら `true`
- [x] `TextFragment` を `#[non_exhaustive]` に変更（将来のフィールド追加で semver が壊れないようにする）

### replace_text の戻り値変更 (`src/document.rs`, `src/replace.rs`)

- [x] `PageHandle::replace_text` 戻り値を `Result<()>` → `Result<usize>`（マッチ件数を返す）
  - 0 = 見つからなかった; ページが変更されない保証
  - マッチ件数はコール時に読み取り専用スキャンで即時計算（save() 前に知ることができる）
- [x] `PageHandle::replace_text_preserve_font` 戻り値を `Result<()>` → `Result<usize>`
  - グリフ検証もコール時に即時実行（`FontCharNotMapped` エラーを save() 前に返す）
- [x] `PageHandle::can_replace_text(old_text, new_text) -> Result<usize>` 追加
  - 純粋読み取り専用スキャン; ドキュメントを変更しない
  - old_text のマッチ件数を返すか `Err(FontCharNotMapped)` を返す
  - 事前チェックとして replace_text の前に使う想定
- [x] `src/replace.rs`: `count_matches_in_page` 関数追加

### add_ellipse (`src/document.rs`, `src/draw/mod.rs`, `src/draw/shapes.rs`)

- [x] `DrawOp::Ellipse { rect, color, opacity, filled }` バリアント追加
- [x] `ellipse_stream()` — 4本の3次ベジェ曲線（K=0.5522…）で楕円を近似; `filled=true` → `rg`/`f`, `filled=false` → `RG`/`S`
- [x] `PageHandle::add_ellipse(rect, color, opacity, filled)` 公開メソッド追加（`draw` feature）

### テスト追加

- [x] `tests/extract_text.rs`: font_name / color / invisible フィールドのテスト 6 件追加（合計 17 件）
- [x] `tests/replace.rs`: replace_text のカウント戻り値・no-match=0・can_replace_text・2ページ同時置換 など 8 件追加（合計 12 件）
- [x] `tests/draw_smoke.rs`: add_ellipse（塗り・輪郭）2 件追加

## Phase 15 — テキスト回転 + fill+stroke + add_path + クロス演算子置換 (完了) [v0.3.0]

### テキスト回転 (`src/content/text.rs`, `src/document.rs`)

- [x] `text_stream()` に `rotation_degrees: f32` パラメータ追加
  - `rotation_degrees == 0.0` → 既存通り `{x} {y} Td`（後方互換）
  - `rotation_degrees != 0.0` → `cos sin -sin cos x y Tm`（PDF テキスト行列）
- [x] `invisible_text_stream()` を `rotation_degrees=0.0` で `text_stream()` に委譲
- [x] `PendingText` に `rotation_degrees: f32` フィールド追加; 既存の `add_text` 系はすべて `0.0` で初期化
- [x] `PageHandle::add_text_with_rotation(text, font, pos, size, color, opacity, degrees)` 追加
- [x] ユニットテスト 2 件追加: `rotation_zero_uses_td`, `rotation_nonzero_uses_tm`

### fill+stroke 同時描画（破壊的変更: stroke_width パラメータ追加）

- [x] `DrawOp::Ellipse` / `DrawOp::Polygon` に `stroke_width: f32` フィールド追加
- [x] `ellipse_stream()` / `polygon_stream()` に `stroke_width: f32` パラメータ追加
  - `filled=true, stroke_width=0.0` → `f`（従来の filled=true）
  - `filled=false, stroke_width>0.0` → `S`（従来の filled=false）
  - `filled=true, stroke_width>0.0` → `rg + RG + w + B`（**新規**: fill-then-stroke）
- [x] `PageHandle::add_ellipse` / `add_polygon` シグネチャに `stroke_width: f32` 追加（破壊的変更）
- [x] 既存テスト呼び出しを `stroke_width=0.0` で更新（`tests/draw_smoke.rs`）
- [x] ユニットテスト追加: `polygon_stream_fill_and_stroke`

### 統合パス API `add_path`

- [x] `DrawOp::Path { points, closed, color, opacity, filled, stroke_width }` バリアント追加
- [x] `path_stream(points, closed, color, gs_name, filled, stroke_width)` 追加（`polygon_stream`/`polyline_stream` の一般化）
  - `closed=true` → 末尾に `h` (closepath)
  - fill/stroke/both の3モード（`f`/`S`/`B`）
- [x] `PageHandle::add_path(points, closed, color, filled, stroke_width, opacity)` 追加（`draw` feature）
- [x] `src/document.rs` の `finalize()` match に `DrawOp::Path` アーム追加
- [x] `tests/draw_smoke.rs` に add_path の統合テスト 1 件追加

### クロス演算子テキスト置換 (`src/replace.rs`, `tests/replace.rs`)

同一フォントコンテキスト（同一 `Tf`、`BT`〜`ET` 内）の連続する `Tj`/`TJ` 演算子にまたがる `old_text` をマッチ・置換できるようにした。

- [x] 新データ構造: `CharEntry { ch, op_idx, raw_bytes }`, `CharSegment { chars, font_name, font_size }`, `CrossOpMatch`, `CrossOpMatchPreserve`
- [x] 新ヘルパー: `push_chars_from_bytes()`, `collect_char_segments()`, `find_cross_op_matches()`, `find_cross_op_matches_preserve()`, `emit_cross_op_replacement()`
- [x] `count_matches_in_page()`: セグメントベースのサブストリング検索に変更（単一演算子完全一致から汎化）
- [x] `rewrite_content_stream()`: クロス演算子マッチのプリコンピュート + `op_role` マップで first/middle/last を処理
- [x] `rewrite_stream_preserve_font()`: 同様にクロス演算子対応
- [x] 位置演算子（`Td`, `Tm`）を挟む演算子間のマッチは意図的に除外
- [x] 新インテグレーションテスト 3 件 (`tests/replace.rs`):
  - `replace_text_cross_operator` — split Tj PDF で "Hello" → "World" の cross-op 置換
  - `can_replace_text_cross_operator_count` — cross-op マッチカウント確認
  - `replace_preserve_font_cross_operator` — cross-op preserve_font 置換
- [x] テストヘルパー `split_first_tj` / `try_split_hex_tj` — harumi 生成 PDF の Tj を2分割して cross-op シナリオを再現

## テスト（v0.3.0 完了時点 — 191件、--all-features）

| スイート | ファイル | 件数 |
|---|---|---|
| ユニット | `src/lib.rs` 他（内部 `#[cfg(test)]`） | 49 |
| インテグレーション | `tests/integration.rs` | 8 |
| スモーク | `tests/smoke.rs` | 8 |
| E2E | `tests/e2e_noto_jp.rs` | 6 |
| draw/image/パス/回転 | `tests/draw_smoke.rs` | 18 |
| ページ操作 | `tests/page_ops.rs` | 42 |
| Document::new | `tests/document_new.rs` | 6 |
| テキスト抽出 | `tests/extract_text.rs` | 17 |
| メタデータ | `tests/metadata.rs` | 5 |
| テキスト置換 | `tests/replace.rs` | 15 |
| ドキュメンテーション | `src/lib.rs`, `src/document.rs` | 17 |

## Phase 16 — FlowDocument builder + HTML→PDF renderer (完了)

- [x] `flow` feature 追加（`draw` を implies）
  - [x] `src/flow/mod.rs`（新規）: `FlowDocument`、`FlowOptions`、`Margins`
    - [x] `push_heading`・`push_paragraph`・`push_key_value_table`・`push_list`・`push_page_break`・`render`
    - [x] `ensure_space(height)` — 自動改ページ + `max_pages` DoS ガード（デフォルト2000ページ）
    - [x] `content_y` 座標系（上端からの距離・下向き正）→ PDF y 変換 (`pdf_baseline_y`・`pdf_top_y`)
    - [x] `measure_lines` — `wrap_paragraph`（`pub(crate)` 化済み）を再利用
    - [x] テーブル罫線は `add_line`（`draw` feature）で描画
  - [x] `html` feature 追加（`flow` + `dep:scraper` を implies）
    - [x] `src/flow/html.rs`（新規）: `render_html_to_pdf`、`HtmlRenderOptions`
    - [x] 反復的な DFS ウォーカー（`walk_iterative<'a>` + `process_one<'a>`）— 再帰によるスタックオーバーフロー防止
    - [x] `table_rows()` ヘルパー — `tbody`/`thead`/`tfoot` を考慮、ネストしたテーブルの行を誤収集しない
    - [x] `process_list` — 直接の `<li>` 子要素のみ収集（ネストリストで重複しない）
    - [x] `has_page_break()` — `style="page-break-after: always"` と `class="page-break"` を検出
  - [x] `src/lib.rs`: `FlowDocument`・`FlowOptions`・`Margins`・`render_html_to_pdf`・`HtmlRenderOptions` を再エクスポート
  - [x] `src/document.rs`: `is_cjk`・`glyph_advance_pt`・`wrap_paragraph` を `pub(crate)` に昇格

### バグ修正（Phase 16 実装中に発見）

- [x] **見出し前スペーシングのオルファン問題**: `ensure_space` を呼ぶ前に `pre_spacing` を計算し、改ページ後に `content_y` に加算するかどうかを判断する
- [x] **`max_pages_limit_respected` テスト失敗**: `"<p>".repeat(200)` は HTML5 パーサが `<p>` を自動クローズするため空段落になる → `format!("<p>Paragraph {}</p>", i)` で修正

### セキュリティ修正（Phase 16 コードレビューで発見）

- [x] **`max_pages` 上限**: `ensure_space` 内で超過時に `Error::InvalidInput` を返す（信頼できない HTML 入力からの無制限ページ生成を防止）
- [x] **反復的 DFS ウォーカー**: 5000 段階のネスト `<div>` でもスタックオーバーフローなし

### テスト追加

- [x] `tests/flow.rs`（新規、11件）: `smoke_single_page`・`auto_pagination`・`heading_levels`・`key_value_table_smoke`・`empty_list_no_panic`・`ordered_and_unordered_list`・`explicit_page_break`・`custom_margins`・`cjk_paragraph_e2e`・`max_pages_limit_returns_error`・`many_table_rows_paginate`
- [x] `tests/html.rs`（新規、16件）: `basic_html`・`full_html_document`・`page_break_style_attribute`・`page_break_class`・`table_two_columns`・`unordered_list`・`ordered_list`・`japanese_html`・`all_heading_levels`・`mixed_content`・`script_and_style_skipped`・`nested_table_no_extra_rows`・`nested_list_no_duplicate_items`・`deeply_nested_divs_no_stack_overflow`・`max_pages_limit_respected`・`empty_font_bytes_error`

## テスト（Phase 16 完了時点 — 218件以上、--all-features）

| スイート | ファイル | 件数 |
|---|---|---|
| ユニット | `src/` 各モジュール内 `#[cfg(test)]` | 49 |
| インテグレーション | `tests/integration.rs` | 8 |
| スモーク | `tests/smoke.rs` | 8 |
| E2E | `tests/e2e_noto_jp.rs` | 6 |
| draw/image/パス/回転 | `tests/draw_smoke.rs` | 18 |
| ページ操作 | `tests/page_ops.rs` | 42 |
| Document::new | `tests/document_new.rs` | 6 |
| テキスト抽出 | `tests/extract_text.rs` | 17 |
| メタデータ | `tests/metadata.rs` | 5 |
| テキスト置換 | `tests/replace.rs` | 15 |
| FlowDocument | `tests/flow.rs` | 11 |
| HTML→PDF | `tests/html.rs` | 16 |
| ドキュメンテーション | `src/lib.rs`, `src/document.rs` | 17 |

## Phase 17 — extract_page_image: スキャン PDF から埋め込み画像を抽出 (完了)

- [x] `src/extract.rs`: `resolve_dict` を `pub(crate)` に昇格（`extract_image` モジュールから再利用）
- [x] `src/draw/image.rs`: `parse_jpeg_dims` を `pub(crate)` に昇格（JPEG サイズ取得のフォールバック）
- [x] `src/extract_image.rs`（新規ファイル）:
  - [x] `PageImageFormat` enum: `Jpeg` / `Png`（`#[non_exhaustive]`）
  - [x] `PageImage` 構造体: `width`・`height`・`bytes`・`format`（`#[non_exhaustive]`）
  - [x] `filter_name()` — `Name` と単一要素 `Array` の両形式に対応
  - [x] `page_image_xobjects()` — `/Resources/XObject` から `Subtype=Image` の ObjectId を収集
  - [x] `extract_xobject_image()` — DCTDecode→JPEG パススルー、FlateDecode/なし→PNG エンコード、それ以外→`InvalidInput`
  - [x] `extract_largest_image_on_page()` — 複数 XObject がある場合は `Width×Height` 最大のものを選択
- [x] `src/document.rs`: `Document::extract_page_image(page_number) -> Result<PageImage>`（`#[cfg(feature = "image")]`）
- [x] `src/lib.rs`: `mod extract_image` + `pub use extract_image::{PageImage, PageImageFormat}` を追加
- [x] `tests/extract_image.rs`（新規ファイル、5件）:
  - [x] `extract_jpeg_roundtrip` — add_image(JPEG) → save → reload → extract → JPEG magic bytes
  - [x] `extract_png_roundtrip` — add_image(PNG) → save → reload → extract → PNG magic bytes
  - [x] `extract_multiple_xobjects_returns_largest` — 2枚画像ページでエラーなく1枚返る
  - [x] `extract_no_image_returns_error` — 画像なしページ → `Error::InvalidInput`
  - [x] `extract_page_not_found` — 存在しないページ → `Error::PageNotFound`

### 設計判断

- **スキャン PDF 専用**: テキスト・ベクター PDF は Image XObject がないため `InvalidInput` を返す。フル PDF レンダリング（pdftoppm 相当）は C++ 必須の `pdfium-render` が必要なため harumi のスコープ外
- **未対応フィルタ**: `CCITTFaxDecode`・`JBIG2Decode`・`JPXDecode` は `InvalidInput` を返す（制限事項として doc comment に明記）
- **既存 fixtures 流用**: 新 fixture 追加なし — `tests/fixtures/red_1x1.jpg` / `red_1x1.png` でラウンドトリップテスト

## テスト（Phase 17 完了時点 — 223件以上、--all-features）

| スイート | ファイル | 件数 |
|---|---|---|
| ユニット | `src/` 各モジュール内 `#[cfg(test)]` | 49 |
| インテグレーション | `tests/integration.rs` | 8 |
| スモーク | `tests/smoke.rs` | 8 |
| E2E | `tests/e2e_noto_jp.rs` | 6 |
| draw/image/パス/回転 | `tests/draw_smoke.rs` | 18 |
| ページ操作 | `tests/page_ops.rs` | 42 |
| Document::new | `tests/document_new.rs` | 6 |
| テキスト抽出 | `tests/extract_text.rs` | 17 |
| メタデータ | `tests/metadata.rs` | 5 |
| テキスト置換 | `tests/replace.rs` | 15 |
| FlowDocument | `tests/flow.rs` | 11 |
| HTML→PDF | `tests/html.rs` | 16 |
| 画像抽出 | `tests/extract_image.rs` | 5 |
| ドキュメンテーション | `src/lib.rs`, `src/document.rs` | 17 |

## Phase 18 — リンクアノテーション + ブックマーク + ヘッダ/フッタ + セキュリティ修正 (完了) [v0.5.0]

### 新機能

- [x] `PageHandle::add_link_url(rect, url)` — 不可視 URI リンクアノテーションをページに追加。PDF ビューアでクリック可能。`/A /URI` アクション使用
- [x] `PageHandle::add_link_internal(rect, target_page)` — ドキュメント内ページジャンプ用内部リンクアノテーション。`/Dest [pageRef /XYZ]` 形式
- [x] `Document::add_bookmark(title, page, y)` — フラットな PDF アウトラインエントリを追加。非 ASCII タイトル（CJK 等）は UTF-16BE+BOM で自動エンコード
- [x] `HeaderFooter` 構造体 (`flow` feature) — `FlowDocument` の各ページにヘッダ/フッタを描画。左/中央/右テキストをサポート。`{{page}}`/`{{total}}` プレースホルダ置換
- [x] `FlowOptions::header` / `FlowOptions::footer` — `Option<HeaderFooter>` フィールド（デフォルト `None`）
- [x] `FlowOptions::auto_bookmarks` — `true`（デフォルト）のとき `push_heading` が自動的にブックマークを記録

### バグ修正（セキュリティ監査起因）

- [x] **`build_outlines_from_bookmarks` データ消失バグ修正** — 既存 PDF に `/Outlines` がある場合、上書きせずに末尾に追記するマージ方式に変更。回帰テスト `add_bookmark_preserves_existing_outlines_on_reload` 追加
- [x] **`set_metadata()` finalized ガード追加** — `save()` 後の呼び出しで `InvalidInput` を返す（他の変更メソッドと一致）
- [x] **`hf_measure` フォールバック修正** — `text.len()`（バイト数）→ `text.chars().count()`（文字数）。CJK テキストで右寄せ/中央寄せがズレる問題を修正
- [x] **`parse_bfrange_line` checked_add** — `dst_start + i` を `checked_add` に変更。悪意のある CMap での `u32` オーバーフローによるテキスト文字化けを防止
- [x] **`add_link_url` セキュリティ注意書き** — doc comment に `javascript:`/`data:` スキームの危険性を追記

### リファクタリング

- [x] **`find_cross_op_matches` 重複削除** — `find_cross_op_matches` と `find_cross_op_matches_preserve` を `find_cross_op_matches_inner` 共通関数に統合。~150行の重複を排除。`CrossOpMatchPreserve` 構造体を削除し `CrossOpMatch` に統合
- [x] **Clippy 警告 9件をすべて解消** — `is_multiple_of`(×5), `excessive_precision`, `needless_late_init`(×2), `too_many_arguments`

### テスト追加

- [x] `tests/annotations.rs` に回帰テスト `add_bookmark_preserves_existing_outlines_on_reload` 追加（リロード後も既存ブックマークが保持されることを検証）
- [x] `tests/flow_hf.rs` に `footer_page_number_substitution_roundtrip` 追加（2ページ文書でページ番号が正しく置換されることを `extract_text_runs` で検証）
- [x] `tests/annotations.rs` のセマンティックテスト 3 件修正（`as_string()` → `as_str()` / パターンマッチ / `doc.inner` 非公開問題を修正）

## テスト（Phase 18 完了時点 — 247件以上、--all-features）

| スイート | ファイル | 件数 |
|---|---|---|
| ユニット | `src/` 各モジュール内 `#[cfg(test)]` | 49 |
| インテグレーション | `tests/integration.rs` | 8 |
| スモーク | `tests/smoke.rs` | 8 |
| E2E | `tests/e2e_noto_jp.rs` | 6 |
| draw/image/パス/回転 | `tests/draw_smoke.rs` | 18 |
| ページ操作 | `tests/page_ops.rs` | 42 |
| Document::new | `tests/document_new.rs` | 6 |
| テキスト抽出 | `tests/extract_text.rs` | 17 |
| メタデータ | `tests/metadata.rs` | 5 |
| テキスト置換 | `tests/replace.rs` | 15 |
| FlowDocument | `tests/flow.rs` | 11 |
| HTML→PDF | `tests/html.rs` | 16 |
| 画像抽出 | `tests/extract_image.rs` | 5 |
| アノテーション/ブックマーク | `tests/annotations.rs` | 19 |
| FlowDoc ヘッダ/フッタ/ブックマーク | `tests/flow_hf.rs` | 10 |
| ドキュメンテーション | `src/lib.rs`, `src/document.rs` | 23 |

## Phase 19 — WASM demo リニューアル + CI 安定化 (完了) [v0.5.1]

### WASM demo

- [x] `wasm-demo/src/lib.rs`: `apply_annotations` WASM エクスポート追加
  - JSON アノテーション配列を受け取り harumi の draw API（`add_text` / `add_rect` / `add_line` / `add_polyline`）で PDF に適用
  - `serde` + `serde_json` で JSON デシリアライズ（WASM target でビルド確認済み）
- [x] `wasm-demo/src/lib.rs`: `Annotation` enum に `Pen` バリアント追加（`add_polyline` を使用）
- [x] `wasm-demo/www/index.html`: stamp/OCR フォームを**アノテーションエディタ**に全面刷新
  - PDF.js (v4.4.168) でブラウザ内 PDF プレビュー + PDF → キャンバス座標変換
  - 4ツール: **Text**（クリックで配置）/ **Rect**（ドラッグ・半透明）/ **Line**（ドラッグ）/ **Pen**（フリーライン）
  - ツール選択で即座に描画モード有効 — "Add" ボタン不要
  - アノテーション一覧 + 個別削除 + ページをまたいだ管理
  - 「Apply & Download」で harumi WASM に送信 → 処理済み PDF をダウンロード
- [x] Hack Regular フォントをデフォルト同梱 — フォントアップロード不要
- [x] `wasm-demo/Cargo.toml`: `harumi` に `features = ["draw"]` を追加
- [x] `wasm-demo/www/index.html`: PDF.js バージョン確認済み CDN URL (4.4.168) に修正、エラー表示 try-catch 追加

### CI 安定化

- [x] `tests/draw_smoke.rs`: `smoke_rect_and_text_same_page` の `Geneva.ttf` を早期リターンパターンに修正（Linux CI で FAILED になっていた）
- [x] `.github/workflows/pages.yml`: GitHub Pages への自動デプロイ (`pages: write` 権限、`actions/deploy-pages`)

## Phase 20 — P1/P2 feature batch (完了) [v0.6.0]

### テキスト抽出の汎用化（P1）
- [x] `src/extract.rs`: `FontInfo` に `identity_fallback: bool` フィールド追加
- [x] `is_identity_cmap()` ヘルパー追加（Identity-H/V 判定）
- [x] `collect_type0_font()`: ToUnicode なし + Identity-H の場合にフォールバックフラグを ON
- [x] `decode_chars_to_fragment()` 2バイトデコードブランチ: `to_unicode` になければ `char::from_u32(gid)` フォールバック（コントロール文字除外）
- [x] `AGL_TABLE` を 214 → ~330 エントリに拡張（中欧文字 Abreve/Cacute/Dcaron/Nacute 等・合字 ff/ffi/ffl・euro 小文字）
- [x] `src/replace.rs` のテスト内 `FontInfo` コンストラクタ更新

### 暗号化対応（P1）
- [x] `src/error.rs`: `WrongPassword` エラー追加
- [x] `src/document.rs`: `from_file_with_password` / `from_bytes_with_password` 追加（lopdf の `load_with_password` / `load_from_with_password` をラップ）
- [x] `src/document.rs`: `is_encrypted()` 追加（lopdf の `was_encrypted()` に委譲）
- [x] `src/document.rs`: `map_lopdf_password_err()` ヘルパー追加（`lopdf::Error::IO` → `harumi::Error::Io` に正しくマッピング）
- [x] `tests/encryption.rs` 新規（13件）: ユーザー/オーナーパスワード・誤パスワード・is_encrypted セマンティクス・復号後の操作・フォント埋め込み・ラウンドトリップ

### 注釈（P2）
- [x] `PageHandle::add_highlight(rect, color)` — ハイライト注釈（QuadPoints 付き）
- [x] `PageHandle::add_underline(rect, color)` — 下線注釈
- [x] `PageHandle::add_strikeout(rect, color)` — 取り消し線注釈
- [x] `PageHandle::add_sticky_note(point, contents)` — スティッキーノート（Text 注釈）
- [x] `build_markup_annot()` ヘルパー（QuadPoints 生成、Acrobat 順序）

### AcroForm（P2）
- [x] `FormField` struct、`FieldType` enum を `src/document.rs` に追加（`src/lib.rs` から再エクスポート）
- [x] `Document::form_fields() -> Result<Vec<FormField>>` — フィールドツリーを再帰的に収集
- [x] `Document::fill_form(values: &[(&str, &str)]) -> Result<usize>` — 名前でフィールドを検索して値をセット、`/NeedAppearances true` を自動設定
- [x] `collect_fields_recursive()` / `collect_field_ids_recursive()` ヘルパー（階層フィールドの走査）
- [x] `acroform_id()` ヘルパー（カタログから /AcroForm の ObjectId を取得）

### テスト（Phase 20 完了時点）

| スイート | ファイル | 件数 |
|---|---|---|
| 暗号化 | `tests/encryption.rs` | 13 |
| 上記以外は v0.5.1 から変化なし | — | — |

## Phase 21 — 書き込み暗号化・スクイグリー・ページボックス (完了) [v0.7.0]

### 書き込み暗号化
- [x] `Document` 構造体に `pending_encryption: Option<(String, String)>` フィールド追加
- [x] 全コンストラクタに `pending_encryption: None` を追加（6箇所）
- [x] `Document::set_encryption(user_pw, owner_pw)` 追加
- [x] `Document::apply_pending_encryption()` プライベートメソッド追加（`save()` / `save_to_writer()` 内で `finalize()` 後に呼ぶ）
- [x] `apply_pending_encryption()` 実装: `/ID` が未設定なら自動生成（システム時刻 + PID から LCG ミックス）、`EncryptionVersion::V2`（RC4-128）で暗号化
- [x] `generate_file_id()` ヘルパー追加
- [x] `tests/write_encryption.rs` 新規（7件）: ラウンドトリップ・誤パスワード・オーナーパスワード・空ユーザーパスワード・save後ガード・ファイル経由・コンテンツ付き

### スクイグリー注釈
- [x] `PageHandle::add_squiggly(rect, color)` 追加（`build_markup_annot(b"Squiggly", ...)` を使用）
- [x] これで PDF の全マークアップ注釈 4 種（Highlight・Underline・StrikeOut・Squiggly）が揃った

### ページボックス
- [x] `PageHandle::media_box()` / `set_media_box(rect)` 追加（MediaBox 読み書き）
- [x] `PageHandle::crop_box()` / `set_crop_box(rect)` 追加（CropBox、未設定時は `None`）
- [x] `PageHandle::trim_box()` / `set_trim_box(rect)` 追加（TrimBox）
- [x] `PageHandle::bleed_box()` / `set_bleed_box(rect)` 追加（BleedBox）
- [x] `read_page_box()` / `set_page_box()` / `parse_box_array()` ヘルパー追加
- [x] 全ボックスメソッドは `[x, y, width, height]` 形式（PDF 内部の `[x1 y1 x2 y2]` と自動変換）
- [x] `tests/page_boxes.rs` 新規（8件）: MediaBox/CropBox/TrimBox/BleedBox のラウンドトリップ・NaN ガード・未設定時 None

### テスト（Phase 21 完了時点）

| スイート | ファイル | 件数 |
|---|---|---|
| 書き込み暗号化 | `tests/write_encryption.rs` | 7 |
| ページボックス | `tests/page_boxes.rs` | 8 |

## 将来課題（v0.8 以降）

- [x] **調査済み: PyMuPDF-Utilities / font-replacement スクリプト — harumi への転用不可**
  - 実態は「真のContent Stream置き換え」ではなく「削除＋再挿入」の3フェーズ構成:
    1. Phase 1: 全ページの使用 unicode コードポイントを収集
    2. Phase 2: `cont_clean()` で対象フォントの Tf/Tj/TJ 行を削除 → `TextWriter` で新フォントを上書きレイヤーとして追記
    3. Phase 3: `indoc.subset_fonts()` でフォントサブセットを再構築
  - `subset_fonts()` は PyMuPDF 経由で MuPDF（C++）内部に委譲されており、Python コードとして参照できる実装はゼロ。
  - 直接流用できるコードはなし。`cont_clean()` のアルゴリズム（Tf 追跡 → 該当フォントの Tj/TJ/Td/Tm 行を除去）は
    `src/replace.rs` の Content Stream パーサ改良の参考にはなる。
  - re-subsetting の実装課題は解決しない。allsorts で自前実装する方針に変わりなし。（調査日: 2026-06-06）
- [x] **調査済み: Apache PDFBox — テキスト置き換えなし、TTF サブセットあり、CFF サブセットなし**
  - **テキスト置き換え**: API なし。`ReplaceURLs.java` はアノテーションの URI を書き換えるだけで
    Content Stream のテキストは一切触らない。`Overlay.java` もレイアウト保持の上書き追加のみで、`harumi-ai` の overlay mode でも必要なら短縮指示や別プロンプトで補う前提になる。
  - **TTF サブセット**: `fontbox/TTFSubsetter.java` — 純粋 Java 実装。新規埋め込み時に使用。
  - **CFF サブセット**: なし。`fontbox/cff/` パッケージは CFF パーサのみ（読み取り専用）。
  - **CJK 対応**: TTF ベースの CIDFontType2 埋め込みで日本語等に対応（`PDCIDFontType2Embedder.java`）。
    既存 PDF への追記は可能だが、テキスト置き換え機能はゼロ。
  - **ライセンス**: Apache 2.0（harumi への参考・移植に制約なし）
  - **調査日**: 2026-06-06
- [x] **調査済み: iText 5 OSS (AGPL) — CFF サブセット実装あり、テキスト置き換えは削除＋再挿入**
  - **テキスト置き換え**: `xtra/PdfCleanUpProcessor.java` — 指定矩形領域を白塗りで消去し、
    `PdfStamper` で新テキストを上書き追記。真の Content Stream 置き換えではない。
  - **TTF サブセット**: `TrueTypeFontSubSet.java` — 純粋 Java 実装。
  - **CFF サブセット**: `CFFFontSubset.java` — **純粋 Java で CFF サブセットを実装**。
    Type1/Type2 Charstring 両対応、CID フォントも対応。`Process(fontName)` → `byte[]` を返す。
    これは調査対象全 OSS の中で唯一の実用的な CFF サブセット実装。
  - **CJK 対応**: `CJKFont.java` — Adobe CMap ファイルを使い UnicodeBigUnmarked エンコーディングで
    日本語・中国語・韓国語フォントをフルサポート。
  - **ライセンス**: AGPL 3.0（商用利用には有償ライセンス必要）。
    `CFFFontSubset.java` のアルゴリズムを参考に Rust 移植することは可能だが、
    コードをそのままコピーすると AGPL が harumi に伝播するため注意。
  - **調査日**: 2026-06-06

- [x] **調査済み: CFFFontSubset.java の詳細実装（Rust 移植検討用）— 1692 行、純粋アルゴリズム**
  - **入力**: CFF フォントバイナリ全体 + `GlyphsUsed: HashMap<GID, _>`（使用グリフIDの集合）
  - **出力**: `Process(fontName) -> byte[]`（サブセット済み CFF バイナリ）
  - **処理フロー（3フェーズ）**:
    1. `BuildNewCharString()` — 使用グリフの CharStrings エントリのみ残す新 INDEX を生成。
       未使用グリフは `ENDCHAR_OP`（0x0E）1バイトに置き換え。
    2. `BuildNewLGSubrs()` — Charstring バイトコードをトレースして実際に呼ばれる
       Local/Global サブルーチンを特定し、再帰的に依存チェーン全体を収集。
       未使用サブルーチンは `RETURN_OP`（0x0B）に置き換え。
    3. `BuildNewFile()` — CFF 全体を再アセンブル。`LinkedList<Item>` に各パーツを追加し
       オフセット参照（前方参照）を xref で解決してから一括 emit。
  - **CID フォント対応**:
    - FDSelect を読み FDArray のどのエントリが使われるかを判定（`BuildFDArrayUsed`）
    - 使われる FD 毎に Local Subr を個別にサブセット
    - 非 CID フォントは CID 形式に変換してから出力（`CreateFDSelect`/`CreateFDArray`）
  - **Type2 Charstring デコーダ（`ReadCommand`）**:
    - 整数エンコーディング: b0=28（2バイト short）、32–246（1バイト）、247–250/251–254（2バイト）、255（4バイト）
    - オペレータ: b0≤31 → `SubrsFunctions[]` 参照、b0=12 → `SubrsEscapeFuncs[]`（2バイトエスケープ）
    - `callsubr`/`callgsubr` を検出して再帰的にサブルーチン依存を追跡
    - `hstem`/`vstem`/`hintmask`/`cntrmask` のヒント数も追跡（マスクバイト幅計算に必要）
  - **Rust 移植の現実性**:
    - アルゴリズム自体は Java 固有の機能に依存しておらず、Rust への移植は技術的に可能。
    - `allsorts` が CFF パーサを内包しているが、低レベルの FDArray/Subr オフセット情報を
      外部から触れるかが鍵。触れない場合は CFF パーサ部分も独自実装が必要。
    - `LinkedList<Item>` の前方参照パターンは Rust では `Vec<Box<dyn Item>>` + 2パスで実現可能。
    - 1692 行のうち実装コアは約 800 行。Rust 換算で 600–900 行規模の実装になる見込み。
  - **AGPL ライセンス上の注意**:
    アルゴリズムの理解・参考は合法。コードの直接コピー・派生物扱いになる移植は
    harumi を AGPL にするか有償ライセンス購入が必要。独立実装（クリーンルーム）は問題なし。
- [x] **調査: lopdf + allsorts を組み合わせてサブセット更新を実装した既存 OSS が存在するか**
  - harumi は既に両クレートを依存に持つ。同じ組み合わせで re-subsetting まで踏み込んだ
    プロジェクト・ブログ記事・crate が存在すれば実装の参考になる。
  - 検索キーワード候補: `lopdf allsorts subsetting`, `rust pdf font resubset`, `allsorts pdf replacement`
- [x] **replace_text のフォントサブセット再構築（re-subsetting）— 実装済み (2026-06-06)**
  - `PageHandle::replace_text_resubset(old_text, new_text, font_bytes)` を追加
  - `src/resubset.rs` に再サブセット・全ページ GID 再エンコード・preserve_font 置換の 3 パスを実装
  - CIDFontType2 (Identity-H / CIDToGIDMap=Identity) 対応。Type1/simple font は `InvalidInput`
  - テスト 3 件追加: `replace_text_resubset_no_match`, `replace_text_resubset_same_chars`,
    `replace_text_resubset_new_char_expanded`（日本語への拡張含む）
- [x] FlowDocument インラインテキストスタイル（太字/斜体/色指定 span）— 実装済み (2026-06-06)
  - `InlineSpan { text, bold, italic, color }` + `FlowDocument::push_paragraph_styled(spans)`
  - Bold: PDF render mode 2 (fill+stroke), stroke_width = font_size * 0.04
  - Italic: 12° 水平シアーの Tm テキスト行列（合成的 / 追加フォント不要）
- [x] HTML レンダリング品質向上（bold/italic/color/link の実装）— 実装済み (2026-06-06)
  - `<strong>/<b>` → bold, `<em>/<i>` → italic
  - `<span style="color: #RRGGBB">` → 色指定
  - `<a href="...">` → 青 (0, 0, 0.8) リンク色
- [x] TTC E2E テスト — 実装済み (2026-06-06)
  - tests/e2e_ttc.rs に 5 件: make_ttc() でオフセット補正済み 2-face TTC を合成して検証、フィクスチャ不要
- [x] WASM ビルド確認 (wasm-pack test --node) — 実装済み (2026-06-06)
  - tests/wasm_smoke.rs: #[wasm_bindgen_test] で Document::new + save_to_bytes を検証
  - CI wasm-test ジョブに追加
- [x] remove_page / insert_blank_page / reorder_pages がネスト /Pages ツリーを平坦化する問題 — 修正済み (2026-06-06)
  - `realize_page_inherited_attrs()` ヘルパー追加: /Parent 変更前に MediaBox/CropBox/Rotate/Resources/UserUnit を継承元から page dict に実体化
  - 回帰テスト 3 件追加 (`nested_pages_remove_page_preserves_mediabox` 他)
- [x] `cargo semver-checks` を CI に追加（破壊的変更の早期検出）— `.github/workflows/ci.yml` に `semver` ジョブ追加 (2026-06-06)
- [x] AES-256 暗号化オプション — 実装済み (2026-06-06)
  - Document::set_encryption_aes256(user_pw, owner_pw) 追加
  - 32バイト鍵を getrandom::fill() で生成（OS RNG、フォールバックなし）
  - lopdf EncryptionVersion::V5 (AES-256-CBC, /V 5 /R 6 /StdCF AESV3)
  - テスト 7 件追加（tests/write_encryption.rs）、/Encrypt dict の V=5 R=6 確認含む

## Phase 22 — CMYK カラー対応 (完了) [v1.0.0]

### 破壊的変更：RGB-only の `[f32; 3]` → `Color` enum に統合

- [x] `Color` enum 定義: `Color::Rgb([f32; 3])` / `Color::Cmyk([f32; 4])`
- [x] `From<[f32; 3]> for Color` / `From<[f32; 4]> for Color` 実装（既存呼び出しの互換性維持）
- [x] `PendingText.color` / `TextRun.color` を `Color` 型に変更
- [x] `DrawOp` 全バリアントのカラー引数を `Color` に変更
- [x] `src/draw/shapes.rs` と `src/content/text.rs` に `color_fill()` / `color_stroke()` 分岐（RGB: `rg`/`RG`, CMYK: `k`/`K`）
- [x] 全公開メソッドシグネチャを `impl Into<Color>` パターンに更新（50+ 箇所）
- [x] テスト更新: `Color::Rgb()` ラッパーまたは `.into()` で対応
- [x] Cargo.toml version 1.0.0 に更新（semver-checks で意図的な破壊的変更を検出）

## Phase 23 — デジタル署名検証 (完了) [v1.1.0]

### PKCS#7 署名メタデータ抽出 + optional feature gate

- [x] `digital-signature` feature 追加（`cms`, `x509-cert`, `sha2`, `rsa`, `der` を optional deps）
- [x] `SignatureInfo` 構造体: `field_name`, `signer_name`, `signing_time`, `is_valid`, `reason`（`#[non_exhaustive]`）
- [x] `Document::verify_signatures(&pdf_bytes) -> Result<Vec<SignatureInfo>>` 実装
  - [x] AcroForm から Sig フィールドを自動検出
  - [x] メタデータ抽出：/T (field_name), /Reason, /M (signing_time)
  - [x] ByteRange 自動解析（4 整数配列）
  - [x] PKCS#7 DER パース（sig contents）
- [x] 機能限定：メタデータ抽出のみ（full cryptographic validation は TODO）
  - [x] X.509 証明書チェーン検証未実装
  - [x] RSA/ECDSA 署名検証未実装 → 常に `is_valid = false` を返す
- [x] feature gate なし時: 空 Vec を返す stub 実装
- [x] テスト: 無署名 PDF → 空 Vec 確認

## Phase 24 — テキスト抽出品質強化 (完了) [v1.4.0]

### 実装済み項目

- [x] `uni<XXXX>` / `u<XXXX>` グリフ名パターン対応（AGL 2.0準拠）
  - [x] `glyph_name_to_char()` を u32::from_str_radix で実装
  - [x] hex string 長さ検証 (1-8 chars)
  - [x] テスト: `uni_glyph_name_pattern_decoding`

- [x] 読み取り順ソートヘルパー関数
  - [x] `sort_by_reading_order()` public API として実装 (src/extract.rs:118)
  - [x] NaN/Infinity 座標に対する defensive programming
  - [x] テスト: `sort_by_reading_order_top_to_bottom_left_to_right`

- [x] 全画像抽出 `extract_page_images()`
  - [x] 既に実装済み (src/document.rs:1420)
  - [x] テスト: `extract_all_images_returns_both`

### 延期項目（v1.5以降）

- [x] Form XObject（`Do`演算子）内テキストの再帰抽出 — 実装済み (2026-06-15, Phase B-2)
- [x] 複数コンテントストリーム間でのグラフィック状態（color/render-mode）継承 — `ParseCarryState` 実装済み (2026-06-15, Phase B-3)
- [ ] `usecmap` 指令（CMap委譲）の解決（legacy PDF対応、ROI低い）

## Phase 25 — デジタル署名作成 + 検証 (完了) [v1.2.2]

### 破壊的変更なし：`digital-signature` feature 拡張

#### 署名作成フロー
- [x] `Document::add_signature_field(page, rect, options)` — AcroForm に Sig フィールド追加（/Reason, /ContactInfo 保存）
- [x] `Document::sign_document(context, field_name) -> Result<Vec<u8>>` — 署名済み PDF バイトを返す

#### 核となるコンポーネント（src/）
- [x] `signature_create.rs`: `SigningContext`, `CertificateInput`, `PrivateKeyInput` enums
  - [x] PEM/DER 証明書・秘密鍵パース（base64 デコード＋DER 解析）
  - [x] X.509 CN 抽出（naive DER parsing）
  - [x] `hash_pdf_content()` — SHA-256 ハッシング
  - [x] `hash_pdf_content_with_byte_range()` — PDF spec per ByteRange ハッシング（署名検証用）
  - [x] `sign_hash()` — **実 RSA PKCS#1 v1.5 署名** (num-bigint via modpow)
    - DigestInfo (SHA-256 OID + hash) を DER エンコード
    - PKCS#1 v1.5 padding: `0x00 || 0x01 || 0xFF... || 0x00 || DigestInfo`
    - RSA: `signature = padded^d mod n` (num-bigint でビッグ整数演算)

- [x] `cms_builder.rs`: PKCS#7/CMS SignedData 構造体生成
  - [x] 改善版 PKCS#7: ContentInfo ラッパー + SignedData（v3）+ OID 完備
  - [x] DigestAlgorithmIdentifier (SHA-256 with NULL params)
  - [x] SignerInfo (version 1 + digest alg + enc alg + signature)
  - [x] CertificateSet [0] に X.509 証明書埋め込み
  - [x] 正しい DER 長さエンコーディング（short + long form）

- [x] `pdf_incremental.rs`: PDF incremental update 実装
  - [x] ByteRange 計算（PDF spec per `[start1, length1, start2, length2]`）
  - [x] xref テーブル生成（署名オブジェクト参照）
  - [x] startxref オフセット（計算値を出力）
  - [x] メタデータ付き署名 dict (/Name signer, /M timestamp)

- [x] `signature.rs` (既存, 拡張): RSA 署名検証実装
  - [x] `/Contents` hex デコード
  - [x] PKCS#7 デコード (簡略版: [0] 証明書 + OCTET STRING 署名抽出)
  - [x] X.509 から RSA 公開鍵抽出 (modulus, exponent 抽出)
  - [x] ByteRange ハッシング
  - [x] DigestInfo 再構築 + PKCS#1 v1.5 padding
  - [x] RSA 検証: `decrypted = signature^e mod n`
  - [x] `decrypted == padded_expected` → `is_valid = true` 返す

#### 設定・依存性
- [x] `Cargo.toml`: `digital-signature` feature に sha2, rsa, **num-bigint** 追加
- [x] `.github/workflows/ci.yml`: `cargo check --features digital-signature` を追加

#### テスト (4 + ドックテスト)
- [x] `tests/digital_signature_create.rs`:
  - [x] `test_signing_context_creation` — cert/key パース
  - [x] `test_add_signature_field` — AcroForm に Sig フィールド追加
  - [x] `test_sign_document_basic` — 署名済み PDF 生成
  - [x] `test_sign_document_with_content` — コンテント付き PDF 署名

#### 既知の制限事項（v1.2.3以降）
- [ ] PKCS#7 パース: 完全な DER デコーダ実装なし（簡略ヒューリスティック）
- [ ] 複数署名: 署名は1つのみサポート（hardcoded object 1）
- [ ] タイムスタンプ: RFC 3161 未実装（固定値 "D:202406121200Z"）
- [ ] 中間 CA 検証: 証明書チェーン検証なし（単一証明書のみ検証）

### Bug fixes（v1.2.2）
- [x] BUG #1: startxref placeholder → 計算済みオフセット値
- [x] BUG #2: ByteRange 計算 → PDF spec [0, length1, start2, length2]
- [x] BUG #3: ダミー署名 → **実 RSA PKCS#1 v1.5 signing** (num-bigint)
- [x] BUG #4: PKCS#7 structure → OID + digestAlg + signerInfo 完備
- [x] BUG #5: 常時 true verification → **実 RSA 検証** (modpow by public key)
- [x] BUG #6: xref テーブル → 署名オブジェクト参照
- [x] BUG #7: hardcoded obj 1 → xref でサポート
- [x] BUG #8: 全 PDF ハッシング → **ByteRange per spec**
- [x] BUG #9: メタデータなし → /Name + /M 追加
- [x] BUG #10: オフセット計算 → 正確な xref_offset

## テスト（Phase 25 完了時点 — 227+ 件、--all-features）

| スイート | ファイル | 件数 |
|---|---|---|
| ユニット | `src/` 各モジュール内 `#[cfg(test)]` | 49 |
| デジタル署名 | `tests/digital_signature_create.rs` | 4 |
| その他（Phase 1–24 から変化なし） | — | 174+ |

### セキュリティ修正（同時実装）

- [x] `group_into_lines()` — 負の font_size に対する defensive check
- [x] `estimate_baseline_font_size()` — NaN/Infinity フォントサイズをフィルタ
- [x] `classify_by_ratio()` — NaN ratio に対する是正
- [x] `sort_by_reading_order()` — NaN/Infinity 座標の安全な処理

---

## Phase 25 — AI/RAG ユーティリティ (完了) [v1.5.0]

### 実装済み

- [x] `TextChunk` / `ChunkType` 構造体 (src/chunk.rs)
  - [x] ChunkType::Heading(u8) / Paragraph
  - [x] #[non_exhaustive] attribute

- [x] `Document::extract_text_chunks()` — セマンティックテキストブロック抽出
  - [x] テキストグループ化（y座標容認値 ±font_size*0.5）
  - [x] Baseline 推定（最小フォントサイズ）
  - [x] 見出し分類（1.8×,1.5×,1.3×,1.15×の閾値）
  - [x] 連続同型チャンク統合
  - [x] Bounding box 計算

- [x] `Document::extract_as_markdown()` — Markdown 出力
  - [x] 見出しマークアップ自動生成
  - [x] 行の連結とホワイトスペース処理

### テスト

- [x] 7 integration tests in tests/chunk.rs
  - empty_page, paragraphs_only, heading_and_paragraph, bbox_valid, filters_invisible, markdown_basic, markdown_empty

### 関連ライブラリとの比較
- unpdf との差分: Phase 24で5項目中3項目をカバー
- oxidize-pdf との差分: TextChunk / extract_as_markdown で基本的なチャンキング・構造化出力を実装

---

## Phase 26 — 依存削減：ゼロ依存を目標 (完了)

### 実装済み項目

- [x] **5つの digital-signature crypto dependencies を削除**
  - [x] `cms`, `x509-cert`, `sha2`, `rsa`, `der` から Cargo.toml を削除
  - [x] `SignatureInfo`, `verify_signatures` を常時コンパイル（stub 実装）
  - [x] `digital-signature` feature 削除（常時 API 利用可能）
  - [x] 影響: 15 transitive deps 削減

- [x] **`thiserror` proc-macro を手動実装に置換**
  - [x] Error 型に `Display` と `std::error::Error` トレイトを手動実装
  - [x] `From<std::io::Error>` / `From<lopdf::Error>` を手動実装
  - [x] thiserror クレート削除、4つの proc-macro transitive deps 削減

- [x] **`image` クレートを `png` クレートで置換**
  - [x] PNG デコード: `image::ImageReader` → `png::Decoder`
  - [x] PNG エンコード: `image::Encoder` → `png::Encoder`
  - [x] JPEG パース: 既に手動実装済み（`parse_jpeg_dims()`）
  - [x] image feature 保持（実装API は変わらず）
  - [x] 影響: 10+ transitive deps 削減（jpeg/zune など）

- [x] **`scraper` を内製 HTML トークナイザで置換**
  - [x] `src/flow/html_tokenizer.rs` 新規ファイル（~400行）
  - [x] `HtmlNode` enum: `Text` / `Element { tag, attrs, children }`
  - [x] `parse_html()` 関数: iterative stack-based parser
  - [x] 機能: タグ・属性・セルフクローズ・HTML entity・コメント対応
  - [x] スタックオーバーフロー防止: 5000段階ネスト `<div>` でも動作
  - [x] 既存 HTML テスト 19件すべてパス、page-break-style-attribute バグ修正
  - [x] scraper + 12 transitive deps 削減

### 削減効果（合計）

| 項目 | 削除直接 deps | 削減 transitive deps |
|------|-------------|---------------------|
| digital-signature 5依存 | 5 | ~15 |
| thiserror + proc-macros | 1 (+4 proc-macro) | ~5 |
| image → png | 1 → 1（入替） | ~10 |
| scraper → 内製 | 1 | ~12 |
| **合計** | **-8 直接** | **~40 transitive** |

**デフォルトビルド結果**: `getrandom` / `lopdf` / `subsetter` / `ttf-parser` の 4依存のみ

### テスト確認

- [x] `cargo test --all-features` — 378+ テスト、すべてパス
- [x] `cargo test --features html,flow` — 19 HTML テスト、すべてパス
- [x] `cargo clippy --all-features -- -D warnings` — 0警告

---

## 将来課題 Phase 2（v0.9 以降）

### 高優先度

- [ ] **全画像抽出 `extract_all_images(page)`** — 現在は最大サイズ1枚のみ返す。`Vec<PageImage>` を返すAPIを追加。スキャン PDF 処理で複数画像が必要なケースに対応（難度: 小）
- [ ] **AcroForm フィールド作成** — 現状は既存 PDF への読み取り・入力のみ。テキストフィールド・チェックボックス・ラジオボタン等を新規 PDF に追加する API（難度: 中）
- [ ] **FlowDocument 複数フォント** — 現在は全ページ同一フォント。`FlowOptions` に `heading_font_bytes` / `code_font_bytes` 等を追加し、見出し・コードブロックに別フォントを適用（難度: 中）

---

## Phase 27 — 依存削減 Round 2：subsetter → 内製 TTF サブセッタ (完了)

### 実装済み項目

- [x] **内製 TTF サブセッタで `subsetter` クレートを置換**
  - [x] `src/font/ttf_subset.rs` 新規ファイル（~460行）
  - [x] `GlyphRemapper` 構造体: 元GIDセットを保持、旧→新GID変換を提供
  - [x] `subset()` 関数: フォントバイト + face_index + remapper → サブセットバイト列
  - [x] フォント種別対応: TTF（TrueType）+ TTC（TrueType Collection）
  - [x] コンポジットグリフの再帰的依存解決（GID dedup via BTreeSet）
  - [x] loca フォーマット自動判定・再構築（short vs long）
  - [x] 全テーブル再構築：glyf / loca / hmtx / head / hhea / maxp + optional tables

- [x] **TTC (TrueType Collection) サポート修正**
  - [x] TTC ファイルの絶対オフセット解釈：`parse_table_records_raw()` でオフセット生値を抽出
  - [x] オフセット検証：TTC ファイル開始（offset 0）からの絶対位置として解釈
  - [x] バグ修正：table vmtx out of bounds → all 5 TTC テスト パス

- [x] **削減効果**
  - [x] `subsetter` クレート削除：1直接依存
  - [x] transitive deps 削減：~12（`kurbo` `skrifa` `write-fonts` `rustc-hash` 他）
  - [x] デフォルトビルド依存：4 → 3（getrandom / lopdf / ttf-parser）

### テスト・検証

- [x] Unit tests: 44 件すべてパス
- [x] Integration tests: 8 件すべてパス
- [x] E2E TTC tests: 5/5 パス（TTF embedded font smoke 含む）
- [x] `cargo clippy --all-features -- -D warnings` — 0警告

### コミット

- [x] feat: implement internal TTF subsetter to replace subsetter crate (commit: fbe4f4c)
- [x] fix: TTC offset handling — correctly interpret absolute table offsets (commit: 47f4445)

---

## Phase 28 — MCP 翻訳ワークフローとフォントマップ診断 (完了)

### 実装・検証済み項目

- [x] **SDS PDF のレイアウト保持英訳を `harumi-mcp` で実施**
  - [x] `pdf_page_info` で `test_documents/kanto_chemical/J_10005.pdf` が4ページとして読めることを確認
  - [x] `pdf_extract_all_pages` で全ページのテキスト断片と位置を抽出
  - [x] `pdf_replace_text` の `mode: "new_font"` で `J_10005_en.pdf` を生成
  - [x] 翻訳置換リストを `test_documents/kanto_chemical/J_10005_en_replacements.json` として保存
  - [x] 抽出テキスト上、意味のある日本語が残っていないことを確認

- [x] **非Identity `CIDToGIDMap` のエラー診断改善**
  - [x] `wrap` / `resubset` が保存時に `CIDToGIDMap=Identity` 制約で失敗した場合、`FILE_WRITE_ERROR` ではなく `UNSUPPORTED_FONT_MAP` を返す
  - [x] エラー文に原因（非Identity `CIDToGIDMap`）と回避策（`mode: "new_font"` + Unicode TTF）を含める
  - [x] `replace_save_error` のユニットテストを追加

### ドキュメント

- [x] README 4言語（英・日・中・韓）に MCP `pdf_extract_all_pages` / `pdf_replace_text` と翻訳ワークフローを追記
- [x] README 4言語に `replace_text_resubset` の `CIDToGIDMap /Identity` 制約と `new_font` 回避策を追記
- [x] `tasks/lessons.md` に非Identity `CIDToGIDMap` とMCPエラー設計の教訓を追記

### テスト・検証

- [x] `cargo fmt`
- [x] `cargo test -p harumi-mcp` — 19 tests passed
- [x] `cargo build -p harumi-mcp --release`

---

## Phase 27 — Digital Signature Creation Foundation (進捗中) [v1.2.0]

### 署名作成フレームワーク（Phase 1-2 完了）

- [x] `digital-signature` feature flag 追加（`sha2`, `rsa`, `pkcs1`, `pkcs8`, `x509-cert`, `der`, `cms`, `rand`, `zeroize` を optional deps）
- [x] `src/signature_create.rs` 新規（証明書・キー管理）
  - [x] `CertificateInput` enum (PEM/DER)
  - [x] `PrivateKeyInput` enum (PEM/DER)
  - [x] `SignatureFieldOptions` struct
  - [x] `SigningContext` — 証明書・秘密鍵・署名者名を管理
  - [x] PEM → DER パーサ（base64デコード）
  - [x] X.509 CN 抽出（基本的なDER解析）
  - [x] SHA-256 ハッシング
  - [x] RSA PKCS#1 v1.5 署名生成
- [x] `src/document.rs` に署名API追加
  - [x] `Document::add_signature_field(page, rect, options)` — 署名フィールドを AcroForm に追加
  - [x] `Document::sign_document(context, field_name)` — 文書をハッシング＆署名生成
    - v1.2.0 では PDF にまだ署名を埋め込まない（スタブ実装）
- [x] `src/error.rs` に署名エラー追加（feature gate 付き）
- [x] `tests/digital_signature_create.rs` 統合テスト（基本的なテスト通過、rcgen互換性は v1.2.1 で解決予定）

### テスト（Phase 27 完了予定時点）

| スイート | ファイル | 件数 |
|---|---|---|
| デジタル署名 | `tests/digital_signature_create.rs` | 4 (1 passing, 3 待機中) |

### 延期項目（v1.2.1+）

- [ ] **PDF incremental update** — ByteRange 計算 + PKCS#7 SignedData 埋め込み
- [ ] **完全な PKCS#7/CMS 実装** —署名を PDF に実装として埋め込む
- [ ] **RFC 3161 タイムスタンプ** — TSA(Time Stamping Authority) 統合
- [ ] **Appearance stream** — 署名フィールドの視覚的表現
- [ ] **PKCS#12 サポート** — .p12 ファイルから直接読み込み
- [ ] **署名検証（cryptographic）** — verify_signatures の is_valid フラグを有効化

---

---

## バグ修正 (v1.3.1) — 2026-06-14

### embed_font + add_text の ● 表示修正（macOS Preview / PSPDFKit）

- [x] **根本原因の特定**: 内製 TTF サブセッターがオプションテーブルを verbatim でコピー
  - NotoSansJP-Regular.ttf は GSUB / GPOS / gvar / fvar / avar / HVAR / STAT / post /
    vhea / vmtx など 23 テーブルを持つ可変フォント
  - サブセット後もこれらのテーブルが元フォントの全 GID（7000+）への参照を保持
  - macOS Core Text がこの不整合を検出してフォントを拒否 → 全グリフ ● 表示
- [x] **Fix 1（主修正）**: テーブル除外をホワイトリスト方式に変更（`src/font/ttf_subset.rs`）
  - コア TrueType（head/hhea/maxp/glyf/loca/hmtx）とヒンティング（fpgm/prep/cvt/gasp）のみ保持
  - GSUB/GPOS/GDEF/BASE/gvar/fvar/avar/HVAR/STAT/post/vhea/vmtx/kern/name 等はすべて除外
- [x] **Fix 2（副修正）**: コンポジットグリフのコンポーネント GID 書き換え（`build_glyf`）
  - `rewrite_composite_gids()` 追加：コンポーネント GID を新位置に更新
- [x] **Fix 3（副修正）**: GID→文字マッピングの正確化（`src/font/subset.rs`）
  - `GlyphRemapper.get()` を廃止、`gids_to_keep`（コンポジット依存含む）から直接計算
  - `subset()` が `(Vec<u8>, BTreeSet<u16>)` を返すよう変更
- [x] **回帰テスト追加**: `subset_font_excludes_tables_with_stale_gid_refs`（`tests/e2e_noto_jp.rs`）
  - "Hello World" + NotoSansJP で PDF 生成し埋め込みフォントに GSUB/GPOS/gvar 等がないことを検証
  - maxp.numGlyphs が小さなサブセットサイズになっていることを確認

### 中優先度

- [ ] **PDF/A 準拠出力** — 対象ユーザー: 長期保存・官公庁PDF。ICC プロファイル埋め込み・XMP メタデータ等が必要（難度: 大）
- [ ] **矩形領域 Redaction（完全除去）** — 対象ユーザー: 機密情報処理。Content Stream から対象領域のテキスト・画像を完全削除（難度: 大）
- [ ] **真の署名検証（RSA/ECDSA）** — verify_signatures での暗号学的署名検証。PKCS#7署名を完全に検証する（難度: 大）
- [ ] **RTL テキスト（右から左書き）** — 対象ユーザー: アラビア語・ヘブライ語。Unicode Bidi アルゴリズム対応が必要（難度: 大）

---

## Phase 29 — Stirling-PDF 対応: コンテンツスケール・PDF オーバーレイ・ブックマーク削除・ファイル添付 (完了) [v1.4.0]

### 背景

stirling-all-docs（Stirling-PDF の Rust 移植）が harumi 1.3.x を利用した際にブロックされた機能群への対応。
P1〜P3 の要望を精査し、pure Rust の範囲で実装可能なものを v1.4.0 として実装。

### 実装済み項目

- [x] `[package.metadata.docs.rs]` を Cargo.toml に追加 → `draw` / `image` / `ocr` / `flow` / `html` / `digital-signature` 全フィーチャーで docs.rs をビルド
  - 根本原因: `add_text_with_opacity` と `add_text_with_rotation` が `#[cfg(feature = "draw")]` impl ブロック内にあり、default features では docs.rs に表示されなかった

- [x] **`PageHandle::scale_page_content(scale_x, scale_y)`**（P1-D）
  - PDF `cm`（Concatenate Matrix）演算子を既存コンテンツストリームの先頭に新ストリームとして挿入
  - `prepend_to_contents()` ヘルパー追加（`append_to_contents` の逆方向版）
  - 正値・有限値チェック、finalized ガード

- [x] **`PageHandle::resize_page_with_content(new_width, new_height)`**（P1-D 便利メソッド）
  - 現サイズから比率を計算 → `scale_page_content` + `set_media_box` + CropBox 除去

- [x] **`Document::overlay_from(other)`**（P2-A）
  - other の全オブジェクトを renumber して self に取り込み（`merge_from` と同方式）
  - 各ページの content bytes（decoded + 連結）・/Resources（親チェーン継承対応）・BBox を取得
  - Form XObject を生成して self の対応ページの /Resources /XObject に登録
  - `Do` 演算子を既存コンテンツの末尾に追加
  - `inherited_media_box_raw()` / `inherited_resources()` ヘルパー追加

- [x] **`Document::clear_outline()`**（P3-B partial）
  - `pending_bookmarks` をクリア + カタログの `/Outlines` エントリを削除

- [x] **`Document::attach_file(filename, data, mime_type)`**（P3-A）
  - EmbeddedFile ストリーム（FlateDecode 圧縮、`/Params /Size` 記録）を生成
  - Filespec dict（`/Type /Filespec`、`/F`、`/UF` UTF-16BE、`/EF`）を生成
  - `/Catalog /Names /EmbeddedFiles /Names` 配列に追加（PDF spec に従い名前順にソート）

- [x] **`Document::list_attachments()` → `Vec<AttachmentInfo>`**（P3-A）
  - フラット `/Names` 配列を走査して `filename`・`size`・`mime_type` を返す

- [x] **`AttachmentInfo` 構造体**（`#[non_exhaustive]`、`lib.rs` から再エクスポート）

- [x] **CHANGELOG.md 修正**（P4-C）
  - v0.4.1 の `data` フィールド記述 → `bytes` に修正

- [x] **`FontHandle` doc comment 改善**（P4-B）
  - `FontHandle is Copy` を明示的に記載

- [x] **ヘルパー関数整理**
  - `with_resources_dict_mut` / `add_xobject_to_resources` の `#[cfg]` ゲートを削除
    （feature に依存しない lopdf 操作のみ。overlay_from から利用するために必要）

### スコープ外（理由付き）

| 要望 | スコープ外の理由 |
|------|----------------|
| PDF ラスタライズ（P1-C） | 完全な PDF レンダリングエンジンが必要。スキャン PDF なら既存 `extract_page_images` で代替可 |
| フォーム visual flatten（P2-B） | Appearance stream のパースと content stream への変換が必要（PDF インタープリタ不在） |
| N-up / ブックレット（P2-C） | `overlay_from` が安定したら次フェーズで検討可能 |
| `page.size()` infallible 化（P4-A） | MediaBox が存在しない PDF は実在するため `Result` が正しい |

### テスト追加 (`tests/overlay_scale_attach.rs` — 15 件)

- [x] `scale_page_content_returns_ok`
- [x] `scale_page_content_rejects_zero_scale`
- [x] `scale_page_content_rejects_negative_scale`
- [x] `scale_page_content_rejects_nan`
- [x] `resize_page_with_content_changes_media_box`
- [x] `overlay_from_produces_valid_pdf`（OVRL0 XObject + Do の存在を lopdf で検証）
- [x] `overlay_from_inherited_resources`（/Pages ノードに /Resources がある構造でも動作確認）
- [x] `overlay_from_pending_ops_returns_err`
- [x] `clear_outline_removes_pending_bookmarks`
- [x] `attach_and_list_single_file`
- [x] `attach_multiple_files`
- [x] `attach_files_sorted_in_names_array`（逆順追加でも /Names 配列が昇順ソートされることを確認）
- [x] `attach_file_empty_filename_returns_err`
- [x] `attach_file_without_mime_type`
- [x] `list_attachments_on_clean_pdf_returns_empty`

## テスト（Phase 29 完了時点 — 288件、デフォルト features）

| スイート | ファイル | 件数 |
|---|---|---|
| ユニット | `src/` 各モジュール内 `#[cfg(test)]` | 44 |
| インテグレーション | `tests/integration.rs` | 8 |
| スモーク | `tests/smoke.rs` | 8 |
| E2E | `tests/e2e_noto_jp.rs` | 6 |
| draw/image/パス/回転 | `tests/draw_smoke.rs` | 18 |
| ページ操作 | `tests/page_ops.rs` | 42 |
| Document::new | `tests/document_new.rs` | 6 |
| テキスト抽出 | `tests/extract_text.rs` | 22 |
| メタデータ | `tests/metadata.rs` | 5 |
| テキスト置換 | `tests/replace.rs` | 22 |
| FlowDocument | `tests/flow.rs` | 11 |
| HTML→PDF | `tests/html.rs` | 19 |
| 画像抽出 | `tests/extract_image.rs` | 8 |
| アノテーション/ブックマーク | `tests/annotations.rs` | 19 |
| FlowDoc ヘッダ/フッタ | `tests/flow_hf.rs` | 10 |
| AcroForm 作成 | `tests/acroform_create.rs` | 7 |
| 暗号化（読込） | `tests/encryption.rs` | 13 |
| 暗号化（書込） | `tests/write_encryption.rs` | 14 |
| ページボックス | `tests/page_boxes.rs` | 8 |
| TTC | `tests/e2e_ttc.rs` | 5 |
| デジタル署名 | `tests/digital_signature_create.rs` | 4 |
| セマンティックチャンク | `tests/chunk.rs` | 7 |
| オーバーレイ/スケール/添付 | `tests/overlay_scale_attach.rs` | 15 |
| ドキュメンテーション | `src/lib.rs`, `src/document.rs` | 24 |

## Phase 30 — TextFragment フォント属性 + detect_text_columns (完了) [v1.4.1]

### TextFragment フィールド追加 (`src/extract.rs`)

- [x] `TextFragment::is_bold: bool` — PostScript フォント名から Bold/Heavy/Black/Semibold/Demibold/Extrabold キーワードで検出
- [x] `TextFragment::is_italic: bool` — Italic/Oblique/Slanted キーワードで検出
- [x] `TextFragment::font_family: String` — サブセットプレフィックス（`"ABCDEF+"`）とスタイルサフィックスを除いた基本ファミリー名
- [x] `TextFragment::base_font: String` — サブセットプレフィックスのみ除いた PostScript 完全名（例: `"Helvetica-BoldOblique"`）
- [x] `parse_font_attributes()` ヘルパー追加（PostScript 名 → (base_font, is_bold, is_italic, font_family) タプル）
- [x] `FontInfo` に同フィールドを追加し `collect_type0_font` / `collect_simple_font` で設定
- [x] `decode_chars_to_fragment` でフラグメントに転記

### detect_text_columns + ColumnZone (`src/extract.rs`, `src/lib.rs`)

- [x] `ColumnZone { x_start: f32, x_end: f32 }` 構造体（`#[non_exhaustive]`）
- [x] `detect_text_columns(fragments: &[TextFragment], page_width: f32) -> Vec<ColumnZone>`
  - 1pt 幅のバケツに X 密度ヒストグラムを構築
  - 15pt 以上の連続空白ギャップを段区切りとして検出
  - 単一段（ギャップなし）の場合はページ幅全体の `ColumnZone` 1件を返す
- [x] `src/lib.rs` から `detect_text_columns` / `ColumnZone` を再エクスポート

### ドキュメント更新

- [x] CHANGELOG.md に v1.4.1 セクション追加
- [x] README.md（英・日・中・韓）の "What you get" テーブルと Roadmap に追記
- [x] Cargo.toml バージョン 1.4.0 → 1.4.1

---

## Phase 31 — harumi-ai Overlay モード レイアウト精度向上 (完了)

### 修正対象ファイル

`harumi-ai/src/overlay.rs` 1ファイルのみ

### 実装済み項目

- [x] **Fix 1: 白矩形の高さ**（Phase 4 白矩形ループ）
  - 変更前: `let h = body_fs * 1.3 + below;`（全ページ中央値の固定倍率）
  - 変更後: `let h = line.line_height + below;`（行間隔から計算済み per-line 値）

- [x] **Fix 2: 白矩形ディセンダー被覆 + 訳文 Y 座標**
  - `descender_ratio` を `ttf-parser` の実データから計算（`-face.descender() / face.units_per_em()`、clamp 0.05–0.35）
  - `below = body_fs * descender_ratio`（旧: `body_fs * 0.08` 固定値）
  - 訳文配置: `[line.x, line.y]`（旧: `line.y - scaled * 0.1` の恣意的ずらし）

- [x] **Fix 3: 白矩形の幅 + avail_w**
  - `OverlayLine.right` フィールド追加（テキスト実際の右端）
  - `OverlayLine.col_right` フィールド追加（所属カラムの右境界）
  - 白矩形幅: `(line.right - x + 2.0).max(10.0)`（旧: `page_width - x - 20.0`）
  - avail_w: `(line.col_right - line.x).max(50.0)`（Phase 3 / Phase 4 両方）

- [x] **Fix 4: 複数段組対応**（`extract_overlay_pages` を改修）
  - `harumi::detect_text_columns()` で段境界を検出
  - `group_into_raw_lines(frags, col_right)` として行グループ化を private 関数に切り出し
  - `RawLine.col_right` / `OverlayLine.col_right` で列右端を伝播
  - 単一段 PDF では `detect_text_columns` が1要素を返すため既存動作と等価

- [x] **Fix 5: `is_bold` を見出し判定に活用**
  - `OverlayLine.is_bold` フィールド追加（`TextFragment::is_bold` から）
  - `is_heading = (gap_above > 2.2× AND 左マージン) || rl.is_bold`

- [x] **reading-order ソートを `harumi::sort_by_reading_order()` に統一**
  - カスタムインライン `sort_by(b.y, a.x)` を廃止（NaN 安全な harumi 版に移行）

### テスト

- [x] `cargo build -p harumi-ai` — 警告ゼロ（`#[allow(dead_code)]` で将来用フィールドを明示）
- [x] `cargo test -p harumi-ai` — 全 9 件パス（unit 2 + integration 7 + doctest 0）

---

## Phase 32 — harumi-ai overlay 品質改善（次リリース）

### 優先度：高

- [x] **① per-line フォントサイズ**（`harumi-ai/src/overlay.rs`）
  - `RawLine` / `OverlayLine` に `font_size: f32` フィールド追加
  - `group_into_raw_lines()` で `frag.font_size` の max を追跡
  - Phase 3・Phase 4 の `body_fs = global_body_fs` を `line.font_size`（fallback: `global_body_fs`）に置換
  - Phase 4 白矩形ループの `below` をループ内に移動（per-line font_size を使用）
  - 根拠: `TextFragment.font_size` は v1.4.1 で取得可能。CJK 高密度レイアウトでは行間からの推定値が実際のサイズより小さくなる

- [x] **③ 白矩形の不透明性確認**（コード精査により修正不要と判断）
  - `add_rect` の `ExtGState` は `ca 1.0` / `CA 1.0` を正しく emit（`draw/mod.rs` `to_lopdf_dict`）
  - 各 `rect_stream` は `q/{gs} gs/.../Q` で包まれており、`q`/`Q` スタック不均衡なし
  - opacity=1.0 でも DrawOp は常に ExtGState を登録するため、白矩形は確実に不透明

### 優先度：中

- [x] **② 太字テキストの合成レンダリング**（`harumi-ai/src/overlay.rs`）
  - `add_text` → `add_text_styled(bold = line.is_heading || line.is_bold)` に変更
  - harumi core の `add_text_styled` が PDF render mode 2 (fill+stroke) を使用
  - `stroke_width ≈ font_size × 0.04`（harumi 内部固定値）

### 優先度：低

- [x] **④ cover_color オプション**（`harumi-ai/src/pdf_translator.rs`、`overlay.rs`）
  - `TranslateOptions.cover_color: Option<[f32; 3]>` 追加（デフォルト None = 白 `[1.0,1.0,1.0]`）
  - `TranslateOptionsBuilder::cover_color()` メソッド追加
  - overlay.rs で `options.cover_color.unwrap_or([1.0, 1.0, 1.0])` を白矩形色として使用

---

## Phase 33 — TranslationMode::InPlace (完了)

- [x] **⑤ コンテンツストリーム直接置換** (`harumi-ai/src/inplace.rs` 新規)
  - `TranslationMode::InPlace` バリアント追加 (`pdf_translator.rs`)
  - `translate_pdf_inplace()` — `harumi::PageHandle::replace_text()` で Tj/TJ を in-place 書き換え
  - 元テキストがコンテンツストリームから消去される（白矩形不要）
  - マッチしない行（per-char Td+Tj PDF）は自動的にその行のみ overlay にフォールバック
  - overlay.rs をリファクタリング: Phase 1+2 を `pub(crate) extract_and_translate()` に抽出
  - テスト 2 件追加 (`inplace_mode_basic`, `inplace_mode_unmatched_falls_back`)

## 将来課題（中期）

- [x] **⑤-b InPlace per-char PDF 対応** (完了)
  - `find_cross_op_matches_inner()` を拡張 — 水平 Td (ty=0) を Tj/TJ 間に許容
    (`harumi/src/replace.rs`)
  - `rewrite_content_stream()` / `rewrite_stream_preserve_font()` に `b"Td"|b"TD"` アームを追加
    — cross-op match の中間 Td を抑制
  - `emit_cross_op_replacement()` に `tz_scale: Option<f32>` 追加
    — 70%〜130% の範囲なら `{scale} Tz` で幅補正、それ以外は Td にフォールバック
  - ユニットテスト 3 件追加: `per_char_td_match_found`, `vertical_td_blocks_cross_op_match`,
    `per_char_td_rewrite_suppresses_intermediate_tds`

---

## Phase 34 — クロス Tf テキストマッチング（InPlace match rate 向上）(完了)

- [x] **`CharEntry` に `font_name` フィールド追加** (`harumi/src/replace.rs`)
  - `push_chars_from_bytes()` で各 `CharEntry` に `font_name: Vec<u8>` を設定
  - クロス Tf 時の per-char advance 幅計算に使用

- [x] **`collect_cross_tf_segments()` 追加** (`harumi/src/replace.rs`)
  - BT/ET ブロック内の `Tf` をまたいで文字を統合収集（セグメント分割しない）
  - 分割トリガー: ET / `Tm` (絶対位置リセット) / 垂直 `Td`/`TD` (|ty| >= 0.01)
  - セグメントの `font_name` = 最後の有効 `cur_font`（置換後のリストア対象）
  - セグメントの `font_size` = 最初の `Tf` のサイズ（置換指示に使用）

- [x] **`find_cross_tf_matches_inner()` 追加** (`harumi/src/replace.rs`)
  - `collect_cross_tf_segments` を使用（単一フォントセグメントではなく統合セグメント）
  - `all_text` ホワイトリストに `b"Tf" => true` を追加
  - `orig_width` に per-char `existing_fonts.get(&e.font_name)` を使用
  - `CrossOpMatch.font_name` に最後のマッチ文字のフォント名を設定（正しいリストア先）
  - `has_tf` ガードで Tf を含まない op 範囲を除外（サンプ演算子マッチとの二重カウント防止）

- [x] **`find_cross_op_matches()` 更新** (`harumi/src/replace.rs`)
  - `find_cross_op_matches_inner` の結果に `find_cross_tf_matches_inner` の結果をマージ

- [x] **`rewrite_content_stream()` 更新** (`harumi/src/replace.rs`)
  - `b"Tf" if in_bt =>` アームにクロス op 中間演算子の抑制ロジックを追加
  - role == 1（middle）の `Tf` を `Td` と同様に抑制

- [x] **`count_matches_in_page()` 更新** (`harumi/src/replace.rs`)
  - クロス Tf パスを追加: `collect_cross_tf_segments` でマッチ検索
  - `first_font != last_font` ガードで同一フォントマッチの二重カウントを防止

- [x] **`diagnose_match_failure()` 追加** (`harumi/src/replace.rs`)
  - `pub(crate)` ヘルパー: fallback の原因を分類（`"cross-Tf"` / `"vertical-Td-or-Tm"` / `"text-not-in-stream"`）

- [x] **`PageHandle::diagnose_replace_failure()` 追加** (`harumi/src/document.rs`)
  - `pub fn` — harumi-ai から呼べる公開ラッパー

- [x] **InPlace fallback デバッグログ追加** (`harumi-ai/src/inplace.rs`)
  - debug ビルドのみ: `[harumi-ai] fallback page=N reason=R text=…` を stderr に出力
  - `PageHandle::diagnose_replace_failure` を利用

### 期待改善効果

GHS_SDS_file03.pdf にて 14% (37/263) → 60〜80% InPlace 置換に改善見込み（Tf フォント切替で分断されていた日本語行が全マッチするようになるため）

## 将来課題（長期）

- [ ] **⑥ 回転テキスト対応** — `Tm` で 90°/270° 回転したテキストの抽出・配置
- [ ] **⑦ 縦書きモード対応** — `writing-mode: vertical-rl` の抽出・配置ロジック
- [x] **⑧ Form XObject 内テキスト抽出** — 実装済み (Phase B-2, 2026-06-15)
- [ ] **⑨ ToUnicode CMap なし PDF 対応** — CIDFont の glyph → Unicode 変換 fallback

---

## Phase 35 — 翻訳品質向上 Phase A/B/C (完了) [v1.5.0]

### Phase A — Quick wins (harumi-ai)

- [x] **`OverflowStrategy` enum** (`harumi-ai/src/pdf_translator.rs`) —
  `Shrink { min_font_size: f32 }`（デフォルト 6.0pt、従来動作）/ `Truncate { min_font_size: f32 }`（縮小後も溢れたら `"…"` で切り捨て）
  - `TranslateOptions::overflow` フィールド追加
  - `TranslateOptionsBuilder::overflow(strategy)` メソッド追加
  - `overlay.rs::fit_font_size()` の min_fs ハードコード 6.0 を引数化
  - `truncate_to_fit()` ヘルパー追加（二分探索でバイナリカット）

- [x] **進捗コールバック** (`harumi-ai/src/pdf_translator.rs`) —
  `TranslateOptions::progress_fn: Option<Arc<dyn Fn(u32, u32) + Send + Sync>>`
  - `TranslateOptionsBuilder::on_progress(fn)` メソッド追加
  - `extract_and_translate()` に `AtomicU32` カウンタ → バッチ完了ごとに発火
  - `translate_pdf_new_document()` も同様に対応
  - Overlay / InPlace / NewDocument 全モード対応

### Phase B — harumi 抽出品質向上

- [x] **`TextGroup` / `GroupingStrategy` / `group_text_fragments`** (`harumi/src/extract.rs`) —
  - `GroupingStrategy` enum: `Raw` / `Line` / `Paragraph`
  - `TextGroup` struct: `text`, `fragments`, `x`, `y`, `width`, `height`（`#[non_exhaustive]`）
  - `group_text_fragments(fragments, strategy) -> Vec<TextGroup>`
    - `Raw`: 1フラグメント = 1グループ
    - `Line`: y ±½ font_size 以内をマージ
    - `Paragraph`: Line グルーピング後、行間ギャップ > 1.5 × line_height で段落区切り
  - `harumi/src/lib.rs` から全3型を re-export

- [x] **Form XObject 内テキスト抽出** (`harumi/src/extract.rs`) —
  `extract_text_from_xobjects()` 追加。`Do` 演算子対応、深さ上限5。
  XObject 固有の `/Resources` フォント辞書を優先使用（フォールバック: ページリソース）。
  `collect_fonts_from_resources()` ヘルパーを抽出してリファクタリング。

- [x] **CS 間グラフィック状態継承** (`harumi/src/extract.rs`) —
  `ParseCarryState { cur_color: [f32;3], cur_render_mode: u8 }` を追加。
  `parse_content_stream()` に `&mut ParseCarryState` 引数を追加し、
  複数ストリーム間で色・レンダーモードを引き継ぐ。

### Phase C — フォントフォールバック (harumi + harumi-ai)

- [x] **`font_covers_char(font_bytes, ch) -> bool`** (`harumi/src/document.rs`) —
  ttf-parser の `face.glyph_index(ch).is_some()` でカバレッジ判定。
  `harumi/src/lib.rs` から public export。

- [x] **`TranslateOptions::font_fallbacks: Vec<Vec<u8>>`** (`harumi-ai/src/pdf_translator.rs`) —
  プライマリフォントでカバーできない文字に使うフォントを複数指定可能。
  `TranslateOptionsBuilder::add_font_fallback(bytes)` で追加。

- [x] **マルチフォント描画** (`harumi-ai/src/overlay.rs`, `inplace.rs`) —
  `split_by_font(text, faces)` — テキストをフォント別 run に分割（プライマリ優先）。
  各 run を対応フォントで描画し run_x を advance width 分ずつ進める。
  未使用の fallback フォントは embed しない（lazy embedding）。
  Overlay / InPlace fallback 両モードに適用。

### Phase D — テーブルセル検出

- [x] **`TableCell` struct** (`harumi/src/extract.rs`) —
  `row: usize`, `col: usize`（0始まり）, `text: String`, `x`, `y`, `width`, `height: f32`。
  `#[non_exhaustive]` 付き。`harumi/src/lib.rs` から public export。

- [x] **`extract_table_cells(fragments, page_width, page_height) -> Vec<TableCell>`** (`harumi/src/extract.rs`) —
  - **列**: `detect_text_columns()` を再利用（X密度ギャップ検出）
  - **行**: `sort_by_reading_order` 後、row_tol = `(first_fs * 0.5).max(2.0)` でY座標クラスタリング
  - 各フラグメントを `col_for_x()` で列ゾーンに割り当て
  - `BTreeMap<(row_idx, col_idx), Vec<&TextFragment>>` で同セルフラグメントを集約
  - セル内テキストはX昇順で結合、BTreeMap順で (row, col) ソート済み出力
  - doc comment に制限事項明記（罫線なし PDF はヒューリスティック、ネストテーブル・不規則配置で誤検出の可能性）

- [x] **ユニットテスト 7件** (`src/extract.rs` 内 `#[cfg(test)]`) —
  `extract_table_cells_single_column`, `_two_columns`, `_merges_same_cell_fragments`, `_empty_returns_empty`,
  `group_text_fragments_raw`, `_line`, `_paragraph`

---

## Phase 36 — CJK パニック修正 + harumi-ai 初回公開 (完了) [v1.5.1]

- [x] **CJK バイトスライスパニック修正** (`harumi-ai/src/inplace.rs`) —
  デバッグログの `&s[..s.len().min(60)]` が日本語文字のマルチバイト境界でパニックする問題を
  `chars().take(60).collect::<String>()` で修正（v1.5.0 リリース後に発見）。

- [x] **ユニットテスト 10件追加** (`harumi-ai/src/inplace.rs`) —
  CJK 境界パニックのリグレッション防止テスト。

- [x] **Clippy lint 3件修正** (`harumi-ai/src/inplace.rs`, `overlay.rs`) —
  - `repeat().take()` → `repeat_n()`
  - `manual_div_ceil` を `div_ceil()` に置換

- [x] **`harumi-ai/Cargo.toml`**: `harumi` path dep に `version = "1.5"` を追加（crates.io 公開要件）

- [x] **`cargo publish -p harumi`** — v1.5.1
- [x] **`cargo publish -p harumi-ai`** — v0.1.0（初回公開）

---

## バグ修正 (v1.5.3) — 2026-06-15

### Chrome/Skia 生成 PDF の翻訳ゼロ問題（P0）

**根本原因**: `extract_text_from_xobjects()` が page dict の `/Resources` を直接参照していた。
Chrome/Skia は PDF 仕様 §7.7.3 に従い `/Resources` を親 `/Pages` ノードに配置するため、
関数がページ dict で `None` を返し即 return → Form XObject が一件も処理されない → テキスト抽出ゼロ。

（`collect_fonts_inner()` は v1.5.2 で parent chain walk を修正済みだが、
`extract_text_from_xobjects()` は未修正のままだった。）

#### 実装

- [x] **`collect_inherited_xobject_ids()` ヘルパー追加** (`src/extract.rs`)
  - `/Parent` チェーンを昇っていき最初に見つかる `/Resources/XObject` の ObjectId 一覧を返す
  - `collect_fonts_inner()` と同じパターンで parent chain walk を実装
  - `while let Ok(obj) = doc.get_object(current_id)` + clippy クリーンな構造

- [x] **`extract_text_from_xobjects()` 修正** (`src/extract.rs`)
  - page dict 直参照 26 行を `collect_inherited_xobject_ids(doc, page_id)` 1 行に置換
  - 各 Form XObject の `/Resources/Font` 取得ロジック（lines 654-657）は元から正しく維持

- [x] **ユニットテスト追加** (`src/extract.rs` 内 `#[cfg(test)]`)
  - `extract_xobjects_from_inherited_resources` — 親 Pages ノードに `/Resources/XObject` を持ち、
    各 Form XObject が独自 `/Resources/Font` を持つ lopdf Document をメモリ内で構築し、
    `extract_text_runs_from_page` が "Hello" を含むフラグメントを返すことを検証

#### 検証

- [x] `cargo test` — 59 件パス（新テスト含む）
- [x] `cargo clippy -- -D warnings` — 0 警告

### P1: Form XObject 内テキストへの `replace_text` 対応 (完了)

実機 Sample.pdf は手元にないため、CID フォント + `<hex> Tj` + 継承 XObject の合成テストで実際のデコードパスを先行検証し、その上で P1 を実装。

#### 根本原因

`count_matches_in_page` と `rewrite_page_streams` は PAGE の /Contents ストリームのみを走査しており、Form XObject 内のテキストを見落としていた。harumi-ai InPlace モードは `replace_text()` の戻り値が 0 のとき Overlay fallback になるため、XObject 内テキストは常に Overlay 経由になっていた。

#### 実装

- [x] **`extract.rs`**: `collect_fonts_from_resources` / `collect_inherited_xobject_ids` を `pub(crate)` に昇格
- [x] **`replace.rs`**:
  - `count_matches_in_raw_streams()` — 生バイト列×フォントマップでパターンカウントを行う共通ヘルパー（ReDoS ガード・cross-Tf パス込み）
  - `count_matches_in_inherited_xobjects()` — 継承 XObject 内ストリームのカウント
  - `count_matches_in_page()` をリファクタリング: page streams + XObject streams を両方カウント
  - `rewrite_form_xobject_streams()` — 継承 XObject のコンテンツを書き換えて `(xobj_id, new_content, fonts_used)` リストを返す
- [x] **`document.rs`**:
  - `add_font_to_xobject_resources()` ヘルパー追加 — inline/indirect 両形式の XObject /Resources/Font に新フォントを登録
  - finalize() Replace パス拡張: `rewrite_page_streams` の後に `rewrite_form_xobject_streams` を呼び出し、XObject ストリームを in-place 更新（Filter 除去・Length 更新・allows_compression=false）し、新フォントを XObject 固有の /Resources/Font に登録

#### 検証テスト

- [x] `extract_cid_xobject_inherited_resources` (src/extract.rs ユニットテスト) — Type0/CID + Identity-H + ToUnicode + `<XXXX> Tj` + 継承 XObject でテキスト抽出が正しく動くことを確認（P0 の CID パス検証）
- [x] `replace_text_in_form_xobject_inherited_resources` (tests/replace.rs 統合テスト) — 同構造の PDF で `replace_text("Hi", "Bye", font)` を呼び出し、XObject 内テキストが置換されることをラウンドトリップ検証

## テスト（v1.5.3 完了時点 — 308 件）

| スイート | ファイル | 件数 |
|---|---|---|
| ユニット | `src/` 各モジュール内 `#[cfg(test)]` | 61 (+2 CID/XObject テスト) |
| テキスト置換 | `tests/replace.rs` | 18 (+1 XObject replace テスト) |
| 上記以外（v1.5.2 から変化なし） | — | 229 |

---

## バグ修正（v1.5.4）— 2026-06-15

### Type3 フォントをテキスト抽出でスキップしていた問題（完了）

**根本原因**: `collect_font_dict_entries()` の match が `Type0`/`Type1`/`MMType1`/`TrueType` のみを処理し、`/Subtype /Type3` は `_ => continue` でスキップ。Chrome/Skia 生成 PDF（Sample.pdf の F34/F35/F36）はすべて Type3 のため `fonts` HashMap が空 → テキスト断片ゼロ → harumi-ai 翻訳出力ゼロ。

**修正**: `src/extract.rs:862` の match arm に `| Some(b"Type3")` を追加し `collect_simple_font()` に流す。Type3 は Type1/TrueType と同じ 1 バイト文字コード + `/ToUnicode` CMap 構造を持つため同関数で正しく処理できる。

- [x] `src/extract.rs` — match arm 1 行変更
- [x] ユニットテスト 2 件追加
- [x] `cargo test` — 28 件パス
- [x] `cargo clippy --all-features` — 0 警告
- [x] `cargo publish` — crates.io v1.5.4 公開済み

## バグ修正（v1.5.5）— 2026-06-15

### Overlay CTM 座標変換バグ修正（完了）

**根本原因**: Chrome/Skia 生成 PDF はページコンテンツストリームの先頭で
`q → 0.24 0 0 -0.24 0 841.92 cm → Do → Q` という変換を確立する。
`parse_content_stream()` が `q`/`Q`/`cm` 演算子を無視していたため、`TextFragment` の
座標が XObject のローカル空間（例: x=500, y=3000）のままになっていた。Overlay モードが
この生座標をページ空間として使用するため、翻訳テキストが極小・上下反転・左上隅に集中。

**アドバイザーが指摘した重要バグ**: 最初の実装では `ctm_stack.first()` を
`state.ctm` に保存していた。これは `q`/`Q` の外側（identity）を返すため、
`q → cm → Do → Q` パターンでは常に identity CTM になっていた。
正しい修正: `Do` 演算子を検出した時点の `ctm_stack.last()` を `state.ctm` に保存し、
Form XObject 抽出時にその CTM を適用する。

- [x] `src/extract.rs` — CTM ヘルパー関数追加（`multiply_ctm` / `apply_ctm` / `ctm_scale` / `read_matrix`）
- [x] `ParseCarryState` に `ctm: [f32; 6]` フィールド追加
- [x] `parse_content_stream()` に `q`/`Q`/`cm`/`Do` アーム追加、`Tj`/`TJ` で CTM 変換適用
- [x] `extract_text_from_xobjects()` で `carry.ctm` と XObject `/Matrix` を合成
- [x] ユニットテスト 2 件追加（`ctm_transforms_coordinates_to_page_space` / `ctm_at_do_captured_in_state`）
- [x] `cargo test` — 30 件パス（新テスト含む）
- [x] `cargo clippy --all-features` — 0 警告
- [x] `cargo publish` — crates.io v1.5.5 公開済み

### 残課題（P1: Type3 InPlace 置換）

- [x] InPlace モードで Type3 フォントのテキスト置換が失敗する原因 → `"type3-char-per-tj"` として分類済み（v1.5.6）

---

## バグ修正（v1.5.6）— 2026-06-16

### Chrome/Skia 生成 PDF のオーバーレイ・XObject CTM バグ修正（完了）

**修正 1: overlay_from のベースページ CTM 分離**

Chrome/Skia が生成した PDF のコンテンツストリームは `cm` 演算子が不均衡なことがある（`q`/`Q` で閉じられない）。
このため `overlay_from()` で重ねたオーバーレイの座標が既存コンテンツの変換行列の影響を受けていた。

- [x] `wrap_page_contents_in_q_q()` ヘルパー追加（`src/document.rs`）
- [x] `overlay_from()` でオーバーレイ追加前に既存ページコンテンツを `q`/`Q` で包む

**修正 2: Form XObject の per-Do CTM 追跡**

旧実装は `carry.ctm`（最後に見た CTM）を全 Form XObject に適用していた。
複数の `Do` 演算子が異なる CTM で XObject を呼び出す場合（Chrome/Skia の典型パターン）、
全 XObject に最後の CTM が適用され、テキスト座標が誤って変換されていた。

- [x] `ParseCarryState.do_ctm_map: HashMap<Vec<u8>, [f32; 6]>` フィールド追加
- [x] `parse_content_stream()` の `Do` アームで `(xobj_name, current_ctm)` を `do_ctm_map` に記録
- [x] `extract_text_from_xobjects()` を per-Do CTM パスとフォールバックパスに分岐
  - do_ctm_map が非空 → 各 XObject を Do 時の CTM で個別に処理
  - do_ctm_map が空 → 従来の Resources スキャンフォールバック（テスト用 PDF 等）
- [x] 新ヘルパー関数: `decode_form_xobject()` / `xobject_fonts()` / `xobject_matrix()` / `collect_inherited_xobject_name_map()`

**修正 3: diagnose_match_failure の cross-BT スキャン**

`diagnose_match_failure()` が Type3 フォントの1文字1Tj形式（BT/ET をまたぐ連続 Tj）を認識できず、
常に `"text-not-in-stream"` を返していた。

- [x] cross-BT スキャンを追加（`src/replace.rs`）: BT/ET 境界をまたいでアクティブフォントを追跡し、全 Tj 出力を連結
- [x] ターゲット文字列が cross-BT で見つかった場合 `"type3-char-per-tj"` を返す
- [x] harumi-ai が Type3 フォントの InPlace 失敗を正確に識別し、Overlay フォールバックを選択できるようになった

### テスト（v1.5.6 完了時点）

テスト追加なし（内部リファクタリングのみ）。デフォルト features で 310 件、--all-features で 407 件。

---

## バグ修正（v1.5.8）— 2026-06-17

### 翻訳ビジュアルバグ 3件修正（完了）

**修正 1: finalize() の CTM 分離**

既存ページに保留テキスト/描画操作を書き込む `finalize()` が `append_to_contents()` の前に
`wrap_page_contents_in_q_q()` を呼び出していなかった。既存コンテンツに不均衡な `cm` 演算子が
あると新規追加ストリームの座標に影響し、テキスト・図形がずれる問題。

- [x] `src/document.rs` `finalize()` — `append_to_contents()` 前に `wrap_page_contents_in_q_q()` 追加

**修正 2: ctm_stack を ParseCarryState に移動（複数ストリーム間 CTM 継続）**

`parse_content_stream()` が `ctm_stack` をローカル変数として保持していたため、
`Contents` 配列内の複数ストリームをまたぐと CTM スタックがリセットされ、
後続ストリームや Form XObject のテキスト座標が誤って変換される問題。

- [x] `ParseCarryState` に `ctm_stack: Vec<[f32; 6]>` フィールド追加（初期値 `[IDENTITY_CTM]`）
- [x] `parse_content_stream()` のローカル `ctm_stack` を `state.ctm_stack` に置換
- [x] `extract_text_from_xobjects()` が各 Form XObject 呼び出し前後で CTM スタックを保存/復元

**修正 3: Type3 フォント向け cross-BT Tj 置換**

Chrome/Skia 生成 PDF は各文字を独立した `BT … Tj … ET` ブロックに配置する。
`rewrite_content_stream()` が BT/ET ブロック内のみをマッチするため `replace_text()` が常に 0 を返す問題。
`diagnose_match_failure()` では `"type3-char-per-tj"` として検出されていたが、実際の置換は未対応だった。

- [x] `src/replace.rs` — `CrossBtMatch` 構造体追加
- [x] `find_cross_bt_matches()` 関数追加（BT/ET 境界をまたぐ置換ターゲットを検出）
- [x] `rewrite_content_stream()` に cross-BT マッチ処理追加（最初のブロックの Tf/Tm/Td を保持・置換テキスト出力・残ブロック抑制）

### テスト（v1.5.8 完了時点）

テスト追加なし。デフォルト features で 310 件、--all-features で 407 件。

- [x] `cargo test` — 全件パス
- [x] `cargo clippy` — 0 警告
- [x] `cargo publish` — crates.io v1.5.8 公開済み

---

## 機能追加（v1.5.9）— 2026-06-17

### ReplaceOptions + replace_text_opts / TextFragment.space_advance（完了）

**追加 1: normalize_whitespace オプション**

harumi-ai が `group_into_raw_lines()` で全フラグメント間にスペースを挿入するため、
`replace_text()` に `"T h e F r e e"` が渡されるが、harumi の cross-BT マッチャーは
`"TheFree"` と照合するため不一致になる問題。

- [x] `src/document.rs` — `ReplaceOptions { normalize_whitespace: bool }` 構造体追加（`#[non_exhaustive]`）
- [x] `PageHandle::replace_text_opts()` メソッド追加（API 境界で空白を正規化）
- [x] `src/lib.rs` — `ReplaceOptions` を公開 re-export に追加

**追加 2: TextFragment.space_advance**

harumi-ai がフラグメント間スペース挿入要否を判断できるよう advance width 情報を追加。

- [x] `src/extract.rs` — `TextFragment` に `space_advance: f32` フィールド追加（U+0020 のフォント advance width）
- [x] `decode_chars_to_fragment()` で `to_unicode` の逆引きにより space GID を特定して計算
- [x] テスト内の `TextFragment` 直接構築箇所に `space_advance: 0.0` を追加

- [x] `cargo test` — 全件パス
- [x] `cargo clippy` — 0 警告
- [x] `cargo publish` — crates.io v1.5.9 公開済み

---

## バグ修正（v1.5.10）— 2026-06-17

### cross-BT マッチカウントバグ + find_cross_bt_matches フォント追跡修正（完了）

**バグ 1（主）: count_matches_in_raw_streams に cross-BT カウントパスがない**

`count_matches_in_page("TheFree")` が 0 を返し `PendingOp::Replace` が積まれない問題。

- [x] `src/replace.rs` — `count_matches_in_raw_streams()` に cross-BT カウントパスを追加
  - `in_bt` ガードなしで `Tf` を追跡（PDF 仕様: テキスト状態は BT/ET をまたいで持続）
  - 各文字に BT ブロックインデックスを付与
  - `first_bt != last_bt` のマッチのみ計上（intra-BT との二重計上防止）

**バグ 2（副）: find_cross_bt_matches のフォント追跡**

- [x] `src/replace.rs` — `BT` ハンドラから `cur_font.clear()` を削除
- [x] `b"Tf" if in_bt` を `b"Tf"` に変更（in_bt ガード除去）

- [x] `cargo test` — 全件パス
- [x] `cargo clippy` — 0 警告
- [x] `cargo publish` — crates.io v1.5.10 公開済み

---

## バグ修正（v1.5.11）— 2026-06-17

### 水平 Tm と テキスト状態演算子の cross-op マッチング対応（完了）

**根本原因**: 伝統的な日本語 PDF（GHS SDS 等）は同一視覚行の各文字を絶対位置 `Tm` で配置する。
中間演算子ホワイトリストが `Tm` を拒否していたため、cross-op マッチが全て破棄されていた。

- [x] `collect_char_segments()` — 垂直 Tm（y 変化量 ≥ 1 pt）のみフラッシュ；水平 Tm は無視
  - `tm_y: Option<f32>` トラッカー追加；BT でリセット
- [x] `collect_cross_tf_segments()` — 同じ y デルタ判定を適用
  - `seg_tm_y: Option<f32>` トラッカー追加；水平 Tm でフラッシュしない
- [x] `find_cross_op_matches_inner()` — `b"Tm"` + `b"Tc" | b"Tw" | b"Tz" | b"TL" | b"Ts"` をホワイトリストに追加
- [x] `find_cross_tf_matches_inner()` — 同上
- [x] `rewrite_content_stream()` — cross-op マッチ範囲内の `Tm`・`Tc`・`Tw`・`Tz`・`TL`・`Ts` を抑制
  - `b"Tm" if in_bt && op_role.contains_key(&op_idx)` アーム追加
  - `b"Tc" | b"Tw" | b"Tz" | b"TL" | b"Ts" if in_bt && op_role.contains_key(&op_idx)` アーム追加

### テスト（v1.5.11 完了時点）

テスト追加なし。デフォルト features で 310 件、--all-features で 407 件。

- [x] `cargo test` — 全件パス
- [x] `cargo clippy` — 0 警告
- [x] `cargo publish` — crates.io v1.5.11 公開済み

---

## バグ修正（v1.5.12）— 2026-06-17

### AES-256 暗号化 PDF のストリームスキップ + ExtractionWarning API（完了）

- [x] `page_content_streams()` / `decode_form_xobject()` — `decompress()` 失敗時に raw content へフォールバック（PScript5/Distiller AES-256 PDF 対応）
- [x] `ExtractionWarning` / `WarningKind` 構造体追加（`StreamDecompressFailed` / `XObjectSkipped`）
- [x] `Document::extract_text_runs_verbose()` 公開 API 追加
- [x] `TextFragment.tf_font_size` / `TextFragment.tm_y_scale` 新フィールド追加
- [x] `TextFragment.space_advance` 新フィールド追加（U+0020 の advance width）
- [x] ゼロ advance width フォールバック（0.5em/文字）
- [x] `cargo publish` — crates.io v1.5.12 公開済み

---

## バグ修正（v1.5.13）— 2026-06-18

### XObject フォント解決バグ修正 PScript5/Distiller PDF（完了）

**根本原因**: `xobject_fonts()` が `/Resources` dict は存在するが `/Font` サブエントリがない
XObject に対して空の HashMap を返し、テキストが全量ドロップされていた。

- [x] `xobject_fonts()` — ページフォントをベースに XObject 固有フォントをオーバーレイする方式に変更
- [x] 回帰テスト `extract_xobject_font_from_page_resources` 追加
- [x] `cargo publish` — crates.io v1.5.13 公開済み

---

## バグ修正（v1.5.14）— 2026-06-18

### クロスストリーム BT/ET テキスト状態の引き継ぎ（完了）

**根本原因**: `parse_content_stream()` が呼ばれるたびに `in_bt`・フォント名・テキスト位置を
ローカル変数として初期化していた。Distiller 製 PDF が単一の BT…ET ブロックを複数の
Contents ストリームに分割する場合、後続ストリームの Tj がすべて無視されていた。

- [x] `ParseCarryState` に `in_bt`・`font_name`・`tf_font_size`・`font_size`・`tm_y_scale`・`text_x`・`text_y` を追加
- [x] `parse_content_stream()` で先頭に state から読み込み、末尾に書き戻す
- [x] 回帰テスト `extract_cross_stream_bt_tj` 追加
- [x] `cargo publish` — crates.io v1.5.14 公開済み

---

## 機能追加（v1.5.15）— 2026-06-18

### オペレータ対応付け + replace_text_fragments API（完了）

**背景**: PScript5/Distiller 製 SDS PDF では供給者情報が 1 文字ずつ `BT (ch) Tj ET` で
描かれており、`replace_text("元テキスト", "訳文")` が文字列検索に失敗する。
元 operator を直接抑制し訳文を配置する API が必要。

#### TextFragment ソーストラッキング

- [x] `tokenize()` を `Vec<(Token, usize)>` に変更（各トークンのバイトオフセットを付与）
- [x] `TextFragment` に 3 フィールド追加：
  - `pub source_stream: Option<usize>` — Contents 配列の 0 始まりインデックス
  - `pub source_op_start: Option<usize>` — Tj/TJ キーワードの先頭バイトオフセット
  - `pub source_op_end: Option<usize>` — キーワード末尾の次バイト（start+2）
- [x] `parse_content_stream()` に `stream_idx: Option<usize>` パラメータ追加
- [x] Tj/TJ 処理時に `decode_chars_to_fragment()` へ source 情報を渡す

#### replace_text_fragments API

- [x] `extract.rs` に `page_content_stream_ids()` ヘルパー追加（Contents の ObjectId 列を返す）
- [x] `document.rs` に `PageHandle::replace_text_fragments(fragments, new_text, font)` 追加
  - `source_op_end` でターゲット Op を特定
  - `parse_ops()` でストリームを再解析
  - ターゲット Tj/TJ を `() Tj` に置換してストリームを再構築
  - `stream.dict.remove(b"Filter")` で非圧縮で書き戻し
  - `PendingOp::Text` で新テキストを最初のフラグメント位置に配置
  - 抑制したオペレータ数を返す
- [x] 統合テスト `replace_text_fragments_suppresses_source_ops` 追加
  - 1 文字ずつ Tj の PDF を合成
  - replace_text_fragments 後に元テキスト消去・新テキスト存在を確認

#### テスト・検証

- [x] `cargo test --all-features` — 全件パス（63 ユニット + 統合テスト）
- [x] `cargo clippy --all-features -- -D warnings` — 0 警告
- [x] `cargo semver-checks` — 互換確認（TextFragment は #[non_exhaustive]）
- [ ] `cargo publish` — crates.io v1.5.15 未公開
