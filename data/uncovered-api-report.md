# 스펙 미추출 API 상세 보고서

> **기준일**: 2026-04-02 HTML 전수조사 결과
> **전체 API**: 12,108건
> **스펙 추출 가능**: 6,475건 (53.5%) — Swagger 3,953 + HTML AJAX 2,522
> **스펙 미추출**: 5,633건 (46.5%) — 이 보고서의 대상

---

## 요약

| 분류 | 수 | 비율 | 개선 가능성 |
|------|---:|-----:|:---:|
| operation 미등록 | 4899 | 40.5% | ❌ |
| AJAX 부분 성공 (서비스URL 폴백 가능) | 644 | 5.3% | ✅ |
| AJAX 부분 성공 (파라미터 없음) | 18 | 0.1% | ⚠️ |
| 폐기/서비스 종료 | 282 | 2.3% | ❌ |
| 페이지 접근 불가 | 10 | 0.1% | ❌ |

---

## 1. operation 미등록 — 4899건 (40.5%)

포탈 페이지(openapi.do)는 존재하고 `publicDataDetailPk`도 있지만, **operation select 드롭다운에 항목이 0개**여서 AJAX로 상세를 가져올 수 없다. 기관이 포탈에 API operation을 등록하지 않은 상태.

### 세분화

| 하위 분류 | 수 | 설명 |
|-----------|---:|------|
| Swagger 빈 backtick (`var swaggerJson = \`\``) | 3399 | 포탈이 Swagger 템플릿만 생성, 값 미입력 |
| Swagger 아예 없음 | 3521 | swaggerJson 변수 자체 미존재 |
| Skeleton (Swagger 있지만 paths 비어있음) | 1378 | Swagger 파일은 있으나 operation 0개 |

### 직접 확인용 샘플 (10건)

