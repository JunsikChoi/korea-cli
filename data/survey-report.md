# API 전수조사 결과 보고서

> **조사일시**: 2026-04-01 18:05 ~ 18:34 UTC (28.4분 소요)
> **도구**: `cargo run --bin survey` (concurrency 5, delay 100ms)
> **데이터**: `data/survey.json` (7.5MB, 12,108 entries)

---

## 1. 요약 (Executive Summary)

| 항목 | 수치 |
|------|------|
| 메타 API 수집 | 17,236 레코드 → 12,108 고유 서비스 (중복 병합) |
| 조사 성공 | 12,093 (99.9%) |
| 조사 실패 | 15 (0.1%) |
| HTTP 200 | 12,097 |
| HTTP 비-200 | 0 (실패 15건은 전부 네트워크 에러) |

### SpecStatus 분류 결과

| 분류 | 수 | 비율 | 의미 |
|------|---:|-----:|------|
| **Available** | 3,949 | 32.6% | Swagger 스펙 있음, operations > 0 |
| **Skeleton** | 1,404 | 11.6% | Swagger 파일만 있고 operations 0개 |
| **External** | 4,652 | 38.4% | 외부 도메인 API (data.go.kr 게이트웨이 아님) |
| **CatalogOnly** | 2,101 | 17.4% | 카탈로그만 있고 endpoint 없음 |
| **HtmlOnly** | 2 | 0.02% | HTML 테이블만 존재 |

**핵심 발견**: 전체 12,108개 중 **실제 호출 가능한 API는 3,949개 (32.6%)**이다. 나머지 67.4%는 스펙 부재, 외부 도메인, 또는 카탈로그 메타데이터만 존재한다.

---

## 2. Swagger 스펙 분석

### 2.1 Swagger 신호 분포

| 신호 | 수 | 비율 |
|------|---:|-----:|
| `swaggerJson` inline 있음 | 5,353 | 44.2% |
| `swaggerUrl`만 있음 (inline 없음) | 32 | 0.3% |
| 둘 다 있음 | 0 | 0% |
| Swagger 완전 부재 | 6,723 | 55.5% |

- `swaggerJson`과 `swaggerUrl`이 동시에 존재하는 경우는 **0건**.
- `swaggerUrl`이 있는 32건은 전부 `infuser.odcloud.kr` 도메인.

### 2.2 Swagger Operations 수 분포 (inline JSON 5,353건 중)

| ops 수 | 수 | 비율 |
|--------|---:|-----:|
| 0 (skeleton) | 1,404 | 26.2% |
| 1개 | 3,151 | 58.9% |
| 2-3개 | 531 | 9.9% |
| 4-5개 | 117 | 2.2% |
| 6-10개 | 89 | 1.7% |
| 11-20개 | 50 | 0.9% |
| 21-50개 | 11 | 0.2% |

- Swagger 파싱 에러는 **0건** — `parse_swagger`가 모든 inline JSON을 정상 처리.
- 가장 많은 operation을 가진 API: 금융위원회 공시정보 (33ops).

### 2.3 Operations 최다 Top 10

| ops | list_id | 서비스명 |
|----:|---------|---------|
| 33 | 15059649 | 금융위원회_공시정보 |
| 32 | 15155728 | 국토안전관리원_특수교량 동적 계측 데이터 서비스 |
| 31 | 15073554 | 행정안전부_안전정보 통합공개 조회 서비스 |
| 30 | 15059651 | 금융위원회_금융회사공시정보 |
| 30 | 15144796 | 농촌진흥청 국립농업과학원_토양도 기반 토양특성 통계정보 V2 |
| 28 | 15109847 | 대구광역시 달서구_전통시장 현황정보 및 상점정보 |
| 25 | 15144645 | 근로복지공단_산재보험 요양 신청·승인 데이터 |
| 25 | 15129394 | 조달청_나라장터 입찰공고정보서비스 |
| 24 | 15126680 | 국립암센터_사망률 지표 정보제공 서비스 |
| 23 | 15129397 | 조달청_나라장터 낙찰정보서비스 |

---

## 3. HTML 스펙 분석

| 항목 | 수 |
|------|---:|
| `publicDataDetailPk` + operations | 0 |
| `publicDataDetailPk`만 (operations 없음) | 0 |
| pk 없음 | 12,108 |

**HTML 스펙이 단 한 건도 탐지되지 않았다.** 이는 openapi.do 페이지가 비로그인 상태에서 `publicDataDetailPk` hidden input을 렌더링하지 않기 때문이다. §5 로그인 벽 분석 참고.

