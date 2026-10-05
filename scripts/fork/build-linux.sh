#!/usr/bin/env bash
# Build and verify the fork's Linux x86_64 .deb from one immutable commit.
#
#   build-linux.sh --tag fork-vX.Y.Z-N [--source DIR]   release asset for a published fork tag
#   build-linux.sh --validate                           tooling self-check from the current HEAD
#
# Release mode builds the exact tree of the tag in a detached worktree, so the
# tooling checkout (this script's repository) may be newer than the tag. The
# compiled identity (revision N, tag commit SHA, upstream base tag) is verified
# by scripts/fork/verify_linux.py from the finished package, never inferred.
# Output: artifacts/fork/<sha>/linux-amd64/ with the .deb, manifest-linux-amd64.json,
# SHA256SUMS-linux-amd64 and build.log.
set -euo pipefail
TAG=""
SOURCE=""
VALIDATE=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --tag) [[ $# -ge 2 ]] || exit 2; TAG="$2"; shift 2 ;;
        --source) [[ $# -ge 2 ]] || exit 2; SOURCE="$2"; shift 2 ;;
        --validate) VALIDATE=1; shift ;;
        *) echo "Usage: $0 --tag fork-vX.Y.Z-N [--source DIR] | --validate" >&2; exit 2 ;;
    esac
done
[[ -n "$TAG" || "$VALIDATE" == 1 ]] || { echo "--tag or --validate is required" >&2; exit 2; }
[[ -z "$TAG" || "$VALIDATE" == 0 ]] || { echo "--tag and --validate are exclusive" >&2; exit 2; }
TOOLING="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$TOOLING"
[[ "$(uname -s)/$(uname -m)" == Linux/x86_64 ]] || { echo "Linux x86_64 host required" >&2; exit 1; }
# sed consumes all of ldd's output: with pipefail, `head -1` would turn ldd's SIGPIPE into a failure.
HOST_GLIBC="$(ldd --version 2>&1 | sed -n '1s/.* //p')"
[[ "$HOST_GLIBC" == 2.35 ]] \
    || { echo "glibc 2.35 build host required (Ubuntu 22.04), found ${HOST_GLIBC:-unknown}; newer glibc would raise the asset's floor" >&2; exit 1; }
test -f scripts/fork/verify_linux.py
# Tooling repository toolchains: the verifier needs Python 3.11+ (uv-managed 3.12 on Ubuntu 22.04).
. ./bin/activate-hermit
PY="$(uv python find '>=3.11' 2>/dev/null || command -v python3)"
"$PY" -c 'import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)' \
    || { echo "python >= 3.11 required by the fork verification tooling (run scripts/fork/linux-toolchain.sh)" >&2; exit 1; }

if [[ -n "$TAG" ]]; then
    [[ "$TAG" =~ ^fork-v([0-9]+\.[0-9]+\.[0-9]+)-([1-9][0-9]*)$ ]] || { echo "tag must be fork-vX.Y.Z-N" >&2; exit 2; }
    TAG_VERSION="${BASH_REMATCH[1]}"
    REVISION="${BASH_REMATCH[2]}"
    CANDIDATE_SHA="$(git rev-parse --verify "refs/tags/$TAG^{commit}")"
    if [[ -z "$SOURCE" ]]; then
        SOURCE="$TOOLING/.fork-linux-src/$TAG"
        if [[ -e "$SOURCE" ]]; then
            git -C "$SOURCE" diff --quiet && git -C "$SOURCE" diff --cached --quiet \
                && [[ "$(git -C "$SOURCE" rev-parse HEAD)" == "$CANDIDATE_SHA" ]] \
                || { echo "stale source worktree $SOURCE; remove it" >&2; exit 1; }
        else
            git worktree add --detach "$SOURCE" "$CANDIDATE_SHA"
        fi
    fi
else
    REVISION="${BUZZ_FORK_REVISION:-1}"
    [[ "$REVISION" =~ ^[1-9][0-9]*$ ]] || { echo "positive BUZZ_FORK_REVISION required" >&2; exit 1; }
    SOURCE="${SOURCE:-$TOOLING}"
    CANDIDATE_SHA="$(git -C "$SOURCE" rev-parse HEAD)"
fi
SOURCE="$(cd "$SOURCE" && pwd)"
[[ "$(git -C "$SOURCE" rev-parse HEAD)" == "$CANDIDATE_SHA" ]] || { echo "source checkout is not $CANDIDATE_SHA" >&2; exit 1; }
git -C "$SOURCE" diff --quiet && git -C "$SOURCE" diff --cached --quiet || { echo "source tree is dirty" >&2; exit 1; }
[[ -z "$(git -C "$SOURCE" ls-files --others --exclude-standard)" ]] || { echo "untracked files in source tree" >&2; exit 1; }
BASE_TAG="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["base_tag"])' "$SOURCE/scripts/fork/patches.json")"
if [[ -n "$TAG" && "$BASE_TAG" != "desktop-v$TAG_VERSION" ]]; then
    echo "tag $TAG does not match the registry base $BASE_TAG" >&2; exit 1
fi

OUTPUT="$TOOLING/artifacts/fork/$CANDIDATE_SHA/linux-amd64"
[[ ! -e "$OUTPUT" ]] || { echo "output $OUTPUT already exists; preserve evidence and choose a fresh checkout" >&2; exit 1; }
mkdir -p "$OUTPUT"
exec > >(tee "$OUTPUT/build.log") 2>&1
printf 'candidate_sha=%s\nbase_tag=%s\nfork_revision=%s\nrelease_tag=%s\nsource=%s\n' \
    "$CANDIDATE_SHA" "$BASE_TAG" "$REVISION" "${TAG:-}" "$SOURCE"

# Same hermetic environment as the macOS producer: no inherited updater inputs,
# demo slugs or build capability overrides; immutable identity for build.rs.
unset BUZZ_UPDATER_ENDPOINT BUZZ_UPDATER_PUBLIC_KEY BUZZ_BUILD_DEMO_SLUG
unset BUZZ_BUILD_AGENT_ENV BUZZ_BUILD_AGENT_ACCESS_OWNER_ONLY BUZZ_BUILD_AUTO_CONNECT_DEFAULT_RELAY
unset BUZZ_BUILD_BUZZ_AGENT_PROVIDER BUZZ_BUILD_BUZZ_AGENT_MODEL BUZZ_BUILD_RELAY_RECONNECT_CMD
unset BUZZ_RELAY_URL BUZZ_RELAY_HTTP
export CARGO_INCREMENTAL=0 CI=true
export CARGO_NET_GIT_FETCH_WITH_CLI=true
export BUZZ_FORK_SHA="$CANDIDATE_SHA" BUZZ_FORK_BASE_TAG="$BASE_TAG" BUZZ_FORK_REVISION="$REVISION"

cd "$SOURCE"
. ./bin/activate-hermit
rustc -vV; node --version; pnpm --version
df -h "$SOURCE" | tail -1
pnpm install --frozen-lockfile
cargo build --release -p buzz-acp -p buzz-agent -p buzz-backend-kubernetes -p buzz-dev-mcp \
    -p git-credential-nostr -p buzz-cli
./scripts/bundle-sidecars.sh
(cd desktop && pnpm tauri build --ci --bundles deb)
DEB_DIR="$SOURCE/desktop/src-tauri/target/release/bundle/deb"
mapfile -t DEBS < <(find "$DEB_DIR" -maxdepth 1 -name 'Buzz_*_amd64.deb' -type f)
[[ ${#DEBS[@]} -eq 1 ]] || { echo "expected exactly one .deb in $DEB_DIR, found ${#DEBS[@]}" >&2; exit 1; }
cp "${DEBS[0]}" "$OUTPUT/"
DEB="$OUTPUT/$(basename "${DEBS[0]}")"

cd "$TOOLING"
"$PY" scripts/fork/verify_linux.py --candidate-sha "$CANDIDATE_SHA" --fork-revision "$REVISION" \
    --base-tag "$BASE_TAG" ${TAG:+--tag "$TAG"} --deb "$DEB" --output "$OUTPUT/manifest-linux-amd64.json"
(
    cd "$OUTPUT"
    sha256sum "$(basename "$DEB")" manifest-linux-amd64.json > SHA256SUMS-linux-amd64
    sha256sum -c SHA256SUMS-linux-amd64
)
# Source tree must be exactly as checked out; build outputs live only in ignored paths.
git -C "$SOURCE" diff --quiet && git -C "$SOURCE" diff --cached --quiet \
    && [[ -z "$(git -C "$SOURCE" ls-files --others --exclude-standard)" ]] \
    || { echo "build modified the source tree" >&2; exit 1; }
printf 'Verified Linux x86_64 artifacts: %s\n' "$OUTPUT"
