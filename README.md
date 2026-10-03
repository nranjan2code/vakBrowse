# vakBrowse

An agent-native browser runtime: a high-speed Rust core that gives AI agents
a real, scriptable web browser powered by headless Chromium under the hood. Humans are optional.

## Architecture

```
vakBrowse/
├── crates/
│   ├── vakbrowse-core        # domain types, errors, wire shapes
│   ├── vakbrowse-perception  # a11y tree -> compact snapshots with stable @eN refs
│   ├── vakbrowse-engine      # EngineLauncher/PageOps traits + CDP backend:
│   │                         #   chrome-headless-shell, stealth, proxy, history,
│   │                         #   extract, WebMCP
│   ├── vakbrowse-stealth     # fingerprint profiles + humanized input
│   ├── vakbrowse-server      # SessionManager, command model, policy, UDS protocol
│   ├── vakbrowse-cli         # `vak` binary
│   ├── vakbrowse-mcp         # `vak-mcp`: MCP server, 35 browser_*
│   ├── vakbrowse-api         # `vakd-rest`: axum REST + WebSocket bridge
│   ├── vakbrowse-ffi         # cdylib: embed in Python/Node/Go via C ABI
├── bins/vakd                 # daemon (session pool, profiles, policy)
└── tests/fixtures            # offline fixture pages
```

One `SessionManager` and command model power every surface — add a
capability once, all five surfaces get it.

## Status

**Roadmap P0–P7 complete.**

- **Core (P0–P1)** — chrome-headless-shell download/pin/cache; a11y snapshots
  with stable `@eN` refs; trusted clicks, framework-safe fills, select/key/
  scroll/wait; cookies + persistent profiles.
- **Surfaces (P2–P3)** — `vakd` daemon over UDS; `vak` CLI; `vak-mcp`
  (**35 tools**, verified MCP handshake); `vakd-rest` REST + WebSocket;
  `libvakbrowse_ffi` cdylib (Python ctypes drives full flows in-process);
  Docker image with the pinned engine baked in.
- **Agent-grade capabilities (P4)** — stealth fingerprints + humanized
  pointer paths, vision fallback (`shot`/`click-at`, MCP returns real
  images), session pooling (caps + idle reaper), WebMCP surfacing.
- **Depth (P5)** — multi-tab sessions, cross-frame (iframe) perception &
  clicks.
- **Battle-tested (P6)** — dogfooded live against Wikipedia / Hacker News /
  GitHub / example.com (click-through to www.iana.org proven); fixed snapshot URL
  self-healing, fill-now-focuses (Enter submits), history actions,
  `eval_text` stringification of all JS return types, scroll-into-view for
  below-fold clicks, and a SPA-safe `wait_url` (waits on `location.href`, not
  `readyState`); per-session `--stealth` and `--proxy`; `extract` action
  returning clean readable article text — including card/div-layout pages
  whose content lives in `div`/`span` blocks rather than `<p>`/`<article>`,
  paged with `offset`/`max_chars` (default 20k chars per window; a footer
  names the offset to continue from); tagged releases shipping binaries.
- **Throughput & bot-wall resilience (P6+, this cycle)** — `Request::Batch`
  runs a sequence of actions in one round-trip with fail-fast, cutting agent
  latency across all five surfaces (`vak batch`, `browser_batch`,
  `POST /batch`, UDS + WS, FFI); per-session proxy rotation via `RotateProxy`
  (`--proxies a,b` / `browser_rotate_proxy` / `vak rotate-proxy`) re-launches
  Chrome on the next endpoint and **restores the session URL** so an agent
  can carry on after a bot-wall challenge; `--human-timing` injects
  sub-150ms randomized input delays (cadence tell, no TLS spoofing);
  `scripts/release.sh` bakes the pinned engine + gates mac/linux-root/
  linux-uid1000 green.
- **Interactive Playground UI** — Industrial showcase with unified JSON RPC,
  CDP screenshots, batch studio, console REPL, cookie manager, and tour guide.

