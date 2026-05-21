# harumi

**텍스트 삽입·추출, 페이지 조작, 도형 그리기까지 — 순수 Rust PDF 조작 라이브러리.**  
한국어/일본어/중국어(CJK) 폰트 완전 지원. C 의존성 없음. WASM 네이티브.

[![Crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

[English](README.md) | [日本語](README_ja.md) | [中文](README_zh.md)

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

## 얻을 수 있는 것

| 과제 | harumi의 답 |
|---|---|
| CJK 폰트 서브셋팅이 복잡하다 | `embed_font()` 한 번 — 실제 사용된 글리프만 포함, GID 정확히 재번호 지정 |
| 기존 PDF 구조를 망가뜨리고 싶지 않다 | 추가 전용 — 원본 객체 그래프를 절대 수정하지 않음 |
| WASM / Lambda / 크로스 컴파일 환경이 필요하다 | 순수 Rust — C/C++ 의존성 없음 |
| OCR 텍스트를 좌표와 함께 삽입하고 싶다 | `add_invisible_text` / 배치 버전 `add_invisible_text_runs` |
| PDF에 도장이나 워터마크를 찍고 싶다 | `add_text(color)` 로 임의의 RGB 색상의 가시적 텍스트 오버레이 |
| 페이지 크기에 맞춰 좌표를 잡고 싶다 | `page.size()` 로 MediaBox 조회 |
| Tauri / WASM에서 메모리 출력이 필요하다 | `save_to_bytes()` 로 `Vec<u8>` 직접 반환 |
| 하이라이트 사각형이나 선을 그리고 싶다 | `add_rect` / `add_line`（`draw` feature, 추가 의존성 없음） |
| 테두리 사각형이나 다각형(말풍선 화살표)을 그리고 싶다 | `add_rect_stroke` / `add_polygon`（`draw` feature） |
| 박스 안에 텍스트를 자동 줄바꿈하고 싶다 | `add_text_box`（feature gate 불필요） |
| JPEG / PNG 이미지를 삽입하고 싶다 | `add_image` / `add_image_with_opacity`（`image` feature） |
| PNG 투명도를 유지하고 싶다（서명, 워터마크） | 투명 배경 PNG는 PDF SMask로 자동 처리 — 흰색 배경 없음 |
| 페이지를 회전, 삭제, 또는 순서 변경하고 싶다 | `rotate_page` / `remove_page` / `insert_blank_page` / `reorder_pages`（feature gate 불필요） |
| 두 PDF를 하나로 합치고 싶다 | `merge_from` 으로 다른 문서의 모든 페이지를 끝에 추가; 콘텐츠와 폰트 보존 |
| 기존 파일 없이 PDF를 처음부터 만들고 싶다 | `Document::new(size)` 로 빈 1페이지 PDF 생성; `insert_blank_page` 로 페이지 추가 |
| PDF를 개별 파일로 분리하고 싶다 | `extract_pages` 로 지정한 페이지만 담은 새 `Document` 를 원하는 순서로 반환 |
| 기존 PDF에서 텍스트 위치 정보를 추출하고 싶다 | `extract_text_runs` 로 CID 폰트 및 표준 단순 폰트（Type1, TrueType, WinAnsi 등）를 디코딩 |
| PDF 메타데이터（제목, 저자 등）를 읽고 쓰고 싶다 | `doc.metadata()` 로 `/Info` 읽기, `doc.set_metadata(&meta)` 로 쓰기 |
| 기존 PDF 텍스트를 새 폰트로 교체하고 싶다 | `page.replace_text(old, new, font)` — 매칭 건수를 `usize` 로 반환; 폰트 전환·폭 보상 자동 처리 |
| 원래 폰트를 그대로 써서 텍스트를 교체하고 싶다 | `page.replace_text_preserve_font(old, new)` — `FontHandle` 불필요; 매칭 건수 반환; 글리프 검증을 호출 시점에 즉시 수행 |
| 문서를 변경하지 않고 교체 가능 여부를 사전 확인하고 싶다 | `page.can_replace_text(old, new)` — 읽기 전용 스캔; 매칭 건수 또는 `Err(FontCharNotMapped)` 반환 |
| 타원이나 원을 그리고 싶다 | `add_ellipse(rect, color, opacity, filled, stroke_width)`（`draw` feature） |
| 채우기와 외곽선을 동시에 그리고 싶다 | `add_ellipse` / `add_polygon` / `add_path`에서 `filled=true`와 `stroke_width>0` 동시 사용 — PDF `B` 연산자 |
| 열린/닫힌 경로를 통합 API로 그리고 싶다 | `add_path(points, closed, color, filled, stroke_width, opacity)`（`draw` feature） |
| 텍스트를 회전하고 싶다（워터마크, 대각선 스탬프） | `add_text_with_rotation(text, font, pos, size, color, opacity, degrees)` |
| 여러 `Tj` 연산자에 걸친 텍스트를 치환하고 싶다 | `replace_text` / `replace_text_preserve_font` — 크로스 연산자 매칭 지원 |

---

## 이 공백이 왜 존재했나

JavaScript에는 [`pdf-lib`](https://pdf-lib.js.org/)가 있어서 폰트 서브셋팅, CMap 생성, 텍스트 레이어 합성을 투명하게 처리해줍니다. Rust의 기존 도구들은 다음 중 하나를 선택하도록 강요합니다:

- **`lopdf`** — 저수준 바이너리 조작; PDF 스펙을 읽으며 CID 폰트 객체를 수동으로 조립해야 함
- **`printpdf`** — 새 PDF 생성 전용; 기존 PDF 수정 불가
- **`pdfium-render`** — C++ 바인딩이 필요하여 WASM, 크로스 컴파일, Lambda 환경에서 빌드 실패

`harumi`는 이 공백을 채웁니다.

---

## 빠른 시작

```toml
[dependencies]
harumi = "0.3"
```

### 보이지 않는 OCR 텍스트 레이어

```rust
use harumi::{Document, TextRun};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::from_file("scanned.pdf")?;

    // 폰트 임베드 — 서브셋팅, CMap 생성, GID 재번호 지정은 save() 시에 자동 처리
    let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;

    doc.page(1)?.add_invisible_text(
        "여기에 OCR로 인식한 한국어 텍스트",
        font,
        [100.0, 250.0],
        12.0,
    )?;

    doc.save("searchable_korean.pdf")?;
    Ok(())
}
```

### 가시적 텍스트 오버레이

```rust
let (w, h) = doc.page(1)?.size()?;
doc.page(1)?.add_text("대외비", font, [w / 2.0 - 30.0, h / 2.0], 24.0, [0.8, 0.0, 0.0])?;
```

### 메모리 출력

```rust
// Tauri 커맨드, WASM, 또는 인메모리 파이프라인용
let pdf_bytes: Vec<u8> = doc.save_to_bytes()?;
```

### 다중 줄 텍스트 박스（feature gate 불필요）

```rust
// 단어 경계(Latin) 또는 임의 위치(CJK)에서 줄바꿈; 박스 하단에서 클립
doc.page(1)?.add_text_box(
    "이 문장은 좁은 박스 안에서 자동으로 줄바꿈됩니다.",
    font,
    [72.0, 400.0, 200.0, 120.0], // [x, y, 너비, 높이]
    12.0,
    [0.0, 0.0, 0.0],              // 검정
    0.0,                          // 0.0 = font_size * 1.2 를 줄 간격으로 사용
)?;
```

### 페이지 조작

```rust
// 모든 페이지를 시계 방향으로 90° 회전
for page_num in 1..=doc.page_count() {
    doc.rotate_page(page_num, 90)?;
}

// 빈 표지 페이지 삭제
doc.remove_page(1)?;

// 1페이지 앞에 빈 A4 제목 페이지 삽입
doc.insert_blank_page(0, (595.0, 842.0))?;

// 3페이지 문서의 페이지 순서 역전
doc.reorder_pages(&[3, 2, 1])?;

doc.save("output.pdf")?;
```

### PDF 합치기

```rust
let mut base = Document::from_file("a.pdf")?;
let appendix = Document::from_file("b.pdf")?;
base.merge_from(appendix)?;
base.save("merged.pdf")?;
```

보존되는 것: 모든 페이지 콘텐츠, 임베드된 폰트, 이미지, 리소스.  
보존되지 않는 것: 아웃라인/북마크, AcroForm, `/Info` 메타데이터（저자, 생성 날짜).

> **전제 조건**: `other` 에 플러시되지 않은 보류 중인 작업이 없어야 함（새로 로드한 상태, 또는 `save_to_bytes()` 후 다시 로드한 상태）.

### 빈 PDF 만들기

```rust
let mut doc = Document::new((595.0, 842.0))?;   // 빈 A4
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;
doc.page(1)?.add_text("Hello, world!", font, [72.0, 700.0], 24.0, [0.0, 0.0, 0.0])?;
doc.save("output.pdf")?;
```

### 페이지 추출

```rust
let doc = Document::from_file("large.pdf")?;
let mut excerpt = doc.extract_pages(&[3, 5, 7])?;  // 3, 5, 7페이지를 이 순서로 추출
excerpt.save("excerpt.pdf")?;
```

### 기존 PDF에서 텍스트 추출

```rust
let doc = Document::from_file("existing.pdf")?;
let runs = doc.extract_text_runs(1)?;
for fragment in &runs {
    println!("{:?} at ({:.1}, {:.1})", fragment.text, fragment.x, fragment.y);
}
```

harumi가 생성한 PDF（Identity-H CID 폰트）뿐 아니라 임의의 기존 PDF도 지원합니다. Type1·TrueType 등 표준 단순 폰트（WinAnsiEncoding, MacRomanEncoding, StandardEncoding, `/Differences` 딕셔너리）도 디코딩합니다.

### 기존 PDF에서 텍스트 바꾸기

```rust
let mut doc = Document::from_file("contract.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansJP-Regular.ttf"))?;
doc.page(1)?.replace_text("Hello", "こんにちは", font)?;
doc.save("translated.pdf")?;
```

동일한 글꼴 컨텍스트（동일 `Tf` / `BT`〜`ET` 블록）내 연속된 `Tj`/`TJ` 연산자에 걸쳐 있는 텍스트도 매칭됩니다（크로스 연산자 매칭）. 위치 연산자（`Td`, `Tm`）가 사이에 있는 경우는 대상 외입니다.

### 기존 임베드 폰트를 사용하여 텍스트 교체

폰트 파일 없이 PDF에 이미 포함된 폰트로 교체할 때 사용합니다:

```rust
let mut doc = Document::from_file("contract.pdf")?;
// FontHandle 불필요 — 해당 위치의 기존 폰트를 그대로 재사용
doc.page(1)?.replace_text_preserve_font("Draft", "Final")?;
doc.save("final.pdf")?;
```

교체 텍스트의 문자가 임베드된 폰트 서브셋에 없으면 `save()`가 `Error::FontCharNotMapped`를 반환합니다. 이 경우 `replace_text`로 폰트를 명시적으로 지정하여 폴백할 수 있습니다:

```rust
if doc.page(1)?.replace_text_preserve_font("Draft", replacement).is_ok() {
    // 서브셋에 글리프 존재 — 추가 폰트 불필요
} else {
    let font = doc.embed_font(include_bytes!("font.ttf"))?;
    doc.page(1)?.replace_text("Draft", replacement, font)?;
}
doc.save("output.pdf")?;
```

### PDF 메타데이터 읽기/쓰기

```rust
use harumi::{Document, PdfMetadata};

let mut doc = Document::from_file("report.pdf")?;

// 메타데이터 읽기
let meta = doc.metadata()?;
println!("제목: {:?}", meta.title);

// 메타데이터 쓰기（None 필드는 /Info에 기록되지 않음）
doc.set_metadata(&PdfMetadata {
    title: Some("2026 연간 보고서".into()),
    author: Some("Harumi Team".into()),
    subject: None,
    keywords: None,
    creator: None,
})?;
doc.save("report_with_meta.pdf")?;
```

### 도형 그리기（`draw` feature）

```toml
harumi = { version = "0.3", features = ["draw"] }
```

```rust
// 노란색 채워진 사각형（x, y, 너비, 높이, PDF 포인트 단위）
doc.page(1)?.add_rect([72.0, 690.0, 200.0, 14.0], [1.0, 1.0, 0.0], 0.4)?;

// 파란색 테두리 사각형（채우기 없음）
doc.page(1)?.add_rect_stroke([72.0, 400.0, 200.0, 100.0], [0.0, 0.0, 1.0], 1.5, 1.0)?;

// 채워진 삼각형（말풍선 화살표 끝）— 마지막 인수는 stroke_width（0.0 = 외곽선 없음）
doc.page(1)?.add_polygon(
    &[[100.0, 500.0], [150.0, 600.0], [200.0, 500.0]],
    [1.0, 0.5, 0.0], 1.0, true, 0.0,
)?;

// 검정 밑줄
doc.page(1)?.add_line([72.0, 600.0], [300.0, 600.0], [0.0, 0.0, 0.0], 1.5, 1.0)?;
```

### 이미지 삽입（`image` feature）

```toml
harumi = { version = "0.3", features = ["image"] }
```

```rust
let jpeg = std::fs::read("stamp.jpg")?;
// JPEG（재인코딩 없음）와 PNG 지원
doc.page(1)?.add_image(&jpeg, [72.0, 500.0, 100.0, 100.0])?;

// 불투명도 지정（0.0 = 완전 투명, 1.0 = 불투명）
doc.page(1)?.add_image_with_opacity(&jpeg, [72.0, 400.0, 100.0, 100.0], 0.75)?;
```

---

## API 개요

```rust
let mut doc = Document::from_file("path/to/file.pdf")?;
let mut doc = Document::from_bytes(&bytes)?;

let font: FontHandle = doc.embed_font(ttf_bytes)?;
let (width, height) = doc.page(1)?.size()?;

doc.page(1)?.add_invisible_text(text, font, [x, y], size)?;
doc.page(1)?.add_text(text, font, [x, y], size, [r, g, b])?;
doc.page(1)?.add_invisible_text_runs(&[
    TextRun { text: "첫 번째 줄".into(), font, x: 72.0, y: 700.0, font_size: 11.0, render_mode: 3, color: [0.0; 3] },
    TextRun { text: "두 번째 줄".into(), font, x: 72.0, y: 685.0, font_size: 11.0, render_mode: 3, color: [0.0; 3] },
])?;

// 페이지 구조（feature gate 불필요）
doc.page_count()                          // u32
doc.rotate_page(n, degrees)?;             // 90의 배수; 누적 적용
doc.remove_page(n)?;                      // 마지막 페이지는 삭제 불가
doc.insert_blank_page(after, (w, h))?;    // after=0 이면 맨 앞에 삽입
doc.reorder_pages(&[new_order...])?;      // 1부터 시작하는 기존 페이지 번호 지정
doc.extract_pages(&[n1, n2, ...])?;       // 지정 페이지만 담은 새 Document

// 처음부터 생성
Document::new((w, h))?;                   // 빈 1페이지 PDF

// 문서 합치기（other 에 보류 중인 작업이 없어야 함）
doc.merge_from(other)?;             // other 의 모든 페이지를 끝에 추가

doc.save("output.pdf")?;
doc.save_to_bytes()?;   // 인메모리 버전

// 기존 PDF에서 텍스트 추출（CID + 표준 단순 폰트）
let runs: Vec<TextFragment> = doc.extract_text_runs(page_number)?;

// PDF 메타데이터（/Info 딕셔너리）
let meta: PdfMetadata = doc.metadata()?;
doc.set_metadata(&PdfMetadata { title: Some("...".into()), ..Default::default() })?;

// 기존 콘텐츠 스트림에서 텍스트 바꾸기（단일 연산자 매칭）
doc.page(1)?.replace_text(old_text, new_text, font)?;
```

### 좌표계

좌표는 **PDF 포인트** (1pt = 1/72인치) 단위이며, 원점은 페이지 **좌하단**입니다:

```toml
harumi = { version = "0.3", features = ["ocr"] }
```

### 기능 플래그

| 플래그 | 활성화되는 기능 | 추가 의존성 |
|---|---|---|
| *(기본)* | 텍스트 오버레이, 폰트 임베드, `add_text_box`, `add_text_box_aligned`, `add_text_with_opacity`, `add_text_box_with_opacity` | lopdf, allsorts, ttf-parser |
| `draw` | `add_rect`, `add_line`, `add_rect_stroke`, `add_polygon`, `add_polyline`, `add_ellipse` — 도형 그리기 | 없음 |
| `image` | `add_image`, `add_image_with_opacity` — JPEG/PNG 이미지（`draw` 자동 활성화） | `image` crate |
| `ocr` | `ocr::hocr_y_to_pdf`, `ocr::hocr_x_to_pdf`, `ocr::pixel_size_to_pt` — Tesseract 좌표 변환 헬퍼 | 없음 |

```rust
let pdf_y = harumi::ocr::hocr_y_to_pdf(pixel_y, page_height_pts, image_dpi);
let pdf_x = harumi::ocr::hocr_x_to_pdf(pixel_x, image_dpi);
let pt    = harumi::ocr::pixel_size_to_pt(pixel_size, image_dpi);
```

---

## 지원 폰트 형식

| 폰트 형식 | 지원 상태 |
|---|---|
| TrueType (`.ttf`) | 지원, 검증 완료 |
| OpenType CFF (`.otf`) | 수락하지만 allsorts 의존 (아래 참고) |
| TTC 컬렉션 | 지원됨（index 0 사용） |

**TrueType** 버전을 권장합니다 (엔드투엔드 검증 완료):

```
NotoSansCJKkr-Regular.ttf  （한국어）
NotoSansCJKjp-Regular.ttf  （일본어）
NotoSansCJKsc-Regular.ttf  （중국어 간체）
NotoSansCJKtc-Regular.ttf  （중국어 번체）
```

> **OTF 참고**: harumi는 `.otf` 파일을 수락하고 `FontFile3 /OpenType`으로 임베드합니다. 단, allsorts v0.17이 일부 CFF 변형(CFF2 가변 폰트 등)을 서브셋팅하지 못할 수 있으며, 이 경우 `save()` 시 `FontParse` 오류가 발생합니다. TTF 변형을 사용하면 확실하게 동작합니다.

---

## 내부 구현

```
harumi
├── lopdf v0.40          — 기존 PDF 객체 그래프 파싱 및 편집
├── allsorts v0.17+      — TrueType 폰트 서브셋팅 (Prince 조판 소프트웨어에서 실제 검증됨)
└── ttf-parser           — 폰트 메타데이터 읽기 (bbox, units_per_em, ascender)
```

폰트 처리 파이프라인:

1. 사용된 문자 수집 → Unicode 코드 포인트 집합 생성
2. 폰트 `cmap` 테이블로 코드 포인트 → 원래 GID 매핑 (ttf-parser)
3. allsorts로 사용된 글리프만 TTF 서브셋 생성 (GID **0..N으로 재번호 지정**)
4. `gid_to_char`와 어드밴스 폭을 원래 GID → 새 GID로 **재매핑** (글자 깨짐 방지)
5. PDF CID 폰트 객체 그래프 구성: `Type0 → CIDFontType2 → FontDescriptor → FontFile2`
6. `/ToUnicode` CMap 스트림 생성 (뷰어에서 텍스트 복사/검색 가능)
7. 페이지 `/Contents` 배열에 새 콘텐츠 스트림 추가

서브셋팅은 **지연 실행**: `embed_font()`는 원시 TTF 바이트를 저장하고, `save()` 시에 모든 페이지의 사용 문자를 수집하여 폰트별로 한 번만 처리합니다.

---

## 이름에 대하여

晴海（はるみ / Harumi）— *晴*（맑은 하늘）＋ *海*（바다）。표면은 잔잔하고, 내부에선 많은 일이 일어나고 있다.

## 로드맵

| 버전 | 범위 |
|---|---|
| **v0.1** | TrueType, 보이지 않는/가시적 텍스트, 일괄 배치, `page.size()`, `save_to_bytes()`, GID 버그 수정, OTF 수락 |
| **v0.2** | `draw` feature（`add_rect`, `add_line`）, `image` feature（`add_image`, `add_image_with_opacity`）, CFF2 조기 오류, TTC 매직 바이트 감지, MediaBox 부모 체인 순회 |
| **v0.3** | `add_text_box`, `add_rect_stroke`, `add_polygon`; 보안 강화（NaN 방어, 이중 저장 방지, 간접 Contents 배열 지원, JPEG 마커 파서 수정, PNG 정수 오버플로 수정） |
| **v0.4** | PNG 실제 투명도（SMask）— 투명 배경 PNG가 흰색 배경 없이 올바르게 렌더링됨 |
| **v0.5** | `add_text_with_opacity`, `add_text_box_aligned`（VerticalAlign）, `add_polyline`, `add_text_box_with_opacity` — **완료** |
| **v0.6** | 페이지 조작 — `rotate_page`, `remove_page`, `insert_blank_page`, `reorder_pages` — **완료** |
| **v0.7** | `merge_from`（PDF 합치기）, `remove_page` 정확성 수정 및 고아 객체 정리 — **완료** |
| **v0.8** | `Document::new`（빈 PDF 처음부터 생성）, `extract_pages`（페이지 분리） — **완료** |
| **v0.9** | `extract_text_runs`（CID + 표준 단순 폰트 지원）, PDF 메타데이터 읽기/쓰기（`metadata()`, `set_metadata()`, `PdfMetadata`） — **완료** |
| **v0.10** | `replace_text` — 진정한 스트림 내 텍스트 교체: Tj/TJ 재작성, 자동 폰트 전환, Td 폭 보상 — **완료** |
| **Next（v0.11 이상）** | `#[non_exhaustive]` on Error, MSRV 선언, WASM CI, crates.io 출시 |

---

## 기여

[github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi)에서 Issue와 PR을 환영합니다.

코드베이스에서 가장 복잡한 부분은 `src/font/embed.rs` (CID 폰트 객체 그래프 구성)입니다. 특정 PDF 뷰어에서 렌더링 버그를 보고할 때는 뷰어 이름과 버전을 명시해 주세요.

---

## 라이선스

MIT OR Apache-2.0
