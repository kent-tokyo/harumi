# 教訓メモ

## Rustエコシステムの現状認識

### PDFクレートの二極化
Rustの既存PDFクレートは「既存PDFの低レイヤ操作（lopdf）」か「新規PDF作成（printpdf）」に二極化している。「既存PDFへの高レイヤな書き込み」というユースケースが完全に抜け落ちていた。これはライブラリ設計の機会として非常に大きい。

### pdf_oxide は創作用途向き
調査では pdf_oxide が有望に見えたが、そのAPIは `DocumentBuilder` パターンであり既存PDFの修正には向いていない。**既存PDFの修正には lopdf 一択**という結論は変わらない。速さや機能の多さに惑わされず、「既存ファイルを読める/書けるか」を最初に確認すること。

### CFF（OTF）フォントの現実
NotoSansCJK の一般配布形式はOTF（CFF outlines）であることが多い。allsorts v0.17 は CFF1 アウトラインのサブセット化には対応しているが、**CFF2 可変フォントは `"unexpected data version"` エラーで失敗する**。macOS のシステムフォント（`.otf`）の多くが CFF2 を使っている。
対策: OTF を弾かずに受け入れ、失敗時は `FontParse` エラーを返す。ユーザーには TTF バリアントを推奨する。

## 設計判断

### CIDフォントオブジェクトグラフは「最難関」と明記する
Type0 → CIDFontType2 → FontDescriptor → FontFile2 の4オブジェクトは相互参照が多く、1つのフィールド名を間違えても（`/Widths` vs `/W` など）あるビューアでは正常表示、別のビューアでは壊れるという問題が起きやすい。**複数のビューア（Preview、Chrome、Acrobat、Okular）での動作確認を必須にすること**。

### 座標系はPDF座標（左下原点）に統一
hOCR等のピクセル座標（左上原点）との変換はライブラリの外部責務とする。APIを純粋なPDF抽象に保つことで、OCR以外のユースケース（テキスト透かし、注釈など）にも再利用できる。座標変換ヘルパーは `ocr` feature flag に分離。

### インクリメンタルセーブは後回しでよい
PDFのIncremental Save（`/Prev` xrefチェーン）は実装が繊細で、任意の入力PDFに対して正確にバイトオフセットを計算する必要がある。MVPでは lopdf のフルリライトセーブで十分。既存オブジェクトは全て保持されるため、「既存PDFを壊す」問題は発生しない。

## allsorts の使い方

### バージョンをピン留めする
allsorts はAPIが変化しやすい。`Cargo.toml` でマイナーバージョンをピン留め（例: `allsorts = "0.17"`）し、マイナーアップデートでも手動で動作確認すること。

### シェーピング vs. 単純なGIDルックアップ
OCRの不可視テキストレイヤーでは合字やカーニングは不要。必要なのは「文字 → GID」のマッピングと、Widths配列用の標準送り幅だけ。allsortsのフルシェーピングパイプラインを起動する必要はない。

## GIDリマッピングバグ（最重要教訓）

**症状**: 埋め込んだフォントで日本語テキストが全て `.notdef`（□）に置き換わる。

**根本原因**: allsorts の `subset()` はグリフを 0..N に**再採番**する（入力 `gids` スライスの順番通り）。しかし旧実装は ttf-parser で取得したオリジナルGID（例: 847, 2031）をそのままコンテントストリームの hex 値として書き込んでいた。サブセット後のフォントには 0..N のGIDしか存在しないため、全グリフが存在しないGIDを指してしまい `.notdef` になっていた。

**修正**: `subset()` 呼び出し後に `orig_to_new: BTreeMap<u16, u16>` を構築し（`gids.iter().enumerate()` から作成）、`gid_to_char` と `gid_to_advance` を新GIDで再構築する。

```rust
let orig_to_new: BTreeMap<u16, u16> = gids.iter()
    .enumerate()
    .map(|(new_gid, &orig_gid)| (orig_gid, new_gid as u16))
    .collect();
```

**教訓**: フォントサブセット化ライブラリを使う際は、サブセット後のGID空間とオリジナルGID空間が異なることを常に意識する。allsorts は `gids[0]→0, gids[1]→1, ...` という単純な規則で再採番する。

## ToUnicode CMap の落とし穴

- `beginbfchar` ブロックは最大100エントリ/ブロック（PDF仕様の制限）
- 補助面文字（U+20000以上の CJK統合漢字拡張B など）はUTF-16BEでサロゲートペアになる
- CMap のテストで「PDF」という部分文字列を検索する場合、実際には各文字が個別のエントリ（`<0050> <0050>` など）として記録されており、連続した文字列では出現しない

## draw / image feature の実装教訓

### Feature flag の粒度：依存コストで分割する
`draw`（矩形・線）は追加依存ゼロ — 純粋な PDF オペレータ文字列を書くだけ。`image`（JPEG/PNG）は `image` クレート（~1MB）が必要。アドバイスは「`draw = ["image"]` で1つにまとめる」だったが、依存コストが異なる場合は分割する方が良い Cargo 設計。shapes-only のユーザーに画像デコーダをコンパイルさせる理由はない。

### ExtGState は /Resources に登録しないと無効
PDF の `gs` オペレータで参照するグラフィクス状態辞書は `/Resources /ExtGState` に名前付きで登録しなければならない。コンテントストリームに `/{name} gs` を書くだけでは動かない。opacity を使う全ての描画 op（矩形・線・画像）に対してページごとに `ExtGStateRegistry` で重複排除しながら登録する。

### JPEG は再エンコード不要（DCTDecode でパススルー）
JPEG バイト列の先頭 `\xff\xd8\xff` で判定し、`image` クレートで再デコードせずそのまま `Filter: DCTDecode` ストリームとして埋め込む。再エンコードすると画質劣化・ファイルサイズ増加・速度低下のいずれも起きる。PNG 等の他形式は `image` クレートで RGB 展開 → FlateDecode 圧縮。

### lopdf 0.40 の型: `Object::Real` は f32
lopdf 0.40 では `Object::Real(f32)` — `f64` を渡すとコンパイルエラー。`ExtGStateRegistry::to_lopdf_dict()` 内で `*key as f64 / 1000.0` と書いていたのを `as f32` に修正が必要だった。lopdf のバージョンアップで変わる可能性があるため注意。

### 単一バリアント enum の irrefutable パターン
`draw` feature が無効なとき `PendingOp::Text` しか存在しない。このとき `if let PendingOp::Text(t) = op` は「反駁不能パターン」として警告になる。`match` + `#[cfg(feature = "draw")]` アームの組み合わせで解決する。

