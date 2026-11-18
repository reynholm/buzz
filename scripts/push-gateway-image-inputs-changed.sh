#!/usr/bin/env bash
# Print `build=true` when the push gateway image must be built for HEAD_REV,
# and `build=false` when its build inputs match the source of the current :main
# gateway image. docker.yml appends the line to $GITHUB_OUTPUT, so a failure
# here fails the step instead of leaving the decision empty.
#
# Usage: scripts/push-gateway-image-inputs-changed.sh HEAD_REV PUBLISHED_IMAGE
#
# The workflow invokes this for upstream main pushes. It moves :main after
# provenance attestation, so the revision identifies a completed publication.
# Comparing against that revision covers changes across failed or replaced runs.
# The gateway uses no other workspace crate. Build on an unreadable image,
# unknown source, or source that is outside the history of HEAD_REV.
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 HEAD_REV PUBLISHED_IMAGE" >&2
  exit 2
fi
head_rev=$1
published_image=$2

inputs=(
  crates/buzz-push-gateway
  Cargo.toml
  Cargo.lock
  .cargo
  rust-toolchain.toml
  Dockerfile.push-gateway
  .dockerignore
  .github/workflows/docker.yml
  scripts/push-gateway-image-inputs-changed.sh
)

# A multi-platform index reports one image per platform; a single image
# reports its config directly.
published_rev=$(docker buildx imagetools inspect "$published_image" \
  --format '{{json .Image}}' 2>/dev/null |
  jq -r '
    def revision: .config.Labels["org.opencontainers.image.revision"];
    if has("config") then
      revision // empty
    else
      [ .["linux/amd64"], .["linux/arm64"] | revision ] | unique
      | select(length == 1) | .[0] // empty
    end' 2>/dev/null || true)
echo "Published :main gateway source: ${published_rev:-unknown}" >&2

if [[ ! "$published_rev" =~ ^[0-9a-f]{40}$ ]] ||
  ! git cat-file -e "${published_rev}^{commit}" 2>/dev/null ||
  ! git merge-base --is-ancestor "$published_rev" "$head_rev" 2>/dev/null; then
  echo build=true
  exit 0
fi

if git diff --quiet "$published_rev" "$head_rev" -- "${inputs[@]}"; then
  echo build=false
else
  echo build=true
fi
