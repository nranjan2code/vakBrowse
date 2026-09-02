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
   `SessionManager` holds `Arc<dyn EngineLauncher>` (the trait is
   `#[async_trait]` object-safe) so the daemon/CLI/MCP/API/FFI surfaces are
   backend-agnostic; the CDP backend lives in `cdp.rs` and is the sole owner of
   chromiumoxide/CDP types. Swap backends by injecting a launcher via
   `SessionManager::new(policy, launcher)`; the convenience
   `SessionManager::with_policy(policy)` uses the CDP launcher. Never leak
   CDP/chromiumoxide types outside the engine crate.
3. **Sandbox stays on, with automatic fallback.** Chromium's sandbox cannot
   operate as root (CI/Docker) — `cdp.rs` detects uid 0 and opts out with a
   warning. Additionally, hardened runners (GitHub ubuntu images restrict
   unprivileged user namespaces) reject the sandbox even for non-root: on a
   sandbox/zygote/namespace launch error we retry once with `--no-sandbox`.
   Never disable the sandbox by configuration alone.
4. **chromiumoxide arg format**: `BrowserConfig::arg()` takes BARE keys —
   it prepends `--` itself (`format!("--{key}")`). Passing `"--flag"`
   produces `----flag`, which Chromium silently ignores (this shipped
   broken for three phases before CI caught it). Always feed args through
   `push_arg()` in `cdp.rs`. Under stealth profiles we also call
   `disable_default_args()` to drop chromiumoxide's `--enable-automation`
   default.
5. **Verify on linux before trusting green locally.** This repo has shipped
   two mac-only-invisible bugs; the fastest local check is a
   `linux/amd64` container run as root *and* as a non-root user:
   ```sh
   docker run --rm -it --platform linux/amd64 -v "$PWD":/src -w /src \
     rust:1-bookworm bash -c "cargo test --workspace"
   ```
6. **GitHub Actions is intentionally disabled** for this repo (private
   account hit metered-minutes billing limits; workflows were removed in
   commit history — restore from git if that changes). Verification is
   LOCAL: `cargo test --workspace` + `cargo clippy --workspace --tests`
   plus the linux-container recipe below. Do not push workflow files
   under `.github/` unless asked.
7. **URL policy gate.** Every navigation passes `validate_url`
   (http/https/file/about/data only). Extend deliberately.
8. **Token efficiency is a feature.** Perception output targets <500 tokens
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
- Browser-launching integration tests serialize on a shared `common::browser_lock()`
  semaphore within each test binary: four parallel chrome launches exhaust
  container resources (especially as non-root, where the sandbox retry thrashes).
  This keeps `cargo test --workspace` green on macOS, Linux-root, AND Linux-non-root.
- Verified green on `linux/amd64` as root (sandbox auto-disabled) and as uid 1000
  (sandbox kept on, auto-falls back to `--no-sandbox` on the non-root sandbox
  rejection): 48 passed / 2 ignored in both, clippy clean.

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
                        #     click/fill/select/press_key/scroll/wait, cookies,
                        #     downloads, WebMCP list/invoke; args passed as
                        #     structured CallArgument via call_on_global)
  vakbrowse-server      # SessionManager, Request/Action/Response model, URL policy,
                        #   ServiceError structured wire errors, snapshot renderer,
                        #   UDS wire protocol (serve + client)
  vakbrowse-cli         # `vak` binary — thin clap wrapper over the wire client
  vakbrowse-mcp         # `vak-mcp` binary + VakMcp lib — MCP server (rmcp, stdio),
                        #   24 browser_* tools (tabs, history, screenshot/click-at,
                        #   extract, webmcp, stealth/proxy on open);
  vakbrowse-api         # `vakd-rest` binary + lib — axum REST + WebSocket bridge;
                        #   endpoints map 1:1 onto Request model
  vakbrowse-ffi         # cdylib C ABI (`vak_request(json) -> json`) w/ embedded
                        #   tokio runtime; consumed via ctypes/koffi/etc.
bins/
  vakd                  # daemon (`serve`, `status`, `doctor`): owns sessions over UDS