### lopdf の Dictionary::get は Result を返す
`d.get(b"ca")` は `Result<&Object, lopdf::Error>` を返す。`Option::and_then` に渡す際は `.ok()` で `Option` に変換してから連鎖する。`Result::and_then` に `Option` を返すクロージャを渡すとコンパイルエラー（E0308）。

## add_text_box / add_polygon の実装教訓

### テキスト折り返しは save() 前に解決する
`add_text_box` は `save()` 時ではなく **呼び出し時** に折り返しを計算して複数の `PendingText` に分解する。理由: `save()` 時点では全フォントのサブセット化を一括処理しており、折り返し計算を混ぜると Pass の責務が混在する。呼び出し時に `ttf_parser::Face::parse` でグリフ advance を取得する方が実装もシンプル。

### ttf_parser は呼び出し時に使える（依存追加不要）
ttf_parser はすでに `finalize()` Pass 2 で依存している。`add_text_box` の呼び出し時点で `raw_fonts[font.0].ttf_bytes` にアクセスして `Face::parse` を実行することで、追加コストゼロで advance 幅を取得できる。

### greedy 折り返しのデータ構造
スペース位置を `last_space_byte: Option<usize>`（文字列中のバイトインデックス）と `width_at_word_start: f32`（そのスペース直後の幅）のペアで追跡する。折り返し時は `current[sp+1..]` を次行の先頭にし、`current_w - width_at_word_start` で幅を調整。ASCII スペース（1バイト）前提なので `sp+1` のバイト加算は安全。

### CJK 判定は Unicode ブロックで十分
CJK 文字か否かは `matches!(ch as u32, 0x3000..=0x9FFF | ...)` で十分。Unicode カテゴリ判定ライブラリは不要。CJK と判定した文字は任意の位置で折り返せるので、`last_space_byte` のバックトラックをせずに即座にブレークする。

### add_polygon の `filled: bool` で fill/stroke を切り替える
`filled=true` → `rg`（fill color）+ `f` オペレータ、`filled=false` → `RG`（stroke color）+ `S` オペレータ。PDF の塗り色（小文字演算子）とストローク色（大文字演算子）の区別は重要。混在させると意図しない色が適用される。

### feature gate なしのテキスト機能
`add_text_box` は `draw` feature なしで使える。PDF の draw 演算子を使わず、テキスト演算子（BT/ET）のみで完結するためで。feature gate の範囲を「何の PDF オペレータを使うか」で判断するのが適切。

## PNG SMask（真の透明度）の実装教訓

### SMask はメイン画像の `/SMask` キーで参照する（/Resources には登録不要）
PDF の SMask（ソフトマスク）は Image XObject の `/SMask` キーから間接参照される。ページの `/Resources /XObject` に登録するのはメイン画像だけでよく、SMask サブオブジェクトは登録不要。`doc.add_object()` で先に SMask のオブジェクト ID を確保し、メイン画像の dict に `dict.set("SMask", Object::Reference(smask_id))` で紐付ける。

### 白背景合成と SMask は排他的に切り替える
alpha=255 の画像（全不透明）に SMask を付けても無害だが、不要なサブオブジェクトが増えてファイルサイズが大きくなる。`rgba.pixels().any(|p| p[3] < 255)` で透明ピクセルの有無を判定し、全不透明なら従来の `Rgb` パスへ。透明ピクセルがあれば `RgbWithAlpha` パスで SMask を生成する。

### SMask は DeviceGray、BitsPerComponent 8
SMask Image XObject の `/ColorSpace` は `/DeviceGray`、`/BitsPerComponent` は `8`。RGB ごとに 1 バイトを書くメイン画像と違い、alpha チャンネルだけを 1 バイト/ピクセルで書く。`compress()` も有効でファイルサイズを削減できる。

### `add_image_with_opacity` との組み合わせは自動的に合成される
PNG の per-pixel alpha（SMask）と `add_image_with_opacity` の overall opacity（`/CA` ExtGState）は、PDF レンダラが自動的に `pixel_alpha × gs_opacity` として合成する。実装側で特別な処理は不要。

## セキュリティ・バグ修正の教訓

### MediaBox 配列の境界チェック
`arr[i]` で配列にインデックスアクセスする前に `arr.len() < 4` を確認しないとパニックになる。lopdf の `as_array()` は要素数を保証しない。公開 API ではパニックよりも `Err(...)` で返す方が常に望ましい。

### 二重 save() は `finalized: bool` フラグで防ぐ
`save()` を2回呼ぶと `finalize()` が2回走り、フォントが重複埋め込みされて PDF が壊れる可能性がある。`Document` に `finalized: bool` フィールドを追加し、`finalize()` の冒頭で `finalized && !pending.is_empty()` を確認して `InvalidInput` エラーを返す。`finalize()` 末尾で `self.finalized = true` をセットする。

### PNG の `w * h * 3` は u64 で計算する
`width: u32, height: u32` の積を `u32` のまま掛けると 65535x65535 程度で u32 オーバーフローし、`Vec::with_capacity` に誤った値が渡る。u64 で計算してから `as usize` にキャストする。また上限チェック（例: 200MP）を設けてメモリ枯渇を防ぐ。

### 間接 Contents 配列（InDesign 形式）
InDesign が出力する PDF は `/Contents` がストリームへの参照ではなく、**配列オブジェクトへの参照**（indirect array）になっていることがある。`Some(Object::Reference(r))` をそのまま「単一ストリーム参照」とみなすと、参照先が既存の配列であっても新たに 2 要素配列を作ってしまい、元のコンテンツが切れる。`doc.get_object(r)` で `Array` かどうかを確認し、配列なら直接 push する。

### JPEG スタンドアロンマーカー（RST0–RST7 / EOI 等）
JPEG マーカーには length フィールドを持たないものがある（`0xD0`–`0xD9`, `0x01`）。これらに対して `data[i+2..i+3]` を length として読むと、以降のマーカー位置計算が完全にずれる。`matches!(marker, 0xD0..=0xD9 | 0x01)` で先にガードして `i += 2; continue` する。

### Preview は CIDFontType2 + Identity-H の不可視テキスト選択が苦手
macOS Previewは `CIDFontType2` + `/Identity-H` エンコーディング + `3 Tr`（不可視レンダーモード）の組み合わせでテキスト選択が動かないことがある。Chrome（PDFium）・Adobe Acrobat では正常に選択・コピーできる。`pypdf` など PDF パーサーからのテキスト抽出も正常。動作検証は Preview ではなく Chrome か Acrobat で行うこと。

