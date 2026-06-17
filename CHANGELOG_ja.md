# 変更履歴

このファイルはプロジェクトの主要な変更をすべて記録します。

フォーマットは [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) に準拠し、
バージョン管理は [Semantic Versioning](https://semver.org/spec/v2.0.0.html) に従います。

---

## [未リリース]

---

## [1.5.8] — 2026-06-17

### 修正 (harumi)

- **`finalize()` CTM 分離** (`src/document.rs`) —
  既存ページに保留中のテキスト/描画操作を書き込む際、`finalize()` が
  `append_to_contents()` の前に `wrap_page_contents_in_q_q()` を呼び出すよう修正。
  既存のページコンテンツに不均衡な `cm` 演算子が含まれる場合、新規追加ストリームの
  座標系に影響を与え、テキストや図形がずれる問題があった。
  `overlay_from()` には既に同修正が適用されており、`finalize()` パスのみ漏れていた。

- **`ctm_stack` を複数コンテンツストリーム間で保持** (`src/extract.rs`) —
  `ParseCarryState` に `ctm_stack: Vec<[f32; 6]>` フィールドを追加（初期値 `[IDENTITY_CTM]`）し、
  `parse_content_stream()` 内のローカル変数を置き換え。PDF 仕様では `Contents` 配列内の
  複数ストリームはグラフィクス状態を共有するが、従来は各ストリーム呼び出しごとに
  `state.ctm`（最後の `Do` 時 CTM のみ）から CTM スタックを再初期化していた。
  `cm` 演算子を含む複数コンテンツストリームのあるページでテキスト座標が誤って変換される
  問題を修正。`extract_text_from_xobjects()` は各 Form XObject の呼び出し前後で
  スタックを保存/復元し、`multiply_ctm(do_ctm, xobj_matrix)` で初期化された独立した
  スタックを XObject ごとに提供する。

- **Type3 フォント向け cross-BT `Tj` 置換** (`src/replace.rs`) —
  Chrome/Skia 生成 PDF は各文字を独立した `BT … Tj … ET` ブロックに格納する。
  従来の `rewrite_content_stream()` は1つの BT/ET ブロック内のテキストしかマッチせず、
  `replace_text()` がこの形式の PDF で常に 0 を返していた。
  新規 `CrossBtMatch` 構造体と `find_cross_bt_matches()` 関数で複数 BT/ET ブロックを
  またぐ置換ターゲットを検出。最初のブロックの位置設定（Tf/Tm/Td）を保持したまま
  置換テキストを出力し、残りのブロックを抑制して1つの BT/ET ブロックに集約する。

---

## [1.5.6] — 2026-06-15

### 追加 (harumi)

- **クロス `Tf` テキストマッチング（`replace_text`）** (`harumi/src/replace.rs`) —
  `replace_text` / `can_replace_text` が同一 BT/ET ブロック内の `Tf`（フォント切替）演算子を
  またいでテキストをマッチできるようになった。日本語 PDF では1行の視覚的なテキストが複数の
  フォントラン（本文漢字を `F1`、括弧文字を `F2` など）に分かれていることが多く、従来の
  シングルセグメントマッチではこれらがすべて fallback していた。新ヘルパー：
  `collect_cross_tf_segments()`（`Tf` をまたいで文字を統合収集）、
  `find_cross_tf_matches_inner()`（Tf を含む op 範囲のみ emit してサンプ演算子マッチと
  二重カウントしない）、`rewrite_content_stream()` の `Tf` 中間演算子抑制。
  `Tm` 演算子は常にセグメントを分割（絶対位置リセット = 新視覚行）。
  `CharEntry` に `font_name` フィールドを追加し、クロス `Tf` 時に各文字の幅を正しいフォントで
  計算できるようにした。

- **`PageHandle::diagnose_replace_failure(old_text) -> &'static str`** —
  `replace_text` が 0 を返したときの原因を分類するデバッグ向けヘルパー。
  `"cross-Tf"`（統合ビューにテキストが見つかるが per-font セグメントには存在しない）、
  `"vertical-Td-or-Tm"`（単一フォントセグメントには存在するが行ブレーク演算子で op 範囲が
  断ち切られた）、`"text-not-in-stream"`（どのセグメントにも存在しない）を返す。

### 追加 (harumi-ai)

- **per-char PDF 対応（InPlace モード向け）** (`harumi/src/replace.rs`) — `find_cross_op_matches_inner()`
  が水平 `Td`/`TD` (ty=0) を `Tj`/`TJ` 間に許容するよう拡張。日本語 PDF に多い
  `(A)Tj 12 0 Td (B)Tj …` パターンがマッチ可能になり、中間 `Td` は書き換え後のストリームから
  除去される。幅補正は 70%〜130% の場合 `Tz`（水平スケール）を使用し、それ以外は `Td`
  にフォールバック。`rewrite_content_stream` と `rewrite_stream_preserve_font` 両方を更新。

- **InPlace fallback デバッグログ** (`harumi-ai/src/inplace.rs`) — debug ビルド
  （`cfg!(debug_assertions)`）では、overlay にフォールバックした行ごとに
  `[harumi-ai] fallback page=N reason=R text=…` を stderr に出力する。
  `R` は `PageHandle::diagnose_replace_failure` が返す理由。リリースビルドではゼロコスト。

- **`TranslationMode::InPlace`** (`harumi-ai/src/inplace.rs`, `pdf_translator.rs`) — コンテンツ
  ストリーム直接置換による新翻訳モード。`harumi::PageHandle::replace_text()` を使って
  `Tj`/`TJ` 演算子を in-place で書き換えるため、原文がストリームから消去される。白矩形は
  不要。マッチしない行（1文字ずつ Td+Tj のスキャン PDF 等）は自動的にその行のみ
  overlay にフォールバック。`opts.mode = TranslationMode::InPlace` で有効化。

- **`TranslateOptions::cover_color`** (`harumi-ai/src/pdf_translator.rs`) — Overlay モードの
  被覆矩形色を指定する optional RGB フィールド（デフォルト: `None` = 白 `[1.0, 1.0, 1.0]`）。
  安全標識や色付きヘッダーなど、背景が白でない PDF での翻訳精度向上に利用可能。
  `TranslateOptionsBuilder::cover_color()` でも設定可能。

### 変更 (harumi-ai)

- **見出し・太字行への合成太字レンダリング** (`harumi-ai/src/overlay.rs`) — `is_heading ||
  is_bold` の翻訳行を `add_text_styled(bold=true)` で出力するよう変更。PDF render mode 2
  (fill+stroke、`stroke_width ≈ font_size × 0.04`) を使用するため追加フォント不要。

---

## [1.5.5] — 2026-06-15

### 修正 (harumi)

- **Overlay CTM 座標変換** (`src/extract.rs`) —
  `parse_content_stream()` が `q`/`Q`/`cm` グラフィクス状態演算子を追跡し、内部 CTM
  スタックを維持するよう変更。テキスト座標は `Tj`/`TJ` 演算子での発行時に現在の CTM を
  適用してページ空間へ変換される。`Do` 演算子を検出した時点の CTM を `ParseCarryState.ctm`
  に記録し、`extract_text_from_xobjects()` に渡すことで、各 Form XObject の `/Matrix` との
  合成 CTM を使ってテキスト座標をページ空間に変換する。

  Chrome/Skia 生成 PDF はページコンテンツストリームの冒頭で
  `q → 0.24 0 0 -0.24 0 841.92 cm → Do → Q` という変換を確立する。この修正以前は
  `TextFragment` 座標が XObject のローカル空間のまま（例: x=500, y=3000）だったため、
  Overlay モードが変換前の生座標を PDF ページ空間として使用し、翻訳テキストが極小・上下反転・
  左上隅に集中するという問題が発生していた。

---

## [1.5.4] — 2026-06-15

### 修正 (harumi)

- **Type3 フォントのテキスト抽出対応** (`src/extract.rs`) —
  `collect_font_dict_entries()` の match 分岐が `Type0`・`Type1`・`MMType1`・`TrueType`
  のみを処理し、`/Subtype /Type3` は `_ => continue` でスキップされてフォントマップに登録されなかった。
  Chrome/Skia 生成 PDF（Sample.pdf の F34/F35/F36 等）はすべて Type3 フォントを使用するため、
  `fonts` HashMap が空になり `TextFragment` が一件も生成されず、`harumi-ai` の翻訳出力がゼロになっていた。
  Type3 は Type1/TrueType と同じ 1 バイト文字コード + `/ToUnicode` CMap 構造を持つため、
  match arm に `| Some(b"Type3")` を追加して `collect_simple_font()` に流すだけで修正できた。

---

## [1.4.3] — 2026-06-15

### 変更 (harumi-ai)

- **Overlay モードの per-line フォントサイズ** (`harumi-ai/src/overlay.rs`) —
  行間ギャップから推定するグローバル `global_body_fs` ヒューリスティックを廃止し、
  各翻訳行に `TextFragment.font_size` を直接使用するよう変更。
  CJK 高密度レイアウトではグローバル推定値が実際のフォントサイズより小さくなるため、
  翻訳後テキストが過小レンダリングされ、白矩形が短すぎる問題が発生していた。

---

## [1.4.2] — 2026-06-15

### 変更 (harumi-ai)

- **Overlay モードのレイアウト精度向上** (`harumi-ai/src/overlay.rs`) — レイアウト保持翻訳の
  7項目を修正：
  - **白矩形の高さ** — グローバルな `body_font_size * 1.3` 定数ではなく、実際の行間から
    導出した per-line `line_height` を使用するよう変更。
  - **白矩形の幅** — `page_width - x - 20` 固定マージンではなく、テキストフラグメントの
    実際の右端 + 2pt パディングを使用するよう変更。
  - **ディセンダー被覆量** — `ttf-parser` の実データ（`face.descender() / face.units_per_em()`）
    からフォントの実ディセンダー比率を計算。従来の `body_fs * 0.08` 固定値を置換。
  - **訳文 Y 座標** — 恣意的な `- scaled * 0.1` ずらしをなくし、原文のベースライン Y
    (`line.y`) に正確に配置するよう変更。
  - **複数段組対応** — `harumi::detect_text_columns()` で段境界を検出し、列ごとに独立した
    行グループ化を実施。2カラム PDF で異なる列の行が混在する問題を解消。`avail_w` を
    ページ幅ではなくカラム右端で制限するよう変更。
  - **見出し判定の改善** — `TextFragment::is_bold`（v1.4.1）を既存のギャップ＋マージン
    ヒューリスティックに加えた追加条件として活用。
  - **読み取り順ソート** — NaN 安全な `harumi::sort_by_reading_order()` に移行（カスタム
    インライン比較関数を廃止）。

---

## [1.4.1] — 2026-06-15

### 追加

- **`TextFragment::is_bold`** — フォント名が太字ウェイトを示す場合に `true`
  （キーワード: Bold、Heavy、Black、Semibold、Demibold、Extrabold）。

- **`TextFragment::is_italic`** — フォント名がイタリック・斜体を示す場合に `true`
  （キーワード: Italic、Oblique、Slanted）。

- **`TextFragment::font_family`** — PostScript `/BaseFont` エントリから派生したフォント
  ファミリー名。サブセットプレフィックス（例: `"ABCDEF+"`）とスタイルサフィックスを除外。
  `/BaseFont` がない場合は空文字列。

- **`TextFragment::base_font`** — サブセットプレフィックスのみ除外した PostScript
  完全名（例: `"Helvetica-BoldOblique"`、`"NotoSansJP-Regular"`）。
  `/BaseFont` がない場合は空文字列。

- **`detect_text_columns(fragments, page_width) -> Vec<ColumnZone>`** — テキストフラグメントの
  X 密度ヒストグラムから段組レイアウトを推定。15pt 以上の連続空白ギャップを段区切りとして検出。
  1段の場合はページ幅全体の `ColumnZone` を1件返す。

- **`ColumnZone`** — `detect_text_columns` が返す構造体。`x_start: f32` と `x_end: f32`
  フィールド（PDF ポイント座標）を持つ。

---

## [1.4.0] — 2026-06-14

### 追加

- **`PageHandle::scale_page_content(scale_x, scale_y)`** — 既存のページコンテンツの先頭に
  `cm`（Concatenate Matrix）演算子を新しいコンテンツストリームとして挿入することで、
  全コンテンツをスケーリング。A4 → A3 のようなコンテンツ比率を保ったページ拡大に有用。

- **`PageHandle::resize_page_with_content(new_width, new_height)`** — ページの MediaBox を
  変更し、既存コンテンツを比率に合わせてスケーリングする処理を1回の呼び出しで実行。

- **`Document::overlay_from(other)`** — PDF Form XObject として `other` の各ページを
  `self` の対応ページに重ね書き。ウォーターマーク・スタンプ・全ページグラフィックの
  合成に有用。`self` のページ数が `other` より多い場合、超過ページはそのまま保持。

- **`Document::clear_outline()`** — ドキュメントのすべてのブックマーク・目次エントリを
  削除（未保存のペンディングブックマークと、読み込み済み PDF の `/Outlines` ツリーの両方）。

- **`Document::attach_file(filename, data, mime_type)`** — 任意のファイルを PDF 添付ファイル
  （`/EmbeddedFiles`）として埋め込む。FlateDecode で圧縮してから埋め込む。

- **`Document::list_attachments()`** — ドキュメントに埋め込まれたすべての添付ファイルを
  一覧表示。`Vec<AttachmentInfo>` を返す（ファイル名・サイズ・MIME タイプ）。

- **`AttachmentInfo`** — `list_attachments()` が返す構造体。フィールド: `filename: String`、
  `size: usize`、`mime_type: Option<String>`。

### 修正

- **`add_text_with_opacity` と `add_text_with_rotation` が docs.rs に表示されるように** —
  これらのメソッドは `#[cfg(feature = "draw")]` の impl ブロック内で定義されていたため、
  デフォルト features でビルドする docs.rs には表示されていなかった。
  全 features で docs.rs をビルドするよう `[package.metadata.docs.rs]` を Cargo.toml に追加。

---

## [1.3.2] — 2026-06-14

### 修正

- **`build_hmtx` の "mono" グリフ（`gid >= num_h_metrics`）advance_width 読み誤り** —
  lsb-only セクションの先頭バイトを advance_width として誤読し、次グリフの lsb を
  現グリフの lsb として誤読していた。最後の longHorMetric の advance_width と
  正しいオフセットの lsb を使うよう修正。

- **`hhea.numberOfHMetrics` の過少設定** — `.min(original_num_h_metrics)` で制限していたが、
  `build_hmtx` は全サブセットグリフを 4 バイト longHorMetric として書くため、
  `gids_to_keep.len()` と等しくなければならない。

- **`head.checkSumAdjustment` が常に 0** — テーブルごとのチェックサムとフォント全体の
  チェックサムが未計算だった。`assemble_font` がテーブルディレクトリに各テーブルの
  チェックサムを書き込み、`checkSumAdjustment = 0xB1B0AFBA - フォント全体の和` を設定
  するよう修正。元フォントの非ゼロ値が `new_head` にコピーされて和の計算を乱していた
  バグも修正（アセンブル前にゼロクリア）。

- **グリフデータの 4 バイトアライメントなし** — TrueType 仕様はグリフを 4 バイト境界に
  配置することを要求している。`build_glyf` が各グリフの後にゼロパディングを追加するよう
  修正。オフセットが 131,070 バイトを超えた場合は `loca` 形式を long (format 1) に
  自動アップグレードし、`head.indexToLocFormat` も更新。

- **hhea と head のアドバイザリメトリクスが元フォントの値のまま** — `advanceWidthMax`、
  `minLeftSideBearing`、`minRightSideBearing`、`xMaxExtent`（hhea）とフォント
  バウンディングボックス（head）を再構築した hmtx と glyf テーブルから再計算するよう修正。

- **2 つの文字が同じグリフを共有する場合に 2 つ目の文字が脱落** — `char_to_gid` を
  `gid_to_char` の逆引きで構築していたため、同じグリフを共有する 2 つ目の Unicode
  コードポイントがサイレントに脱落していた。`SubsetResult` に入力文字から直接構築した
  `char_to_gid` フィールドを追加し、両コードポイントが正しく同じ new GID にマップされる
  よう修正。

- **`hhea`/`maxp` の境界チェックが 2 バイト短い** — `hhea.len() >= 34` で
  インデックス 34-35 を読んでいた（`>= 36` に修正）。`maxp.len() >= 4` で
  インデックス 4-5 を読んでいた（`>= 6` に修正）。

- **`head.indexToLocFormat` アップグレードロジックが無効かつ誤っていた** — 以前の
  チェックは long 形式 loca の長さを short 形式の期待値と比較し、large サブセットで
  誤って発火。発火時は big-endian u16 の高バイトのみ書いて 0x0100 (256) という無効値を
  生成していた。グリフオフセットの最大値が short 形式の限界（131,070 バイト）を超えた
  場合のみアップグレードする正しいロジックに置き換え。

---

## [1.3.1] — 2026-06-14

### 修正

- **埋め込みフォントがmacOS PreviewおよびPSPDFKitで●として表示されるバグ** —
  内製 TTF サブセッターがソースフォントのオプションテーブルをすべて verbatim でコピーしていました。
  対象テーブル: `GSUB`, `GPOS`, `gvar`, `fvar`, `post`, `vhea`, `vmtx` など。
  少数の文字にサブセット化した後も、これらのテーブルは存在しない GID への参照を含んでいます。
  macOS Core Text と PSPDFKit は埋め込みフォントを検証する際に GID 参照の整合性をチェックするため、
  フォントを不正なものとして拒否し、すべてのグリフが ● に置き換えられていました。

  サブセッターをホワイトリスト方式に変更しました：コア TrueType テーブル（`head`, `hhea`,
  `maxp`, `glyf`, `loca`, `hmtx`）と安全なヒンティングテーブル（`fpgm`, `prep`, `cvt`, `gasp`）
  のみをサブセットに含めます。OpenType レイアウトテーブル（`GSUB`, `GPOS`, `GDEF`, `BASE`）、
  可変フォントテーブル（`gvar`, `fvar`, `avar`, `HVAR`, `STAT`）、メタデータテーブル（`post`,
  `name`, `vhea`, `vmtx`, `kern`）などはすべて除外します。
  Identity-H エンコーディングを使った PDF CIDFont 埋め込みでは OpenType シェーピングが
  適用されないため、これらのテーブルは不要です。

- **サブセット化後にコンポジットグリフのコンポーネント GID が書き換えられていなかった** —
  コンポーネントグリフから合成されたグリフがサブセットに含まれる場合、`build_glyf` が
  コンポジットレコードを verbatim でコピーしていたため、コンポーネント GID 参照がサブセット化前の
  元の GID 値を指したままでした。サブセット化後にこれらの GID は新しい連番位置に再採番されるため、
  コンポーネント参照が誤った位置を指していました。`build_glyf` が新しい位置に合わせて
  コンポジットグリフレコード内のコンポーネント GID を書き換えるようになりました。

- **コンポジット依存 GID を含む場合の GID→文字マッピングが不正確だったバグ** —
  文字から GID へのマッピングは `GlyphRemapper` から導出していましたが、このリマッパーは
  明示的にリクエストされたグリフしか知らず、`subset()` 内部で追加されたコンポジット依存 GID を
  含んでいませんでした。コンポジット依存 GID の元 GID がリクエストされたグリフより小さい場合、
  新 GID の位置がひとつ以上ずれていました。マッピングが `subset()` から返される最終的な
  `gids_to_keep` セット（コンポジット依存を含む）を使用するようになりました。

---

## [1.3.0] — 2026-06-13

### 追加（Phase 24: テキスト抽出品質強化）

- **`sort_by_reading_order(fragments: &mut [TextFragment])`** — テキスト抽出結果をコンテント
  ストリーム順から人間が読む順序（上から下へ、左から右へ）に並べ替えます。NaN/Infinity 座標を
  安全に処理します。複数段組レイアウトや右から左への言語対応で `extract_text_runs()` の出力後処理が必要な場合に便利です。

- **`glyph_name_to_char()` が `uni<XXXX>` パターンに対応** — AGL グリフ名デコーディングを拡張して
  AGL 2.0 スタイルの `uni0041` 形式に対応。16 進コードが直接 Unicode スカラーにマップされます。
  例えば `uni30A2` → `'ア'`（U+30A2）。16 進長の検証（1–8 文字）により、不正な形式をパニックさせずに
  静かに無視します。

- **`Document::extract_page_images(page) -> Vec<PageImage>`** — スキャン PDF ページから
  すべての画像を抽出します（以前は最大サイズの画像のみ返していました）。Image XObject が見つからない場合はエラーを返します。

### 追加（Phase 25: AI/RAG ユーティリティ）

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