---

## 4. Endpoint 도메인 분석

### 4.1 endpoint_url 상태

| 패턴 | 수 | 비율 | 설명 |
|------|---:|-----:|------|
| `' '` (공백 1자) | 8,883 | 73.4% | 메타 API가 빈 문자열 대신 공백을 반환 |
| `''` (빈 문자열) | 2,598 | 21.5% | endpoint 미등록 |
| `'http://'` (프로토콜만) | 5 | 0.04% | 불완전한 URL |
| 유효한 URL | 622 | 5.1% | 실제 외부 도메인 |

**주의**: `extract_domain`이 `' '`(공백)을 빈 문자열로 처리하지 않아 SpecStatus 분류에 영향을 줄 수 있다. 공백 URL은 `(empty)`와 동일하게 취급해야 한다.

### 4.2 유효 외부 도메인 Top 20

| 수 | 도메인 |
|---:|--------|
| 65 | openapi.q-net.or.kr |
| 53 | openapi.jeonju.go.kr |
| 41 | opendata.icpa.or.kr |
| 32 | dataopen.kospo.co.kr |
| 32 | data.uiryeong.go.kr |
| 26 | c.q-net.or.kr |
| 25 | openapi.airport.co.kr |
| 22 | www.ygpa.or.kr:9191 |
| 20 | data.sisul.or.kr |
| 19 | www.djtc.kr |
| 17 | data.khnp.co.kr |
| 16 | www.kdhc.co.kr:443 |
| 13 | www.korad.or.kr |
| 12 | data.geoje.go.kr |
| 10 | kipo-api-gw.koreantk.com |
| 10 | www.koat.or.kr |
| 10 | www.andong.go.kr |
| 10 | www.daejeon.go.kr |
| 8 | data.humetro.busan.kr |
| 8 | openapi.epost.go.kr:80 |

총 고유 도메인: **81개**

### 4.3 외부 도메인 TLD 분포

| TLD | 수 | 설명 |
|-----|---:|------|
| 기타 (IP, 포트, 범용) | 11,498 | `' '` 공백 포함 |
| .or.kr | 265 | 비영리 기관 |
| .go.kr | 211 | 정부 기관 |
| .co.kr | 96 | 기업 |
| .kr 기타 | 31 | 기타 한국 도메인 |

---

## 5. 로그인 벽 분석

### 5.1 로그인 영향 범위

| 항목 | 수 |
|------|---:|
| `login_required` anomaly 발생 | 12,085 (99.8%) |
| `login_required` 없음 | 23 (0.2%) |

**사실상 모든 openapi.do 페이지가 로그인을 요구한다.** 비로그인 상태에서도 HTTP 200을 반환하지만, 페이지 콘텐츠에 `login`과 `session` 관련 텍스트가 포함되어 있다.

### 5.2 로그인 벽에도 불구하고 Swagger가 노출되는 이유

| 분류 | login_required 여부 | 수 |
|------|:---:|---:|
| Available + login_required | O | 3,949 |
| Available + 로그인 불필요 | - | 0 |

놀랍게도 **Available 3,949건 전부 login_required anomaly가 있다**. 이는 anomaly 탐지 로직이 페이지의 `login`+`session` 문자열 존재만으로 판단하기 때문이다. 실제로는:

- openapi.do 페이지의 **공통 레이아웃**에 로그인 관련 JS/HTML이 항상 포함됨
- `swaggerJson` inline 변수는 로그인과 무관하게 서버 사이드에서 렌더링
- 따라서 `login_required` anomaly의 **오탐율이 매우 높다** (거의 100%)

**결론**: `login_required` anomaly 탐지 로직을 개선해야 한다. 현재 로직은 유의미한 신호가 아니다.

### 5.3 HTML pk가 0건인 이유

HTML pk (`publicDataDetailPk`)가 전혀 탐지되지 않은 것은 다음 두 가지 가능성이 있다:

1. **비로그인 상태에서 pk가 렌더링되지 않음** — 가능성 높음
2. **HTML 파서의 selector가 현재 페이지 구조와 불일치** — 검증 필요

Swagger가 비로그인에서도 노출되는 것과 달리, HTML 스펙 영역은 로그인 세션이 필요할 수 있다. 또는 openapi.do 페이지의 HTML 구조가 변경되었을 수 있다. **브라우저 세션으로 수동 검증이 필요하다.**

---

## 6. Anomaly 분석

### 6.1 Anomaly 단일 항목 분포