### 全公開 API に NaN/Infinity ガードを置く
f32 パラメータに NaN や Infinity が渡ると、PDF コンテントストリームに `NaN` や `inf` という文字列が書き込まれ、ビューアが壊れる。境界でのみ検証する原則に従い、全 `add_*` 公開メソッドの冒頭に `check_finite()` を入れる。内部ヘルパーへの伝播は防がなくてよい。`add_invisible_text_runs` のようなバッチ API ではループ内でランごとに呼ぶ。

### 画像デコード前に寸法チェックを行う（OOM 防止）
`image::load_from_memory()` は内部でピクセル全体をアロケートするため、200MP 上限チェックをこの呼び出しの「後」に置いても意味がない。`image::ImageReader::new(...).with_guessed_format()?.into_dimensions()?` を先に呼ぶとヘッダだけを読んでピクセルアロケートをしない。次に上限チェックをし、OKなら `load_from_memory` を呼ぶ順序が正しい。

### Vec::contains() の O(N²) に注意（フォントサブセット）
CJK フォントでは使用グリフ数が数千に達することがある。`gids.contains(&gid)` は O(N) なので全体で O(N²) になる。`HashSet<u16>` を並走させて O(1) でチェックするだけで、大規模テキストのサブセット処理が大幅に速くなる。`dedup()` も不要になる。

## OSSとしての学び

### 「痛みのあるニッチ」を狙う
pdf-lib（JS）が解決している問題をRustで再現する、という動機付けは非常にわかりやすく、READMEでも説明しやすい。競合比較が明快なOSSは採用されやすい。

### WASMサポートは差別化になる
pdfium-renderはWASMで動かない。これを「Zero C-dependency、WASMネイティブ」として前面に出すことで、ブラウザ/Edge環境向けのユーザーを取り込める。CI に `wasm-pack test --headless` を追加することでこの主張を担保する。

## ページ操作 API の実装教訓

### rotate_page: Object::Real の /Rotate を as_i64() で読むと 0 扱いになる
lopdf の `as_i64()` は `Object::Integer` にしか対応していない。`Object::Real(270.0)` を `.and_then(|o| o.as_i64().ok())` で読むと `None` → `unwrap_or(0)` で 0 扱いになり、270° + 90° = 90° と誤って計算される（正しくは 0°）。修正: `match` で `Integer` と `Real` を明示的に分岐する。

### rotate_page: i32 演算でオーバーフローする可能性がある
`/Rotate` を i32 にキャストした後 `current + degrees` を計算すると、クラフトされた入力（例: `/Rotate 2147483600` + `degrees = i32::MAX`）でデバッグビルドがパニックする。i64 で計算して最後に正規化すれば安全。

### finalize() 後の page() 呼び出しはデータをサイレントに捨てる
`save()` 後に `doc.page(1)?.add_invisible_text(...)` を呼ぶと、テキストは `pending` キューに積まれるが `finalize()` の二重呼び出しガード（`finalized && !pending.is_empty()`）によってエラーが返される。それまでの間テキストは「消えた」状態になる。`page()` に `finalized` ガードを追加してすぐにエラーを返すのが正しい設計。

### insert_blank_page の size は check_finite でガードが必要
他の公開 API は全て `check_finite()` を呼んでいるが、`insert_blank_page` は `size: (f32, f32)` をそのまま `Object::Real` に渡していた。NaN や Infinity は lopdf が PDF に書き込んでしまい、ビューアが破損ファイルとして扱う。`check_finite` + `> 0.0` チェックを追加すること。

### ページ削除後のオーファンオブジェクト
`remove_page` は `/Kids` から参照を除くが、ページオブジェクト自体は `self.inner.objects` に残る。lopdf はガベージコレクションを行わないため、出力ファイルにオーファンオブジェクトが含まれる。ファイルサイズが膨らむ原因になるが PDF の仕様上は無害（ビューアは無視する）。制限として文書化しておくこと。

### ネスト /Pages ツリーの平坦化と継承属性の消失
remove/insert/reorder はすべてルート /Pages の /Kids を全ページ ID で再構築し、各ページの /Parent を root に張り直す。これにより /Pages ツリーが平坦化される。副作用として、中間の /Pages ノードが持っていた継承属性（/Resources, /MediaBox, /CropBox 等）が失われる可能性がある。スキャン PDF（InDesign や Word 製でない）では問題になることは少ないが、制限として明記しておくこと。

## merge_from の実装教訓

### renumber_objects_with は impl Document (processor.rs) にある
lopdf の `processor.rs` は `impl Document { ... }` を定義しているので、`Document` に直接 `renumber_objects_with(starting_id: u32)` を呼べる。`Processor` という別の構造体が存在するわけではない。

### merge: ID 衝突回避は `renumber_objects_with(max_id + 1)` で
`renumber_objects_with(n)` は n から始まる連番を割り当て、`self.max_id` を更新する。`self.inner.max_id + 1` を渡すことで other の全 object ID が self の既存 ID と重ならない。

### get_pages() はツリーをたどるため extend 後も自分のページだけ返す
`self.inner.objects.extend(other.objects)` で other のオブジェクトを全部マージしてからでも、`self.inner.get_pages()` は self の trailer → Catalog → /Pages をたどるため、other のページは含まれない。other のページは /Kids に入るまで get_pages() に現れない。

### finalized は pending ops があるときだけ true になる
`finalize()` は `self.pending.is_empty()` なら早期リターンし、`self.finalized = true` をセットしない。テスト内で「finalized な document を作る」には、embed_font + page().add_text() で pending を作り、save_to_bytes() で flush する必要がある。pending なしで save_to_bytes() を呼んでも finalized のままにならない。

### remove_page の pending.retain と objects.remove は必ずセットで
`objects.remove(&target_id)` だけ入れると、pending ops が残っている場合に save() が「オブジェクトが見つからない」エラーを返す。`pending.retain` だけ入れると、page dict がオーファンとして残る（無害だがファイルが膨らむ）。両方セットで入れることで正確性とサイズを両立できる。

## Document::new / extract_pages の実装教訓

### Document が Debug を実装していないためテストで unwrap_err() が使えない
`Result<Document, _>` に対して `.unwrap_err()` を呼ぶと、`Ok` 側で `T: Debug` を要求されコンパイルエラーになる（`Document` は `Debug` を derive していない）。代わりに `.map(|_| ()).unwrap_err()` とすることで、`Ok` 値を `()` に変換してから `unwrap_err()` を呼ぶ。エラーが返ることだけ確認したいテストではこのパターンを一貫して使うこと。

