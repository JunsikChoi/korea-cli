#!/usr/bin/env bash
# crates.io 배포 스크립트.
#
# 실행 순서:
#   1. 최신 bundle-* GitHub Release에서 bundle.zstd 다운로드
#   2. cargo publish (include 필드에 의해 번들이 패키지에 포함됨)
#
# 사용법:
#   ./scripts/publish.sh                  # 최신 bundle 릴리즈 사용
#   BUNDLE_TAG=bundle-2026-04-01-5 ./scripts/publish.sh  # 특정 번들 지정
#   DRY_RUN=1 ./scripts/publish.sh        # 실제 publish 없이 검증만
#
# 의존성: gh (GitHub CLI), cargo

set -euo pipefail

REPO="${GITHUB_REPOSITORY:-$(gh repo view --json nameWithOwner -q .nameWithOwner)}"
BUNDLE_PATH="data/bundle.zstd"

echo "=== korea-cli crates.io publish ==="

# 1. 번들 다운로드
if [ -n "${BUNDLE_TAG:-}" ]; then
  TAG="$BUNDLE_TAG"
  echo "지정된 번들 태그: $TAG"
else
  TAG=$(gh release list --limit 20 --json tagName \
    --jq '[.[].tagName | select(startswith("bundle-"))][0] // empty' \
    --repo "$REPO")
  if [ -z "$TAG" ]; then
    echo "오류: bundle-* 릴리즈를 찾을 수 없습니다."
    exit 1
  fi
  echo "최신 번들 태그: $TAG"
fi

mkdir -p data
if ! gh release download "$TAG" --pattern bundle.zstd --dir data --clobber --repo "$REPO"; then
  rm -f "$BUNDLE_PATH"
  echo "오류: 번들 다운로드 실패"
  exit 1
fi
echo "번들 다운로드 완료: $(du -sh "$BUNDLE_PATH" | cut -f1)"

# 1.5. 번들 schema_version 검증
cargo run --quiet --bin verify-bundle -- "$BUNDLE_PATH" || {
  echo "오류: 번들 schema_version이 현재 바이너리와 불일치합니다."
  echo "번들 또는 코드를 업데이트한 후 다시 시도하세요."
  rm -f "$BUNDLE_PATH"
  exit 1
}
echo "번들 schema 검증 통과"

# 2. 번들 크기 검증 (crates.io 10MB 제한 기준으로 전체 패키지 크기 추정)
BUNDLE_SIZE=$(stat -c%s "$BUNDLE_PATH" 2>/dev/null || stat -f%z "$BUNDLE_PATH")
MAX_BUNDLE_SIZE=$((6 * 1024 * 1024))  # 6MB (나머지 소스 코드 여유분 포함)
if [ "$BUNDLE_SIZE" -gt "$MAX_BUNDLE_SIZE" ]; then
  echo "경고: 번들 크기가 ${BUNDLE_SIZE}바이트로 6MB를 초과합니다."
  echo "crates.io 전체 패키지 10MB 제한에 근접할 수 있습니다."
  if [ -t 0 ]; then
    echo "계속하려면 Enter, 중단하려면 Ctrl+C"
    read -r
  else
    echo "비대화형 환경 — 경고만 출력하고 계속 진행"
  fi
fi

# 3. cargo publish (--allow-dirty: data/bundle.zstd가 .gitignore 대상이라 필요)
DIRTY_SRC=$(git diff --name-only HEAD -- src/ build.rs 2>/dev/null || true)
if [ -n "$DIRTY_SRC" ]; then
  echo "경고: 커밋되지 않은 소스 변경이 있습니다:"
  echo "$DIRTY_SRC"
  if [ "${FORCE_DIRTY:-0}" != "1" ]; then
    echo "계속하려면 FORCE_DIRTY=1 설정"
    exit 1
  fi
fi

if [ "${DRY_RUN:-0}" = "1" ]; then
  echo "DRY_RUN=1 — cargo publish --dry-run 실행"
  cargo publish --dry-run --allow-dirty
else
  echo "cargo publish 실행..."
  cargo publish --allow-dirty
fi

echo "=== 배포 완료 ==="
