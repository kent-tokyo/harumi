# harumi

> **HARUMI** — **H**igh-level **A**PI for **R**ust-native **U**nicode **M**anipulation and **I**njection

**텍스트 삽입·추출, 페이지 조작, 도형 그리기까지 — 순수 Rust PDF 조작 라이브러리.**  
한국어/일본어/중국어(CJK) 폰트 완전 지원. C 의존성 없음. WASM 네이티브.

[![Crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Demo](https://img.shields.io/badge/demo-live-brightgreen)](https://kent-tokyo.github.io/harumi/)

[English](README.md) | [中文](README_zh.md) | [日本語](README_ja.md)

**[브라우저에서 데모 체험하기 →](https://kent-tokyo.github.io/harumi/)** — 텍스트·사각형·직선·자유 펜 어노테이션 편집기 (WASM으로 브라우저 완결)

### 🔌 MCP 서버로 사용 가능

Claude Code, Cursor 또는 Continue IDE에서 harumi의 PDF 도구를 직접 사용합니다:

```bash
# MCP 서버 빌드 (순수 Rust, 런타임 의존성 없음)
cargo build -p harumi-mcp

# IDE 설정에서 사용 가능한 도구:
# - pdf_extract_text: 위치 정보 포함 텍스트 추출
# - pdf_extract_all_pages: 모든 페이지의 위치 정보 포함 텍스트 추출
# - pdf_replace_text: 레이아웃을 유지한 텍스트 교체/번역
# - pdf_add_invisible_text: OCR 검색 가능 레이어
# - pdf_html_to_pdf: HTML→PDF 변환
# - pdf_merge: PDF 병합
# - pdf_page_info: 페이지 정보 조회
```

PDF 번역은 `pdf_extract_all_pages` 로 모든 페이지의 텍스트 조각을 추출하고,
번역한 뒤 `pdf_replace_text` 로 기존 레이아웃을 유지하며 교체합니다. 비 Identity
`CIDToGIDMap` 때문에 재서브셋팅할 수 없는 PDF는 Unicode TTF를 지정하고
`mode: "new_font"` 를 사용하세요.
`harumi-ai` CLI는 기존 레이아웃을 유지할 때 기본적으로 `overlay` mode를 사용합니다.
새 문서로 다시 만들고 싶을 때만 `new` 를 지정하세요.
Overlay mode는 `detect_text_columns` 로 다단 레이아웃을 감지하고, 번역 텍스트를
원문의 정확한 베이스라인 Y에 배치합니다（흰 사각형은 실제 descender 깊이로 보정）。

[smithery.ai](https://smithery.ai) 또는 [mcp.so](https://mcp.so)에 등록 예정.

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
| CJK 폰트 서브셋팅이 복잡하다 | `embed_font()` 한 번 — 실제 사용된 글리프만 포함, GID 정확히 재번호 지정; GSUB/GPOS/가변 폰트 테이블 제거로 macOS Preview 및 PSPDFKit 호환성 보장 |
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
| 기존 PDF에서 텍스트 위치 정보를 추출하고 싶다 | `extract_text_runs` 로 CID 폰트 및 표준 단순 폰트（Type1, TrueType, Type3, WinAnsi 등）를 디코딩 |
| PDF 메타데이터（제목, 저자 등）를 읽고 쓰고 싶다 | `doc.metadata()` 로 `/Info` 읽기, `doc.set_metadata(&meta)` 로 쓰기 |
| 기존 PDF 텍스트를 새 폰트로 교체하고 싶다 | `page.replace_text(old, new, font)` — 매칭 건수를 `usize` 로 반환; 폰트 전환·폭 보상 자동 처리 |
| 원래 폰트를 그대로 써서 텍스트를 교체하고 싶다 | `page.replace_text_preserve_font(old, new)` — `FontHandle` 불필요; 매칭 건수 반환; 글리프 검증을 호출 시점에 즉시 수행 |
| 문서를 변경하지 않고 교체 가능 여부를 사전 확인하고 싶다 | `page.can_replace_text(old, new)` — 읽기 전용 스캔; 매칭 건수 또는 `Err(FontCharNotMapped)` 반환 |
| 타원이나 원을 그리고 싶다 | `add_ellipse(rect, color, opacity, filled, stroke_width)`（`draw` feature） |
| 채우기와 외곽선을 동시에 그리고 싶다 | `add_ellipse` / `add_polygon` / `add_path`에서 `filled=true`와 `stroke_width>0` 동시 사용 — PDF `B` 연산자 |
| 열린/닫힌 경로를 통합 API로 그리고 싶다 | `add_path(points, closed, color, filled, stroke_width, opacity)`（`draw` feature） |
| 텍스트를 회전하고 싶다（워터마크, 대각선 스탬프） | `add_text_with_rotation(text, font, pos, size, color, opacity, degrees)` |
| 여러 `Tj` 연산자 또는 폰트 런에 걸친 텍스트를 치환하고 싶다 | `replace_text` / `replace_text_preserve_font` — 크로스 연산자 **및** 크로스 `Tf` 매칭 지원 |
| 스캔 PDF에서 임베드된 이미지를 추출하고 싶다 | `extract_page_image` 로 JPEG 또는 PNG 바이트 반환（`image` feature）; 스캔 PDF 전용 |
| PDF에 클릭 가능한 URL 링크를 넣고 싶다 | `add_link_url([x, y, w, h], url)` — 보이지 않는 URI 어노테이션; 클릭하면 어떤 뷰어에서도 URL을 열어줌 |
| 내부 페이지 이동 링크（목차）가 필요하다 | `add_link_internal([x, y, w, h], target_page)` — 같은 문서 내 페이지로 이동 |
| 북마크/문서 개요를 만들고 싶다 | `add_bookmark(title, page, y)` — 플랫 PDF 아웃라인 항목; CJK 제목은 UTF-16BE로 자동 인코딩 |
| 모든 페이지에 페이지 번호가 있는 헤더/푸터를 넣고 싶다 | `FlowOptions { header: Some(hf), footer: Some(hf), .. }` + `HeaderFooter`（`flow` feature）; `{{page}}`/`{{total}}` 치환 |
| 제목에서 자동으로 개요 항목을 생성하고 싶다 | `FlowOptions { auto_bookmarks: true, .. }`（기본값）— `push_heading` 호출마다 자동으로 북마크 생성 |
| 암호로 보호된 PDF를 불러오고 싶다 | `Document::from_file_with_password(path, pw)` / `from_bytes_with_password(bytes, pw)` — 사용자/소유자 비밀번호 모두 지원 |
| 암호화된 PDF로 저장하고 싶다 | `doc.set_encryption(user_pw, owner_pw)` — `save()` 시 128-bit RC4로 암호화 |
| PDF가 원래 암호화되어 있었는지 확인하고 싶다 | `doc.is_encrypted()` — 복호화 후에도 `true` 반환 |
| 텍스트에 하이라이트/밑줄/취소선을 추가하고 싶다 | `add_highlight` / `add_underline` / `add_strikeout` / `add_squiggly` — QuadPoints 포함 PDF 마크업 주석 |
| 페이지에 포스트잇 메모를 추가하고 싶다 | `add_sticky_note([x, y], "메모 내용")` — 유니코드 지원 텍스트 주석 |
| PDF 폼 필드 값을 읽고 싶다 | `doc.form_fields()` — `Vec<FormField>` 반환（이름, 종류, 현재 값） |
| PDF 폼을 프로그래밍으로 채우고 싶다 | `doc.fill_form(&[("필드명", "값")])` — NeedAppearances 자동 설정 |
| 페이지 크롭 박스와 인쇄 박스를 조작하고 싶다 | `page.crop_box()` / `set_crop_box(rect)` / `trim_box()` / `bleed_box()` — `[x,y,w,h]` 형식으로 모든 박스 타입 지원 |
| 페이지 콘텐츠를 스케일하고 싶다（예: A4 → A3） | `page.scale_page_content(sx, sy)` 기존 콘텐츠 앞에 `cm` 행렬 삽입；`resize_page_with_content(w, h)` 스케일링과 MediaBox 변경을 한 번에 처리（v1.4+） |
| 다른 PDF를 현재 PDF에 오버레이하고 싶다（스탬프 합성） | `doc.overlay_from(other)` 로 `other`의 각 페이지를 `self`의 해당 페이지에 Form XObject로 겹쳐 쓰기；폰트, 이미지, 불투명도 보존（v1.4+） |
| 모든 북마크/목차를 삭제하고 싶다 | `doc.clear_outline()` 으로 대기 중인 북마크와 이미 로드된 PDF의 `/Outlines` 트리를 일괄 삭제（v1.4+） |
| PDF에 파일을 첨부하고 싶다 | `doc.attach_file(name, data, mime)` 로 임의 파일을 EmbeddedFiles로 첨부（FlateDecode 압축, 이름순 정렬）；`doc.list_attachments()` → `Vec<AttachmentInfo>`（v1.4+） |
| 추출한 텍스트에서 굵기/기울임/폰트 이름을 얻고 싶다 | `TextFragment::is_bold`・`is_italic`・`font_family`・`base_font` — PostScript `/BaseFont` 이름에서 파싱（v1.4.1+） |
| 추출한 텍스트에서 단 레이아웃을 감지하고 싶다 | `detect_text_columns(&frags, page_width)` — X 밀도 히스토그램으로 빈 간격을 감지해 `Vec<ColumnZone>` 반환（v1.4.1+） |
| 추출된 텍스트를 줄 또는 단락 단위로 그룹화 | `group_text_fragments(&frags, GroupingStrategy::Paragraph)` — 인접 fragment를 `TextGroup`으로 병합. `Paragraph`는 단락 경계까지, `Line`은 같은 줄만 병합. 번역 모델 입력 품질 향상에 활용（v1.5+） |
| 폰트가 특정 문자를 지원하는지 확인 | `font_covers_char(font_bytes, ch) -> bool` — ttf-parser로 cmap 조회. 폴백 폰트 선택에 활용（v1.5+） |
| 테이블 PDF에서 셀 단위로 텍스트 추출 | `extract_table_cells(&frags, page_width, page_height)` — `detect_text_columns`로 열, Y 좌표 클러스터링으로 행을 감지해 `Vec<TableCell>` 반환. 각 셀에 `row`/`col`(0-기반)·`text`·바운딩 박스 포함. 격자선 없는 PDF는 휴리스틱（v1.5+） |
| PDF 디지털 서명을 검증하고 싶다 | `doc.verify_signatures(&pdf_bytes)` — 서명 메타데이터（서명자, 타임스탬프, 필드명）추출；암호화 검증은 TODO（`digital-signature` feature） |
| PDF에 디지털 서명을 생성하고 추가하고 싶다 | `doc.add_signature_field(page, rect, options)` + `doc.sign_document(context, field_name)` — `digital-signature` feature 필요；서명 필드 생성, RSA PKCS#1 v1.5 서명 생성；완전한 PDF 임베딩은 v1.2.1 예정 |
| TextFragment가 어떤 PDF 연산자에서 생성됐는지 추적 | `TextFragment.source_stream` / `source_op_start` / `source_op_end` — 원본 `Tj`/`TJ` 키워드의 내용 스트림 내 바이트 오프셋（v1.5.15+） |
| 문자별 Tj로 그려진 PDF 텍스트를 교체하고 싶다 | `page.replace_text_fragments(&frags, new_text, font)` — 소스 연산자를 `() Tj`로 억제하고 첫 번째 fragment 위치에 `new_text` 배치；PScript5/Distiller·Type3 레이아웃에서 `replace_text()`가 일치하지 않을 때 사용（v1.5.15+） |
| Tm 행렬의 수평 스케일 인수가 필요하다 | `TextFragment.tm_x_scale: Option<f32>` — Tm 행렬의 √(a²+b²)；`font_size=1` + 대형 Tm 스케일 PDF에서도 올바른 시각적 너비와 열 오프셋 계산 가능（v1.6.0+） |
| 폼/표 PDF 인플레이스 번역을 위한 안정적인 열/행 앵커가 필요하다 | `TextFragment.tm_lm_x / tm_lm_y: Option<f32>` — 각 `Tj` 시작 시의 텍스트 라인 행렬(T_lm) 좌표；`Td`마다 리셋되어 이전 행의 글자 너비에 관계없이 레이블 열과 값 열이 항상 정확한 위치를 반환（v1.7.0+） |

---

## 이 공백이 왜 존재했나

JavaScript에는 [`pdf-lib`](https://pdf-lib.js.org/)가 있어서 폰트 서브셋팅, CMap 생성, 텍스트 레이어 합성을 투명하게 처리해줍니다. Rust의 기존 도구들은 다음 중 하나를 선택하도록 강요합니다:

- **`lopdf`** — 저수준 바이너리 조작; PDF 스펙을 읽으며 CID 폰트 객체를 수동으로 조립해야 함
- **`printpdf`** — 새 PDF 생성 전용; 기존 PDF 수정 불가
- **`pdfium-render`** — C++ 바인딩이 필요하여 WASM, 크로스 컴파일, Lambda 환경에서 빌드 실패

`harumi`는 이 공백을 채웁니다.

---

## 유사 도구와의 비교

| 기능 | **harumi** | pdf-lib (JS) | printpdf (Rust) | lopdf (Rust) | pdfium-render (Rust) |
|---|:---:|:---:|:---:|:---:|:---:|
| 순수 Rust (C/C++ 없음) | Yes | N/A | Yes | Yes | No |
| WASM / 크로스 플랫폼 | Yes | Yes | Yes | Yes | Partial |
| 기존 PDF에 CJK 텍스트 추가 | Yes | Yes | No | No | Yes |
| 텍스트 추출 | Yes | Partial | No | Partial | Yes |
| 텍스트 교체 (서브셋 확장 포함) | Yes | No | No | No | No |
| 페이지 조작 | Yes | Yes | Partial | Yes | Yes |
| 도형 그리기 | Yes | Yes | Yes | No | Yes |
| 플로우 문서 / 자동 페이지 분할 | Yes | No | No | No | No |
| HTML → PDF | Yes | No | No | No | No |
| 인라인 굵기/기울임/색상 | Yes 합성 | No | No | No | Yes |
| 암호화 (읽기) | Yes | Yes | No | Partial | Yes |
| 암호화 (쓰기) | Yes (RC4-128) | Yes | No | No | Yes |

---

## 빠른 시작

```toml
[dependencies]
harumi = "0.7"
```

### CJK 글꼴 다운로드

일본어, 중국어, 한국어 PDF 처리를 위해 Google Fonts(무료, OFL 라이선스)에서 **NotoSansCJK 글꼴**을 다운로드하세요:

```bash
# 일본어
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKjp-Regular.ttf

# 간체 중국어
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKsc-Regular.ttf

# 번체 중국어
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKtc-Regular.ttf

# 한국어
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKkr-Regular.ttf
```

**다른 출처:**
- **Google Fonts 웹사이트**: https://fonts.google.com ("Noto Sans CJK" 검색)
- **Adobe Fonts**: https://fonts.adobe.com (구독 버전)
- **시스템 글꼴**: `fc-list | grep -i noto` 로 이미 설치된 글꼴 확인

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

harumi가 생성한 PDF（Identity-H CID 폰트）뿐 아니라 임의의 기존 PDF도 지원합니다. Type1·TrueType 등 표준 단순 폰트（WinAnsiEncoding, MacRomanEncoding, StandardEncoding, `/Differences` 딕셔너리）도 디코딩합니다. 각 `TextFragment`는 `text`、`x`/`y`、`width`、`font_size`、**`font_name`**、**`color`**、**`invisible`** 외에 **`is_bold`**・**`is_italic`**・**`font_family`**・**`base_font`**（PostScript `/BaseFont` 이름에서 파싱）를 포함합니다。

### 기존 PDF에서 텍스트 바꾸기

```rust
let mut doc = Document::from_file("contract.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansJP-Regular.ttf"))?;
doc.page(1)?.replace_text("Hello", "こんにちは", font)?;
doc.save("translated.pdf")?;
```

동일 BT/ET 블록 내 연속된 `Tj`/`TJ` 연산자에 걸쳐 있는 텍스트도 매칭됩니다（크로스 연산자 매칭）. 또한 `Tf` 폰트 전환 연산자를 가로지르는 매칭도 지원합니다（크로스 `Tf` 매칭）. 일본어 PDF에서는 하나의 시각적 줄이 여러 폰트 런（본문 한자를 `F1`, 괄호 문자를 `F2`）으로 나뉘는 경우가 많은데, 이러한 케이스도 올바르게 매칭됩니다. 수직 방향의 `Td` 또는 `Tm`（새로운 시각적 줄）이 사이에 있는 경우는 대상 외입니다.

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

### 문서를 수정하지 않고 교체 가능 여부 확인

```rust
let mut doc = Document::from_file("contract.pdf")?;
match doc.page(1)?.can_replace_text("Draft", "Final") {
    Ok(0) => println!("페이지 1에 'Draft'가 없음"),
    Ok(n) => println!("{n}건 발견됨; 글리프 OK"),
    Err(e) => println!("글리프 누락: {e}"),
}
```

### 폰트 서브셋 확장 텍스트 교체

새 텍스트에 기존 폰트 서브셋에 없는 문자가 포함된 경우 `replace_text_resubset`을 사용합니다. 원본 TTF/OTF 바이트를 전달하면 harumi가 서브셋을 확장하고 모든 콘텐츠 스트림을 재인코딩하여 한 번의 `save()` 호출로 교체를 완료합니다.

```rust
let font_bytes = include_bytes!("NotoSansCJK-Regular.ttf");
let mut doc = Document::from_file("contract.pdf")?;
let n = doc.page(1)?.replace_text_resubset("Hello", "한국어", font_bytes)?;
doc.save("output.pdf")?;
```

> 원본 서브셋화되지 않은 폰트 파일이 필요합니다. `CIDToGIDMap /Identity` 를 사용하는 CIDFontType2 폰트만 지원됩니다.
> 다른 도구가 만든 PDF는 비 Identity `CIDToGIDMap` 을 사용할 수 있습니다. 이 경우 새로 임베드한 폰트로
> `replace_text` 를 사용하거나 MCP `pdf_replace_text` 의 `mode: "new_font"` 를 사용하세요.

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
harumi = { version = "1", features = ["draw"] }
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
harumi = { version = "1", features = ["image"] }
```

```rust
let jpeg = std::fs::read("stamp.jpg")?;
// JPEG（재인코딩 없음）와 PNG 지원
doc.page(1)?.add_image(&jpeg, [72.0, 500.0, 100.0, 100.0])?;

// 불투명도 지정（0.0 = 완전 투명, 1.0 = 불투명）
doc.page(1)?.add_image_with_opacity(&jpeg, [72.0, 400.0, 100.0, 100.0], 0.75)?;
```

### 스캔 PDF에서 임베드된 이미지 추출（`image` feature）

OCR 워크플로용: 스캔 PDF 로드 → 래스터 이미지 추출 → OCR 실행 → 보이지 않는 텍스트 레이어 작성.

```rust
use harumi::{Document, PageImageFormat};

let doc = Document::from_file("scanned.pdf")?;
let img = doc.extract_page_image(1)?;

match img.format {
    PageImageFormat::Jpeg => std::fs::write("page1.jpg", &img.bytes)?,
    PageImageFormat::Png  => std::fs::write("page1.png", &img.bytes)?,
}
println!("{}×{} 픽셀", img.width, img.height);
```

> **스캔 PDF 전용.** 기존 Image XObject를 추출하는 기능으로, 페이지를 래스터화하지 않습니다. 텍스트·벡터 PDF에는 Image XObject가 없어 `Error::InvalidInput`이 반환됩니다.

### 자동 페이지 나누기 구조화 문서 생성（`flow` feature）

```toml
harumi = { version = "1", features = ["flow"] }
```

```rust
use harumi::{FlowDocument, FlowOptions};

let font = include_bytes!("NotoSansCJK-Regular.ttf");
let mut doc = FlowDocument::new(font.as_ref(), FlowOptions::default())?;

doc.push_heading("연간 보고서", 1)?;
doc.push_paragraph("이 문서는 당기 실적을 정리한 것입니다.")?;
doc.push_key_value_table(&[
    ("매출액", "100만 원"),
    ("비용", "80만 원"),
    ("이익", "20만 원"),
])?;
doc.push_list(&["3개 신시장 진출", "신제품 2종 출시"], false)?;

// 콘텐츠가 페이지를 초과하면 자동으로 페이지가 추가됩니다.
// push_page_break()로 원하는 위치에 수동 페이지 나누기를 삽입할 수 있습니다.

let pdf_bytes = doc.render()?;
```

한국어·일본어·중국어를 기본 지원 — CJK TTF 폰트를 전달하면 임의의 문자 위치에서 자동 줄바꿈합니다.

### 페이지 번호가 있는 헤더/푸터（`flow` feature）

```rust
use harumi::{FlowDocument, FlowOptions, HeaderFooter};

let opts = FlowOptions {
    // 모든 페이지의 왼쪽에 "harumi docs", 오른쪽에 "v0.5"
    header: Some(HeaderFooter {
        left:  Some("harumi docs".into()),
        right: Some("v0.5".into()),
        ..Default::default()
    }),
    // 가운데에 "1 / 3" 페이지 카운터
    footer: Some(HeaderFooter::page_number()),
    // push_heading()이 자동으로 북마크 항목을 생성（기본값: true）
    auto_bookmarks: true,
    ..Default::default()
};

let mut doc = FlowDocument::new(font, opts)?;
doc.push_heading("제1장", 1)?;
doc.push_paragraph("본문 텍스트입니다.")?;
let pdf_bytes = doc.render()?;
```

### FlowDocument 인라인 텍스트 스타일 (`flow` feature)

```rust
use harumi::{FlowDocument, FlowOptions, InlineSpan};

let mut doc = FlowDocument::new(font_bytes, FlowOptions::default())?;
doc.push_paragraph_styled(&[
    InlineSpan::plain("일반 텍스트, "),
    InlineSpan::bold("굵은 텍스트, "),
    InlineSpan::italic("기울임 텍스트, "),
    InlineSpan::colored("빨간 텍스트.", [0.8, 0.0, 0.0]),
])?;
```

굵은 글씨와 기울임은 **합성 효과**로, 별도의 폰트 파일이 필요하지 않습니다.

### 마크업 어노테이션（하이라이트, 밑줄, 취소선, 물결 밑줄）

```rust
// 노란색 하이라이트
doc.page(1)?.add_highlight([72.0, 690.0, 200.0, 14.0], [1.0, 1.0, 0.0])?;

// 빨간색 밑줄
doc.page(1)?.add_underline([72.0, 640.0, 200.0, 12.0], [1.0, 0.0, 0.0])?;

// 취소선
doc.page(1)?.add_strikeout([72.0, 590.0, 200.0, 12.0], [0.0, 0.0, 0.0])?;

// 물결 밑줄
doc.page(1)?.add_squiggly([72.0, 540.0, 200.0, 12.0], [0.0, 0.6, 0.2])?;

// 스티키 노트 댓글
doc.page(1)?.add_sticky_note([500.0, 700.0], "이 섹션을 검토하세요")?;
doc.save("annotated.pdf")?;
```

### 비밀번호 보호 PDF

```rust
// 암호화된 PDF 불러오기
let mut doc = Document::from_file_with_password("protected.pdf", "secret")?;
assert!(doc.is_encrypted());

// 잘못된 비밀번호는 Error::WrongPassword 반환
match Document::from_bytes_with_password(&bytes, "wrong") {
    Err(harumi::Error::WrongPassword) => println!("비밀번호가 틀렸습니다"),
    _ => {}
}

// 비밀번호 보호하여 저장
let mut doc = Document::new((595.0, 842.0))?;
doc.set_encryption("userpass", "ownerpass")?;
doc.save("protected_output.pdf")?;
```

### AcroForm：양식 필드 읽기 및 채우기

```rust
// 모든 양식 필드 읽기
let mut doc = Document::from_file("form.pdf")?;
for field in doc.form_fields()? {
    println!("{}: {:?} = {:?}", field.name, field.field_type, field.value);
}

// 이름으로 필드 채우기
let updated = doc.fill_form(&[
    ("FullName",   "홍길동"),
    ("Agree",      "yes"),       // 체크박스 → /Yes
    ("Department", "Engineering"),
])?;
println!("{updated}개 필드 업데이트됨");
doc.save("filled_form.pdf")?;
```

### 페이지 박스（인쇄 워크플로우）

```rust
// CropBox（보이는 영역 클립）읽기/쓰기
let cb = doc.page(1)?.crop_box()?;   // Option<[f32;4]>

doc.page(1)?.set_crop_box([10.0, 10.0, 575.0, 822.0])?;   // [x,y,w,h]
doc.page(1)?.set_trim_box([0.0, 0.0, 595.0, 842.0])?;
doc.page(1)?.set_bleed_box([0.0, 0.0, 601.0, 848.0])?;
doc.save("print_ready.pdf")?;
```

### 링크 어노테이션

```rust
// 클릭 가능한 URL 영역（x, y, 너비, 높이）
doc.page(1)?.add_link_url([72.0, 40.0, 200.0, 18.0], "https://example.com")?;

// 내부 링크: 해당 영역을 클릭하면 같은 문서의 3페이지로 이동
doc.page(1)?.add_link_internal([72.0, 700.0, 150.0, 18.0], 3)?;
```

### 북마크/문서 개요

```rust
// PDF 뷰어의 북마크 패널을 구성합니다.
// ASCII 이외의 제목（CJK, 악센트 라틴어 등）은 UTF-16BE로 자동 인코딩됩니다.
doc.add_bookmark("제1장",        1, 800.0)?;   // 제목, 페이지（1부터 시작）, y 좌표
doc.add_bookmark("第2章 概要",   2, 800.0)?;
doc.save("report.pdf")?;
```

### HTML → PDF 변환（`html` feature）

```toml
harumi = { version = "1", features = ["html"] }
```

```rust
use harumi::{render_html_to_pdf, HtmlRenderOptions};

let font = include_bytes!("NotoSansCJK-Regular.ttf").to_vec();
let html = r#"
    <h1>연간 보고서</h1>
    <p>서론 단락입니다.</p>
    <table>
      <tr><th>매출액</th><td>100만 원</td></tr>
      <tr><th>이익</th><td>20만 원</td></tr>
    </table>
    <h2>주요 성과</h2>
    <ul><li>3개 신시장 진출</li><li>신제품 2종 출시</li></ul>
    <div style="page-break-after: always"></div>
    <h1>2페이지</h1>
"#;

let pdf_bytes = render_html_to_pdf(html, HtmlRenderOptions {
    font_bytes: font,
    ..HtmlRenderOptions::default()
})?;
```

지원 요소: `<h1>`–`<h6>`, `<p>`, `<table>/<tr>/<th>/<td>`, `<ul>/<ol>/<li>`, `<div>/<section>/<article>`（블록 컨테이너）.  
페이지 나누기: `style="page-break-after: always"` 또는 `class="page-break"`.  
건너뜀: `<script>`, `<style>`, `<head>`.  
깊은 중첩 HTML도 스택 오버플로 없이 처리（반복형 파서, 5000단계 `<div>` 중첩 검증 완료）.

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

// 기존 콘텐츠 스트림에서 텍스트 바꾸기（단일 연산자 매칭）; 매칭 건수 반환
let n: usize = doc.page(1)?.replace_text(old_text, new_text, font)?;
// 원래 임베드 폰트를 사용해 교체; 글리프 즉시 검증; 매칭 건수 반환
let n: usize = doc.page(1)?.replace_text_preserve_font(old_text, new_text)?;
// 읽기 전용 스캔: 매칭 건수 또는 Err(FontCharNotMapped) 반환
let n: usize = doc.page(1)?.can_replace_text(old_text, new_text)?;

// 링크 어노테이션（feature gate 불필요）
doc.page(1)?.add_link_url([x, y, w, h], "https://example.com")?;   // URL 링크
doc.page(1)?.add_link_internal([x, y, w, h], target_page)?;         // 문서 내 링크

// 문서 아웃라인 / 북마크（feature gate 불필요）
doc.add_bookmark("섹션 제목", page, y)?;  // 플랫 아웃라인 항목 추가
```

### 좌표계

좌표는 **PDF 포인트** (1pt = 1/72인치) 단위이며, 원점은 페이지 **좌하단**입니다:

```toml
harumi = { version = "1", features = ["ocr"] }
```

### 기능 플래그

| 플래그 | 활성화되는 기능 | 추가 의존성 |
|---|---|---|
| *(기본)* | 텍스트 오버레이, 폰트 임베드, `add_text_box`, `add_text_box_aligned`, `add_text_with_opacity`, `add_text_box_with_opacity` | lopdf, ttf-parser |
| `draw` | `add_rect`, `add_line`, `add_rect_stroke`, `add_polygon`, `add_polyline`, `add_ellipse` — 도형 그리기 | 없음 |
| `image` | `add_image`, `add_image_with_opacity` — JPEG/PNG 이미지 삽입；`extract_page_image` — 스캔 PDF에서 임베드 이미지 추출（`draw` 자동 활성화） | `png` crate（순수 Rust） |
| `ocr` | `ocr::hocr_y_to_pdf`, `ocr::hocr_x_to_pdf`, `ocr::pixel_size_to_pt` — Tesseract 좌표 변환 헬퍼 | 없음 |
| `flow` | `FlowDocument` 푸시형 빌더, 자동 페이지 나누기（`push_heading`, `push_paragraph`, `push_key_value_table`, `push_list`, `push_page_break`, `render`）; `{{page}}`/`{{total}}` 치환 기능이 있는 `HeaderFooter`; 제목에서 자동 북마크 생성 `auto_bookmarks` | 없음 |
| `html` | `render_html_to_pdf` — HTML→PDF 변환（h1–h6, p, table, ul/ol, 페이지 나누기; `flow` 자동 활성화）; 내장 순수 Rust HTML 토크나이저 | 없음 |

```rust
let pdf_y = harumi::ocr::hocr_y_to_pdf(pixel_y, page_height_pts, image_dpi);
let pdf_x = harumi::ocr::hocr_x_to_pdf(pixel_x, image_dpi);
let pt    = harumi::ocr::pixel_size_to_pt(pixel_size, image_dpi);
```

---

## 지원 폰트 형식

| 폰트 형식 | 지원 상태 |
|---|---|
| TrueType (`.ttf`) | ✅ 완벽한 지원 — 순수 Rust 서브셋팅 엔진 |
| TTC 컬렉션 | ✅ 완벽한 지원 — `embed_font_at(bytes, face_index)`로 면 지정 가능 |
| OpenType CFF (`.otf`) | ⚠️ 수락하지만（서브셋팅 미지원） — 그대로 임베드됨 |

**TrueType** 버전을 권장합니다 (엔드투엔드 검증 완료):

```
NotoSansCJKkr-Regular.ttf  （한국어）
NotoSansCJKjp-Regular.ttf  （일본어）
NotoSansCJKsc-Regular.ttf  （중국어 간체）
NotoSansCJKtc-Regular.ttf  （중국어 번체）
```

> **OTF 참고**: harumi는 `.otf` 파일을 수락하고 `FontFile3 /OpenType`으로 임베드하지만, **CFF 폰트 서브셋팅을 지원하지 않습니다** — 폰트 내의 모든 글리프가 임베드되어 PDF 파일이 커집니다. 크기 최적화를 위해 위의 TrueType 버전을 사용하세요.

---

## 내부 구현

```
harumi
├── lopdf v0.40          — 기존 PDF 객체 그래프 파싱 및 편집
├── ttf-parser           — 폰트 메타데이터 읽기 (bbox, units_per_em, ascender)
└── [내장 TTF 서브셋팅]   — 순수 Rust TrueType 서브셋팅 엔진（외부 의존성 없음）
```

폰트 처리 파이프라인:

1. 사용된 문자 수집 → Unicode 코드 포인트 집합 생성
2. 폰트 `cmap` 테이블로 코드 포인트 → 원래 GID 매핑 (ttf-parser)
3. 내장 엔진으로 사용된 글리프만 TTF 서브셋 생성 (GID **0..N으로 재번호 지정**)
4. `gid_to_char`와 어드밴스 폭을 원래 GID → 새 GID로 **재매핑** (글자 깨짐 방지)
5. PDF CID 폰트 객체 그래프 구성: `Type0 → CIDFontType2 → FontDescriptor → FontFile2`
6. `/ToUnicode` CMap 스트림 생성 (뷰어에서 텍스트 복사/검색 가능)
7. 페이지 `/Contents` 배열에 새 콘텐츠 스트림 추가

서브셋팅은 **지연 실행**: `embed_font()`는 원시 TTF 바이트를 저장하고, `save()` 시에 모든 페이지의 사용 문자를 수집하여 폰트별로 한 번만 처리합니다.

### 의존성 최소화

harumi는 **외부 런타임 의존성 없음**（PDF 핵심 처리 제외）을 목표로 합니다.

- **TrueType 서브셋팅** — 내장 순수 Rust 구현（v1.1+）; TTF + TTC（컬렉션） 지원, 재귀적 복합 글리프 분석
- **폰트 파싱** — ttf-parser（전문 용도, 추이적 의존 없음）
- **이미지 디코딩** — `png` crate（선택사항, feature 게이트됨）
- **암호화** — getrandom（OS 엔트로피만; AES-256 암호화 키 생성 필요）

**직접 의존수**: 3개（getrandom, lopdf, ttf-parser, 옵션 `png`）  
**추이적 의존（기본 빌드）**: 약 8개（lopdf 내부 유틸리티만）

---

## 이름에 대하여

晴海（はるみ / Harumi）— *晴*（맑은 하늘）＋ *海*（바다）。표면은 잔잔하고, 내부에선 많은 일이 일어나고 있다.

## 로드맵

| 버전 | 범위 |
|---|---|
| **v0.1** | TrueType 폰트, 보이지 않는/가시적 텍스트, 일괄 배치, `page.size()`, `save_to_bytes()`, GID 재매핑, OTF 수락 |
| **v0.2** | `draw` feature（`add_rect`, `add_line`）, `image` feature（`add_image`, PNG SMask 투명도）, 페이지 조작（`rotate_page`, `remove_page`, `insert_blank_page`, `reorder_pages`） |
| **v0.3** | `add_text_box`, `add_rect_stroke`, `add_polygon`, `add_ellipse`, `add_path`; `add_text_with_rotation`; 보안 강화; `merge_from`; `Document::new`; `extract_pages` |
| **v0.4** | `extract_text_runs`（CID + 표준 폰트）, PDF 메타데이터 읽기/쓰기, `replace_text`（Tj/TJ 재작성, 크로스 연산자 매칭, 폭 보상, 폰트 유지 모드）, `flow` feature（`FlowDocument`, CJK 자동 줄바꿈）, `html` feature, `extract_page_image` |
| **v0.5** | `add_link_url`, `add_link_internal` — 클릭 가능한 PDF 링크 어노테이션; `add_bookmark` — CJK UTF-16BE 아웃라인; `HeaderFooter`; 보안 수정 |
| **v0.6** | 암호화 PDF 읽기（`from_file_with_password` / `is_encrypted` / `Error::WrongPassword`）; 마크업 주석（하이라이트·밑줄·취소선·메모）; AcroForm `form_fields()` / `fill_form()`; AGL 테이블 +116 항목; Identity-H 텍스트 추출 폴백 |
| **v0.7** *（현재）* | `set_encryption` — 암호화된 PDF 저장; `add_squiggly` — 물결 밑줄 주석; 페이지 박스 전체 지원（크롭·트림·블리드·미디어 박스 읽기/쓰기） |
| **v0.8** | FlowDocument 인라인 스타일（`InlineSpan` 굵기/기울임/색상 합성 효과）; `replace_text_resubset` — 서브셋 확장 포함 텍스트 교체; MCP `pdf_replace_text` 레이아웃 유지 번역 워크플로와 비 Identity `CIDToGIDMap` 진단; `cargo semver-checks` CI |
| **v1.4.1** | `TextFragment` 폰트 속성（`is_bold`・`is_italic`・`font_family`・`base_font`）；`detect_text_columns` + `ColumnZone` 단 레이아웃 추론 |
| **v1.4.2** | `harumi-ai` overlay 모드 정확도 향상：행별 흰색 사각형 크기（높이·너비·디센더 피복）、정확한 기준선 Y 배치、`detect_text_columns` 다단 지원、굵기 기반 제목 감지、NaN 안전 읽기 순서 정렬 |
| **v1.5.0** | `group_text_fragments` — `TextFragment`을 행/단락 `TextGroup`으로 병합; `font_covers_char` — cmap 커버리지 조회; Form XObject 재귀 텍스트 추출 (`Do` 연산자); 다중 CS 간 그래픽 상태 보존; `harumi-ai`: `OverflowStrategy`(Shrink/Truncate), `font_fallbacks` 다중 폰트 렌더링, `on_progress` 콜백; `extract_table_cells` — 테이블 행/열 감지（휴리스틱） |
| **v1.5.1** | `harumi-ai` v0.1.0 첫 crates.io 출시; InPlace 디버그 로그의 CJK 바이트 슬라이스 패닉 수정; Clippy lint 수정（`repeat_n`、`div_ceil`） |
| **v1.5.2** | 조상 Pages 노드의 폰트 상속 수정（`collect_fonts_inner` 부모 체인 탐색）; LLM 미이스케이프 따옴표 JSON 수복; 기본 `max_tokens` 4096→16000 |
| **v1.5.3** | 상속된 `/Resources`에서 Form XObject 발견 수정（Chrome/Skia PDF — `extract_text_from_xobjects` 부모 체인 탐색）; `replace_text()`가 Form XObject 콘텐츠 스트림도 재작성하여 `harumi-ai` InPlace 모드가 Chrome/Skia PDF에서 동작 |
| **v1.5.4** | Type3 폰트 텍스트 추출 지원（`collect_font_dict_entries`에 `/Subtype /Type3` 추가）; Type3 폰트만 사용하는 Chrome/Skia PDF의 번역 출력 제로 문제 수정 |
| **v1.5.5** | Overlay CTM 좌표 변환 수정 — `parse_content_stream`이 `q`/`Q`/`cm`을 추적하여 `TextFragment` 좌표를 페이지 공간으로 변환; Chrome/Skia PDF（스케일 + Y축 반전 CTM）의 overlay 텍스트 위치가 정확히 배치됨 |
| **v1.5.6** | Chrome/Skia PDF 3개 버그 수정：(1) `overlay_from`이 기존 페이지 콘텐츠를 `q`/`Q`로 감싸 불균형한 `cm` 연산자를 격리；(2) `extract_text_from_xobjects`가 `ParseCarryState.do_ctm_map`으로 per-`Do` CTM을 추적하여 마지막 누적 CTM 대신 각 XObject 고유의 CTM을 적용；(3) `diagnose_match_failure`에 cross-BT 스캔을 추가하여 BT/ET 경계를 넘는 Type3 폰트 텍스트를 `"type3-char-per-tj"`로 보고 |
| **v1.5.8** | 번역 시각 버그 3개 수정：(1) `finalize()`가 `append_to_contents()` 전에 `wrap_page_contents_in_q_q()`를 호출하여 기존 콘텐츠의 불균형한 `cm`이 추가된 스트림에 영향을 주지 않도록 격리；(2) `ctm_stack`을 `ParseCarryState`로 이동하여 여러 `Contents` 배열 스트림 간에 올바르게 지속（Form XObject per-XObject CTM 추적）；(3) Chrome/Skia Type3 PDF용 cross-BT `Tj` 교체 — `find_cross_bt_matches()` + `CrossBtMatch`로 BT/ET 블록을 넘는 텍스트 감지, `replace_text()`가 이런 PDF에서 작동 |
| **v1.5.9** | `ReplaceOptions { normalize_whitespace }` + `replace_text_opts()` — 매칭 전 `old_text`에서 공백을 제거하여 harumi-ai의 공백 결합 프래그먼트（`"T h e F r e e"` 등）가 Chrome/Skia Type3 PDF와 일치하도록（harumi-ai 그룹핑 로직 변경 불필요）；`TextFragment.space_advance` — 해당 폰트 크기에서 공백 글리프 전진 너비를 추가, 인접 프래그먼트가 단어 간격인지 문자 간격인지 판별하여 `"10M+"` → `"1 0 M +"` 문제 해소 |
| **v1.5.10** | cross-BT 매치 카운트가 항상 0을 반환하는 버그 수정：`count_matches_in_raw_streams()`에 cross-BT 카운팅 패스를 추가하여 `replace_text_opts(normalize_whitespace: true)`가 Chrome/Skia Type3 PDF에서 실제로 교체 작업을 큐에 쌓도록；`find_cross_bt_matches()` 폰트 추적 수정 — `BT`의 `cur_font.clear()`와 `Tf`의 `in_bt` 가드를 제거하여 `BT`/`ET` 간에 폰트가 올바르게 유지되도록（PDF 규격 준수） |
| **v1.5.11** | 전통적인 일본어 PDF（GHS SDS 등）InPlace 일치율 향상：같은 시각 줄에서 문자별 `Tm`으로 위치를 지정하는 패턴（일본어 PDF 생성 도구에서 일반적）이 cross-op/cross-Tf 매칭에서 올바르게 작동；`collect_char_segments` 및 `collect_cross_tf_segments`가 수직 Tm（y 변화량 ≥ 1 pt）에만 플러시；`Tm` 및 텍스트 상태 연산자（`Tc`/`Tw`/`Tz`/`TL`/`Ts`）를 중간 연산자 허용 목록에 추가하고 `rewrite_content_stream`에서 억제 |
| **v1.5.12** | AES-256 암호화 PDF의 자동 스트림 스킵 수정：`page_content_streams()`와 `decode_form_xobject()`가 `decompress()` 실패 시 `stream.content`로 폴백（lopdf가 `load_with_password` 중 이미 압축 해제한 경우 대응）, 페이지당 13→40+ 프래그먼트로 개선；`ExtractionWarning`/`WarningKind` + `extract_text_runs_verbose()` 진단 API；`TextFragment.tf_font_size` + `TextFragment.tm_y_scale` 새 필드；제로 전진 너비 폴백（문자당 0.5em） |
| **v1.5.13** | XObject 폰트 해석 버그 수정（PScript5/Distiller PDF）：`xobject_fonts()`가 페이지 수준 폰트를 기반으로 사용하도록 변경；Form XObject에 `/Resources`는 있지만 `/Font` 하위 항목이 없는 경우（Distiller PDF의 전형적인 구조）텍스트 전체 누락 수정 |
| **v1.5.14** | 크로스 스트림 BT/ET 상태 유지：`in_bt`・현재 폰트・텍스트 위치를 `ParseCarryState`에 이동해 스트림 경계를 초월해 보존；Distiller PDF가 단일 BT…ET 블록을 여러 `/Contents` 배열 스트림으로 분할할 때 후속 스트림의 `Tj` 전체 누락 수정 |
| **v1.5.15** | `TextFragment.source_stream` / `source_op_start` / `source_op_end` — 연산자 수준 소스 추적；`PageHandle::replace_text_fragments(fragments, new_text, font)` — 소스 Tj/TJ를 `() Tj`로 억제하고 번역 텍스트 배치；PScript5/Distiller·Type3 문자별 PDF의 InPlace 번역 가능 |

---

## 기여

[github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi)에서 Issue와 PR을 환영합니다.

코드베이스에서 가장 복잡한 부분은 `src/font/embed.rs` (CID 폰트 객체 그래프 구성)입니다. 특정 PDF 뷰어에서 렌더링 버그를 보고할 때는 뷰어 이름과 버전을 명시해 주세요.

---

## 라이선스

MIT OR Apache-2.0
