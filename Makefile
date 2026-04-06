# korea-cli 개발 DX 헬퍼
REPO ?= JunsikChoi/korea-cli

.PHONY: update-bundle verify-bundle-local

# 최신 bundle-* 릴리즈에서 data/bundle.zstd를 받아온다.
# 다운로드 직후 verify-bundle로 schema_version 일치 확인 → 불일치면 삭제.
# Round 1 B2: v3 번들로 덮어써서 임베드 번들 panic 유발 방지.
update-bundle:
	@mkdir -p data; \
	BUNDLE_TAG=$$(gh release list --repo $(REPO) --limit 20 --json tagName \
	  --jq '[.[].tagName | select(startswith("bundle-"))][0] // empty'); \
	if [ -z "$$BUNDLE_TAG" ]; then \
	  echo "ERROR: bundle-* 릴리즈를 찾을 수 없음"; exit 1; \
	fi; \
	echo "다운로드: $$BUNDLE_TAG"; \
	if ! gh release download "$$BUNDLE_TAG" --repo $(REPO) \
	  --pattern bundle.zstd --dir data --clobber; then \
	  rm -f data/bundle.zstd; \
	  echo "ERROR: 다운로드 실패 — 부분 파일 삭제"; \
	  exit 1; \
	fi; \
	if ! cargo run --quiet --bin verify-bundle -- data/bundle.zstd; then \
	  echo "ERROR: 번들 schema_version이 현재 바이너리와 불일치 → 삭제"; \
	  rm -f data/bundle.zstd; \
	  echo "바이너리를 최신 버전으로 업데이트하거나 'korea-cli update'를 사용하세요"; \
	  exit 1; \
	fi; \
	echo "OK: $$BUNDLE_TAG 동기화 완료"

# verify-bundle을 로컬에서 직접 실행 (CI 동등)
verify-bundle-local:
	@if [ ! -f data/bundle.zstd ]; then \
	  echo "ERROR: data/bundle.zstd가 없습니다. 'make update-bundle'을 먼저 실행하세요"; \
	  exit 1; \
	fi; \
	cargo run --quiet --bin verify-bundle -- data/bundle.zstd