```

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
  defeat behavioral biometrics or TLS fingerprinting. Site isolation is
  intentionally left ON — disabling it (`--disable-features=site-per-process`)
  only masked bugs in cross-frame perception and is unnecessary against modern
  Chrome's automation detection. See stealth crate doc.
- Vision fallback flow for agents: browser_screenshot -> reason over pixels ->
  browser_click_at(x,y). Pair with a11y snapshots first; coords are last resort.
- Snapshot self-heals: at snapshot time we reconcile against `page.url()`,
  so click-driven navigations (form submits, SPA links) update the reported
  URL and start a fresh ref turn — even though only explicit navigate()
  goes through our code.
- `fill` focuses the element before setting its value; the human pattern
  fill -> press_key(Enter) therefore submits forms and SPA search boxes.
- History actions (`back`/`forward`/`reload`) wait for navigation and
  tolerate two races: the old execution context dying mid-reload, and
  `wait_for_navigation` rejecting with "Inspected target navigated or
  closed" (which means the navigation succeeded).
- `extract` is the token-cheap reading tool: readability-style main-content
  extraction returning title/url/markdown-ish text (20KB from a 500KB
  Wikipedia page). Agents should prefer extract over eval for reading.
- Per-session proxy ships as `SessionOptions.proxy` (`--proxy` on CLI,
  `browser_open {proxy}` in MCP) — the answer to IP-reputation walls.
  Proxy *rotation* across a pool of endpoints remains a future idea.
- Dogfood findings (real web): Wikipedia/HN/GitHub/Bing flows work
  end-to-end. DuckDuckGo hard-walls automation (CAPTCHA on html endpoint,
  empty JS shell on main) even WITH a stealth profile — their detection is
  TLS/behavioral, not webdriver-level. Agent recipe: prefer Bing for search
  flows. Stealth (`vak open --stealth`, `browser_open {stealth:true}`,
  seed via `stealth_seed`) defeats webdriver/plugin/pointer tells but not
  TLS-fingerprint walls.
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
- Error model: `handle()` always returns `Result<ResponsePayload, String>`
  where app-level failures are `Ok(ResponsePayload::Error(ServiceError))`;
  `Err(String)` is reserved for handler-panic transport errors. `ServiceError`
  carries a stable kind (Engine/Protocol/Perception/Policy/NotFound/Timeout/
  Http/Unsupported/Io) so HTTP/MCP/CLI map errors to codes/status without
  string matching. `VakError` remains the single in-process taxonomy; engines
  map native errors into it at the boundary, the server maps it into
  `ServiceError` at the seam.
- Cookies carry an optional `samesite` field (`Cookie` + `CookieInput`);
  `set_cookie` parses it via chromiumoxide's `CookieSameSite` (case-insensitive
  `FromStr`) and `cookies()` maps it back.
- Embedding from Python (ctypes) works in-process: the FFI owns a long-lived
  multi-thread runtime that `block_on`s the shared `SessionManager`; if the
  caller thread is already inside a tokio runtime, the call is offloaded to a
  dedicated blocking thread so `block_on` never panics ("cannot block the
  current thread from within a runtime context"). The runtime stays alive
  across calls so the CDP event-handler task is never starved.
- Vision fallback flow for agents: `browser_screenshot` returns real PNG bytes
  as an MCP `Blob` content block (decode the `iVBORw0KGgo` base64 prefix), not
  a placeholder string. Pair with a11y snapshots first; coords are last resort.

## Phase status

| Phase | Scope | Status |
|---|---|---|
| P0 | Scaffold, engine download/pin, CDP connect, navigate smoke | **done** |
| P1 | A11y snapshots w/ stable refs, click/fill/select/wait actions, cookies, profiles, downloads dir | **done** |
| P2 | `vakd` daemon (UDS) + `vak` CLI + `vak-mcp` MCP server (rmcp), policy allowlist | **done** |
| P3 | REST/WS API (`vakd-rest`), FFI cdylib (Python ctypes verified), Dockerfile w/ baked engine | **done** |
| P4 | Stealth module, session pooling, vision fallback, WebMCP surfacing | **done** |
| P5 | Multi-tab sessions, cross-frame (iframe) perception & clicks; CI built then disabled (billing) | **done** |
| P6 | Real-web dogfooding fixes (snapshot self-heal, fill-focuses, history) + per-session stealth/proxy + `extract` action + release workflow | **done** |

Post-roadmap ideas (not committed): pip/npm packaging of the FFI,
WebDriver BiDi backend behind the engine trait, Servo/Lightpanda
experimental backends, proxy rotation across endpoint pools.

## Conventions

- Edition 2024, workspace deps centralized in root `Cargo.toml`.
- Errors: single taxonomy in `vakbrowse-core::VakError`; engines map native
  errors into it at the boundary.
- No comments unless explaining non-obvious intent; keep them about "why".
- Commits: small, conventional-ish prefixes (feat:, fix:, test:, docs:).
