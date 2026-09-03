#!/usr/bin/env bash
# Local CI substitute (GH Actions is intentionally disabled — see AGENTS.md).
# Reproduces the linux/amd64 root + uid-1000 green gate against a real Chrome.
#
#   ./scripts/verify.sh                 # linux root inside Docker
#   ./scripts/verify.sh --uid 1000      # linux non-root inside Docker
#   ./scripts/verify.sh --mac           # host mac (default target)
#
# Reuses the `vk-cargo` and `vk-target` docker volumes to avoid re-downloading
# the Rust toolchain / crates; the chrome-headless-shell CFT cache is NOT
# volume-mounted, so it downloads once per fresh container (~30s).
set -euo pipefail

CARGO_LIBS=(
  libnss3 libnspr4 libatk1.0-0 libatk-bridge2.0-0 libcups2 libdrm2 libxkbcommon0
  libxcomposite1 libxdamage1 libxfixes3 libxrandr2 libgbm1 libpango-1.0-0
  libcairo2 libasound2 libdbus-1-3
)

cd "$(git rev-parse --show-toplevel)"

mode="root"
if [[ "${1:-}" == "--uid" ]]; then
  mode="uid1000"; shift
  Uid="${1:?--uid needs a uid}"
elif [[ "${1:-}" == "--mac" ]]; then
  echo "=== mac: cargo test + clippy ==="
  cargo test --workspace --no-fail-fast
  cargo clippy --workspace --tests
  exit 0
fi

LIB_APTS=$(IFS=' '; echo "${CARGO_LIBS[*]}")
# Always mount the source tree at /src (root path originally dropped it when
# the vk-cargo volume existed — Docker then saw no Cargo.toml).
VOL_SRC="-v $(pwd):/src"
VOL_CARGO=""
if docker volume ls --format '{{.Name}}' | grep -q '^vk-cargo$'; then
  VOL_CARGO="--mount type=volume,src=vk-cargo,target=/usr/local/cargo"
fi
VOL_TARGET="--mount type=volume,src=vk-target,target=/src/target"

if [[ "$mode" == "root" ]]; then
  docker run --rm --platform linux/amd64 \
    $VOL_SRC $VOL_CARGO $VOL_TARGET -v /tmp/vaklogs:/logs:rw -w /src \
    rust:1-bookworm \
    bash -c "set -e
      apt-get update -qq && apt-get install -y -qq $LIB_APTS >/dev/null 2>&1
      rustup component add clippy >/dev/null 2>&1 || true
      cargo test --workspace --no-fail-fast
      cargo clippy --workspace --tests"
else
  # Non-root (mirrors the constrained-container CI that blocks the sandbox even
  # for unprivileged users): build as root, then hand off to a uid-N user.
  docker run --rm --platform linux/amd64 \
    $VOL_SRC $VOL_CARGO $VOL_TARGET -v "$(pwd)":/src -v /tmp/vaklogs:/logs:rw -w /src \
    rust:1-bookworm \
    bash -c "set -e
      apt-get update -qq && apt-get install -y -qq $LIB_APTS >/dev/null 2>&1
      rustup component add clippy >/dev/null 2>&1 || true
      chmod -R 777 /usr/local/cargo /src/target
      useradd -m tester
      su tester -c 'cd /src && cargo test --workspace --no-fail-fast && cargo clippy --workspace --tests'"
fi