### Document::new の実装パターン（lopdf ベース）
`lopdf::Document::with_version("1.4")` で空のドキュメントを作成し、`new_object_id()` でページオブジェクト ID と Pages ノード ID を確保する。`objects.insert()` で辞書オブジェクトを登録し、`add_object()` でカタログを登録し、`trailer.set("Root", ...)` でルートを設定する流れは `tests/helpers.rs` の `minimal_pdf_bytes()` と同じパターン。違いは、最終的に `Vec<u8>` に書き出すのではなく `Document { inner: lopdf_doc, raw_fonts: Vec::new(), pending: Vec::new(), finalized: false }` を返す点。`size` パラメータは `check_finite` + `> 0.0` ガードを通してから `Object::Real` に変換する。

### extract_pages の実装パターン（clone + /Kids 再構築）
`self.inner.clone()`（`lopdf::Document` は `Clone` を derive している）で内部ドキュメントをコピーし、新しい `new_inner` に対して変更を加える。`new_inner.get_pages()` で全ページの `ObjectId` を取得し、呼び出し元が指定したページ番号スライスに従って `/Kids` 配列を再構築する。指定されなかったページの dict オブジェクトは `new_inner.objects.remove(&page_id)` で削除する（孤児オブジェクトの削減）。`/Catalog` の `Outlines`・`AcroForm`・`/Names`・`/PageLabels`・`/OpenAction`・`/StructTreeRoot` はそれぞれ `catalog.remove(b"Outlines")` 等で除去する（削除済みページを参照していた場合にビューアが壊れるのを防ぐ）。返り値は `Document { inner: new_inner, raw_fonts: Vec::new(), pending: Vec::new(), finalized: false }`。`self` は変更しない。

## extract_text_runs 拡張の実装教訓（Phase 12）

### lopdf の StringFormat::Literal は自動エスケープする
`set_metadata` で `Object::String(bytes, StringFormat::Literal)` を使う場合、`(`, `)`, `\` を事前にエスケープする必要はない。lopdf の `write_string` が自動的に escape_indice を計算してエスケープする。セキュリティレビューで「Literal 文字列が壊れる」と指摘されやすいが、lopdf 利用時には false positive になる。実際に writer コードを確認してから判断すること。

### bfrange の `lo > hi` は saturating_sub で誤マッピングを生む
`parse_bfrange_line` で `lo > hi` の場合、`saturating_sub` が 0 を返すので `0..=0` のループが1回走り、`lo` に対して `dst_start + 0` の誤ったマッピングを挿入してしまう。`if lo > hi { return; }` のガードを追加することで正しくスキップできる。`saturating_sub` は「安全」に見えて意図しない副作用を起こすことがある。

### 未マップグリフで `?` を使うとフラグメント全体が消える
`decode_chars_to_fragment` の内側ループで `font_info.to_unicode.get(&gid)?` を使うと、1つでも未マップなグリフが含まれる文字列全体が `None` を返す（`?` が関数から早期 return するため）。正しい挙動は「そのグリフをスキップして継続」なので、`let Some(&ch) = ... else { continue };` パターンを使う。

### PDF リテラル文字列の octal エスケープは「最大3桁」
`\ddd` の octal エスケープは0–7の数字が続く間だけ最大3桁まで読む。`0–7` 以外の文字（例: `\89`）は octal の終端として扱い、`8` は次の通常文字として処理する。読み過ぎ（over-read）を防ぐため、ループの各イテレーションで桁数カウンタと値の範囲の両方を確認すること。

### CMap の `beginbfchar` 行は "N beginbfchar" 形式
PDF の ToUnicode CMap は `beginbfchar` の前に `N beginbfchar`（N はエントリ数）という形式で記述される。`line == "beginbfchar"` という完全一致チェックは必ず失敗する。`line.ends_with("beginbfchar")` または `line.contains("beginbfchar")` を使うこと。`beginbfrange` も同様。

## replace_text の実装教訓（Phase 13）

### フォント名衝突バグ: 新規埋め込みフォントは既存 PDF のフォント名と衝突してはならない
harumi が埋め込むフォントを `F0`, `F1` と命名すると、多くの PDF ジェネレータが同じ名前（`F0`, `F1`）を使っているため、`/Resources /Font` ディクショナリ上で既存フォントを上書きしてしまう。その結果、既存のコンテントストリームが新しいフォントの GID テーブルで解釈されてしまい、全テキストが `.notdef` や文字化けになる。`HR{idx}` のような衝突しにくいプレフィックスを使うこと。`HARUMI+HR{idx}` の形式（`HARUMI+` は PostScript 上の埋め込みフォントの慣例）が安全。

### 選択的リソース登録: マッチしなかった場合は既存リソースを一切変更しない
`rewrite_content_stream` は置換処理に実際に使ったフォント名の `HashSet<Vec<u8>>` を返す設計にする。`old_text` がどのページにも存在しない場合は、この `HashSet` が空になるため、`add_font_to_resources` を一切呼ばない。これにより既存の `/Resources /Font` ディクショナリを一切破壊しない。マッチが不確かな状況では「使ったものだけ登録する」が原則。

### TJ 配列スプリットアルゴリズム
PDF の `Tf`（フォント切替）演算子は `TJ` 配列の内部に置けない。よって `TJ` 配列の要素（文字列）が置換ターゲットにマッチした場合は、配列全体をスプリットする必要がある：
1. マッチ前の文字列要素 → 個別の `Tj` 演算子として出力
2. マッチ前の数値（カーニング）→ `Td` 演算子として吸収（`kern/1000 * font_size` をピクセル換算）
3. マッチ要素 → `/NewFont size Tf`, `<gid_hex> Tj`, 幅補正 `Td`, `/OldFont size Tf`
4. マッチ後の数値 → 幅補正 `Td` に加算してまとめる（保留カーニング）
5. マッチ後の文字列要素 → 個別の `Tj` 演算子として出力

### 幅補正の計算式が既存フォントと新フォントで異なる
既存 PDF の `/W` 配列の幅値は 1000 単位系（`adv / 1000 * font_size`）。一方 allsorts が返す `gid_to_advance` の値はデザイン単位系（`adv * font_size / units_per_em`）。この2つを混ぜて計算するとドリフトが生じる。それぞれ別の式で実幅を求め、差分を `Td` で補正すること。

### gid_to_advance は SubsetResult から clone して保存する
`allsorts` の `SubsetResult.gid_to_advance` は `EmbedParams`（フォント埋め込みロジック）にも渡す必要があるが、置換パスでも必要になる。Rust の move セマンティクスにより二重使用はコンパイルエラーになるため、`EmbedParams` に渡す前に `clone()` して `EmbedState` に別保存する。

### MSRV は「使っている言語機能の stable 化バージョン」を確認する
let-chain（`if let ... && let ...`）は Rust 1.88.0（2025-06-26 リリース）で stable 化。`edition = "2024"` は Rust 1.85 が最低要件だが、let-chain を使っている場合は 1.88 が実際の MSRV になる。`cargo build` が通るだけでなく、機能ごとの stable 化バージョンを確認して `rust-version` に記載すること。

## テキスト回転・fill+stroke・統合パス・cross-op replace の実装教訓（Phase 15）

### PDF Tm 演算子はテキスト行列を絶対位置で設定する
`Td` は現在位置からの相対移動だが、`Tm` は行列全体を絶対的にセットする。回転テキストでは `cos(θ) sin(θ) -sin(θ) cos(θ) x y Tm` という6要素の変換行列を発行する。`rotation_degrees == 0.0` の場合は従来の `Td` にフォールバックすることで後方互換を維持できる。`Td` の後に `Tm` を発行しても意味がない — `Tm` はそれ以前の Td 蓄積をすべてリセットする。

### fill+stroke 同時描画は PDF B 演算子を使う
PDF の塗りつぶし演算子: `f`（fill のみ）、`S`（stroke のみ）、`B`（fill-then-stroke）。`B` を使う際は `rg` (fill color) と `RG` (stroke color) の両方をパスの前に発行し、さらに `w` (line width) も指定する必要がある。`filled=true` かつ `stroke_width>0.0` の条件で `B` に切り替えるシンプルな三分岐で実装できる。

### `split_first_tj` テストパターン: GIDをハードコードせずに cross-op テストを書く
harumi の `add_invisible_text` は1呼び出し = 1 BT/ET ブロック = 1 Tj 演算子を生成するため、cross-op マッチング（同一 BT/ET 内に複数 Tj が必要）を直接テストできない。解決策: lopdf でハルミ生成 PDF を読み込み、最初の `<HEX> Tj` の hex 文字列を文字境界で2分割して2つの Tj に変換するヘルパー `split_first_tj(pdf_bytes, split_at_char)` を作る。CID フォントは1文字 = 4 hex 文字なので `split_at_char * 4` が分割位置になる。GID の実値に依存しないため、フォントが変わっても壊れない。

### cross-op replace の設計: CharSegment バッファと演算子境界またぎ検索（Phase 15）
`collect_char_segments()` は BT/ET ブロック単位で `CharEntry { ch, op_idx, raw_bytes }` を蓄積する。`find_cross_op_matches()` は文字列レベルのサブストリング検索を行い、マッチが複数演算子にまたがる場合のみ cross-op マッチとして扱う（`first_op != last_op`）。Td/Tm 演算子をまたぐマッチは位置が変わるため拒否するのが正しい設計（テキストを視覚的順序でなくストリーム順序で扱う場合に誤置換が起きる）。

## FlowDocument / HTML renderer の実装教訓（Phase 16）

### FlowDocument の座標系変換
FlowDocument は論理座標系（`content_y`：上端からの距離、下向き正）を使う。PDF 座標（左下原点）への変換: `pdf_baseline_y = page_h - margin_top - content_y - font_size`。テーブルセルの行頭テキスト y は `row_top_y - cell_pad - font_size - i * line_h`（数学的に等価）。座標系の混在を避けるため変換は `pdf_baseline_y` / `pdf_top_y` メソッドに集約する。

### PageHandle を使うときのボローチェッカー対応
`PageHandle<'_>` は `Document` を可変ボローする。`PageHandle` を生成する前に `FontHandle`（`Copy`）・ページ番号（`u32`）・座標（`f32`）などの値を変数にコピーしておく。`{}` ブロックで `PageHandle` をスコープ外に出してから次の `self.inner` アクセスを行う。テーブル描画では `ensure_space`（`self` を変更する可能性あり）を呼んだ後に座標を計算し、PageHandle を生成する設計が必要。

