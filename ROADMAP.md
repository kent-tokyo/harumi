# harumi roadmap

この文書は、現在の対応範囲と次の開発判断だけをまとめた公開用ロードマップです。
実装履歴は [`CHANGELOG.md`](CHANGELOG.md)、API の説明は [`docs/API.md`](docs/API.md)、
機能一覧は [`docs/FEATURES.md`](docs/FEATURES.md) を参照してください。

## 現在の基準

- `harumi` v1.23.3、MSRV Rust 1.88、edition 2024
- `harumi-ai` v0.10.1、`harumi-mcp` v0.1.2
- Pure Rust を基本とし、通常依存と WASM 依存に C/C++ ランタイムを追加しない
- 既存 PDF の CJK 抽出・置換・overlay・診断と、新規 PDF の帳票組版を主対象とする
- 外部 renderer の利用は optional adapter または検証用 oracle として扱う

## 完了した基盤

- CJK TrueType/TTC フォントの埋め込み、サブセット化、ToUnicode
- テキスト抽出、フォント維持置換、再サブセット、overlay
- ページ操作、結合・分割、注釈、フォーム、メタデータ、署名
- 図形・画像・透明度、HTML→PDF、`FlowDocument`、WASM 境界
- Form XObject、ページツリー継承、contents/resources の安全な処理
- 表、結合セル、ページ分割、ヘッダー・フッター、脚注、TOC、layout diagnostics
- Poppler を使った固定 fixture の仕様・帳票検証と、比較用の機械可読 manifest
- CJK/Latin 帳票の固定比較で、保存・再読込後のヘッダー・ページ番号フッター再利用
- ページ操作前後の構造監査と mutation report（ページ・オブジェクト・リソース数）
- CJK・画像・URLリンク・メタデータを含む最小writerのsave/reload・決定性fixture
- Unicode改行境界の固定fixtureと、UAX #14・辞書hyphenation・視覚的RTL/縦書きを分けた制約
- CJK編集後のsave/reload・構造監査・Poppler描画を一つにしたPDF仕様fixture

## 現在の対応範囲

v1.23.0 の組版契約は、CJK/Latin 混在段落、表、ページ分割、テンプレート、
基本的な HTML break semantics を対象とします。次の機能は未対応または
best effort です。

- 完全な Unicode Line Breaking
- 視覚的な bidi / RTL shaping と縦書きの完全互換
- 任意の block grouping と、完全な HTML/CSS 互換
- Pdfium、Chrome、Acrobat と同一の pixel parity
- 競合ライブラリの全 API の再実装

## 次の優先順位

### 1. 共通の証拠契約

Phase 52 の五領域を同一条件で比較できる状態にします。

- viewer-like editing
- 複雑な帳票・組版
- PDF object surgery
- bulk extraction / Markdown / JSON
- 軽量 Pure Rust writer

各領域で fixture、runner、artifact report、正しさの oracle、runtime boundary を
固定します。速度・メモリ・成果物サイズは同じ入力と環境で測り、workflow fit と
性能を混同しません。画像差分は診断情報として扱い、単一の総合点にはしません。

### 2. 組版の境界を明確にする

Unicode の改行境界、opt-in の hyphenation、RTL/Bidi、縦書き、脚注参照、残りの
HTML/CSS semantics を、機能ごとの fixture と明示的な制約付きで追加します。
shaping や視覚的な reordering を実装しない場合は、公開 API とドキュメントで
best effort の範囲を保ちます。

### 3. ドメイン別の実証

次の順で実装・測定します。

1. Pdfium を標準依存にせず、編集前後の抽出・構造保存・描画検証を固定する
2. CJK/Latin 帳票の表構造、改ページ、抽出順、テンプレート再利用を検証する
3. 高レベルの構造操作と mutation report を拡張する
4. 大規模抽出の page batch、JSON/Markdown、警告、再開可能な失敗を検証する
5. 最小 writer の CJK、画像、リンク、メタデータ、決定的 save/reload を検証する

機能追加は、fixture と保存後の検証で再現できる欠陥に限定します。未知の PDF
構造を暗黙に正規化せず、未対応部分は警告またはエラーとして返します。

## 完了条件

Phase 52 の各領域は、次を満たした時点で完了とします。

- 固定 fixture、実行コマンド、artifact report が保存されている
- 抽出文字列・順序・座標・ページ geometry・構造保存を検証できる
- renderer と native runtime のバージョン、feature flag、依存境界が明記されている
- 同一条件の結果を再現でき、性能主張の対象 workload が明確である
- README と API docs が実装済み範囲と non-goal に一致している

## 検証コマンド

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
bash scripts/check-pdf-spec-coverage-render.sh
bash scripts/check-report-generation-fixture.sh
```

Pdfium の runtime 検証は、明示した `PDFIUM_LIBRARY_PATH` と
`PDFIUM_LIBRARY_SHA256` がある場合だけ実行します。未検証環境では golden を
生成せず、compile-only の結果と分けて報告します。
