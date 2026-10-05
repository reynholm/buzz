#!/usr/bin/env bash
# Prepare a bare Ubuntu 22.04 (glibc 2.35) host for scripts/fork/build-linux.sh.
# Only system libraries come from apt; Rust, Node, pnpm, cmake and just are the
# repository's pinned hermit packages, exactly as upstream and the macOS recipe use.
# Idempotent. Run as root (CI container job) or with sudo.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
. /etc/os-release
[[ "${ID:-}" == ubuntu && "${VERSION_ID:-}" == 22.04 ]] \
    || { echo "Ubuntu 22.04 is required: its glibc 2.35 is the compatibility floor of fork Linux assets" >&2; exit 1; }
export DEBIAN_FRONTEND=noninteractive
APT=(apt-get)
[[ "$(id -u)" == 0 ]] || APT=(sudo -E apt-get)
"${APT[@]}" update
"${APT[@]}" install -y --no-install-recommends \
    build-essential ca-certificates clang cmake curl desktop-file-utils dpkg-dev file git \
    libasound2-dev libayatana-appindicator3-dev libclang-dev libgtk-3-dev librsvg2-dev \
    libssl-dev libwebkit2gtk-4.1-dev libxdo-dev patchelf pkg-config python3 xdg-utils xz-utils
cd "$REPO_ROOT"
# Hermit installs the pinned toolchains on first activation; rustup then honors
# rust-toolchain.toml. Both are network downloads, so retry transient failures.
for attempt in 1 2 3; do
    if . ./bin/activate-hermit && rustup show active-toolchain >/dev/null && node --version >/dev/null \
        && pnpm --version >/dev/null && cmake --version >/dev/null; then
        break
    fi
    [[ $attempt -lt 3 ]] || { echo "hermit toolchain activation failed" >&2; exit 1; }
    sleep 10
done
# Ubuntu 22.04 ships Python 3.10; the fork verification tooling needs 3.11+.
# The pinned hermit uv provides an isolated CPython 3.12 without touching apt.
uv python install 3.12
uv python find 3.12 >/dev/null
rustc -vV
node --version
pnpm --version
cmake --version | head -1
echo "python $(uv python find 3.12)"
echo "Linux toolchain ready on $(. /etc/os-release; echo "$PRETTY_NAME"), glibc $(ldd --version 2>&1 | sed -n '1s/.* //p')"