| list_id | 서비스명 | 포탈 페이지 |
|---------|---------|------------|
| 15057554 | 국토교통부_경관지구 | [openapi.do](https://www.data.go.kr/data/15057554/openapi.do) |
| 15057860 | 경기도_아동 양육시설 현황 | [openapi.do](https://www.data.go.kr/data/15057860/openapi.do) |
| 15041682 | 국가철도공단_역사별 엘리베이터 현황 | [openapi.do](https://www.data.go.kr/data/15041682/openapi.do) |
| 15058758 | 국토교통부_아동복지시설 | [openapi.do](https://www.data.go.kr/data/15058758/openapi.do) |
| 15033352 | 문화재청 국립고궁박물관_문화행사 목록조회 정보 | [openapi.do](https://www.data.go.kr/data/15033352/openapi.do) |
| 15061076 | 국토교통부_공공용지 취득실적 (부동산소유사실 확인서) | [openapi.do](https://www.data.go.kr/data/15061076/openapi.do) |
| 15056507 | 경기도_취수장 현황 | [openapi.do](https://www.data.go.kr/data/15056507/openapi.do) |
| 15153186 | 행정안전부_수시구간통계 | [openapi.do](https://www.data.go.kr/data/15153186/openapi.do) |
| 15140547 | 농림축산식품부_도매시장 산지공판장 정산 가격 | [openapi.do](https://www.data.go.kr/data/15140547/openapi.do) |
| 15128487 | 대전광역시 서구_재해우려지역 현황정보 | [openapi.do](https://www.data.go.kr/data/15128487/openapi.do) |

> 위 페이지에서 "활용 API 목록" 섹션의 select 드롭다운을 확인하면 **항목이 없음**을 볼 수 있다.

### 브라우저에서 확인하는 방법

1. 위 링크를 클릭하여 openapi.do 페이지 접속
2. 페이지 중간의 "활용 API 목록" 드롭다운 확인
3. 드롭다운이 비어있음 → operation 미등록 상태
4. "조회" 버튼을 눌러도 아무 결과 없음

---

## 2. AJAX 부분 성공 — 662건 (5.5%)

AJAX 호출은 성공하여 파라미터 테이블과 서비스URL은 추출했지만, **요청주소(request URL)를 찾지 못한** 케이스. 대부분 외부 기관 API.

### 세분화

| 하위 분류 | 수 | 개선 방안 |
|-----------|---:|----------|
| 서비스URL + 파라미터 있음 | 644 | ✅ 서비스URL을 base_url로 사용 가능 |
| 서비스URL만 (파라미터 없음) | 4 | ⚠️ URL만으로는 호출 불가 |
| 아무것도 없음 | 14 | ❌ 추출 불가 |

### 서비스URL 도메인 분포 (Top 15)

| 수 | 도메인 |
|---:|--------|
| 65 | openapi.q-net.or.kr |
| 53 | openapi.jeonju.go.kr |
| 45 | opendata.icpa.or.kr |
| 32 | dataopen.kospo.co.kr |
| 32 | data.uiryeong.go.kr |
| 26 | www.djtc.kr |
| 26 | c.q-net.or.kr |
| 24 | openapi.airport.co.kr |
| 22 | data.khnp.co.kr |
| 22 | www.ygpa.or.kr:9191 |
| 20 | www.iwest.co.kr:8082 |
| 20 | data.sisul.or.kr |
| 16 | www.kdhc.co.kr:443 |
| 15 | openapi.epost.go.kr |
| 13 | api.forest.go.kr |

### 직접 확인용 샘플 (10건)

| list_id | 서비스명 | 서비스URL | 파라미터 수 | 포탈 페이지 |
|---------|---------|-----------|:---:|------------|
| 15036876 | 한국연구재단_ ICT통계간행물 정보 | `http://open.itfind.or.kr/openapi/service/ITStatsService/getS...` | 13 | [openapi.do](https://www.data.go.kr/data/15036876/openapi.do) |
| 15056844 | 경상남도 의령군_노인복지시설 | `http://data.uiryeong.go.kr/rest/uiryeongseniorwelfare/getUir...` | 17 | [openapi.do](https://www.data.go.kr/data/15056844/openapi.do) |
| 15001015 | 대구광역시_시설정보 조회 서비스 | `https://www.dgwater.go.kr/api/openData/CntrwkList` | 20 | [openapi.do](https://www.data.go.kr/data/15001015/openapi.do) |
| 15000935 | 국립농산물품질관리원 친환경인증정보 | `http://data.naqs.go.kr/openapi/service/rest/naqsenv/envparam` | 16 | [openapi.do](https://www.data.go.kr/data/15000935/openapi.do) |
| 15028086 | 전북특별자치도 전주시_택시현황 | `http://openapi.jeonju.go.kr/rest/corporatetaxi/getCorporatet...` | 21 | [openapi.do](https://www.data.go.kr/data/15028086/openapi.do) |
| 15059061 | 한국사회보장정보원_사회서비스 공통코드 조회 | `https://api.socialservice.or.kr:444/api/service/common/servi...` | 6 | [openapi.do](https://www.data.go.kr/data/15059061/openapi.do) |
| 15077347 | 대전교통공사_간행물정보 | `http://www.djtc.kr/OpenAPI/service/PublicationSVC/getPublica...` | 21 | [openapi.do](https://www.data.go.kr/data/15077347/openapi.do) |
| 15056583 | 한국수력원자력(주)_수력발전소 발전 현황 | `http://data.khnp.co.kr/environ/service/realtime/waterPwr` | 10 | [openapi.do](https://www.data.go.kr/data/15056583/openapi.do) |
| 15056736 | 경상남도 의령군_여성복지시설 | `http://data.uiryeong.go.kr/rest/uiryeongsingleparent/getUiry...` | 20 | [openapi.do](https://www.data.go.kr/data/15056736/openapi.do) |
| 15014125 | 경상남도 거제시_버스 정보 | `http://data.geoje.go.kr/rfcapi/rest/geojebis/getGeojebisBuss...` | 17 | [openapi.do](https://www.data.go.kr/data/15014125/openapi.do) |

> 위 페이지에서 "활용 API 목록" 드롭다운에서 operation을 선택하고 "조회"를 누르면 상세 스펙이 표시된다.
> "요청주소"가 비어있고 "서비스URL"만 보이는 것을 확인할 수 있다.

### 개선 방안

이 644건은 `html_parser.rs`에서 요청주소 추출 실패 시 **서비스URL을 base_url로 폴백**하면 추출 가능:

```
현재: 요청주소 없음 → 추출 실패
개선: 요청주소 없음 → 서비스URL 사용 → operation path를 서비스URL에 붙여 스펙 구성
```

이 수정으로 커버리지: 53.5% → **58.8%** (+644건)

---

## 3. 폐기/서비스 종료 — 282건 (2.3%)

포탈 페이지에 "폐기" 또는 "서비스 종료" 문구가 포함된 API. 더 이상 호출할 수 없는 비활성 API.

### 세분화

| 유형 | 수 |
|------|---:|
| "서비스 종료" 문구 | 5 |
| "폐기" 문구 | 277 |

### 직접 확인용 샘플 (10건)

| list_id | 서비스명 | 포탈 페이지 |
|---------|---------|------------|
| 15108173 | 대전광역시 서구_종량제봉투지정판매소현황 | [openapi.do](https://www.data.go.kr/data/15108173/openapi.do) |
| 15119668 | 한국동서발전(주)_기록물 생산 현황 정보 | [openapi.do](https://www.data.go.kr/data/15119668/openapi.do) |
| 15111877 | 한국수산자원공단_수산종자생산업 어류 | [openapi.do](https://www.data.go.kr/data/15111877/openapi.do) |
| 15112439 | 대구공공시설관리공단_상리음식물 처리현황 조회 서비스 | [openapi.do](https://www.data.go.kr/data/15112439/openapi.do) |
| 15059478 | 경기도_가축 분뇨 수집 운반업체 현황 | [openapi.do](https://www.data.go.kr/data/15059478/openapi.do) |
| 15098364 | 세종특별자치시_세종특별자치시 대형폐기물수거정보 조회 서비스 | [openapi.do](https://www.data.go.kr/data/15098364/openapi.do) |
| 15061217 | 한국지역정보개발원_지방공기업 시도코드 조회 서비스 | [openapi.do](https://www.data.go.kr/data/15061217/openapi.do) |
| 15105035 | 한국원자력환경공단_불용선원 재활용 현황 | [openapi.do](https://www.data.go.kr/data/15105035/openapi.do) |
| 15157571 | 행정안전부_기부관련단체정보서비스_GW | [openapi.do](https://www.data.go.kr/data/15157571/openapi.do) |
| 15057506 | 경기도_하수슬러지 처리 집계 현황 | [openapi.do](https://www.data.go.kr/data/15057506/openapi.do) |

> 위 페이지에 접속하면 "이 API는 폐기되었습니다" 또는 "서비스가 종료되었습니다" 안내를 확인할 수 있다.

---

## 4. 페이지 접근 불가 — 10건 (0.1%)

openapi.do 페이지 자체가 존재하지 않거나 네트워크 에러.

### 전체 목록

| list_id | 서비스명 | 원인 | 포탈 페이지 |
|---------|---------|------|------------|
| 15084799 | 한국중부발전(주)_발전정비관리 정보 | 네트워크 에러 | [openapi.do](https://www.data.go.kr/data/15084799/openapi.do) |
| 15000849 | 한국자산관리공사_온비드 이용기관 공매물건 조회서비스 | JS 리다이렉트 | [openapi.do](https://www.data.go.kr/data/15000849/openapi.do) |
| 15000837 | 한국자산관리공사_온비드 물건 정보 조회서비스 | JS 리다이렉트 | [openapi.do](https://www.data.go.kr/data/15000837/openapi.do) |
| 15000851 | 한국자산관리공사_온비드 캠코공매물건 조회서비스 | JS 리다이렉트 | [openapi.do](https://www.data.go.kr/data/15000851/openapi.do) |
| 15124953 | 국토교통부_교통약자사고다발지점 조회 서비스 | 네트워크 에러 | [openapi.do](https://www.data.go.kr/data/15124953/openapi.do) |
| 3055105 | 조건불리지역직접지불제 지원현황 | JS 리다이렉트 | [openapi.do](https://www.data.go.kr/data/3055105/openapi.do) |
| 15000907 | 한국자산관리공사_온비드 정부 재산 정보공개 조회서비스 | JS 리다이렉트 | [openapi.do](https://www.data.go.kr/data/15000907/openapi.do) |
| 15000920 | 한국자산관리공사_온비드 코드 조회서비스 | JS 리다이렉트 | [openapi.do](https://www.data.go.kr/data/15000920/openapi.do) |
| 3081229 | 국방부_전투 정보 | JS 리다이렉트 | [openapi.do](https://www.data.go.kr/data/3081229/openapi.do) |
| 15000485 | 인사혁신처_공공취업정보 조회 | JS 리다이렉트 | [openapi.do](https://www.data.go.kr/data/15000485/openapi.do) |

> JS 리다이렉트 페이지는 포탈에서 해당 API가 삭제/이전된 것으로 보임. 접속 시 에러 페이지로 이동.

---

## 5. 커버리지 개선 로드맵

```
현재:        ████████████████████████████░░░░░░░░░░░░░░░░░░░░  53.5% (6,475)
+서비스URL:  ██████████████████████████████░░░░░░░░░░░░░░░░░░  58.8% (+644)
이론적 상한: ██████████████████████████████░░░░░░░░░░░░░░░░░░  58.8% (7,119)
                                          ▲
                                     기관이 operation을
                                     등록해야 넘을 수 있는 벽
```

| 단계 | 액션 | 추가 건수 | 누적 커버리지 |
|------|------|-------:|:---:|
| 현재 | Swagger 추출 | 3,953 | 32.6% |
| 완료 | HTML AJAX 추출 | +2,522 | 53.5% |
| 다음 | 서비스URL 폴백 | +644 | 58.8% |
| 한계 | operation 미등록 해소 (기관 의존) | +4,899 | 99.6% |
| 한계 | 폐기 API 제외 시 | -282 | — |

**58.8%가 기술적 상한**이며, 나머지 41.2%는 기관이 포탈에 데이터를 입력해야 해결된다.