115 tests, clippy clean. Hardened against real environments: launch args verified
against chromiumoxide's double-dash footgun, sandbox auto-fallback for
root/hardened runners (validated in linux containers as root *and* non-root). `Action::Click`
now returns a navigation signal (`ActionResult::Clicked { navigated, url }`):
anchor clicks are probed for a URL change (ground truth, not the unreliable
`wait_for_navigation` result) so a bot-wall click — Bing/DDG accept the click
but never navigate — surfaces as `navigated:false` + a daemon WARN instead of
the old silent `Done`. **Recovery is opt-in** (`--click-recovery` /
`browser_open {click_recovery:true}`): only then does the click escalate to a
ground-truth DOM `.click()` and a forced `location.href =` assignment (defeats
click-interception walls; proven by the `preventDefault` fixture) before giving
up honestly. It is off by default because the escalation re-fires the link's
handlers — an add-to-cart/vote/delete link could run up to three times. A
navigating click returns only once the landed document has loaded
(`readyState === 'complete'`, bounded at 5s) — like `navigate()` — so chained
click-throughs (pager "Next" → "Next") don't fire trusted input into a page
still render-blocked on its stylesheet, which used to drop the second click. Honest
bot-wall findings: Bing blocks ALL synthetic navigation (a trusted
mouse-event click AND a ground-truth DOM `element.click()` both leave the
browser on the SERP, even under `--stealth` — their detection is behavioral,
not webdriver-level), and DuckDuckGo CAPTCHAs / serves an empty shell
regardless of fingerprint. Search+click-through recipes use Wikipedia and
example.com. Architectural notes: app errors now flow as a
structured `ServiceError` over the wire (never raw strings), the engine
seam is `CdpLauncher` held by `SessionManager` (CDP is the sole backend — the
`EngineLauncher`/`PageOps` trait seam remains for future backends but is not
exposed as a runtime-swappable backend),
MCP `browser_screenshot` returns real PNG pixels (not a placeholder), and the
FFI embeds a long-lived runtime with a nested-context guard so it works from
inside a caller tokio loop. The stdio transport is spec-robust: `vak-mcp`
accepts BOTH NDJSON (the framing the official `mcp` Python + TS SDKs use —
rmcp 3.1.4's stdio is newline-delimited JSON) AND spec-literal Content-Length
blocks, via a byte-level `StdioFramer` that rewrites incoming framing to NDJSON
without touching outgoing responses. Covered by two real-subprocess handshake
tests (NDJSON + Content-Length client) and three hermetic codec unit tests.
See `AGENTS.md` for the full map and conventions.

## Python SDK

```bash
pip install vakbrowse          # wheel bundles the native lib; no daemon
```

```python
from vakbrowse import Session
s = Session()
sid, _ = s.open("https://example.com")
print(s.extract(sid)["text"])          # readable main-content text
s.batch(sid, [{"type": "navigate", "url": "https://example.com"},
              {"type": "extract"}])    # one round-trip, fail-fast
s.close(sid)
```

The wheel builds `crates/vakbrowse-ffi` and bundles `libvakbrowse_ffi.so`/
`.dylib`/`.dll`. `pip install` needs a Rust toolchain once (the lib is compiled
into the wheel, so end users don't). Or go bare-ctypes against a cargo build:
`cargo build -p vakbrowse-ffi --release --config profile.release.strip=false`
(the workspace `strip=true` corrupts a cdylib's `__LINKEDIT` alignment on
macOS and dyld rejects it; the wheel disables stripping for the FFI lib only).

## Playground

A React + TypeScript showcase app that exercises every vakBrowse capability
through the `vakd-rest` HTTP API.

```bash
# Build frontend + backend in one go
./scripts/build-playground.sh

# Serve both API and UI from the same binary
target/debug/vakd-rest
# Open http://localhost:7788/playground/
```

The playground speaks the unified `Request` model via `POST /playground/rpc`,
the same JSON protocol the daemon, CLI, MCP, and FFI use. A guided tour
built into the UI walks through sessions, snapshots, clicks, fills, CSS
find, extract, source, screenshots, JavaScript eval, file upload, cookies,
downloads, batching, stealth, and proxy rotation.

See [`playground/README.md`](playground/README.md) for component architecture.


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
./target/release/vak --socket /tmp/vakd.sock find s1 'a[href*="iana"]'   # CSS -> @eN refs
./target/release/vak --socket /tmp/vakd.sock click s1 @e1            # click that ref
./target/release/vak --socket /tmp/vakd.sock extract s1

# MCP server for Claude/Cursor/opencode: command = target/release/vak-mcp

# HTTP API
./target/release/vakd-rest &       # :7788
curl -X POST localhost:7788/sessions -d '{"url":"https://example.com"}' \
  -H 'content-type: application/json'

# embed in Python (in-process, no daemon)
import ctypes; ctypes.CDLL("target/release/libvakbrowse_ffi.dylib")

# or container
docker build -t vakbrowse . && \
  docker run -e VAKBROWSE_API_TOKEN=change-me -p 127.0.0.1:7788:7788 vakbrowse
# clients then send `Authorization: Bearer change-me`
```

## License

MIT — see [LICENSE](LICENSE).
