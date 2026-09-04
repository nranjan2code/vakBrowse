#!/usr/bin/env bash
# Release path with GH Actions intentionally disabled (billing). CUT A RELEASE:
#   ./scripts/release.sh v0.4.0
# Steps: (1) bake the pinned chrome-headless-shell into the engine cache via
# `vakd doctor --no-probe` (no probe = offline-friendly), (2) run the full
# green gate on macOS + linux/amd64 root + linux/amd64 uid 1000, (3) tag +
# push, (4) append a CHANGELOG.md entry. Aborts if any gate fails.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

VERSION="${1:?usage: ./scripts/release.sh v<version>  (e.g. v0.4.0)}"
echo "=== releasing $VERSION ==="

# 0. tree must be clean (no uncommitted docs/code).
if [[ -n "$(git status --porcelain)" ]]; then
    echo "refusing: working tree is dirty. commit first." >&2
    exit 1
fi

# 1. Bake the engine binary (chrome-headless-shell) into the local cache so the
#    Docker images + release artifacts are self-contained. --no-probe avoids a
#    live launch in offline/sandboxed contexts. The `dom-backend` feature is
#    enabled so the daemon ships chrome-free sessions too.
cargo build -p vakd --features vakbrowse-server/dom-backend
target/debug/vakd doctor --no-probe

# 2. Green gate across all three environments. Each writes to its own log dir.
echo "=== mac gate ==="
./scripts/verify.sh --mac

echo "=== linux/amd64 root gate ==="
./scripts/verify.sh --root

echo "=== linux/amd64 uid 1000 gate ==="
./scripts/verify.sh --uid 1000

# 3. Tag + push.
git tag "$VERSION"
git push origin "$VERSION"
git push

# 4. CHANGELOG entry (auto-generated; edit the file to add detail if desired).
DATE=$(date +%Y-%m-%d)
{
    echo "## $VERSION ($DATE)"
    echo
    echo "Released via \`scripts/release.sh\` — verified green on macOS arm64, linux/amd64 root, and linux/amd64 uid 1000."
    echo
    git --no-pager log --oneline "$(git describe --tags --abbrev=0 2>/dev/null || echo "$VERSION^")..$VERSION" 2>/dev/null \
        | sed 's/^/- /'
    echo
} >> CHANGELOG.md

echo "=== released $VERSION (tag pushed, CHANGELOG updated) ==="
