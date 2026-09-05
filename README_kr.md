# harumi

**기존 PDF에서 추출한 좌표와 추정 영역을 바탕으로 CJK/다국어 텍스트를 다시 쓰는 순수 Rust PDF 엔진.**

harumi는 기존 PDF에서 위치 정보를 포함한 텍스트를 추출하고,
번역하거나 교체한 결과를 추정된 레이아웃 영역에 다시 씁니다. Overlay 모드는 기존
페이지 내용을 유지하지만 픽셀 단위의 동일성을 보장하지는 않습니다. 복잡한 다단,
회전·세로쓰기, 이미지 배경 문서는 품질 보고서와 육안 검토가 필요합니다.
CID 폰트, CMap, 유니코드 매핑, 폰트 서브셋팅, 텍스트 피팅,
레이아웃 충돌 감지까지 모두 자동으로 처리됩니다.

[![harumi on crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![harumi-ai on crates.io](https://img.shields.io/crates/v/harumi-ai.svg?label=harumi-ai)](https://crates.io/crates/harumi-ai)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Demo](https://img.shields.io/badge/demo-live-brightgreen)](https://kent-tokyo.github.io/harumi/)

[English](README.md) | [中文](README_zh.md) | [日本語](README_ja.md)

**[브라우저에서 데모 체험하기 →](https://kent-tokyo.github.io/harumi/)** — 텍스트·사각형·직선·자유 펜 어노테이션 편집기 (WASM으로 브라우저 완결)

**주요 용도：**
- 디지털 PDF 번역 (텍스트 추출 → LLM 번역 → 레이아웃 추정 기반 다시 쓰기)
- 스캔 PDF 번역 (OCR JSON 전달 → 번역 → 원문 마스킹 → 번역문 오버레이)
- 스캔 PDF에 OCR 검색 가능 텍스트 레이어 추가
- 한국어·일본어·중국어 텍스트 오버레이 및 스탬프
- 페이지 조작, 주석, 폼 편집, PDF 병합
- WASM·Lambda·Tauri·MCP 기반 AI 문서 워크플로

> OCR 엔진도 PDF 뷰어도 아닙니다.  
> harumi는 문서 자동화를 위한 「PDF 쓰기 레이어」입니다.

---

## harumi가 해결하는 것

**harumi 이전:**  
PDF 스펙 문서를 보며 CID 폰트 객체를 수동으로 조립하고, CMap 생성·GID 매핑·서브셋팅을 수백 줄로 직접 구현하면서 글자 깨짐과 씨름.

**harumi 이후:**

```rust
let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;
doc.page(1)?.add_invisible_text("검색 가능한 텍스트", font, [72.0, 700.0], 12.0)?;
doc.save("searchable.pdf")?;
```

폰트 서브셋팅, CID 인코딩, ToUnicode CMap 생성, GID 재번호 지정이 모두 자동으로 처리됩니다.

---

## 디지털 PDF와 스캔 PDF 번역

**harumi-ai**는 단일 API로 디지털 PDF와 스캔 PDF를 모두 번역합니다:

```rust
use harumi_ai::{InputTextSource, TranslateOptions, translate_pdf};

// 디지털 PDF (기본값)
let opts = TranslateOptions::new("ko", my_llm, font_bytes);
let output = translate_pdf(&digital_pdf, opts).await?;

// 스캔 PDF — ocrs-cjk, PaddleOCR 등의 OCR JSON 전달
let ocr_json = std::fs::read("ocr.json")?;
let mut opts = TranslateOptions::new("ko", my_llm, font_bytes);
opts.input_source = InputTextSource::OcrJson(ocr_json);
let output = translate_pdf(&scanned_pdf, opts).await?;
```

OCR은 ocrs-cjk, PaddleOCR, hOCR, ALTO 또는 HierText 형식을 출력하는 모든 도구에서 가져올 수 있습니다.

**스캔 PDF 시작하기:**

```bash
# 1. CJK OCR CLI 설치
cargo install ocrs-cjk-cli

# 2. OCR 실행 (텍스트 + 바운딩 박스 + 신뢰도 출력)
ocrs scanned.pdf --json -o ocr.json

# 3. harumi-ai로 번역 및 다시 쓰기
# (TranslateOptions에 InputTextSource::OcrJson 설정)
```

### 검색 가능 텍스트 레이어 (번역 없음)

번역 없이 보이지 않는 검색 가능 텍스트 레이어를 추가하려면 (RAG 인덱싱 및 PDF 검색용),
`add_invisible_text`에 OCR 출력을 직접 전달합니다:

```bash
cargo run --example ocrs_cjk_to_searchable_pdf -- \
  examples/fixtures/scanned_sample.pdf \
  examples/fixtures/ocrs_sample.json \
  /path/to/NotoSansCJKjp-Regular.ttf \
  searchable.pdf
```

### AI 보조 OCR 교정

OCR 엔진은 자형이 비슷한 CJK 문자를 오인식하는 경우가 있습니다.
LLM을 사용해 원본 바운딩 박스를 유지하면서 오류를 교정할 수 있습니다.

**핵심 제약: AI는 텍스트만 교정합니다. 바운딩 박스는 이동하지 않습니다.**

> **OCR 교정 ≠ 번역.** 번역 write-back은 영역 단위 레이아웃 설계가 필요합니다. `harumi-ai`를 사용하세요.

`examples/fixtures/ocrs_sample_raw.json`, `ocrs_sample_corrected.json`, `ai_correction_report.json`에서 작동 예제를 확인하세요.

harumi는 OCR 엔진이 아닙니다. 번역 경로에는 임의의 LLM 위에 구축된 `harumi-ai`를 사용하세요.

---

## 얻을 수 있는 것

**[전체 기능 목록 →](docs/FEATURES.md)**

| 요구 사항 | harumi의 해결책 |
|---|---|
| CJK 폰트 서브셋팅이 복잡하다 | `embed_font()` 한 번 — 실제 사용된 글리프만 포함, GID 정확히 재번호 지정 |
| 기존 PDF 구조를 손상시키고 싶지 않다 | 추가 전용: harumi는 원본 객체 그래프에 절대 손대지 않음 |
| WASM / Lambda에서 실행해야 한다 | 순수 Rust — C/C++ 의존성 없음 |
| OCR 텍스트를 좌표와 함께 삽입해야 한다 | `add_invisible_text` / 배치 버전 `add_invisible_text_runs` |
| 기존 PDF의 텍스트를 교체해야 한다 | `replace_text` / `replace_text_preserve_font` / `replace_text_resubset` |
| 레이아웃 유지 번역의 품질 게이트가 필요하다 | `extract_layout_regions` → `plan_text_for_regions` → `assess_page_layout_quality` |

비교표 및 상세 기능 설명은 [영어 README](README.md)를 참조하세요.

---

## MCP 서버 (harumi-mcp)

Claude Code, Cursor 또는 Continue IDE에서 harumi의 PDF 도구를 직접 사용합니다:

```bash
cargo build -p harumi-mcp
# 사용 가능한 도구: pdf_extract_text, pdf_extract_all_pages, pdf_replace_text,
#                   pdf_add_invisible_text, pdf_html_to_pdf, pdf_merge, pdf_page_info
```

[smithery.ai](https://smithery.ai) 또는 [mcp.so](https://mcp.so)에서 원클릭 설치가 가능합니다.

---

## 빠른 시작

```toml
[dependencies]
harumi = "1"
```

### CJK 폰트 다운로드

한국어·일본어·중국어 처리에는 **NotoSansCJK** 폰트(Google Fonts, 무료, OFL 라이선스)를 사용하세요:

```bash
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKkr-Regular.ttf
```

또는 [fonts.google.com](https://fonts.google.com)에서 「Noto Sans CJK」를 검색하세요.

### 보이지 않는 OCR 텍스트 레이어

```rust
let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;

doc.page(1)?.add_invisible_text(
    "검색 가능한 한국어 텍스트",
    font,
    [100.0, 250.0], // x, y (PDF 포인트, 원점: 왼쪽 하단)
    12.0,
)?;

doc.save("searchable.pdf")?;
```

### 보이는 텍스트 오버레이

```rust
let (w, h) = doc.page(1)?.size()?;
doc.page(1)?.add_text(
    "기밀",
    font,
    [w / 2.0 - 20.0, h / 2.0],
    24.0,
    [0.8, 0.0, 0.0], // 빨간색
)?;
```

**[더 많은 예제 →](docs/EXAMPLES.md)** — 페이지 조작, PDF 병합, HTML→PDF, 주석, 폼, 도형, 이미지, FlowDocument

**[API 레퍼런스 →](docs/API.md)** — 좌표계, 기능 플래그, 지원 폰트, 내부 구조

---

## Contributing

[github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi)에서 Issue와 PR을 환영합니다.

---

## License

MIT OR Apache-2.0