### Vec<ElementRef<'a>> の生存期間不変性
`Vec<T>` は `T` に対して不変（invariant）。`ElementRef<'_>` の匿名生存期間 `'_` が `process_one` と `walk_iterative` で別の生存期間として推論されるため、`stack.push(child)` でコンパイルエラーになる（"lifetime may not live long enough"）。解決: `walk_iterative<'a>` と `process_one<'a>` に明示的な名前付き生存期間を導入し、`Vec<ElementRef<'a>>` と統一する。

### 反復的 DFS で再帰によるスタックオーバーフローを防ぐ
HTML の `<div>` が5000段重なるとデフォルト 8 MB のスタックを超えて再帰ウォーカーがパニックする。明示的な `Vec<ElementRef<'a>>` スタックを使った反復的 DFS（`walk_iterative`）に切り替えることで、メモリが続く限り任意の深さを処理できる。子を逆順にプッシュすることで深さ優先・左→右の処理順を維持する。

### CSS セレクタ `"tr"` / `"li"` は全子孫にマッチする
`scraper` の CSS セレクタ（`table_element.select(&Selector::parse("tr").unwrap())`）は直接子要素だけでなく全子孫にマッチする。ネストしたテーブル内の `<tr>` が外側テーブルの行として収集される。修正: CSS セレクタを使わず `children().filter_map(ElementRef::wrap)` で直接子要素のみを走査する `table_rows()` ヘルパーを使う。`process_list` も同様（直接の `<li>` 子要素のみ）。

### max_pages DoS 防止パターン
信頼できない HTML 入力から PDF を生成する場合、無制限な改ページによるメモリ枯渇を防ぐため `max_pages` 上限を設ける。`ensure_space` 内でページ追加前に `page_count >= max_pages` を確認し `Error::InvalidInput` を返す。デフォルト値 2000 は一般的な文書サイズには十分で、悪意ある入力への制限として機能する。

### HTML5 パーサの `<p>` 自動クローズ
html5ever は `<p>` 開始タグで前の `<p>` を自動クローズする。`"<p>".repeat(200)` は200個の空の `<p>` 要素になり、`push_paragraph` が空文字列をスキップするためコンテンツが生成されない。テストで `max_pages` を超えさせるには `format!("<p>Paragraph {}</p>", i)` のようにコンテンツを含む `<p>` が必要。

## PDF Image XObject 抽出の実装教訓（Phase 17）

### スキャン PDF 専用 API — 「PDF のレンダリング」との混同を避ける
`extract_page_image` は「ページに埋め込まれた Image XObject を取り出す」機能であり、「PDF をラスタ画像に変換する」機能ではない。テキスト PDF・ベクター PDF には Image XObject が存在しないため `Error::InvalidInput` を返す。pure Rust で PDF の完全レンダリングは不可能（`pdfium-render` 等は C++ 必須）。この設計判断はスコープを「OCR ワークフローのスキャン PDF」に明確に絞り込んでいる。

