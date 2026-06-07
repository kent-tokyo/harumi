# 変更履歴

このファイルはプロジェクトの主要な変更をすべて記録します。

フォーマットは [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) に準拠し、
バージョン管理は [Semantic Versioning](https://semver.org/spec/v2.0.0.html) に従います。

---

## [未リリース]

### 追加（v1.4.0 — Phase 24: テキスト抽出品質強化）

- **`sort_by_reading_order(fragments: &mut [TextFragment])`** — テキスト抽出結果をコンテント
  ストリーム順から人間が読む順序（上から下へ、左から右へ）に並べ替えます。NaN/Infinity 座標を
  安全に処理します。複数段組レイアウトや右から左への言語対応で `extract_text_runs()` の出力後処理が必要な場合に便利です。

- **`glyph_name_to_char()` が `uni<XXXX>` パターンに対応** — AGL グリフ名デコーディングを拡張して
  AGL 2.0 スタイルの `uni0041` 形式に対応。16 進コードが直接 Unicode スカラーにマップされます。
  例えば `uni30A2` → `'ア'`（U+30A2）。16 進長の検証（1–8 文字）により、不正な形式をパニックさせずに
  静かに無視します。

- **`Document::extract_page_images(page) -> Vec<PageImage>`** — スキャン PDF ページから
  すべての画像を抽出します（以前は最大サイズの画像のみ返していました）。Image XObject が見つからない場合はエラーを返します。

### 追加（v1.5.0 — Phase 25: AI/RAG ユーティリティ）

- **`TextChunk` 構造体** — ページから抽出されたセマンティックテキストブロック。フィールド:
  - `text: String` — 複数フラグメントの連結テキスト
  - `bbox: [f32; 4]` — 境界ボックス `[x, y, width, height]`
  - `chunk_type: ChunkType` — `Heading(1..=4)` または `Paragraph`（フォントサイズ比率から自動判定）
  - `avg_font_size: f32` — 構成フラグメントの平均フォントサイズ

- **`ChunkType` enum** — `Heading(u8)`（レベル 1–6、フォントサイズ比率から推定）または
  `Paragraph`。`#[non_exhaustive]` でマーク済み（将来の見出しレベル拡張に対応）。

- **`Document::extract_text_chunks(page) -> Vec<TextChunk>`** — ページからセマンティック
  テキストブロックを抽出し、見出しを自動判定します。アルゴリズム:
  1. `extract_text_runs()` でテキストフラグメント抽出
  2. 読み取り順ソート（上→下、左→右）
  3. 不可視フラグメント除外（OCR層）
  4. y 座標でグループ化（許容値 ±`font_size * 0.5`）
  5. ベースラインフォントサイズ推定（最初の 10 行の最小値）
  6. フォントサイズ比率で分類:
     - ≥1.8× → H1、≥1.5× → H2、≥1.3× → H3、≥1.15× → H4
     - それ以外 → Paragraph
  7. 連続する同型行をチャンク統合
  8. 境界ボックスを構成フラグメントの合集合で計算

- **`Document::extract_as_markdown(page) -> String`** — ページをマークダウン形式で
  抽出します。内部で `extract_text_chunks()` を使用:
  - 見出し: `"#".repeat(level) + " " + text`
  - 段落: プレーンテキスト
  - ブロック間: 2 行改行

### セキュリティ・堅牢性

- **NaN/Infinity に対する防御的プログラミング** — 信頼できない PDF から生じる可能性のある
  不正な浮動小数点値（NaN、Infinity、負のフォントサイズ）をすべての新規関数でフィルタリング:
  - `group_into_lines()`: `font_size` が非有限の場合はフォールバック許容値
  - `estimate_baseline_font_size()`: ベースライン計算から NaN/Infinity を除外
  - `classify_by_ratio()`: NaN ratio は段落として扱う（未分類）
  - `sort_by_reading_order()`: NaN/Infinity 座標をソート順末尾に配置

### テストカバレッジ

- Phase 24: 2 個の統合テスト（`uni_glyph_name_pattern_decoding`、`sort_by_reading_order_*`）
- Phase 25: 7 個の統合テスト（`extract_text_chunks_*`、`extract_as_markdown_*`）
- 全 doctest コンパイル・実行成功

---

## [0.8.0] — 2026-06-06

### 追加

- **`PageHandle::replace_text_resubset(old_text, new_text, font_bytes)`** — フォントサブセットを
  拡張しながら既存コンテントストリームのテキストを置換します。元の（未サブセット）TTF/OTF バイトを渡すと、
  harumi が新旧文字を含む新サブセットを生成し、GID がシフトした場合は全コンテントストリームを再エンコードして
  1 回の `save()` で置換を完了します。フォントが対象文字を含む限り中国語・韓国語・アラビア語など
  任意の言語に対応。`CIDToGIDMap /Identity` の CIDFontType2 フォントのみサポート。

- **`InlineSpan`** 構造体 + **`FlowDocument::push_paragraph_styled(spans)`** — `FlowDocument`
  の段落内で太字・斜体・色を混在させます。太字は PDF fill+stroke モード（render mode 2、
  ストローク幅 = フォントサイズの 4%）、斜体は 12° 水平せん断テキスト行列で実現します。
  いずれも合成効果のため別途太字・斜体フォントは不要です。

- **`PageHandle::add_text_styled(text, font, pos, size, color, bold, italic)`** — `PageHandle`
  に直接使えるスタイル付きテキスト配置メソッド。

- `render_html_to_pdf` のインラインスタイル対応: `<strong>`/`<b>` → 太字、`<em>`/`<i>` → 斜体、
  `<span style="color: #RRGGBB">` → 色指定（hex・3桁・`rgb()` 形式）、`<a href>` → 青色リンク。

- **`Document::set_encryption_aes256(user_password, owner_password)`** — AES-256-CBC 書き込み暗号化
  （PDF 2.0 V5/R6）。`save()` ごとに `getrandom::fill()` で 32 バイト鍵を生成（OS RNG — 弱い
  ソースへのフォールバックなし）。Acrobat X+ / Chrome / Firefox など最新の PDF リーダーが必要。
  古いビューアとの後方互換性が必要な場合は `set_encryption`（RC4-128）を使用してください。

- **`TextFragment::height`** — テキスト抽出の境界ボックス高さフィールド（`font_size` と同値、
  em 全体の近似値）。`x`, `y`, `width`, `height` の 4 つで空間的なテキスト処理が可能になります。

- TTC（TrueType Collection）E2E テスト — `tests/e2e_ttc.rs`（5 件）。既存の NotoSansJP
  フィクスチャから実行時に合成した 2-face TTC を使用。

- WASM スモークテスト — `tests/wasm_smoke.rs`（`#[wasm_bindgen_test]`、`wasm-pack test --node`）。

- CI: `cargo semver-checks` ジョブ（default + all-features）、`wasm-test` ジョブ
  （`wasm-pack test --node`）。

### 修正

- **ネスト `/Pages` ツリーの継承属性ロス**（`remove_page`、`insert_blank_page`、`reorder_pages`）
  — これらのメソッドがページを root `/Pages` ノードに直接再接続する際、中間 `/Pages` ノードの
  継承属性（`/MediaBox`、`/CropBox`、`/Rotate`、`/Resources`、`/UserUnit`）が失われていた問題を修正。
  新しい `realize_page_inherited_attrs()` ヘルパーが `/Parent` 変更前に各ページ辞書へ値を実体化します。
  回帰テスト 3 件追加（`nested_pages_*`）。

---

## [0.7.0] — 2026-06-03

### 追加

- **`Document::set_encryption(user_password, owner_password)`** — `save()` / `save_to_bytes()` /
  `save_to_writer()` 呼び出し時に 128-bit RC4（PDF revision 3）でドキュメントを暗号化します。
  `user_password` を空文字列にすると誰でもファイルを開けますが、編集はオーナーパスワードのみに制限されます。
  暗号化に必要なドキュメントの `/ID` トレーラーエントリはシステム時刻 + プロセス ID から自動生成されます。
  `save()` 後に呼び出した場合は `Error::InvalidInput` を返します。

- **`PageHandle::add_squiggly(rect, color)`** — 波線アンダーライン注釈。
  これにより PDF 標準の 4 種類のテキストマークアップ（Highlight・Underline・StrikeOut・Squiggly）がすべて揃いました。

- **`PageHandle::media_box()`** / **`PageHandle::set_media_box(rect)`** — ページの物理サイズ（`/MediaBox`）を
  親チェーン継承を考慮して読み取り／上書きします。

- **`PageHandle::crop_box()`** / **`PageHandle::set_crop_box(rect)`** — 表示領域のクリップ（`/CropBox`）を
  読み書きします。未設定の場合は `None` を返します。

- **`PageHandle::trim_box()`** / **`PageHandle::set_trim_box(rect)`** — 印刷対象領域（`/TrimBox`）を
  読み書きします。未設定の場合は `None` を返します。

- **`PageHandle::bleed_box()`** / **`PageHandle::set_bleed_box(rect)`** — 印刷用の塗り足し領域（`/BleedBox`）を
  読み書きします。未設定の場合は `None` を返します。

ページボックス系のメソッドはすべて、harumi の他の API と統一して PDF ポイント単位・左下原点の
`[x, y, width, height]` 形式を使用します。

---

## [0.6.0] — 2026-06-03

### 追加

- **`Document::from_file_with_password(path, password)`** /
  **`Document::from_bytes_with_password(bytes, password)`** — パスワード保護された PDF を読み込んで復号します。
  ユーザーパスワード・オーナーパスワードの両方を受け付け、ドキュメントはメモリ上で完全に復号されます。

- **`Document::is_encrypted()`** — 読み込んだ PDF が暗号化されていた場合に `true` を返します。
  `from_*_with_password` の呼び出しが成功した後も `true` のままです。

- **`Error::WrongPassword`** — 指定したパスワードがドキュメントのユーザーパスワードまたはオーナーパスワードと
  一致しない場合に返される専用エラーバリアント。

- **`PageHandle::add_highlight(rect, color)`** — ハイライト注釈。
  Adobe Acrobat の規則（左上 → 右上 → 左下 → 右下の順）に従った `QuadPoints` を自動生成します。

- **`PageHandle::add_underline(rect, color)`** — 下線注釈（`QuadPoints` 付き）。

- **`PageHandle::add_strikeout(rect, color)`** — 取り消し線注釈（`QuadPoints` 付き）。

- **`PageHandle::add_sticky_note(point, contents)`** — テキスト（スティッキーノート）注釈。
  `contents` は完全な Unicode サポートのために UTF-16BE でエンコードされます。
  アイコンは PDF ポイントの `[x, y]` 位置に表示され、デフォルトは折りたたみ状態（`/Open false`）です。

- **`Document::form_fields() -> Result<Vec<FormField>>`** — インタラクティブフォームフィールドを一覧表示します。
  AcroForm の `/Fields` ツリーを再帰的に走査し、リーフフィールドのみを完全なドット区切り名称・
  `FieldType`・現在の文字列値とともに返します。

- **`Document::fill_form(values: &[(&str, &str)]) -> Result<usize>`** — フィールド名でフォームを入力します。
  テキストフィールドには文字列値を直接設定します。チェックボックス・ラジオフィールドは、
  真値を表す文字列（`"true"`・`"yes"`・`"on"`・`"1"`、大小文字不問）なら `/Yes`、
  それ以外は `/Off` に設定されます。
  ビューアが視覚的な外観を再生成できるよう、AcroForm 辞書に `/NeedAppearances true` を自動設定します。
  更新したフィールド数を返します。`save()` 後に呼び出した場合は `Error::InvalidInput` を返します。

- **`FormField`** — 公開構造体: `name: String`、`field_type: FieldType`、`value: String`。`#[non_exhaustive]` 属性付き。

- **`FieldType`** 列挙型 — `Text`・`Checkbox`・`Radio`・`Choice`・`Signature`・`Unknown`。

- **AGL テーブルを 214 → 約 330 エントリに拡張** — 中欧文字（Abreve/abreve・Cacute/cacute・
  Dcaron/dcaron・Nacute/nacute・Uring/uring ほか約 60 文字）、一般的な合字（`ff` / `ffi` / `ffl`）、
  小文字 `euro` を追加。ポーランド語・チェコ語・ハンガリー語・トルコ語・ルーマニア語ドキュメントでの
  `extract_text_runs` カバレッジが向上しました。

- **`extract_text_runs` での Identity-H GID フォールバック** — `/ToUnicode` CMap エントリを持たない
  Type0 CID フォントを、2 バイトの文字コードを直接 Unicode スカラー値として扱うことでデコードします
  （ベストエフォート、Identity-H フォントの BMP 文字では正確）。

### 修正

- `from_file_with_password` / `from_bytes_with_password` — `lopdf::Error::IO` が `harumi::Error::Pdf`
  ではなく `harumi::Error::Io` に正しくマッピングされるようになりました。

---

## [0.5.1] — 2026-05-28

### 変更

- **WASM デモ** — シンプルなスタンプ/OCR フォームを、テキスト配置・矩形ハイライト・直線・
  手描きペンツールを持つフルアノテーションエディタに全面刷新。
  PDF.js でライブプレビューを表示し、アノテーションを harumi の `draw` API 経由で適用して
  加工済み PDF をダウンロードできます。
- **WASM デモ** — Hack Regular フォントをデフォルト同梱。フォントのアップロードが不要になりました。
- **CI** — Linux CI ランナーで `FAILED` になっていた macOS 専用の `Geneva.ttf` テストをスキップ処理に修正。

---

## [0.5.0] — 2026-05-27

### 追加

- **`Document::add_bookmark`** — PDF ドキュメントアウトラインエントリ（フラットなブックマーク一覧）を追記します。
  ブックマークは PDF ビューアのナビゲーション/アウトラインパネルに表示されます。
  非 ASCII 文字（CJK・アクセント付きラテン文字など）を含むタイトルは、
  完全な Unicode 互換性のために BOM 付き UTF-16BE で自動エンコードされます。

- **`PageHandle::add_link_url`** — ページに不可視の URI リンクアノテーションを追加します。
  PDF ビューアでその領域をクリックすると指定 URL に移動します。標準の `/A /URI` アクションを使用します。

- **`PageHandle::add_link_internal`** — 同一ドキュメント内の特定ページへジャンプする不可視の内部リンクアノテーションを追加します。
  `/Dest [pageRef /XYZ]` 形式のデスティネーションを使用します。

- **`HeaderFooter`**（`flow` フィーチャー）— [`FlowDocument`] 用のヘッダ/フッタ設定構造体。
  `FlowOptions::header` または `FlowOptions::footer` を設定すると、すべてのページに
  左/中央/右テキストを描画します。レンダリング時に `{{page}}` と `{{total}}` プレースホルダーを置換します。
  `HeaderFooter::page_number()` 便利コンストラクタを提供します。

- **`FlowOptions::header`** / **`FlowOptions::footer`**（`flow` フィーチャー）—
  `FlowOptions` の `Option<HeaderFooter>` フィールド（両方デフォルト `None`）。

- **`FlowOptions::auto_bookmarks`**（`flow` フィーチャー）— `true`（デフォルト）のとき、
  `push_heading` の呼び出しごとにその見出しの先頭を指す PDF ブックマークを自動記録します。
  アウトライン生成を抑制するには `false` に設定してください。

### 修正

- **`build_outlines_from_bookmarks`** が既存の `/Outlines` ツリーを上書きせずに**マージ**するようになりました。
  既にブックマークのある PDF を読み込んで `add_bookmark()` を呼び出しても、
  元のアウトラインエントリがサイレントに破棄されなくなりました。
- **`set_metadata()`** が `save()` / `save_to_bytes()` 後に呼ばれた場合に `Error::InvalidInput` を返すようになりました。
  他のミューテーションメソッドと同様の `finalized` ガードです。
- **`FlowDocument` の `hf_measure` フォールバック** が `text.len()`（バイト数）ではなく
  `text.chars().count()`（文字数）を使うようになりました。フォントが利用できない場合に
  CJK や他のマルチバイト文字を含むヘッダ/フッタのテキストで右寄せ・中央寄せがズレる問題を修正。
- **`parse_bfrange_line`** が `u32` オーバーフローを防ぐために `checked_add` を使うようになりました。
  悪意を持って作成された ToUnicode CMap ストリームで Unicode 抽出が破損していた問題を修正。
  オーバーフローが発生した場合は範囲を切り捨てます。

### 変更

- **`add_link_url`** のドキュメントコメントにセキュリティ注記を追加: `url` に `javascript:` や `data:` など
  潜在的に安全でない URI スキームが含まれないことを確認する責任は呼び出し元にあります。

### 内部変更

- `pdf_text_string` ヘルパー: ASCII 文字列はリテラルエンコーディング、非 ASCII 文字列は BOM 付き
  UTF-16BE を使用して `/Title` などのテキスト文字列フィールドに対応。
- `build_link_annot_base` + `append_annotation_to_page` ヘルパーが、直接/間接の `/Annots` 配列および
  既存ページの `/Annots` エントリ未設定ケースを処理します。
- `find_cross_op_matches` と `find_cross_op_matches_preserve` を共通の `find_cross_op_matches_inner`
  関数にまとめ、約 150 行の重複ロジックを排除。`CrossOpMatchPreserve` 構造体を削除し、
  両方のコードパスで `CrossOpMatch` を使用するようにしました。
- Clippy の全警告を解消（`is_multiple_of`・`excessive_precision`・`needless_late_init`・`too_many_arguments`）。

---

## [0.4.2] — 2026-05-23

### 修正

- `flow/mod.rs` の壊れたドキュメント内リンクを修正: `Error::InvalidInput` → `crate::Error::InvalidInput`。

---

## [0.4.1] — 2026-05-23

### 追加

- **`Document::extract_page_image`**（`image` フィーチャー）— スキャン PDF の 1 ページから
  埋め込みラスター画像を抽出します。`format`（`Jpeg` または `Png`）とデータバイト列を持つ `PageImage` を返します。
  スキャン PDF のラウンドトリップに便利です: `from_file` で読み込み →
  `extract_page_image` で画像を取得 → 画像を加工 → `add_image` で再埋め込み。

---

## [0.4.0] — 2026-05-23

### 追加

- **`flow` フィーチャー**（`draw` を内包）— `FlowDocument`: 自動ページネーション付きのプッシュ型ドキュメントビルダー。
  ブロック要素を順番に追加し、コンテンツがページからあふれると自動的に改ページします。
  - `FlowDocument::new(font_bytes, options)` — フォントを埋め込んでドキュメントを作成
  - `push_heading(text, level)` — レベル 1–6 の見出し（フォントサイズは `FlowOptions::heading_size_scale` でスケール）
  - `push_paragraph(text)` — 自動折り返し付きの本文段落（ラテン語は単語境界、CJK は任意位置で折り返し）
  - `push_key_value_table(rows)` — 薄いグレーの水平区切り線付き 2 カラムのキー/値テーブル
  - `push_list(items, ordered)` — 箇条書き（`•`）または番号付きリスト
  - `push_page_break()` — 明示的な改ページ
  - `render()` → `Vec<u8>` — 確定して PDF バイト列を返す
  - `FlowOptions` — `page_size`・`margins`・`body_font_size`・`heading_size_scale`・`line_height_factor`・
    `paragraph_spacing`・`table_key_ratio`・`max_pages`
  - `Margins` — `uniform(pt)` と `a4_standard()`（全辺約 20 mm / 56.7 pt）

- **`html` フィーチャー**（`flow` を内包）— `render_html_to_pdf(html, options) -> Result<Vec<u8>>`:
  HTML 文字列を PDF バイト列に変換します。`FlowDocument` が基盤で、HTML は `scraper`（html5ever ベース）で解析されます。
  - 対応要素: `<h1>`–`<h6>`・`<p>`・`<table>/<tr>/<th>/<td>`・`<ul>/<ol>/<li>`・
    `<div>/<section>/<article>/<body>`（ブロックコンテナ）
  - 改ページ: `style="page-break-after: always"` または `class="page-break"`
  - 完全にスキップ: `<head>`・`<script>`・`<style>`・`<meta>`・`<link>`・`<noscript>`
  - 深いネスト HTML（5,000 以上の div レベル）でもスタックオーバーフローなし（反復 DFS ウォーカー）
  - `HtmlRenderOptions` — `font_bytes`（必須）・`page_size`・`margins`・`body_font_size`・
    `line_height_factor`・`max_pages`
  - `font_bytes` は必須で、空の場合は `Error::InvalidInput` を返します

### セキュリティ

- **`max_pages` 上限**（`flow` および `html` フィーチャー）— `FlowOptions::max_pages` と
  `HtmlRenderOptions::max_pages`（両方デフォルト 2000）で生成可能なページ数を制限します。
  上限を超えると `ensure_space` が `Error::InvalidInput` を返し、信頼できない HTML をレンダリングする際の
  無制限なメモリ消費を防止します。
- **反復 HTML ツリーウォーカー** — `walk_iterative` は再帰の代わりに明示的な `Vec` スタックを使用し、
  深いネスト HTML（5,000 `<div>` レベルでテスト済み）でのスタックオーバーフローを防止します。

---

## [0.3.0] — 2026-05-21

### 追加

- **`PageHandle::add_text_with_rotation`** — 任意の角度（反時計回り、度数）でテキストをオーバーレイします。
  PDF の `Tm`（テキスト行列）演算子を使用: `cos(θ) sin(θ) -sin(θ) cos(θ) x y Tm`。
  0 度の場合は後方互換性のため標準の `Td` 演算子にフォールバックします。
  `add_text_with_opacity` と同じフォント/サイズ/色/透明度パラメータを受け付けます。

- **図形の塗り＋輪郭同時描画**（`draw` フィーチャー）— `add_ellipse` と `add_polygon` が
  最後の引数として `stroke_width: f32` パラメータを受け付けるようになりました。
  `filled = true` かつ `stroke_width > 0.0` の場合、PDF の `B`（塗り＋輪郭）演算子を使用します。
  `stroke_width = 0.0` は従来の動作（`filled` に応じて `f` または `S`）を維持します。
  **破壊的変更**: 既存の呼び出しに `0.0`（または正の輪郭幅）を追加する必要があります。

- **`PageHandle::add_path`**（`draw` フィーチャー）— `add_polygon` と `add_polyline` を包含する
  統合パス API。パラメータ: `points`・`closed: bool`・`color`・`filled: bool`・`stroke_width: f32`・`opacity`。
  `closed = true` は PDF の `h`（closepath）演算子を追加し、`closed = false` はパスを開いたままにします。
  既存の `add_polygon` と `add_polyline` は変更なし（置き換えではなく追加）。

- **クロス演算子テキスト置換** — `replace_text` と `replace_text_preserve_font` が、同一フォントコンテキスト
  （同じ `Tf` 演算子、同じ `BT`/`ET` ブロック内）の連続する `Tj`/`TJ` 演算子にまたがる `old_text` を
  マッチ・置換できるようになりました。以前は単一演算子の完全一致のみサポートしていました。
  演算子間に位置演算子（`Td`・`Tm` など）がある場合のマッチは意図的にスキップします。
  `can_replace_text` もクロス演算子マッチをカウントします。

- **`TextFragment::font_name`** — 抽出位置でのフォントの PDF リソース名（例: `"HR0"`・`"F1"`）。
  CJK グリフ診断などで、あるランがどのフォントファミリーに属するかを特定するのに便利です。
- **`TextFragment::color`** — 抽出位置での RGB 塗りつぶし色 `[f32; 3]`。
  最後の `rg` または `g` Content Stream 演算子を追跡します。デフォルトは黒 `[0.0, 0.0, 0.0]`。
- **`TextFragment::invisible`** — テキストレンダーモードが 3（OCR 検索レイヤー、`Tr 3`）のとき `true`。
  不可視の OCR テキストと可視コンテンツを区別できます。
- **`TextFragment` が `#[non_exhaustive]` になりました** — 将来のフィールド追加で semver の破壊的変更が不要になります。
- **`PageHandle::replace_text` の戻り値が `Result<usize>` になりました**（以前は `Result<()>`）。
  戻り値はページ上の `old_text` の出現件数です。0 の場合はマッチなし・オペレーションはキューされません。
  マッチ件数は呼び出し時に即時計算（読み取り専用スキャン）されます。
- **`PageHandle::replace_text_preserve_font` の戻り値が `Result<usize>` になりました**（以前は `Result<()>`）。
  グリフ検証も呼び出し時に即時実行されます — `new_text` の文字が既存フォントの ToUnicode マッピングに
  存在しない場合、`save()` を待たずに即座に `Err(FontCharNotMapped)` が返ります。
- **`PageHandle::can_replace_text(old_text, new_text) -> Result<usize>`** — 純粋な読み取り専用スキャン。
  `old_text` の出現件数を返すか、`new_text` の文字が既存フォントのサブセットに存在しない場合に
  `Err(FontCharNotMapped)` を返します。ドキュメントを変更しません。
  どの置換メソッドを呼ぶか決める前の事前チェックとして使用します。
- **`PageHandle::add_ellipse(rect, color, opacity, filled)`**（`draw` フィーチャー）—
  4 本の 3 次ベジェ曲線（`c` 演算子）で近似した楕円または円を描画します。
  `filled = true` は `rg`/`f` を、`filled = false` は `RG`/`S` を使用します。

### 変更

- **`replace_text_preserve_font` の検証タイミング**: `FontCharNotMapped` が `save()` 時ではなく
  呼び出し時に返されるようになりました。エラーの遅延を前提としていた呼び出し元は動作が変わります。

---

## [0.2.0] — 2026-05-16

### 追加

- **`PageHandle::replace_text_preserve_font(old_text, new_text)`** — マッチした位置で PDF に
  既に埋め込まれているフォントをそのまま使ったインプレーステキスト置換。`FontHandle` は不要で、
  harumi が直前の `Tf` 演算子からフォント参照を読み取ります。
  `new_text` の文字がフォントの ToUnicode マッピングに存在しない場合（フォントがサブセット化されて
  そのグリフが含まれていないなど）、`save()` が `Error::FontCharNotMapped` を返します。
  呼び出し元は明示的なフォントを指定して `replace_text` にフォールバックできます。
  幅補正（`Td`）は自動的に適用されます。

- **`PageHandle::replace_text(old_text, new_text, font)`** — 既存 PDF の Content Stream に対する
  真のインプレーステキスト置換。既存の `Tj` および `TJ` 演算子をデコードし、
  デコードされた Unicode 文字列が `old_text` と一致する最初の箇所を特定して、
  `font`（新たに埋め込むフォント）でエンコードした新しいテキストでストリームを書き換えます。
  フォント切り替え（`Tf`）は自動的に挿入されます。置換後には幅の差を補正するための `Td` 演算子が追記され、
  後続テキストのズレを防ぎます。`TJ` 配列の場合、マッチした要素は独立した `Tj` として切り出されます。
  `old_text` がページ上に見つからない場合は PDF を変更せずに `Ok(())` を返します。
  **制限**: `old_text` は 1 つの `Tj` 演算子または `TJ` 配列内の 1 つの文字列要素の内容全体と一致する必要があります。
  複数の演算子にまたがるテキストはマッチしません。

- **`Document::extract_text_runs(page)`** — ページから位置付きテキストランを抽出し `Vec<TextFragment>` を返します。
  各 `TextFragment` は `text`（Unicode 文字列）・`x`/`y`（PDF ポイント座標、左下原点）・
  `width`（送り幅から推定）・`font_size` を持ちます。
  **Identity-H CID フォント**（Type0、harumi が書き出す形式）および **標準シンプルフォント**
  （Type1・MMType1・TrueType、WinAnsiEncoding・MacRomanEncoding・StandardEncoding・
  `/Differences` 配列付き `/Encoding` 辞書）をサポートします。
  `/ToUnicode` CMap（`beginbfchar` および `beginbfrange`）とエンコーディングテーブルへのフォールバックの
  両方を処理します。`Tj` および `TJ` 演算子の PDF リテラル文字列 `(...)` と hex 文字列 `<...>` を両方デコードします。
  未保存の操作は含まれません — 必要な場合は `save_to_bytes()` してからリロードしてください。
  範囲外のページ番号には `PageNotFound` を返します。

- **`TextFragment`** — `extract_text_runs` が返す公開構造体:
  `text: String`・`x: f32`・`y: f32`・`width: f32`・`font_size: f32` フィールド。

- **`Document::new(size)`** — 白紙の 1 ページ PDF をゼロから作成します（`size` は PDF ポイント単位の `(width, height)`）。
  サイズがゼロ・負値・非有限値の場合は `InvalidInput` を返します。
  `insert_blank_page` でページを追加し、`page(1)?` でテキストや図形を追加できます。

- **`Document::extract_pages(page_numbers)`** — 指定したページのみを含む新しい `Document` を返します
  （1 始まり、呼び出し元が指定した順序）。ページコンテンツ・フォント・画像は保持されます。
  アウトライン/ブックマーク・AcroForm・`/Names`・`/PageLabels`・`/OpenAction`・`/StructTreeRoot` は除去されます。
  空の配列や重複するページ番号には `InvalidInput`、範囲外の番号には `PageNotFound` を返します。
  ソースドキュメント（`self`）は変更されません。

- **`Document::merge_from(other)`** — `other` の全ページをこのドキュメントの末尾に追記します。
  `other` のすべてのページコンテンツ・フォント・画像が保持されます。
  アウトライン/ブックマーク・AcroForm・`/Info` メタデータは引き継がれません。
  `other` には未フラッシュの pending 操作がないことが必要です（`from_file`/`from_bytes` で読み込むか、
  `save_to_bytes()` 後にリロードしてください）。

- **ページ操作 API** — Content Stream を変更せずにページツリーを操作します
  - `Document::rotate_page(number, degrees)` — 90 の倍数の `degrees` をページの `/Rotate` エントリに加算。
    繰り返し呼び出すと累積し、負の値は反時計回りになります
  - `Document::remove_page(number)` — ページを削除して残りを繰り上げます。
    無効な番号には `PageNotFound`、最後のページの削除には `InvalidInput` を返します
  - `Document::insert_blank_page(after, (width, height))` — `after` の位置に白紙ページを挿入します
    （0 = 先頭、`page_count()` = 末尾）。ネストした `/Pages` ツリーを平坦化して処理します
  - `Document::reorder_pages(new_order)` — 1 始まりの旧ページ番号でページを並べ替えます。
    長さ・範囲・重複を検証します
  - 4 メソッドすべてが `save()` 後に呼び出されると `InvalidInput` を返します

- **`draw` フィーチャー**（追加依存なし）
  - `PageHandle::add_rect(rect, color, opacity)` — チャンネルごとの RGB 色と塗り透明度を持つ塗り矩形
  - `PageHandle::add_line(from, to, color, line_width, opacity)` — ストロークの線分
  - `ExtGStateRegistry` — 同一ページの描画操作間で `/ExtGState` エントリを重複排除

- **`image` フィーチャー**（`image` クレートを追加、`draw` を有効化）
  - `PageHandle::add_image(bytes, rect)` — JPEG または PNG を完全不透明で埋め込む
  - `PageHandle::add_image_with_opacity(bytes, rect, opacity)` — 透明度付きで埋め込む
  - JPEG は再エンコードなしで埋め込み（DCTDecode パススルー）
  - PNG とその他の形式は生 RGB にデコードして FlateDecode で圧縮。アルファチャンネルは白背景に合成

- `page.size()` での MediaBox 親チェーン走査（最大 32 ホップ、循環耐性）—
  親 `/Pages` ノードから MediaBox を継承するページが正しく処理されるようになりました。
- CFF2 可変フォントの早期検出: `save()` が破損した PDF を無音で生成する代わりに、明確な `FontParse` エラーを返します。
- TTC コレクションのマジックバイト検出（`ttcf`）— `embed_font` が `.ttc` ファイルを受け付けるようになりました
  （インデックス 0 を使用。allsorts と ttf-parser が TTC をネイティブに処理します）。

- **`Document::metadata()`** — ドキュメントの `/Info` 辞書を読み取り、`title`・`author`・`subject`・
  `keywords`・`creator` フィールド（すべて `Option<String>`）を持つ `PdfMetadata` を返します。
  `/Info` 辞書が存在しない場合は `PdfMetadata::default()`（すべて `None`）を返します。
  UTF-16BE 文字列（BOM `\xFE\xFF`）および生 UTF-8/Latin-1 バイト文字列の両方を処理します。

- **`Document::set_metadata(&PdfMetadata)`** — `/Info` 辞書を書き込みます（または置き換えます）。
  `Some` のフィールドのみ辞書に書き込まれ、`None` のフィールドは省略されます。
  フォントサブセット化とは独立して、`save()` の前後どちらでも呼び出せます。

- **`PdfMetadata`** — `title`・`author`・`subject`・`keywords`・`creator` フィールドを持つ公開構造体
  （すべて `Option<String>`）。`Debug`・`Clone`・`Default`・`PartialEq` を derive。

### 修正

- **`remove_page` の正確性** — 削除されたページへの pending テキスト/描画操作が `save()` 前に破棄されるようになりました。
  削除済みページオブジェクトへの書き込みを防止します。
  削除されたページの辞書オブジェクトも PDF オブジェクトグラフから除去し、孤立オブジェクトの肥大化を軽減します
  （ページが参照するストリームオブジェクトは他ページで共有されている可能性があるため除去しません）。
- `build_widths_array`: `unwrap()` を `unwrap_or(units_per_em)` に変更 —
  送り幅エントリが欠損してもパニックしなくなりました。
- `finalize()`: `embedded.get().unwrap()` を `.ok_or(Error::InvalidFont(...))` に変更 —
  無効なフォントハンドルはパニックではなくエラーを返します。
- CFF/OTF フォントの `FontFile3` ストリームに、一部のバリデータが要求する `Length1` エントリを追加しました。

---

## [0.1.0] — 2026-04-xx

### 追加

- `Document::from_file` / `from_bytes` — 既存 PDF を読み込む
- `Document::save` / `save_to_bytes` — 元のオブジェクトグラフを破壊せずに書き出す
- `Document::embed_font(ttf_bytes)` — TrueType または OpenType フォントを登録。サブセット化は `save()` 時に遅延実行
- `PageHandle::add_invisible_text` — OCR 不可視テキストレイヤー（レンダーモード 3）
- `PageHandle::add_text` — RGB 色指定付きの可視テキストオーバーレイ
- `PageHandle::add_invisible_text_runs` — バッチ API。ラン数によらず 1 回のサブセットパス
- `page.size()` — MediaBox から PDF ポイント単位でページ寸法（幅 × 高さ）を取得
- 完全な CJK 対応: CID フォントオブジェクトグラフ（`Type0 → CIDFontType2 → FontDescriptor → FontFile2`）、
  ToUnicode CMap、allsorts サブセット後の GID 再マッピング
- `ocr` フィーチャー: `hocr_y_to_pdf`・`hocr_x_to_pdf`・`pixel_size_to_pt` 座標変換ヘルパー
- `NotoSansJP-Regular.ttf`（日本語）・`NotoSansCJKsc-Regular.ttf`（簡体字中国語）・
  `NotoSansCJKkr-Regular.ttf`（韓国語）でエンドツーエンド動作確認済み