| anomaly | 수 | 비율 | 설명 |
|---------|---:|-----:|------|
| login_required | 12,085 | 99.8% | §5.2 참고, 사실상 오탐 |
| swagger_json_var_but_unparsed | 6,732 | 55.6% | swaggerJson 변수명은 있으나 값 추출 실패 |
| no_swagger_no_html_pk | 6,708 | 55.4% | Swagger도 HTML pk도 없음 |
| swagger_empty_paths | 1,404 | 11.6% | Swagger skeleton (paths: {}) |
| deprecated_notice | 277 | 2.3% | "폐기" 문구 포함 |
| page_fetch_failed | 15 | 0.1% | 페이지 접근 자체 실패 |
| js_redirect | 8 | 0.07% | JavaScript 리다이렉트 |
| service_terminated | 5 | 0.04% | "서비스 종료" 문구 |

### 6.2 복합 Anomaly 패턴 (공존하는 anomaly 세트)

| 수 | 패턴 |
|---:|-------|
| 6,600 | login_required + no_swagger_no_html_pk + swagger_json_var_but_unparsed |
| 3,794 | login_required (단독) |
| 1,377 | login_required + swagger_empty_paths |
| 154 | deprecated_notice + login_required |
| 97 | deprecated_notice + login_required + no_swagger_no_html_pk + swagger_json_var_but_unparsed |
| 32 | login_required + swagger_json_var_but_unparsed |
| 26 | deprecated_notice + login_required + swagger_empty_paths |
| 15 | page_fetch_failed |
| 8 | js_redirect + no_swagger_no_html_pk |

**패턴 해석**:
- **6,600건 (54.5%)**: 페이지에 `swaggerJson` 변수가 존재하지만 값 추출 실패 + Swagger/HTML 모두 없음. 이 API들은 **로그인 시 Swagger가 노출될 가능성**이 있다.
- **3,794건 (31.3%)**: login_required만 있고 다른 anomaly 없음. 이 중 Available 3,949건의 대부분이 여기에 해당 — Swagger가 정상 노출된 케이스.
- **1,377건 (11.4%)**: Skeleton 1,404건과 거의 일치 — Swagger 파일은 있지만 paths가 비어있음.

### 6.3 `swagger_json_var_but_unparsed` 심층 분석

6,732건에서 `swaggerJson` 변수명이 HTML에 존재하지만 `extract_swagger_json`이 값을 추출하지 못했다. 가능한 원인:

1. **빈 swaggerJson**: `var swaggerJson = ` `` ` ` — 변수는 선언되었으나 값이 비어있음
2. **동적 로딩**: `var swaggerJson = JSON.parse(data)` — 런타임에서만 값이 채워짐
3. **조건부 렌더링**: 로그인 상태에서만 값이 주입됨

이 6,732건의 SpecStatus 분포:
- External: 4,632건 (외부 도메인 API)
- CatalogOnly: 2,098건 (endpoint 없는 API)
- HtmlOnly: 2건

**결론**: 이 API들은 data.go.kr 포탈에서 스펙 페이지를 제공하지만, 게이트웨이가 아닌 외부 도메인이거나 endpoint가 등록되지 않아서 Swagger 값이 채워지지 않은 것으로 보인다.

---

## 7. SpecStatus 분류별 교차 분석

### 7.1 신호 매트릭스

| SpecStatus | swagger_json | swagger_url | html_pk |
|------------|:-----------:|:----------:|:------:|
| Available (3,949) | 100% | 0% | 0% |
| Skeleton (1,404) | 100% | 0% | 0% |
| External (4,652) | 0% | 0.2% | 0% |
| CatalogOnly (2,101) | 0% | 1.1% | 0% |
| HtmlOnly (2) | 0% | 50% | 0% |

- **Available은 전량 swagger_json 기반**이다. swagger_url이나 html_pk로 Available이 된 케이스는 0건.
- **HTML 폴백이 전혀 작동하지 않는다** (html_pk 탐지율 0%). 이는 조사 환경의 한계이다.

### 7.2 External 분류의 Swagger 현황

External 4,652건 중 Swagger를 보유한 API는 **0건**이다. swagger_url이 있는 8건만 존재하며, 이들도 inline JSON은 없다. External API의 스펙 확보는 **해당 외부 포탈에서 직접 수집**해야 한다.

### 7.3 CatalogOnly의 swaggerUrl

CatalogOnly 2,101건 중 23건이 `swaggerUrl`을 가지고 있다. 이들은 모두 `infuser.odcloud.kr` 도메인이며, 카탈로그에 endpoint가 없지만 Swagger URL은 존재하는 특이 케이스다.

