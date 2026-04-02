# HTML 패턴 발견 도구 설계

## 목표

크롤링된 12,108개 HTML 파일에서 **가정 없이** 모든 구조적 요소를 추출하고,
빈도 분석을 통해 API 페이지의 실제 패턴을 발견한다.

이 결과물은 다음 세션에서 `build_bundle.rs` 번들링 로직 개선의 근거 자료로 사용된다.
HTML 파싱이 번들링 런타임에 포함되는 것이 아니라, **코드 개선을 위한 1회성 연구**.

## 배경

- 현재 Swagger 기반 스펙 커버리지: 32.6% (3,953/12,108)
- 페이지마다 패턴이 달라서 분류/추출 정확도가 낮음
- 크롤링 완료: `data/pages/{list_id}.html` (12,108개, 2.0GB)
- 크기 분포: < 5KB(8), 100-150KB(1,858), 150-200KB(9,282), 200-300KB(946), 300-500KB(11), 500KB+(3)

## 샘플 검증 결과 요약

Codex의 4패턴 분류를 A+B(계층+크기) 샘플링과 C(랜덤+극단값) 샘플링으로 검증한 결과:

- Codex 기본 프레임워크는 맞지만 중요한 누락 존재
- `tyDetailCode` (PRDE01/PRDE02/PRDE04)가 실제 분기점
- PRDE01(SOAP) 6개, PRDE04 내 swaggerJson 3가지 변형 등 미분류 케이스 발견
- `oprtinUrl`, JSON-LD Dataset, `/catalog/{pk}/openapi.json` 등 추가 데이터 소스 발견
- **사전 정의한 신호만 찾으면 미지의 패턴을 놓침** → raw 데이터에서 패턴을 뽑는 접근 필요

## 아키텍처

```
data/pages/*.html (12,108개)
        │
        ▼
[1단계: Raw 구조 추출 — analyze_pages.rs]
  가정 없이 각 파일의 모든 구조적 요소를 기계적으로 나열
        │
        ▼
data/page_raw_signals.json (12,108개 엔트리)
        │
        ▼
[2단계: 빈도 분석 — summarize_signals.rs]
  - 12,108개 전부에 있는 요소 → 공통 템플릿 (제거)
  - 일부에만 있는 요소 → 패턴 변별 신호
  - 요소 조합별 클러스터링 → 실제 패턴 그룹
        │
        ▼
data/signal_summary.json (패턴 그룹 + 분포)
```

1단계/2단계 분리 이유: 크롤러의 수집/분석 분리와 같은 원칙.
추출 규칙을 수정할 때 전체 HTML 파싱을 다시 돌릴 필요 없이, 2단계만 반복 실행.

## 1단계: Raw 구조 추출 (analyze_pages.rs)

### 추출 대상 — 가정 없이 모든 구조적 요소

**A) DOM 요소 기반**
- 모든 태그별 출현 수 (`table: 6, select: 2, input[hidden]: 5, ...`)
- 모든 id 속성값
- 모든 name 속성값
- 모든 class 속성값
- 모든 data-* 속성 (key + value)
- 모든 `<th>` 텍스트 (테이블 헤더)
- 모든 `<option>` 텍스트/value

**B) JavaScript 기반**
- 모든 변수 선언 (`var/let/const xxx = ...` 에서 변수명 + 값의 타입/크기)
- 모든 함수 호출 패턴 (`$.ajax({url: ...})`, `fetch(...)` 등)
- 인라인 JSON 블록 (`<script type="application/ld+json">` 등)

**C) 메타 기반**
- 모든 `<meta>` 태그
- 모든 `<link>` 태그
- `<title>` 텍스트

### 출력 형식

```json
// data/page_raw_signals.json
{
  "15095367": {
    "file_size_bytes": 203000,
    "tag_counts": { "table": 6, "select": 2, "input": 12, ... },
    "ids": ["publicDataDetailPk", "publicDataPk", "open_api_detail_select", ...],
    "names": ["publicDataDetailPk", ...],
    "classes": ["dataset-table", "api-data-bx", ...],
    "data_attrs": { "data-paramtr-nm": ["serviceKey", "pageNo", ...], ... },
    "th_texts": ["항목명", "타입", "구분", "설명", ...],
    "options": [{ "select_id": "open_api_detail_select", "values": ["37400"], "texts": ["수입식품..."] }],
    "js_vars": { "swaggerJson": { "type": "string", "size": 0 }, "tyDetailCode": { "type": "string", "value": "PRDE02" }, ... },
    "ajax_urls": ["/tcs/dss/selectApiDetailFunction.do", ...],
    "json_ld": [{ "type": "Dataset", "keys": ["name", "description", ...] }],
    "meta_tags": [{ "name": "og:title", "content": "..." }, ...],
    "title": "공기질측정 정보"
  },
  ...
}
```

### 구현

- `src/bin/analyze_pages.rs`
- `scraper` 크레이트 (DOM 파싱) + regex (JavaScript 변수/AJAX 추출)
- 병렬 처리 (rayon 또는 tokio) — 12,108개를 빠르게 순회
- 에러 페이지(< 5KB)도 스킵하지 않고 동일하게 추출 (패턴 발견 목적)

## 2단계: 빈도 분석 (summarize_signals.rs)

### 분석 내용

- **요소별 빈도**: 각 id/class/js_var/th_text 등이 몇 개 파일에 나타나는가
  - 12,108개 전부 → 공통 템플릿 요소
  - N개에만 → 패턴 변별 신호 (N값과 함께 보고)
  - 1-10개 → 이상치
- **요소 조합 클러스터링**: 변별 신호들의 조합으로 파일 그룹핑
  - 예: {tyDetailCode=PRDE02, swaggerJson.size>0} → 그룹 A
  - 예: {tyDetailCode=PRDE04, options=0} → 그룹 C2
- **그룹별 대표 파일**: 각 그룹에서 파일 3개 선정 (다음 세션에서 상세 확인용)

### 출력 형식

```json
// data/signal_summary.json
{
  "element_frequencies": {
    "ids": {
      "publicDataDetailPk": 12100,
      "open_api_detail_select": 7200,
      "some_unknown_id": 45,
      ...
    },
    "js_vars": { ... },
    "th_texts": { ... },
    ...
  },
  "discriminating_signals": [
    { "signal": "js_vars.tyDetailCode=PRDE02", "count": 7255 },
    { "signal": "js_vars.tyDetailCode=PRDE04", "count": 4845 },
    ...
  ],
  "clusters": [
    {
      "name": "auto_cluster_1",
      "signals": ["tyDetailCode=PRDE02", "swaggerJson.size>0"],
      "count": 4010,
      "sample_files": ["15125655", "15073554", "15154402"]
    },
    ...
  ]
}
```

## 산출물

| 파일 | 용도 |
|------|------|
| `src/bin/analyze_pages.rs` | 1단계 추출기 |
| `src/bin/summarize_signals.rs` | 2단계 빈도 분석기 |
| `data/page_raw_signals.json` | 12,108개 raw 신호 (다음 세션 입력) |
| `data/signal_summary.json` | 패턴 그룹 + 분포 (다음 세션 입력) |

## 다음 세션 (이번 스코프 밖)

- signal_summary.json을 분석하여 패턴 그룹 확정
- 각 그룹과 현재 SpecStatus 매핑 비교
- build_bundle.rs 분류 로직 개선