### `/Resources/XObject` のナビゲーション: resolve_dict パターン
PDF のリソース辞書（`/Resources /XObject`）は、インライン辞書の場合と間接参照（`/Reference`）の場合の両方があり得る。ページ辞書の値を直接 `as_dict()` で扱うと間接参照のケースでパニックになる。`resolve_dict(doc, obj)` として参照を辿ってから辞書を取得するヘルパーが必要。このパターンはすでに `src/extract.rs` に実装されており、`pub(crate)` に昇格させることで `extract_image.rs` から再利用できる。

### DCTDecode（JPEG）はパススルーで返す
PDF に JPEG が埋め込まれている場合、ストリームフィルターは `DCTDecode` になる。このストリームのバイト列はそのまま有効な JPEG ファイルになるため、再エンコード不要でそのまま返せる。image クレートで再デコードすると画質劣化・ファイルサイズ増加・速度低下が同時に起きる。JPEG かどうかは `filter_name()` で確認するが、PDF の `/Filter` は単一の Name（`/DCTDecode`）か単一要素の Array（`[/DCTDecode]`）の両形式があるため、両方を処理する必要がある。

### FlateDecode → PNG エンコード: pixel buffer size 検証が必須
FlateDecode ストリームは `stream.decompress()` で raw ピクセルバイトを取得できる。その前に `width * height * channels` のバイト数が一致するかを検証しないと、`RgbImage::from_raw()` が `None` を返してエラーになる（または不正なサイズでパニックする可能性がある）。チャネル数は `/ColorSpace` が `DeviceGray` なら 1、それ以外（`DeviceRGB` など）は 3 とみなす。PNG エンコードには `image::DynamicImage::write_to(&mut Cursor::new(Vec::new()), ImageFormat::Png)` パターンを使う。

### pub(crate) 昇格パターン: 既存プライベート関数のスコープ拡張
`src/extract.rs` の `resolve_dict` と `src/draw/image.rs` の `parse_jpeg_dims` はどちらも元はプライベートだったが、新しいモジュール `extract_image.rs` から再利用するために `pub(crate)` に昇格させた。外部 API を変えずに内部モジュール間での再利用を可能にする最小限の変更。

### Filter 形式の差異: Name と Array の両方を処理する
PDF 仕様では `/Filter` の値は単一の `Name`（例: `/FlateDecode`）か、Array（例: `[/FlateDecode]`）のどちらでも許されている。Image XObject では実質的に1つのフィルターしか持たないが、ビューアの実装によっては Array 形式で書かれることがある。`filter_name()` ヘルパーで両形式を統一して扱うことで、呼び出し側のコードをシンプルに保てる。

### 複数 XObject がある場合: 最大面積（Width × Height）を返す
スキャン PDF のページには通常 1 枚だけ Image XObject があるが、サムネイルやスタンプが混在することもある。複数ある場合は `width * height` が最大のものを返す「best effort」で対応する。`u64` で計算して u32 オーバーフローを避けること。

## リンクアノテーション + ブックマーク + ヘッダ/フッタの実装教訓（Phase 18 / v0.5.0）

### PDF /Annots 配列への追記は3ケースを処理する
ページ辞書の `/Annots` は「直接配列」「間接参照で配列」「存在しない」の3パターンがあり得る。`append_annotation_to_page` のような専用ヘルパーを実装し、すべてのケースを統一して処理するのが正しい設計。間接参照の場合は `doc.get_object_mut(ref_id)?` で配列を取り出して push する。存在しない場合は新しい配列を `add_object` で作成してから /Annots に設定する。

### 非 ASCII ブックマークタイトルは UTF-16BE + BOM が必要
PDF の `/Title` フィールドなどのテキスト文字列（text string）は、ASCII 外の文字を含む場合は必ず UTF-16BE エンコーディング + BOM（`0xFE 0xFF`）でエンコードしなければならない。lopdf の `as_str()` は `&[u8]` を返すので、先頭2バイトが `[0xFE, 0xFF]` なら `u16::from_be_bytes` でデコードしてから `String::from_utf16` に渡す。ASCII テキストはそのままリテラル文字列で保存できる。

### build_outlines_from_bookmarks は既存アウトラインを保持する設計が必要
既存 PDF に `/Outlines` がある状態で `add_bookmark` → `save()` を呼ぶと、新しいアウトラインを作って `/Catalog` に上書きするだけでは既存のブックマークが消失する（データ消失バグ）。正しい実装は: `/Catalog` に既存 `/Outlines` があれば末尾の `/Last` ノードの `/Next` を新しい最初のアイテムに張り直し、既存ルートの `/Last` と `/Count` を更新する。既存アウトラインがなければ通常通り新規ルートを作成して `/Catalog` に登録する。回帰テストで「save → reload → add_bookmark → save → reload → 件数確認」を書くことで検証できる。

### lopdf 0.40 の `as_str()` は `&[u8]` を返す（`&str` ではない）
lopdf 0.40 の `Object::as_str()` は `Result<&[u8]>` を返す。`&str` を期待して `assert_eq!(result, "some string")` と書くとコンパイルエラー（E0277）になる。ASCII 文字列との比較は `result == b"some string"` または `std::str::from_utf8(result).unwrap() == "some string"` とする。UTF-16BE でエンコードされた文字列（CJK など）は `Object::String(bytes, _)` をパターンマッチして `bytes` を取り出す方が確実。

### テスト: `doc.inner` はプライベートフィールドなので統合テストから直接参照できない
`Document::inner` は `pub(crate)` であり統合テスト（`tests/` ディレクトリ）からは参照できない。「save 前のページ ObjectId を取得して save 後に照合する」ようなテストは、代わりに「save → reload したドキュメントからページ ID を取得して比較する」設計に変える。オブジェクト ID は save/reload サイクルを経ても変わらないため、この方法で正しく検証できる。

### 共通内部ロジックの重複排除: 型パラメータの代わりにクロージャや `old_texts: &[&str]` スライスを使う
`find_cross_op_matches`（`ResolvedReplacement` を受ける）と `find_cross_op_matches_preserve`（`TextReplacePreserveOp` を受ける）は、両者とも `r.old_text: String` を持つのに別関数として ~150行重複していた。両入力型から `old_text` を `&[&str]` スライスとして事前に抽出し、共通のコアループ `find_cross_op_matches_inner(ops, old_texts, fonts)` に渡すことで重複を排除できる。出力の構造体フィールド名（`orig_font_name` vs `font_name`）の差異はリネームして統一する。`CrossOpMatchPreserve` のような「ほぼ同じ構造体」を作るより、1つの構造体にフィールド名を統一する方が保守性が高い。

