# harumi

**기존 PDF에 좌표 기반 CJK 텍스트를 다시 쓰는 Pure Rust PDF 라이브러리.**

harumi는 기존 PDF에서 텍스트와 좌표를 추출하고, 번역문·대체문·OCR 검색 레이어를
기존 문서에 기록합니다. CJK 글꼴, ToUnicode CMap, 서브셋, 레이아웃 진단을 자동으로
처리합니다. OCR 엔진이나 PDF 뷰어는 아닙니다.

현재 릴리스는 **v1.23.8**입니다.

[English](README.md) | [日本語](README_ja.md) | [中文](README_zh.md)

## 적합한 작업

- 기존 페이지를 유지하면서 PDF 텍스트를 번역하거나 교체하기
- 중국어·일본어·한국어 글꼴을 임베드하기
- 스캔 PDF에 OCR 검색 텍스트를 추가하기
- WASM, Lambda, Tauri에서 PDF 자동화하기

화면 렌더링은 `pdfium-render`, 새 보고서는 `printpdf`/`genpdf`, 저수준 객체 편집은
`lopdf`가 적합합니다. 역할별 범위는 [PDF ecosystem contract](docs/PDF_ECOSYSTEM.md)를
참조하세요.

## 빠른 시작

```toml
[dependencies]
harumi = "1"
```

서브셋이 가능한 TrueType 형식의 Noto Sans CJK를 사용하세요.

```rust
use harumi::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::from_file("input.pdf")?;
    let font = doc.embed_font(include_bytes!("NotoSansCJKkr-Regular.ttf"))?;
    doc.page(1)?.add_invisible_text(
        "검색 가능한 한국어 텍스트", font, [72.0, 700.0], 12.0,
    )?;
    doc.save("output.pdf")?;
    Ok(())
}
```

## 주요 기능

| 작업 | API 또는 feature |
|---|---|
| 텍스트와 좌표 추출 | `extract_text_runs`, `extract_text_chunks` |
| 기존 텍스트 교체 | `replace_text`, `replace_text_preserve_font`, `replace_text_resubset` |
| OCR 검색 레이어 | `add_invisible_text`, `add_invisible_text_runs` |
| 페이지 조작 | 병합, 분할, 회전, 순서 변경, 북마크, 폼, 메타데이터 |
| 도형·이미지 | `draw`, `image` feature |
| 보고서·HTML | `flow`, `html` feature |

자세한 내용은 [기능 목록](docs/FEATURES.md), [예제](docs/EXAMPLES.md),
[API 개요](docs/API.md)를 참조하세요.

## 번역과 스캔 PDF

[`harumi-ai`](harumi-ai/README.md)는 디지털 PDF와 OCR JSON을 번역합니다. 번역 영역을
오버레이하거나 교체하고, 넘침과 겹침을 경고합니다. 픽셀 단위 일치는 보장하지 않습니다.

## MCP 서버

[`harumi-mcp`](harumi-mcp/README.md)는 MCP 클라이언트에 추출, 교체, OCR 레이어, HTML,
병합, 회전, 페이지 정보 도구를 제공합니다.

```bash
cargo install harumi-mcp
harumi-mcp
```

## 제한 사항

- 완전한 Unicode 줄바꿈, RTL, 세로쓰기, 브라우저 수준 CSS는 미완성 또는 제한적입니다.
- OpenType CFF 서브셋은 지원하지 않습니다. TrueType 글꼴을 사용하세요.
- PDF 페이지 렌더링은 별도 renderer가 필요합니다.
- 복잡한 다단, 회전, 세로쓰기, 이미지 기반 문서는 저장 후 확인하세요.

문서 목록은 [docs/README.md](docs/README.md), 변경 내역은 [CHANGELOG.md](CHANGELOG.md),
우선순위는 [ROADMAP.md](ROADMAP.md)를 참조하세요. 라이선스는 MIT OR Apache-2.0입니다.
