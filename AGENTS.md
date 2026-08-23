# AGENTS.md — vakBrowse

Instructions for AI agents (and humans) working in this repository.

## What this is

vakBrowse is an **agent-native browser**: a Rust core that gives AI agents a
real, scriptable web browser. Humans are optional. All work happens in this
directory (`/Users/nisheethranjan/Projects/vakBrowse`).

## Golden rules

1. **Keep docs current with every change.** Update `AGENTS.md` (this file) and
   `README.md` status sections as part of any phase/feature, not after.
2. **Engine seam is sacred.** All capabilities go through
   `crates/vakbrowse-engine/src/lib.rs` traits (`EngineLauncher`, `PageOps`).
   Never leak CDP/chromiumoxide types outside the engine crate.
3. **No `--no-sandbox`.** Launch flags live in `cdp.rs::DEFAULT_ARGS`; sandbox
   stays on.
4. **URL policy gate.** Every navigation passes `validate_url`
   (http/https/file/about/data only). Extend deliberately.
5. **Token efficiency is a feature.** Perception output targets <500 tokens
   per typical page snapshot.

## Build & verify (run from repo root)

```sh
cargo build --workspace          # compile everything
cargo test  --workspace          # unit + offline integration tests
cargo clippy --workspace --tests # must be warning-free before finishing work
cargo run -p vakd -- doctor      # end-to-end engine probe (downloads on first run)
```

Manual E2E (daemon + CLI):

```sh
vakd serve --socket /tmp/vakd.sock &
vak --socket /tmp/vakd.sock open https://example.com
vak --socket /tmp/vakd.sock snapshot s1
```

MCP client config (Claude Code / Cursor / opencode):

```json
{ "mcpServers": { "vakbrowse": { "command": "/path/to/target/release/vak-mcp" } } }
```

Optional env: `VAKBROWSE_ALLOW_PREFIXES=https://a.com,https://b.com` (URL
allowlist for the MCP surface). `VAKBROWSE_HTTP_PORT` (default 7788) for
`vakd-rest`.

Embedding from Python (no daemon needed — library owns its runtime):

```python
import ctypes, json
lib = ctypes.CDLL("target/release/libvakbrowse_ffi.dylib")   # .so on linux
lib.vak_request.argtypes = [ctypes.c_char_p]
lib.vak_request.restype = ctypes.c_void_p
lib.vak_string_free.argtypes = [ctypes.c_void_p]
def call(o):
    p = lib.vak_request(json.dumps(o).encode())
    s = ctypes.string_at(p).decode(); lib.vak_string_free(p)
    return json.loads(s)
call({"type":"open","options":{"url":"https://example.com"}})
```

Docker: `docker build -t vakbrowse .` then `docker run -p 7788:7788 vakbrowse`
(engine binary is baked in at build time via `vakd doctor --no-probe`).

- Tests must pass and clippy must be clean before declaring work done.
- Network-dependent tests are marked `#[ignore]`; the offline suite must stay
  hermetic (fixtures under `tests/fixtures/`).

## Architecture map

```
crates/
  vakbrowse-stealth     # deterministic fingerprint profiles, init-script patches,
                        #   bezier mouse paths; consumed by engine LaunchOptions.stealth
  vakbrowse-core        # VakError, ids, Snapshot/Cookie/WebMcpTool wire shapes
  vakbrowse-perception  # AX tree -> compact snapshot w/ stable @eN refs (pure, unit-tested)
  vakbrowse-engine      # EngineLauncher/PageOps traits; backends:
    ├── cft.rs          #   chrome-headless-shell download/pin/cache (Chrome-for-Testing)
    └── cdp.rs          #   CDP backend via chromiumoxide (launch, navigate,
                        #     click/fill/select/press_key/scroll/wait, cookies, downloads)
  vakbrowse-server      # SessionManager, Request/Action/Response model, URL policy,
                        #   snapshot text renderer, UDS wire protocol (serve + client)
  vakbrowse-cli         # `vak` binary — thin clap wrapper over the wire client
  vakbrowse-mcp         # `vak-mcp` binary + VakMcp lib — MCP server (rmcp, stdio),
                        #   20 browser_* tools incl. tabs/screenshot/click-at/webmcp;
  vakbrowse-api         # `vakd-rest` binary + lib — axum REST + WebSocket bridge;
                        #   endpoints map 1:1 onto Request model
  vakbrowse-ffi         # cdylib C ABI (`vak_request(json) -> json`) w/ embedded
                        #   tokio runtime; consumed via ctypes/koffi/etc.

(No remaining planned crates — the phase roadmap is complete.)
bins/
  vakd                  # daemon (`serve`, `status`, `doctor`): owns sessions over UDS
```

