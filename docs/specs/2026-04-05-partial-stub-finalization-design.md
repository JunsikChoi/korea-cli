# PartialStub 마무리 + 번들 인프라 정비 설계

**작성일**: 2026-04-05
**선행 작업**: 2026-04-04 PartialStub + CI 수집 파이프라인 (`docs/specs/2026-04-04-partial-stub-and-ci-pipeline-design.md`)

## 배경

2026-04-04 PartialStub + CI 파이프라인이 완성되고 첫 CI 실행(4/5)이 성공했다. 이제 남은 3가지 마무리 작업:

1. `bundle-gateway.zstd` orphan 정리 + `data/bundle.zstd`를 v3 스키마로 교체
2. `gen_catalog_docs`에서 PartialStub을 "기타" → "호출 가능" 섹션으로 분류 전환
3. Gateway AJAX 추출 파이프라인의 실제 호출 가능성 E2E 검증

이 과정에서 **번들 배포 전략**과 **PartialStub 실제 발생률**에 대한 아키텍처 결정이 필요함이 드러나 함께 설계한다.

## 사전 조사 결과

### PartialStub 실제 발생: 0건

첫 CI 실행(2026-04-04, `bundle-2026-04-04-4`) 결과:
- `failed_ops.json`: 비어있음
- PartialStub API: **0건**
- Gateway AJAX 추출: 3,125건 전부 완전 성공 또는 Bail (all-or-nothing)

**원인 분석**: `fetch_gateway_spec`(build_bundle.rs:479-585)의 감지 로직은 버그 없음. operation 순회 시 각 AJAX POST가 독립적으로 실패/성공 기록. 0건인 이유는 CI 환경 안정성 + rate limiting 적절 + timeout 30초 넉넉 → operation 단위 간헐적 실패가 안 발생.

**결론**: PartialStub은 edge case 방어망(미래 timeout/일시적 서버 오류 대비). feature 유지하되, Task 2/3 원래 계획을 현실에 맞춰 조정.

### 번들 배포 전략 (Option A' 확정)

전문가 토론(deployment-engineer + backend-architect) + Codex 교차검증 결과: **Option A (임베드 번들 유지)** + 배포/DX 개선.

**핵심 근거** (Codex가 MCP Transport 스펙 직접 확인):
- Claude Desktop은 MCP 서버의 stderr를 log 파일로 리다이렉트 (사용자 화면에 안 보임)
- 첫 실행 시 런타임 번들 fetch하면 → `initialize` timeout → 서버 실패 → 사용자는 "도구 없음"만 봄
- **No-embed 전략은 MCP stdio 컨텍스트에서 구조적 실패**

## 목표

