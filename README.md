# vakBrowse

An agent-native browser: a Rust core that gives AI agents a real, scriptable
web browser. Humans are optional.

## Architecture

```
vakBrowse/
├── crates/
│   ├── vakbrowse-core        # domain types, errors, ids
│   └── vakbrowse-engine      # Engine trait + CDP backend (chrome-headless-shell)
├── bins/vakd                 # daemon (session pool, profiles, policy)  [P2]
└── tests/fixtures            # offline fixture pages
```

Surfaces land per phase: MCP server + CLI (P2), REST/WS API + FFI library (P3),
stealth, session pooling, vision fallback, WebMCP (P4).

## Status

- **P0 done** — scaffold; chrome-headless-shell download/pin/cache; CDP
  launch/connect/navigate/eval.
- **P1 done** — a11y snapshots with stable `@eN` refs; trusted click /
  framework-safe fill / select / key / scroll / wait; cookies + profiles.
- **P2 done** — one `SessionManager`, three surfaces:
  - `vakd serve` — daemon owning sessions over a UDS socket (`vakd status`)
  - `vak` — CLI: open/navigate/snapshot/click/fill/select/key/wait/eval/close
  - `vak-mcp` — MCP server (rmcp), 12 tools, verified handshake via stdio;
    optional URL allowlist via `VAKBROWSE_ALLOW_PREFIXES`
- **P3 done** — `vakd-rest` (axum): REST + WebSocket bridge over the same
  Request model; `libvakbrowse_ffi` cdylib — Python ctypes drives a full
  form flow in-process (no daemon); multi-stage Dockerfile with the pinned
  engine baked into the image.
- **P4 done** — stealth profiles (deterministic per identity, `navigator.webdriver`
  patched, humanized bezier pointer paths), vision fallback (`shot`/`click-at`,
  MCP returns real image content blocks), session pooling (hard caps + idle
  reaper in `vakd serve`), WebMCP surfacing (`web-mcp-tools`/`invoke`, graceful
  when absent).

**Roadmap P0-P4 complete.** 37 tests green, clippy clean. See `AGENTS.md` for
the full map and post-roadmap ideas.

## Try it

```sh
cargo run -p vakd -- doctor        # engine probe

# daemon + CLI
./target/release/vakd serve &
./target/release/vak --socket /tmp/vakd.sock open https://example.com
./target/release/vak --socket /tmp/vakd.sock snapshot s1

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

## Quick start

```sh
cargo run -p vakd -- doctor   # downloads/pins chrome-headless-shell if needed
cargo test -p vakbrowse-engine
```
