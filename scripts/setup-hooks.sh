#!/usr/bin/env bash
# Install the local pre-push gate (host cargo test + clippy). Safe to re-run.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
HOOKS_DIR="$(git rev-parse --git-path hooks)"
mkdir -p "$HOOKS_DIR"
cp scripts/pre-push "$HOOKS_DIR/pre-push"
chmod +x "$HOOKS_DIR/pre-push"
echo "installed: $HOOKS_DIR/pre-push (runs 'cargo test' + clippy w/ dom-backend + playground features before each push)"
echo "linux gate is NOT automatic — run:"
echo "  ./scripts/verify.sh --root"
echo "  ./scripts/verify.sh --uid 1000"
