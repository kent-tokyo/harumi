# harumi roadmap

この文書は、現在の対応範囲と次の開発判断だけをまとめた公開用ロードマップです。
実装履歴は [`CHANGELOG.md`](CHANGELOG.md)、API の説明は [`docs/API.md`](docs/API.md)、
機能一覧は [`docs/FEATURES.md`](docs/FEATURES.md) を参照してください。

## 現在の基準

- `harumi` v1.23.5、MSRV Rust 1.88、edition 2024
- `harumi-ai` v0.10.1、`harumi-mcp` v0.1.2
- Pure Rust を基本とし、通常依存と WASM 依存に C/C++ ランタイムを追加しない
- 既存 PDF の CJK 抽出・置換・overlay・診断と、新規 PDF の帳票組版を主対象とする
- 外部 renderer の利用は optional adapter または検証用 oracle として扱う

## 完了した基盤

- CJK TrueType/TTC の埋め込み、サブセット化、ToUnicode、抽出、置換、overlay
- ページ操作、結合・分割、注釈、フォーム、メタデータ、署名、構造監査
- 図形・画像・透明度、HTML→PDF、`FlowDocument`、表、脚注、TOC、WASM 境界
- Form XObject とページツリーの継承を含む安全な `contents` / `resources` 処理
- 固定 fixture による保存・再読込、抽出、帳票、構造、Poppler 検証
- CJK/Latin 帳票のテンプレート再利用と、最小 writer の決定性検証
- SVG の限定サブセット（図形、曲線、変換、マーカー、CSS 色、`metadata`）

## 現在の対応範囲

v1.23.5 の組版契約は、CJK/Latin 混在段落、表、ページ分割、テンプレート、
基本的な HTML break semantics を対象とします。次は未対応または best effort です。

- 完全な Unicode Line Breaking
- 視覚的な bidi / RTL shaping と縦書きの完全互換
- 任意の block grouping と、完全な HTML/CSS 互換
- Pdfium、Chrome、Acrobat と同一の pixel parity
- 競合ライブラリの全 API の再実装
- SVG の完全なブラウザ互換（対応範囲は `docs/API.md` を参照。図形のみならフォント不要）

## 次の優先順位

### 1. 共通の証拠契約

Phase 52 の五領域（編集、帳票、object surgery、bulk extraction、writer）を、
同じ fixture、runner、oracle、runtime 境界で比較します。速度・メモリ・成果物
サイズは入力と環境を固定し、用途適合度とは分けて報告します。

### 2. 組版の境界を明確にする

Unicode 改行、opt-in hyphenation、RTL/Bidi、縦書き、脚注参照、HTML/CSS semantics
を機能別 fixture で追加します。shaping や視覚的 reordering を扱わない部分は、
公開 API とドキュメントで best effort の範囲を明示します。

### 3. ドメイン別の実証

1. Pdfium を標準依存にせず、編集前後の抽出・構造保存・描画を固定する
2. CJK/Latin 帳票の表構造、改ページ、抽出順、テンプレート再利用を検証する
3. 構造操作と mutation report、大規模抽出の再開可能性を拡張する
4. 最小 writer の CJK、画像、リンク、メタデータ、決定的 save/reload を検証する

機能追加は、fixture と保存後の検証で再現できる欠陥に限定します。未知の構造は
暗黙に正規化せず、未対応部分は警告またはエラーとして返します。

## 完了条件

Phase 52 の各領域は、次を満たした時点で完了とします。

- 固定 fixture、実行コマンド、artifact report が保存されている
- 抽出文字列・順序・座標・ページ geometry・構造保存を検証できる
- renderer/native runtime の版、feature flag、依存境界が明記されている
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