### セキュリティ監査: 「構造の存在確認」だけでなく「値のラウンドトリップ」もテストする
アノテーション/ブックマークの実装後に `lopdf::Document` を reload して `/Annots` や `/Outlines` の存在を assert するだけでは不十分。URI 文字列・/Dest ページ参照・/Title バイト列など、「書き込んだ値が正しく保存・復元されているか」を検証するセマンティックテストが必要。これらのテストを足してから「done」宣言することがアドバイザーの推奨。

## MCP 翻訳ワークフローの実装教訓（Phase 28）

### `resubset` / `wrap` は CIDToGIDMap=Identity 前提。失敗時は `new_font` に切り替える
`replace_text_resubset` は既存CIDフォントのGID対応を読み、フォントサブセットを再構築して全ページを再エンコードする。そのため既存PDFが `/CIDToGIDMap /Identity` でない場合、元CIDとGIDの対応を安全に推定できず `replace_text_resubset only supports CIDToGIDMap=Identity` で失敗する。これはKanto ChemicalのSDS（`J_10005.pdf`）でも発生した。翻訳用途でレイアウトを保ちたい場合は、既存ページ構造を維持しつつ該当テキストだけ新規埋め込みフォントへ切り替える `pdf_replace_text` の `mode: "new_font"` が現実的な回避策になる。`harumi-ai` は基本的に overlay mode を使い、再生成が必要な場合だけ new document に切り替える方が運用しやすい。

### MCPツールは低レベル保存エラーをユーザー向け診断に変換する
`doc.save()` まで遅延されるエラーをそのまま `FILE_WRITE_ERROR` として返すと、利用者にはパスや権限の問題に見える。原因がフォントマップ非対応なら `UNSUPPORTED_FONT_MAP` のように意味のあるコードを返し、エラー文に「非Identity `CIDToGIDMap`」「`mode: "new_font"`」「Unicode TTF」を含める。MCPはLLMやIDEから呼ばれるため、次に試すべきパラメータまで返す方が実用的。

### PDF翻訳は抽出・翻訳・置換リストを成果物として残す
`pdf_extract_all_pages` の出力を見て、PDF内のテキスト断片の粒度（見出し・項目・行末分割）を把握してから置換する。長い文は元PDFの行分割に合わせて短く訳すとレイアウト崩れが減る。翻訳結果は直接コマンドに埋め込むのではなく `*_en_replacements.json` のような置換リストに残すと、再生成・レビュー・微修正が容易になる。`harumi-ai` の overlay mode はこの方針と相性がよい。

## TTF サブセッターの実装教訓（v1.3.1 バグ修正）

### GSUB/GPOS/gvar などを含む埋め込みフォントは macOS Core Text に拒否される
`src/font/ttf_subset.rs` の TTF サブセッターがオプションテーブルをブラックリスト方式で除外していた（`cmap`, `OS/2`, `VORG` のみ）。ほとんどのオプションテーブルがそのままコピーされていたため、サブセット後のフォントに以下が含まれていた:
- `GSUB` / `GPOS` / `GDEF` / `BASE`: OpenType レイアウトテーブル（GID参照大量）
- `gvar` / `fvar` / `avar` / `HVAR` / `STAT`: 可変フォントテーブル（GID索引データ）
- `post`: `numGlyphs` フィールドが `maxp.numGlyphs` と不一致
- `vhea` / `vmtx`: 縦書きメトリクス（GID索引）

これらのテーブルに含まれる GID 参照はサブセット後に無効になる（存在しない GID を指す）。NotoSansJP のような大規模フォントでは 7000+ GID のうち 50 程度にサブセットされるため、ほぼすべての参照が無効になる。macOS Core Text はこの不整合を検出してフォント全体を拒否するため、全グリフが ● に置き換わる。

**修正**: ホワイトリスト方式に変更。PDF CIDFont 埋め込みに必要なテーブルのみ保持する。Identity-H エンコーディング + CIDToGIDMap=Identity の構成では OpenType シェーピング（GSUB/GPOS）が適用されないため、これらのテーブルは安全に削除できる。

```rust
// PDF CIDFont 埋め込みに必要なテーブルのみ保持
// head/hhea/maxp/glyf/loca/hmtx: 必須コア
// fpgm/prep/cvt/gasp: ヒンティング（安全）
// それ以外はすべて除外
```

### コンポジットグリフのコンポーネント GID は verbatim コピーで壊れる
TTF のコンポジットグリフデータには「コンポーネント GID」フィールドがあり、元フォントの GID を参照する。サブセット化で GID が 0..N に再採番されるが、`build_glyf` が glyph data を verbatim コピーしていたため、コンポーネント GID が旧GIDを指したままになっていた。サブセット後のフォントでその GID は異なるグリフを指すか、存在しない。修正: `rewrite_composite_gids()` でコンポーネント GID を新GIDに書き換える。

### GlyphRemapper はコンポジット依存 GID を含まない
`subset.rs` の `GlyphRemapper` はリクエストされたグリフのみを保持し、`ttf_subset.rs` 内で `collect_composite_deps()` によって追加されるコンポジット依存 GID は含まない。そのため `remapper.get(orig)` が返す「新GID位置」が、実際のフォント内の位置とずれる場合があった（コンポジット依存 GID がリクエストされた GID の間に挿入されると位置がひとつ以上ずれる）。修正: `subset()` が最終的な `gids_to_keep` セット（コンポジット依存含む）を返すようにし、`gid_to_char` / `gid_to_advance` の計算に使用する。

### 新しいフォントを試みる場合は必ず Preview と PSPDFKit で確認する
フォント埋め込みのバグは構造テスト（PDF reloadできる、ToUnicode CMapが存在する）では検出できない。実際の描画ではじめて ● が現れる。フォントサブセット化を変更した場合は必ず:
1. `cargo test` で既存テスト全通過
2. `Document::new() + embed_font + add_text` でテスト PDF を生成
3. macOS Preview でファイルを開き、文字が正しく表示されることを確認
4. PSPDFKit があれば同様に確認

## PDF コンテンツスケール・オーバーレイ・ファイル添付の実装教訓（Phase 29 / v1.4.0）

### feature gate の見落としが docs.rs 非表示の原因になる
`add_text_with_opacity` と `add_text_with_rotation` は `#[cfg(feature = "draw")]` の impl ブロック内に定義されており、`default = []` の harumi をビルドする docs.rs にはこれらが表示されなかった。解決策: `Cargo.toml` に `[package.metadata.docs.rs]` セクションを追加し `features = [...]` ですべてのフィーチャーを指定する。feature gate のついたメソッドが docs.rs に出ない場合は、まず docs.rs のビルド設定を確認すること。

