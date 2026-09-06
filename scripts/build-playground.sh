#!/usr/bin/env bash
# Build the vakBrowse Playground UI and wire it into the vakd-rest binary.
#
# Usage:
#   ./scripts/build-playground.sh         # Build frontend + compile vakd-rest
#   ./scripts/build-playground.sh --dev   # Start Vite dev server (proxy mode)
#
# The built frontend lands in playground/static/ and is served by vakd-rest
# when compiled with --features vakbrowse-api/playground.

set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--dev" ]]; then
    echo "Starting Playground dev server (http://localhost:3000)"
    echo "Make sure vakd-rest is running on :7788 for the API proxy."
    exec npm --prefix playground/frontend exec vite -- --port 3000
    exit 0
fi

echo "=== Building Playground frontend ==="
npm --prefix playground/frontend install
NODE_ENV=production npm --prefix playground/frontend run build

echo "=== Building vakd-rest with playground feature ==="
cargo build --features vakbrowse-api/playground -p vakd-rest

echo ""
echo "Playground ready."
echo "  Run:  target/debug/vakd-rest"
echo "  Open: http://localhost:7788/playground/"
echo ""
echo "To serve the UI separately (dev mode):"
echo "  ./scripts/build-playground.sh --dev"