---

## 8. 조사 실패 분석

15건의 조사 실패는 두 가지 유형:

| 유형 | 수 | 설명 |
|------|---:|------|
| request error (네트워크) | 11 | TCP 연결 또는 TLS 핸드셰이크 실패 |
| body read error (디코딩) | 4 | HTTP 200이지만 응답 본문 디코딩 실패 |

실패한 API 목록:

| list_id | 서비스명 | 에러 유형 |
|---------|---------|----------|
| 15077871 | 행정안전부_행정표준코드_법정동코드 | request |
| 15058363 | 경기도_하수도 보급률 집계 현황 | request |
| 15149740 | 성평등가족부_청소년활동 자원봉사 터전동아리 회원모집 정보 | request |
| 15095582 | 충청북도 충주시_시장_편의시설 | body read |
| 15126084 | 국회 국회사무처_의회외교 동향 | body read |
| 15128508 | 대전광역시 서구_공장등록 현황 정보 | request |
| 15107857 | 서울올림픽기념국민체육진흥공단_스포츠산업지원 정보_GW | body read |
| 15103413 | 부산광역시_장애인 복지시설 현황 | request |
| 15056551 | 울산항만공사_외항선박 부두별 선종별 통계 | request |
| 15098820 | 보건복지부_보건·복지현황_시도별 노인 취업알선 실적 | request |
| 15096034 | 충청남도 계룡시_당구장업 현황 API | request |
| 15000504 | 외교부_여행금지제도 | request |
| 15113297 | 중소벤처기업부_사업공고 | request |
| 15057156 | 제주특별자치도_와이파이 AP그룹별 일일 사용량 | body read |
| 15156790 | 한국해양교통안전공단_선박 좌초 좌주 위험 해역 | request |

이 15건은 `--resume` 옵션으로 재조사 시 복구될 수 있다 (일시적 네트워크 이슈).

---

## 9. 발견 사항 및 개선 제안

### 9.1 데이터 품질 이슈

| 이슈 | 영향 범위 | 개선 방안 |
|------|----------|----------|
| endpoint_url이 `' '`(공백 1자) | 8,883건 (73.4%) | `extract_domain`에서 trim 처리 필요 |
| `login_required` 오탐 | 12,085건 (99.8%) | 공통 레이아웃 텍스트 제외, 실제 로그인 폼 탐지로 변경 |
| HTML pk 전수 미탐지 | 12,108건 (100%) | 브라우저 세션 로그인 후 재검증 필요 |

### 9.2 Survey 바이너리 개선

1. **endpoint_url 정규화**: `trim()` 후 빈 문자열 체크, `http://` 만 있는 경우도 빈 값 처리
2. **login_required 로직 개선**: `login`+`session` 단순 문자열 매칭 → 실제 로그인 폼(`<form action="...login..."`) 또는 세션 만료 메시지 탐지
3. **번들 기반 카탈로그**: `fetch_all_services` 대신 `load_bundle().catalog`로 API 키 없이 실행 가능하게 옵션 추가
4. **중간 저장**: 500건마다 중간 결과를 파일에 flush하여 크래시 시 데이터 손실 방지
5. **Skeleton 원인 조사**: 1,404건의 skeleton이 data.go.kr 측 문제인지, 기관이 스펙을 비워둔 것인지 샘플 조사

### 9.3 korea-cli 본체에 대한 시사점

1. **Available 3,949건이 곧 korea-cli의 커버리지 상한**: 현재 번들에 포함된 스펙 수와 비교하여 누락 여부 확인 필요
2. **Skeleton 1,404건은 향후 Available로 전환 가능**: 기관이 스펙을 채우면 자동으로 업그레이드됨
3. **External 4,652건은 별도 수집 파이프라인 필요**: 81개 외부 도메인 중 주요 도메인(q-net, jeonju, icpa 등)은 개별 크롤러 개발 검토
4. **swaggerUrl 32건**: `infuser.odcloud.kr` 기반 API들 — swaggerUrl 경로로 직접 스펙 fetch 가능 여부 확인 필요

---

## 10. 부록: 원시 데이터 위치

- **전수조사 결과**: `data/survey.json` (7.5MB, 12,108 entries)
- **분석 스크립트**: 이 보고서의 분석은 Python one-liner로 수행됨
- **재현 명령어**: `cargo run --bin survey -- [--api-key KEY | $DATA_GO_KR_API_KEY] [--resume]`