1. Task 1: 번들 인프라 정리 (orphan 제거, v3 교체, DX 헬퍼)
2. Task 2: PartialStub을 카탈로그 문서에서 호출 가능으로 분류 (schema v4 확장 포함)
3. Task 3: Gateway AJAX 추출 API의 실제 호출 가능성 E2E 스모크 테스트
4. 번들 배포 파이프라인 문서화 (Option A')

## Task 1: 번들 인프라 정리

### 1.1 orphan 파일 삭제

```bash
rm data/bundle-gateway.zstd
```

- `bundle-gateway.zstd`는 2026-04-02 Gateway AJAX 통합 테스트용 일회성 산출물
- 소스·빌드·CI 어디에서도 참조 안 됨 (`grep -rn "bundle-gateway"` 확인)
- 삭제 영향 없음

### 1.2 `data/bundle.zstd`를 v3로 교체

**완료 상태**: `gh release download bundle-2026-04-04-4 --pattern bundle.zstd --dir data/` 로 v3 번들 배치 완료. `schema_version: 3` 확인.

### 1.3 DX 헬퍼: `Makefile` 추가

```makefile
# Makefile
update-bundle:
	BUNDLE_TAG=$$(gh release list --repo JunsikChoi/korea-cli --limit 20 --json tagName \
	  --jq '[.[].tagName | select(startswith("bundle-"))][0]') && \
	gh release download "$$BUNDLE_TAG" --repo JunsikChoi/korea-cli \
	  --pattern bundle.zstd --dir data --clobber

.PHONY: update-bundle
```

**사용**: `make update-bundle` → 최신 `bundle-*` 태그 Release 번들을 `data/bundle.zstd`로 덮어쓰기. 1분 작업.

**주의**:
- 태그를 `bundle-*`로 명시 필터하지 않으면 `gh release download` 기본 동작이 `--latest` 태그를 잡아 바이너리 릴리즈(`v0.x.x`)로 빠짐 → `bundle.zstd` asset 없음 → 실패 (W-Dep1).
- repo hard-coding(`JunsikChoi/korea-cli`)은 fork 환경 미지원. 공식 상류 번들만 수용하는 design 결정.
- 개발자 DX 관점에서는 **`korea-cli update`가 schema_version 검증 포함이므로 우선 권장**. Makefile은 CI 재현성/스크립팅용.

### 1.4 번들 배포 전략 확정 (Option A')

3-tier 번들 공급 파이프라인 (기존 변경사항 + Makefile):

| 시나리오 | 해결 방법 | 담당 파일 |
|---------|---------|----------|
| 로컬 개발자 (최초) | `build.rs` placeholder 자동 생성 | `build.rs` |
| 로컬 개발자 (최신화) | `make update-bundle` | `Makefile` (신규) |
| 바이너리 CI 릴리즈 | `BUNDLE_DOWNLOAD_URL` env + curl | `.github/workflows/release.yml` (기존) |
| crates.io publish | `Cargo.toml include` + publish 래퍼 | `Cargo.toml`, `scripts/publish.sh` (기존) |

**기존 변경사항 유지** (이전 세션에서 작성됨):
- `build.rs`: 3단계 fallback (로컬 → env → placeholder)
- `.github/workflows/release.yml`: 크로스 빌드 + Release 번들 주입
- `Cargo.toml`: `include` 필드로 `.gitignore` override
- `scripts/publish.sh`: publish 전 번들 다운로드 래퍼

## Task 2: gen_catalog_docs PartialStub 분류 개선

### 2.1 현재 문제

`src/bin/gen_catalog_docs.rs:97`:
```rust
let other: Vec<_> = entries.iter().copied()
    .filter(|e| !matches!(e.spec_status, SpecStatus::Available | SpecStatus::External))
    .collect();
```

PartialStub은 `is_callable() == true`인데도 "기타" 섹션에 묻힘 → 사용자 발견성 저하.

### 2.2 설계: Option B (배지 방식)

Available 섹션에 PartialStub 포함, 테이블에 **상태 컬럼** 추가:

**변경 후 테이블:**
```markdown
## 호출 가능 (Available) — 7,160개

| API | ID | 설명 | 오퍼레이션 | 상태 | 누락 |
|-----|-----|------|----------|------|------|
| 기상청 단기예보 | [15...](url) | ... | 5/7 | ⚠️ 부분 | getFcstVersion, getMidFcst |
| 한국천문연구원 특일 | [15...](url) | ... | 10 | ✓ | — |
```

**통계 업데이트**: `호출 가능 {N}개 (완전 {M}개 + 부분 {K}개)`

### 2.3 schema v4: `missing_operations` 필드 추가

**저장 위치 대안 비교**:

- **Option ①** `CatalogEntry.missing_operations`: catalog 레벨에 저장. 장점 — `ApiSpec` 스키마 불변. 단점 — `catalog`/`specs` 이중 저장, spec 없는 엔트리도 빈 벡터 보유.
- **Option ②** `ApiSpec.missing_operations` (**채택**): spec 로컬 필드. 장점 — spec과 함께 응집, `PartialStub` 상태의 spec에만 값 존재. 단점 — schema bump 필수.

**번들 확장 (Option ② 채택):**

`src/core/types.rs` — 실제 구조에 맞춘 필드 추가 (반드시 **맨 마지막**에 배치, postcard varint 순서 보존):
```rust
pub struct ApiSpec {
    pub list_id: String,
    pub base_url: String,
    pub protocol: ApiProtocol,
    pub auth: AuthMethod,             // 기존
    pub extractor: ResponseExtractor, // 기존
    pub operations: Vec<Operation>,   // 기존
    pub fetched_at: String,           // 기존
    pub missing_operations: Vec<String>,  // 신규 (schema v4) — 반드시 마지막
}

pub const CURRENT_SCHEMA_VERSION: u32 = 4;  // 3 → 4
```

**필드 배치 원칙**: postcard는 필드 선언 순서를 직렬화 순서로 사용한다. 새 필드를 중간에 삽입하면 구 v3 번들의 기존 필드가 신 필드로 오역되어 garbage 데이터가 생기거나 panic이 난다. **새 필드는 항상 맨 마지막에 append**한다.

**필드 의미**: PartialStub API에서 수집 실패한 operation의 사람 읽을 이름 (`FailedOp.op_name`). Available API에서는 항상 빈 벡터.

**수정 대상 (3개 빌더 경로 모두 기본값 주입 필요):**

1. `src/core/html_parser.rs::build_api_spec` (Gateway AJAX 경로): `missing_operations: vec![]` 기본값 추가
2. `src/core/swagger.rs::parse_swagger` (Swagger 경로): `missing_operations: vec![]` 기본값 추가
3. `src/bin/build_bundle.rs::fetch_gateway_spec`: PartialStub 시 `failed_ops → missing_operations` overwrite

**build_bundle.rs 변경점** (`fetch_gateway_spec`):
```rust
// is_partial일 때 failed_ops를 missing_operations로 변환
let missing_operations: Vec<String> = failed_ops.iter()
    .filter(|f| !f.op_name.trim().is_empty())  // 빈 문자열 방어
    .map(|f| f.op_name.clone())
    .collect();

SpecResult::Spec {
    spec: Box::new(ApiSpec { missing_operations, ..spec }),
    is_gateway: true,
    is_partial,
    failed_ops,
}
```

### 2.4 schema v3 → v4 마이그레이션

postcard의 varint 구조상 **필드 추가만으로 자동 호환 불가** → 명시적 fallback + 배포 순서 강제가 필요.

**실제 fallback 동작 흐름** (`src/core/bundle.rs:20-42` 기준):

| 케이스 | 외부 override (`~/.config/…/bundle.zstd`) | 임베드 번들 | 결과 |
|--------|------------------------------------------|-------------|------|
| A | v3 | v4 | 외부 역직렬화 실패 → `Err(_)` → 임베드 fallback ✓ |
| B | v3 | v3 | 임베드 역직렬화 실패 → `.expect()` **panic** ✗ |
| C | 없음 | v3 (구 바이너리) + v4 번들 로컬 | N/A — `make update-bundle`이 override로 저장하면 케이스 A와 동일 |
| D | v4 | v3 (구 바이너리) | schema_version mismatch 조기 감지 → 임베드 fallback ✓ |

**케이스별 실제 동작 근거**:
- 케이스 A: 외부 bytes의 postcard 역직렬화가 새 필드 부족으로 에러 → `bundle.rs:33`의 `Err(_)` 분기 → 임베드 fallback ✓
- 케이스 B: 외부는 스키마 일치(v3==v3)이므로 성공 로드되지만, 바이너리 struct가 v4라 역직렬화 단계에서 에러 → 임베드 번들도 동일한 불일치 → `Lazy::new`의 `.expect("Failed to load bundle")` **panic**
- 케이스 D: v3 구 바이너리는 v4 bytes의 `missing_operations` 추가 필드를 **trailing bytes 에러**로 반환 → `Err(_)` → 임베드 fallback ✓ (schema_version 필드를 읽기 전에 `specs` HashMap entry 역직렬화 중 실패하므로 "schema_version 조기 감지"가 아니라 "역직렬화 에러" 경로)

**역방향 호환 주의** (v3 바이너리 + v4 번들 시나리오):
- `korea-cli update` 명령은 `remote.schema_version > CURRENT_SCHEMA_VERSION`일 때 저장 거부 (기존 `src/cli/update.rs` 로직)
- **`make update-bundle`은 raw 파일을 직접 덮어쓰므로 이 체크를 우회**한다 → 케이스 D 동작은 fallback으로 안전하지만 override 파일은 사실상 죽은 파일이 됨 → 사용자 혼란
- 대응: `korea-cli update` 우선 안내

**배포 순서 강제 메커니즘 (필수, 케이스 B 방지)**:

현재 `release.yml`은 바이너리 빌드 전 번들 schema_version을 검증하지 않는다. 다음 중 하나를 도입:

1. **release.yml gate (권장, Codex 권고)**: 검증 전용 바이너리 `src/bin/verify_bundle.rs`를 만들어 release workflow에서 `cargo run --bin verify-bundle -- data/bundle.zstd` 실행. 불일치 시 workflow 실패. 이 바이너리는 crate 타입(`Bundle`, `CURRENT_SCHEMA_VERSION`)을 그대로 import 가능하므로 구조적으로 단순.
2. **build.rs mirror-type 검증 (대안)**: build.rs는 crate 타입을 import할 수 없으므로 `BundleForValidation` mirror 타입과 `EXPECTED_SCHEMA_VERSION` 상수를 build.rs에 수동 동기화. postcard는 partial deserialize 미지원이라 Bundle 전체를 덮는 mirror 구조체가 필요 → 12K entries 전체 역직렬화 오버헤드 수 십 초 + mirror drift 유지보수 비용 → **비권장**.
3. **bundle.rs 임베드 graceful error (병행 권장)**: `decompress_and_deserialize(EMBEDDED_BUNDLE)` 실패 시 `.expect()` panic 대신 `"번들이 이 바이너리 버전과 호환되지 않습니다. 최신 릴리즈로 업데이트하세요."` 메시지 반환. Option 1/2를 못 잡은 케이스의 사용자 경험 개선.

**Makefile 보강** (W-Dep1 대응):

```makefile
update-bundle:
	BUNDLE_TAG=$$(gh release list --repo JunsikChoi/korea-cli --limit 20 --json tagName \
	  --jq '[.[].tagName | select(startswith("bundle-"))][0]') && \
	gh release download "$$BUNDLE_TAG" --repo JunsikChoi/korea-cli \
	  --pattern bundle.zstd --dir data --clobber
```

- 태그 미지정 시 `--latest` 폴백이 바이너리 릴리즈 태그를 잡아 `bundle.zstd` asset 누락으로 실패할 수 있음 → `bundle-*` 필터로 최신 번들 태그 명시
- `bundle-ci.yml`의 `--latest` 플래그 제거도 별도 조치 (W-Dep2): 번들 릴리즈가 바이너리 릴리즈의 latest를 덮어쓰지 않도록

**조치 순서**:
1. `types.rs`에 `missing_operations` 추가 + `CURRENT_SCHEMA_VERSION = 4`
2. build.rs에 schema 검증 추가 (케이스 B 방지)
3. `bundle-ci.yml` workflow_dispatch 트리거 → v4 번들 Release 생성
4. 바이너리 릴리즈 실행 (이제 v4 번들이 embed됨)
5. 사용자 `make update-bundle` 또는 `korea-cli update`로 로컬 동기화

### 2.5 테스트

- `gen_catalog_docs` 단위 테스트 업데이트: PartialStub 엔트리에서 "⚠️ 부분" 배지 + 누락 목록 렌더링 검증
- `types.rs` postcard roundtrip 테스트에 `missing_operations` 포함
- 구 v3 번들 로드 시 graceful fallback 동작 검증

## Task 3: E2E 스모크 테스트 (Gateway AJAX Available)

### 3.1 원래 계획 vs 현실

**원래**: "실제 PartialStub API에서 available operation 호출 확인"
**현실**: PartialStub 0건 → 대상 없음
**전환**: **Gateway AJAX로 추출된 Available API 호출 가능성 검증**으로 확장

이는 Gateway AJAX 추출 파이프라인(2026-04-02~03 구현) 자체의 신뢰성 검증이며, PartialStub 기능도 포괄 (존재 시).

**선결 과제 (Task 3 실행 전 필수)**:

현재 `src/core/caller.rs:59`는 응답을 `response.json().await?`로 무조건 파싱한다. 그러나 Gateway AJAX로 추출된 spec은 `html_parser.rs:168`에서 `ResponseFormat::Xml`로 고정 생성된다. data.go.kr Gateway API는 기본적으로 XML을 반환하므로, **현재 caller로는 Gateway Available API를 호출할 수 없다** (serde_json 파싱 실패).

Task 3 실행 옵션:
- **Option R1 (권장)**: Task 3 시작 전 `caller.rs`에 `ResponseFormat` 분기 추가 — XML 응답은 `response.text()` + `quick-xml` 또는 `serde-xml-rs`로 파싱. 이 caller 수정이 Task 3의 **prerequisite**이 됨.
- **Option R2**: Task 3 범위를 "raw HTTP 200 + non-empty body" 검증으로만 제한. caller.rs를 거치지 않고 직접 reqwest로 호출. E2E 의미는 약해지지만 Gateway AJAX의 URL/파라미터 구조가 유효함은 확인 가능.
- **Option R3**: `_type=json` query parameter 추가로 JSON 응답 강제 (data.go.kr 일부 API 지원). spec 빌더가 이를 탐지해 `ResponseFormat::Json`으로 설정하는 추가 로직 필요.

**채택**: **Option R1**. caller.rs의 XML 처리는 어차피 필요한 기능이며, E2E의 "호출 가능성" 주장 의미가 살아남는다. Option R2는 무의미한 smoke에 가까워 기각.

**Task 3의 prerequisite 체크리스트**:
- [ ] `caller.rs`가 `spec.extractor.format` 분기 처리 (XML/JSON)
- [ ] 5개 테스트 대상 API의 `spec.protocol`이 `DataGoKrRest`임을 사전 확인 (Gateway 경로 검증)

### 3.2 대상 선정 (5개)

번들에서 `SpecStatus::Available` + `spec.base_url contains "apis.data.go.kr"` + **`spec.protocol == DataGoKrRest`** 기준으로 추출한 Gateway AJAX API 중 인기 + 다양성 기준 5개:

| list_id | API명 | ops | 선정 이유 |
|---------|-------|-----|-----------|
| 15059468 | 기상청_중기예보 조회서비스 | 8 | 인기, 표준 도메인 |
| 15012690 | 한국천문연구원_특일 정보 | 10 | 초인기 (req 26,966) |
| 15073855 | 한국환경공단_에어코리아_대기오염통계 | 8 | 실용 API, 일 호출 多 |
| 15000415 | 기상청_기상특보 조회서비스 | 20 | operation 수 多 |
| 15134735 | 국토교통부_건축HUB_건축물대장정보 | 10 | 다른 도메인, 복잡 param |

**테스트 실행 전 사전 검증 (W-Back4 대응)**: 테스트 코드는 5개 각각에 대해 `spec.protocol == DataGoKrRest` assert. Swagger 경로로 추출된 API가 섞여 있으면 Gateway AJAX 파이프라인 검증 목적이 희석되므로 assert 실패로 조기 중단.

**사용자 이용신청 필수**: 사용자가 각 list_id의 `https://www.data.go.kr/data/{id}/openapi.do`에서 "활용신청" 승인 완료해야 실행 가능.

### 3.3 구현

`tests/integration/e2e_gateway_smoke.rs` (신규):

```rust
#[tokio::test]
#[ignore] // cargo test -- --ignored로만 실행
async fn e2e_gateway_smoke_available_operations() {
    // 1. 환경변수에서 DATA_GO_KR_API_KEY 로드 (없으면 skip with message)
    // 2. 번들 load → 5개 list_id spec 확보
    //    - assert: spec.protocol == DataGoKrRest (Gateway 경로 검증)
    // 3. 각 spec에서 "호출 용이한" operation 선정:
    //    - required=true 파라미터가 없거나
    //    - 있더라도 테스트용 기본값(예: regionCode 고정) 주입 가능한 op
    // 4. 각 API call → caller::call_api() (XML 처리 포함 prereq 필수)
    // 5. 검증: HTTP 200 + response 바디의 에러 코드 파싱
    //    - data.go.kr은 인증/파라미터 에러를 HTTP 200 + <resultCode>/<errMsg>로 반환
    // 6. 실패 시 list_id/operation/요청 URL/파라미터/응답 본문 전체 로깅
    // 7. 5/5 성공 시 pass
}
```

**허용 실패 사유 (test skip, fail 아님)** — data.go.kr 바디 에러 코드 기준:
- `DATA_GO_KR_API_KEY` 미설정
- `SERVICE_ACCESS_DENIED_ERROR` (이용신청 미승인)
- `SERVICE_KEY_IS_NOT_REGISTERED_ERROR` (키 미등록)
- `TEMPORARILY_DISABLE_THE_SERVICEKEY_ERROR` (키 일시 정지)

**실제 실패 사유 (test fail)**:
- HTTP 4xx/5xx (인증 관련 제외)
- HTTP 200 + 다음 에러 코드: `LIMITED_NUMBER_OF_SERVICE_REQUESTS_EXCEEDS_ERROR` (쿼터 초과 — 환경 문제이므로 명시 로그 후 fail), `APPLICATION_ERROR`, `MISSING_REQUIRED_PARAMETER` (파라미터 주입 로직 버그 신호), `INVALID_REQUEST_PARAMETER_ERROR`
- 응답 body 비어있음
- XML/JSON 파싱 실패

**에러 코드 추출**: 응답이 XML이면 `<resultCode>` 태그, JSON이면 `header.resultCode` 또는 `response.header.resultCode` 경로에서 추출. 표준 경로가 고정되어 있지 않으므로 테스트 코드에 explicit 파싱 로직 명시.

**실패 시 디버그 로깅 (필수)**:
```rust
eprintln!("=== FAIL: {list_id}/{operation} ===");
eprintln!("URL: {url}");
eprintln!("Params: {params:?}");
eprintln!("HTTP: {status}");
eprintln!("Body (first 500 chars): {body_preview}");
```
환경 의존 테스트(API key, 쿼터, 승인 상태)의 재현성 확보.

### 3.4 실행 방식 (Option A: 수동 스모크)

- `cargo test --test e2e_gateway_smoke -- --ignored --nocapture`
- 로컬 환경 + 개발자 `$DATA_GO_KR_API_KEY` 사용
- CI 통합 X (API key rotation, 승인 상태 관리 부담)
- 번들 업데이트 후 개발자가 수동 검증

### 3.5 결과 기록

E2E 결과는 devlog에 기록:
- 실행일, 성공/실패 API, 에러 메시지
- Gateway AJAX 추출 품질 모니터링 데이터

## 구현 순서

1. **Task 1 즉시 완료** (완료 대부분)
   - orphan 삭제 ✓ (완료 예정)
   - v3 번들 교체 ✓ (완료)
   - Makefile 추가 (bundle-* 태그 필터 포함)
   - `bundle-ci.yml:106`에서 `--latest` 플래그 제거 (W-Dep2)
   - 기존 변경사항 (build.rs/release.yml/Cargo.toml/scripts) 커밋 대상 식별

2. **Task 2 schema v4 마이그레이션**
   - `types.rs`: `missing_operations` 추가 (**필드 맨 마지막**) + `CURRENT_SCHEMA_VERSION: 4`
   - `html_parser.rs::build_api_spec`, `swagger.rs::parse_swagger`: `missing_operations: vec![]` 기본값
   - `build_bundle.rs::fetch_gateway_spec`: PartialStub 시 `failed_ops → missing_operations` overwrite (빈 문자열 필터 포함)
   - `src/bin/verify_bundle.rs` (신규) + `release.yml` 검증 스텝 (B3 대응)
   - `bundle.rs`: 임베드 번들 로드 실패 시 친화적 에러 메시지 (병행 권장)
   - 단위 테스트 업데이트 + postcard roundtrip (`missing_operations` 필드 포함)
   - 구 v3 번들 graceful fallback 테스트 (외부 override 케이스 A)
   - CI 재빌드 트리거 (workflow_dispatch) → Release bundle-v4 생성 → **이후** 바이너리 릴리즈

3. **Task 2 문서 분류 개선**
   - `gen_catalog_docs.rs`: PartialStub을 Available 섹션으로 이동
   - 테이블에 "상태" + "누락" 컬럼 추가
   - README 통계 업데이트
   - 단위 테스트 업데이트

4. **Task 3 E2E 스모크 테스트**
   - **Prerequisite**: `caller.rs`에 `ResponseFormat::Xml` 분기 추가 (XML 파싱)
   - 5개 테스트 대상 API의 `spec.protocol == DataGoKrRest` 사전 확인
   - 사용자 이용신청 대기 (5개 API)
   - `tests/integration/e2e_gateway_smoke.rs` 작성 (바디 에러 코드 파싱 포함)
   - 개발자 로컬에서 수동 실행 → devlog 기록

## 검증 계획 (TDD)

### Task 2 테스트

```rust
// types.rs
#[test]
fn test_missing_operations_serialization_roundtrip() {
    let spec = ApiSpec { missing_operations: vec!["getFcstVersion".into()], ..default() };
    let bytes = postcard::to_allocvec(&spec).unwrap();
    let decoded: ApiSpec = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(decoded.missing_operations, vec!["getFcstVersion"]);
}

#[test]
fn test_missing_operations_empty_default_roundtrip() {
    // Available API의 기본값 (빈 벡터) 직렬화/역직렬화 검증
    let spec = ApiSpec { missing_operations: vec![], ..default() };
    let bytes = postcard::to_allocvec(&spec).unwrap();
    let decoded: ApiSpec = postcard::from_bytes(&bytes).unwrap();
    assert!(decoded.missing_operations.is_empty());
}

#[test]
fn test_schema_v4_constant() {
    assert_eq!(CURRENT_SCHEMA_VERSION, 4);
}

// bundle.rs — B3 대응: schema mismatch graceful fallback
#[test]
fn test_v3_bundle_bytes_fail_v4_deserialization() {
    // v3 ApiSpec 구조로 직렬화된 bytes를 v4 구조로 역직렬화 시 실패해야 함
    // postcard 필드 부족 → trailing bytes 에러 또는 EOF
    // load_bundle() fallback 경로 트리거 확인
}

// gen_catalog_docs.rs
#[test]
fn test_partial_stub_rendered_in_available_section() {
    // PartialStub 엔트리 → "호출 가능" 섹션에 ⚠️ 배지 + 누락 목록
}

#[test]
fn test_available_statistics_splits_complete_and_partial() {
    // "호출 가능 2개 (완전 1개 + 부분 1개)"
}
```

### verify-bundle 검증 바이너리 (B3, Codex 권고안)

`src/bin/verify_bundle.rs` (신규):
```rust
use korea_cli::core::bundle;
use korea_cli::core::types::CURRENT_SCHEMA_VERSION;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: verify-bundle <path>"))?;
    let bytes = std::fs::read(&path)?;
    let bundle = bundle::decompress_and_deserialize(&bytes)?;
    if bundle.metadata.schema_version != CURRENT_SCHEMA_VERSION {
        anyhow::bail!(
            "schema_version 불일치: 번들={}, 바이너리={}. 올바른 번들 태그 사용 필요",
            bundle.metadata.schema_version, CURRENT_SCHEMA_VERSION
        );
    }
    eprintln!("OK: schema_version = {}", bundle.metadata.schema_version);
    Ok(())
}
```

`.github/workflows/release.yml` 추가 스텝 (번들 다운로드 후, 바이너리 빌드 전):
```yaml
- name: 번들 schema_version 검증
  if: steps.bundle.outputs.url != ''
  shell: bash
  run: |
    curl -sSLf "${{ steps.bundle.outputs.url }}" -o /tmp/bundle.zstd
    cargo run --bin verify-bundle -- /tmp/bundle.zstd
```

불일치 시 `exit 1` → workflow 실패 → 잘못된 바이너리 배포 차단.

**장점**:
- build.rs의 mirror-type drift 없음 (crate 타입 그대로 사용)
- 빌드 타임 오버헤드 없음 (release.yml에서만 1회 실행)
- 로컬 개발자는 영향 없음 (release 시점만 검증)

### Task 3 테스트 결과 기록

```
✓ 15059468 (기상청 중기예보): getFcstVersion → HTTP 200, 1.2KB
✓ 15012690 (한국천문연구원 특일): getHolidayInfo → HTTP 200, 845B
⚠ 15073855 (에어코리아): SERVICE_ACCESS_DENIED → skip (이용신청 미승인)
```

## 미결정 사항 / 후속 작업

- **PartialStub 재평가 (3개월 후)**: CI 누적 실행 결과에서 PartialStub 발생률 측정. 여전히 0% 근접 시 feature 제거 검토 (schema v5).
- **Task 3 자동화**: 향후 별도 E2E CI 워크플로우 고려 (월 1회 cron, 고정 API 세트 사용).
- **Gateway API 메타 플래그**: `ApiSpec`에 `extraction_method: String` 추가 시 Gateway AJAX 출처 구분 정확. 현재는 미도입.

## 참고

- 선행 설계: `docs/specs/2026-04-04-partial-stub-and-ci-pipeline-design.md`
- 번들 인프라 참고: `docs/specs/2026-03-31-bundle-transition-design.md`
- Gateway AJAX 추출: `docs/specs/2026-04-02-gateway-spec-extraction-design.md`

## Eval 수정 이력

**2026-04-05 /eval Round 1** (architect-reviewer + backend-architect + deployment-engineer):
- B1: Section 2.3 ApiSpec snippet을 실제 7개 필드에 맞춰 교정 (`extractor`, `fetched_at` 추가, `AuthSpec` → `AuthMethod`). 새 필드는 반드시 맨 마지막 배치 원칙 명시
- B2: Section 3.1에 caller.rs XML 처리 prerequisite 추가 (Option R1 채택)
- B3: Section 2.4에 배포 순서 강제 메커니즘(build.rs schema 검증) 추가. bundle.rs fallback 동작 케이스 매트릭스 추가
- W-Arch6: Option ①/② 비교 추가
- W-Arch7: 빌더 3개 경로 모두 수정 필요 명시
- W-Back1: 바디 에러 코드 분류 확장 (SERVICE_KEY_IS_NOT_REGISTERED_ERROR 등)
- W-Back2: op_name 빈 문자열 필터 추가
- W-Back3: required 파라미터 처리 전략 명시
- W-Back4: Gateway 경로 검증 (`protocol == DataGoKrRest` assert)
- W-Dep1: Makefile에 `bundle-*` 태그 필터 추가
- W-Dep2: `bundle-ci.yml:106` `--latest` 플래그 제거 명시

**2026-04-05 /eval Round 2** (Codex 교차검증):
- Codex-B1: build.rs에서 crate 타입 import 불가 → build.rs schema 검증 제안 철회, `src/bin/verify_bundle.rs` + release.yml gate로 전환 (Option 1 채택)
- Codex-B2: 케이스 D 동작 근거 교정 (schema_version 조기 감지 → postcard trailing bytes 에러 경로)
- 케이스 A/B/D 각각의 실제 역직렬화 실패 경로 명시 추가
