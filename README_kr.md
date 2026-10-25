# harumi

**기존 PDF의 위치 정보를 유지하면서 CJK·다국어 텍스트를 다시 쓰는 순수 Rust 라이브러리.**

현재 버전은 **1.23.0**입니다.

[English](README.md) · [日本語](README_ja.md) · [中文](README_zh.md)

## 주요 기능

- 기존 PDF의 텍스트 추출, 교체, 번역 오버레이
- CJK 폰트 임베딩·서브셋팅과 OCR 검색 레이어
- 페이지 병합·분할·회전·재배열, 주석, 폼, 암호화
- `draw` 기능의 도형·점선, `image` 기능의 이미지 임베딩
- `flow` / `html` 기능의 보고서·HTML PDF 생성
- WASM, Lambda, Tauri, MCP 지원

OCR 엔진이나 PDF 뷰어는 아닙니다. 복잡한 다단, 회전·세로쓰기, 이미지 배경 문서는
렌더러로 결과를 확인하세요.

## 빠른 예

```rust
use harumi::Document;

let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJKkr-Regular.ttf"))?;
doc.page(1)?.add_invisible_text("검색 가능한 한국어", font, [72.0, 700.0], 12.0)?;
doc.save("searchable.pdf")?;
# Ok::<(), harumi::Error>(())
```

## 번역

`harumi-ai`는 디지털 PDF와 OCR JSON을 사용하는 스캔 PDF 번역을 지원합니다. LLM 번역,
영역 배치, 페이지별 품질 보고서를 제공합니다.

## 자세한 문서

- [API 개요](docs/API.md)
- [코드 예제](docs/EXAMPLES.md)
- [기능 목록](docs/FEATURES.md)
- [PDF 비교 범위](docs/PDF_ECOSYSTEM.md)
- [MCP 서버](harumi-mcp/README.md)

## 기여 및 라이선스

Issue와 Pull Request를 환영합니다. MIT OR Apache-2.0.
