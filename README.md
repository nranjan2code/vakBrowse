# vakBrowse

[![CI](https://github.com/nranjan2code/vakBrowse/actions/workflows/ci.yml/badge.svg)](https://github.com/nranjan2code/vakBrowse/actions/workflows/ci.yml)

An agent-native browser: a Rust core that gives AI agents a real, scriptable
web browser. Humans are optional.

## Architecture

```
vakBrowse/
├── crates/
│   ├── vakbrowse-core        # domain types, errors, wire shapes
│   ├── vakbrowse-perception  # a11y tree -> compact snapshots with stable @eN refs
│   ├── vakbrowse-engine      # Engine trait + CDP backend (chrome-headless-shell),
│   │                         #   stealth integration, proxy, history, extract
│   ├── vakbrowse-stealth     # fingerprint profiles + humanized input
│   ├── vakbrowse-server      # SessionManager, command model, policy, UDS protocol
│   ├── vakbrowse-cli         # `vak` binary
│   ├── vakbrowse-mcp         # `vak-mcp`: MCP server, 24 browser_* tools
│   ├── vakbrowse-api         # `vakd-rest`: axum REST + WebSocket bridge
│   └── vakbrowse-ffi         # cdylib: embed in Python/Node/Go via C ABI
├── bins/vakd                 # daemon (session pool, profiles, policy)
└── tests/fixtures            # offline fixture pages
```

One `SessionManager` and command model power every surface — add a
capability once, all five surfaces get it.

## Status

**Roadmap P0–P6 complete.**

- **Core (P0–P1)** — chrome-headless-shell download/pin/cache; a11y snapshots
  with stable `@eN` refs; trusted clicks, framework-safe fills, select/key/
  scroll/wait; cookies + persistent profiles.
- **Surfaces (P2–P3)** — `vakd` daemon over UDS; `vak` CLI; `vak-mcp`
  (**24 tools**, verified MCP handshake); `vakd-rest` REST + WebSocket;
  `libvakbrowse_ffi` cdylib (Python ctypes drives full flows in-process);
  Docker image with the pinned engine baked in.
- **Agent-grade capabilities (P4)** — stealth fingerprints + humanized
  pointer paths, vision fallback (`shot`/`click-at`, MCP returns real
  images), session pooling (caps + idle reaper), WebMCP surfacing.
- **Depth (P5)** — multi-tab sessions, cross-frame (iframe) perception &
  clicks, CI on ubuntu/macos.
- **Battle-tested (P6)** — dogfooded live against Wikipedia / Hacker News /
  GitHub / Bing / DuckDuckGo; fixed snapshot URL self-healing,
  fill-now-focuses (Enter submits), history actions; per-session
  `--stealth` and `--proxy`; `extract` action returning clean readable
  article text; tagged releases shipping binaries.

45 tests green, clippy clean. Honest bot-wall findings: Bing works,
DuckDuckGo CAPTCHAs automation regardless of fingerprint (their detection
is TLS/behavioral). See `AGENTS.md` for the full map and conventions.

## Install (from a release tag)

Download the tarball for your platform from GitHub Releases — it contains
`vakd`, `vak`, `vak-mcp` and `vakd-rest`. The engine binary downloads and
pins itself on first run (or bake it into Docker via the included
Dockerfile).

## Try it

```sh
cargo run -p vakd -- doctor        # engine probe (downloads engine on first run)
cargo test --workspace             # hermetic test suite

# daemon + CLI
./target/release/vakd serve &
./target/release/vak --socket /tmp/vakd.sock open https://example.com
./target/release/vak --socket /tmp/vakd.sock snapshot s1
./target/release/vak --socket /tmp/vakd.sock extract s1

# MCP server for Claude/Cursor/opencode: command = target/release/vak-mcp

# HTTP API
./target/release/vakd-rest &       # :7788
curl -X POST localhost:7788/sessions -d '{"url":"https://example.com"}' \
  -H 'content-type: application/json'

# embed in Python (in-process, no daemon)
import ctypes; ctypes.CDLL("target/release/libvakbrowse_ffi.dylib")

# or container
docker build -t vakbrowse . && docker run -p 7788:7788 vakbrowse
```

## License

MIT — see [LICENSE](LICENSE).