Planned crates (add when their phase starts): `vakbrowse-stealth`.

## Key facts

- Engine binary: chrome-headless-shell, cached at
  `~/Library/Caches/vakbrowse/cft/{version}/{platform}/…` with a
  `{channel}.version` marker for offline reuse. System Chrome is the offline
  fallback (`cft::find_system_chrome`).
- Element refs look like `@e42` (`ElementRef`). Stable across snapshots,
  reset on navigation; stale refs return `VakError::NotFound`, never a misfire.
- Actions use trusted input where it matters: clicks are real
  `Input.dispatchMouseEvent` sequences at box-model centers; fills use the
  native value setter + input/change events (React/Vue-safe).
- One command model everywhere: `SessionManager::handle(Request)` serves the
  daemon (UDS newline-JSON), the CLI and MCP tools identically. Add a
  capability ONCE in `Action` + engine trait; every surface gets it.
- Wire gotchas (both bit us): nested internally-tagged serde enums duplicate
  their tag key, and primitive/seq newtype variants cannot be tagged at all.
  Keep outer enums externally tagged or struct-style variants only.
- Stealth is honest by design: it defeats `navigator.webdriver` exposure,
  missing plugin/language data and robotic pointer teleports; it does NOT
  defeat behavioral biometrics or TLS fingerprinting. See stealth crate doc.
- Vision fallback flow for agents: browser_screenshot -> reason over pixels ->
  browser_click_at(x,y). Pair with a11y snapshots first; coords are last resort.
- Sessions own a TAB REGISTRY (`tabs/new_tab/switch_tab/close_tab`); refs are
  per-tab. Snapshots merge AX trees across the frame tree (frame-prefixed ids
  `f0:` root, `f1:` …). Cross-frame clicks fire real DOM clicks on the
  resolved element because child-frame box coords are frame-relative —
  trusted mouse events apply to root-frame elements only today. OOPIF frames
  that reject frame-scoped CDP commands are skipped, not fatal. file://
  iframes are unique-origin: they cannot navigate `_top`; test signals must
  stay inside the frame.
- chromiumoxide v0.9.x is tokio-only; ureq v3 API (`into_body().into_reader()`),
  zip v8 extraction. CDP gotcha: in `Runtime.callFunctionOn` the resolved DOM
  node arrives as `this`, not as an argument.

## Phase status

| Phase | Scope | Status |
|---|---|---|
| P0 | Scaffold, engine download/pin, CDP connect, navigate smoke | **done** |
| P1 | A11y snapshots w/ stable refs, click/fill/select/wait actions, cookies, profiles, downloads dir | **done** |
| P2 | `vakd` daemon (UDS) + `vak` CLI + `vak-mcp` MCP server (12 tools), policy allowlist | **done** |
| P3 | REST/WS API (`vakd-rest`), FFI cdylib (Python ctypes verified), Dockerfile w/ baked engine | **done** |
| P4 | Stealth module, session pooling, vision fallback, WebMCP surfacing | pending |

## Conventions

- Edition 2024, workspace deps centralized in root `Cargo.toml`.
- Errors: single taxonomy in `vakbrowse-core::VakError`; engines map native
  errors into it at the boundary.
- No comments unless explaining non-obvious intent; keep them about "why".
- Commits: small, conventional-ish prefixes (feat:, fix:, test:, docs:).
