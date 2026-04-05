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

Codex 권고안 (복잡도 최소):

```makefile
# Makefile
update-bundle:
	gh release download --repo JunsikChoi/korea-cli \
	  --pattern bundle.zstd --dir data --clobber

.PHONY: update-bundle
```

**사용**: `make update-bundle` → 최신 Release 번들을 `data/bundle.zstd`로 덮어쓰기. 1분 작업.

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

**번들 확장 (Option ② 채택):**

`src/core/types.rs`:
```rust
pub struct ApiSpec {
    pub list_id: String,
    pub base_url: String,
    pub protocol: ApiProtocol,
    pub auth: AuthSpec,
    pub operations: Vec<Operation>,
    pub missing_operations: Vec<String>,  // 신규 (schema v4)
}

pub const CURRENT_SCHEMA_VERSION: u32 = 4;  // 3 → 4
```

**필드 의미**: PartialStub API에서 수집 실패한 operation의 사람 읽을 이름 (`ParsedOperation.summary` 또는 `op.name` fallback). Available API에서는 항상 빈 벡터.

**build_bundle.rs 변경점** (`fetch_gateway_spec`):
```rust
// is_partial일 때 failed_ops를 missing_operations로 변환
let missing_operations: Vec<String> = failed_ops.iter()
    .map(|f| f.op_name.clone())  // op_name: 이미 FailedOp에 수집됨
    .collect();

// build_api_spec에 전달
SpecResult::Spec {
    spec: Box::new(ApiSpec { missing_operations, ..spec }),
    ...
}
```

### 2.4 schema v3 → v4 마이그레이션

postcard의 varint 구조상 **필드 추가만으로 자동 호환 불가** → graceful fallback 필요:
- 바이너리가 v4인데 Release 번들이 v3: `load_bundle()` schema mismatch → 임베드 번들 fallback (기존 로직)
- v3 번들은 `missing_operations` 부재 → 실패

**조치**: CI 재빌드로 v4 번들 생성 후 Release. Makefile로 개발자 로컬 동기화.

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

### 3.2 대상 선정 (5개)

번들에서 `SpecStatus::Available` + `spec.base_url contains "apis.data.go.kr"` 기준으로 추출한 5,162개 중 인기 + 다양성 기준 5개:

| list_id | API명 | ops | 선정 이유 |
|---------|-------|-----|-----------|
| 15059468 | 기상청_중기예보 조회서비스 | 8 | 인기, 표준 도메인 |
| 15012690 | 한국천문연구원_특일 정보 | 10 | 초인기 (req 26,966) |
| 15073855 | 한국환경공단_에어코리아_대기오염통계 | 8 | 실용 API, 일 호출 多 |
| 15000415 | 기상청_기상특보 조회서비스 | 20 | operation 수 多 |
| 15134735 | 국토교통부_건축HUB_건축물대장정보 | 10 | 다른 도메인, 복잡 param |

**사용자 이용신청 필수**: 사용자가 각 list_id의 `https://www.data.go.kr/data/{id}/openapi.do`에서 "활용신청" 승인 완료해야 실행 가능.

### 3.3 구현

`tests/integration/e2e_gateway_smoke.rs` (신규):

```rust
#[tokio::test]
#[ignore] // cargo test -- --ignored로만 실행
async fn e2e_gateway_smoke_available_operations() {
    // 1. 환경변수에서 DATA_GO_KR_API_KEY 로드 (없으면 skip with message)
    // 2. 번들 load → 5개 list_id에서 첫 operation 추출
    // 3. 각 API call → HTTP 200 + non-empty body 검증
    // 4. 실패 시 list_id/operation/에러 리포트
    // 5. 5/5 성공 시 pass, 실패 1개 이상이면 상세 리포트 후 fail
}
```

**허용 실패 사유** (test skip, fail 아님):
- `DATA_GO_KR_API_KEY` 미설정
- "SERVICE_ACCESS_DENIED_ERROR" (이용신청 미승인 상태)

**실제 실패 사유** (test fail):
- HTTP 4xx/5xx (인증 제외)
- 응답 body 비어있음
- 파싱 실패

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
   - Makefile 추가 (1줄)
   - 기존 변경사항 (build.rs/release.yml/Cargo.toml/scripts) 커밋 대상 식별

2. **Task 2 schema v4 마이그레이션**
   - `types.rs`: `missing_operations` 추가 + `CURRENT_SCHEMA_VERSION: 4`
   - `build_bundle.rs`: PartialStub 시 missing_operations 수집
   - 단위 테스트 업데이트 + postcard roundtrip
   - 구 v3 번들 graceful fallback 테스트
   - CI 재빌드 트리거 (workflow_dispatch) → Release bundle-v4 생성

3. **Task 2 문서 분류 개선**
   - `gen_catalog_docs.rs`: PartialStub을 Available 섹션으로 이동
   - 테이블에 "상태" + "누락" 컬럼 추가
   - README 통계 업데이트
   - 단위 테스트 업데이트

4. **Task 3 E2E 스모크 테스트**
   - 사용자 이용신청 대기 (5개 API)
   - `tests/integration/e2e_gateway_smoke.rs` 작성
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
fn test_schema_v4_constant() {
    assert_eq!(CURRENT_SCHEMA_VERSION, 4);
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