### scale_page_content は `cm` ストリームをコンテンツ先頭に追加する
PDF の `cm`（Concatenate Matrix）演算子はページ内の全座標変換に影響するため、既存コンテンツの前に適用する必要がある。`append_to_contents` とは逆の `prepend_to_contents` ヘルパーが必要になる。実装パターン: `/Contents` が `Reference(r)` の場合は `Array([new_ref, original_ref])` に変換；`Array(arr)` の場合は `arr.insert(0, new_ref)` で先頭に挿入；InDesign 形式（`Reference` が `Array` を指す）も考慮して `doc.get_object(r)` で配列かどうかを確認する。

### overlay_from: /Resources は必ず親チェーンを遡る
PDF のページ辞書は `/Resources` を持たず、親 `/Pages` ノードから継承していることがある（Word・InDesign 製 PDF でよく見られる）。harumi 生成の PDF のみをテストフィクスチャに使うと常に通過するが、実際のドキュメントで無リソースになる。`inherited_resources(doc, page_id)` ヘルパーで `/Parent` チェーンを最大 32 段遡って `/Resources` を探すことで、あらゆる PDF に対応できる。同様に MediaBox も `inherited_media_box_raw()` で親チェーンを遡る。

### overlay_from: BBox は `[x1, y1, x2, y2]`（parse_box_array の `[x,y,w,h]` と異なる）
`PageHandle::media_box()` は `parse_box_array()` を経由して `[x, y, w, h]` 形式を返すが、Form XObject の BBox は PDF 仕様上 `[x1, y1, x2, y2]` で指定する。`inherited_media_box_raw()` を別途実装して生の `[x1, y1, x2, y2]` を返すようにすることで、ゼロ原点でないページ（例: `MediaBox [10 10 605 852]`）でもオーバーレイが正しい位置に描画される。

### overlay_from: Form XObject の /Resources にページリソースをコピーする
Form XObject は自分の `/Resources` 辞書を持てる。オーバーレイ元ページの `/Resources` をそのまま Form XObject の `/Resources` に設定することで、フォント名・画像名の名前空間が Form XObject 内部に閉じ、ベースページとの衝突が発生しない。`with_resources_dict_mut` / `add_xobject_to_resources` ヘルパーはフィーチャーゲートが不要（lopdf 型のみ使用）なので cfg を外すことで overlay_from から利用できる。

### overlay_from: テストは「Do が出力に存在する」まで確認する
`overlay_from_produces_valid_pdf` が「ページ数が 2」「reload できる」だけを assert しても、Do 演算子が出力されなくてもテストが通過してしまう。lopdf で直接 `/Resources/XObject` に `OVRL0` があること、コンテンツバイト列に `"Do"` が含まれることを assert することで、機能の実効性まで確認できる。

### PDF 名前ツリーの /Names 配列は昇順ソートが必須
PDF 仕様 §7.9.6 では名前ツリーの `/Names` 配列のキーを昇順ソートと定めている。Adobe Acrobat や PDF/A バリデーターはバイナリ探索でキーを検索するため、ソートが保証されない場合に添付ファイルが見えなくなる。`attach_file` ではエントリ追加後に `/Names` 配列を `(key_bytes, value)` ペアとして取り出してソートし直す。テストでは逆順に添付してから生の配列を lopdf で検査し、昇順になっていることを確認する（`list_attachments` は線形スキャンするため誤りを検出できない）。

### `pdf_text_string()` は `Object` を返す（`Vec<u8>` ではない）
既存の `pdf_text_string(s: &str) -> Object` ヘルパーは ASCII なら `Literal`、非 ASCII なら UTF-16BE+BOM の `Object::String(...)` を返す完成した Object を返す。これを `Object::String(pdf_text_string(filename), ...)` のように二重ラップしようとするとコンパイルエラー（型不一致）になる。正しい使い方: `fs_dict.set("UF", pdf_text_string(filename))` のように Object のまま渡す。

## クロス Tf テキストマッチングの実装教訓（Phase 34）

### 日本語 PDF の1行がなぜ複数フォントランに分かれるか
PDF の1つの視覚的な行内に `Tf` 演算子が多発する。例: 本文漢字に `F1`、括弧・約物に `F2`（文字コード体系が異なる専用フォント）。overlay 抽出は `"化学品及び（SDS）"` を1行として返すが、`collect_char_segments()` は `Tf` のたびにセグメントを分割するため、`count_matches_in_page()` はどのセグメントにも完全な文字列を見つけられず 0 を返す。これが GHS PDF での 14% 止まりの根本原因。

### クロス Tf マッチングの設計パターン
既存の「サンプ演算子クロスマッチ（`Td` またぎ）」と同様のパターンを `Tf` に対しても適用する:
1. **収集**: `collect_cross_tf_segments()` — `Tf` をまたいで文字を統合収集（`Td` による縦方向移動 / `Tm` のみで分割）
2. **マッチ**: `find_cross_tf_matches_inner()` — `all_text` ホワイトリストに `b"Tf" => true` を追加
3. **二重カウント防止**: `has_tf` ガードで Tf を含まない op 範囲を除外（同一フォントのクロス演算子マッチは既存の `find_cross_op_matches_inner()` が処理するため）
4. **リストア**: `CrossOpMatch.font_name` に最後のマッチ文字のフォント名を設定（emit 時に `/restore_font size Tf` として出力する対象）
5. **中間 Tf 抑制**: `rewrite_content_stream()` の `b"Tf" if in_bt =>` アームに `op_role` チェックを追加し、role==1（中間 op）の `Tf` を `Td` と同様に削除する

### `CharEntry` に `font_name` フィールドが必要な理由
クロス Tf セグメントでは1つのセグメント内に複数フォントの文字が混在する。`orig_width`（幅補正計算に使用）を per-char の正しいフォントで計算するために、各 `CharEntry` がどのフォントに属するかを保持する必要がある。`push_chars_from_bytes()` はすでに `font_name: &[u8]` を受け取っているため、`CharEntry` にフィールドを追加するだけでよい。

### マクロで Rust のボローチェッカー問題を回避する
`collect_cross_tf_segments()` では「フラッシュ（push + take）」を複数箇所で呼ぶ必要がある。クロージャだと `cur_chars`（可変ボロー）と `cur_font`（不変ボロー）を同時に保持できずコンパイルエラーになる。`macro_rules! flush_seg!()` でインライン展開することで解決できる。Rust でコレクションとその参照を同時に扱うクロージャはボローチェッカーに引っかかりやすいため、このパターンを覚えておくこと。
