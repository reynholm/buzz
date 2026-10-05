#!/usr/bin/env bash
# Build a candidate at one immutable commit. Never infer its SHA after building.
set -euo pipefail
CANDIDATE_SHA=""
BASELINE_REPORT=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --candidate-sha) [[ $# -ge 2 ]] || exit 2; CANDIDATE_SHA="$2"; shift 2 ;;
        --baseline-report) [[ $# -ge 2 ]] || exit 2; BASELINE_REPORT="$2"; shift 2 ;;
        *) echo "Usage: $0 --candidate-sha SHA --baseline-report PATH" >&2; exit 2 ;;
    esac
done
[[ "$CANDIDATE_SHA" =~ ^[0-9a-f]{40}$ && -n "$BASELINE_REPORT" ]] || { echo "Exact candidate SHA and baseline report required" >&2; exit 2; }
REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"
BASELINE_REPORT="$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve(strict=True))' "$BASELINE_REPORT")"
assert_source() {
    [[ "$(git rev-parse HEAD)" == "$CANDIDATE_SHA" ]] || { echo "Candidate SHA differs from checkout" >&2; exit 1; }
    git diff --quiet && git diff --cached --quiet || { echo "Candidate source is dirty" >&2; exit 1; }
    [[ -z "$(git ls-files --others --exclude-standard)" ]] || { echo "Untracked source files in candidate checkout" >&2; exit 1; }
}
assert_source
python3 scripts/fork/verify_artifact.py --candidate-sha "$CANDIDATE_SHA" --baseline-report "$BASELINE_REPORT"
[[ "$(uname -s)" == Darwin && "$(uname -m)" == arm64 ]] || { echo "Native Apple Silicon host required" >&2; exit 1; }
# Clear inherited upstream updater inputs and named-demo/build capability overrides.
unset BUZZ_UPDATER_ENDPOINT BUZZ_UPDATER_PUBLIC_KEY BUZZ_BUILD_DEMO_SLUG
unset BUZZ_BUILD_AGENT_ENV BUZZ_BUILD_AGENT_ACCESS_OWNER_ONLY BUZZ_BUILD_AUTO_CONNECT_DEFAULT_RELAY
unset BUZZ_BUILD_BUZZ_AGENT_PROVIDER BUZZ_BUILD_BUZZ_AGENT_MODEL BUZZ_BUILD_RELAY_RECONNECT_CMD
unset BUZZ_RELAY_URL BUZZ_RELAY_HTTP
export CARGO_INCREMENTAL=0 CI=true
export BUZZ_FORK_SHA="$CANDIDATE_SHA"
export BUZZ_FORK_BASE_TAG="$(python3 -c 'import json; print(json.load(open("scripts/fork/patches.json"))["base_tag"])')"
export BUZZ_FORK_REVISION="${BUZZ_FORK_REVISION:-1}"
[[ "$BUZZ_FORK_REVISION" =~ ^[1-9][0-9]*$ ]] || { echo "Positive fork revision required" >&2; exit 1; }
TARGET=aarch64-apple-darwin
OUTPUT="artifacts/fork/$CANDIDATE_SHA"
[[ ! -e "$OUTPUT" ]] || { echo "Candidate output already exists; preserve evidence and select a fresh output checkout" >&2; exit 1; }
mkdir -p "$OUTPUT"
# Kept under the immutable candidate directory so failed builds retain diagnostics.
exec > >(tee "$OUTPUT/build.log") 2>&1
printf 'candidate_sha=%s\nbase_tag=%s\nfork_revision=%s\n' "$BUZZ_FORK_SHA" "$BUZZ_FORK_BASE_TAG" "$BUZZ_FORK_REVISION"
df -h "$REPO_ROOT"
cargo build --release --target "$TARGET" -p buzz-acp -p buzz-agent \
    -p buzz-backend-kubernetes -p buzz-dev-mcp -p git-credential-nostr -p buzz-cli
./scripts/bundle-sidecars.sh "$TARGET"
python3 scripts/fork/verify_artifact.py --candidate-sha "$CANDIDATE_SHA" \
    --baseline-report "$BASELINE_REPORT" --sidecar-dir desktop/src-tauri/binaries
# CI=true uses upstream's supported non-GUI DMG packaging and preserves version/id.
# A linker signature does not seal the app bundle. Tauri must sign before creating the DMG.
# Ad-hoc signing needs no certificate; it still requires explicit first-launch permission.
export APPLE_SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:--}"
just desktop-release-build "$TARGET"
APP="desktop/src-tauri/target/$TARGET/release/bundle/macos/Buzz.app"
VERSION="$(python3 -c 'import json; print(json.load(open("desktop/package.json"))["version"])')"
DMG="desktop/src-tauri/target/$TARGET/release/bundle/dmg/Buzz_${VERSION}_aarch64.dmg"
python3 scripts/fork/verify_artifact.py --candidate-sha "$CANDIDATE_SHA" \
    --baseline-report "$BASELINE_REPORT" --app "$APP" --dmg "$DMG" --output "$OUTPUT/manifest.json"
# Archive the verified app, including executable modes and native signatures.
tar -czf "$OUTPUT/Buzz_${VERSION}_aarch64.app.tar.gz" -C "$(dirname "$APP")" Buzz.app
cp "$DMG" "$OUTPUT/$(basename "$DMG")"
cp "$BASELINE_REPORT" "$OUTPUT/baseline.json"
assert_source
(
    cd "$OUTPUT"
    shasum -a 256 Buzz_*.app.tar.gz Buzz_*.dmg manifest.json baseline.json > SHA256SUMS
    shasum -a 256 -c SHA256SUMS
)
printf 'Verified candidate artifacts: %s/%s\n' "$REPO_ROOT" "$OUTPUT"
