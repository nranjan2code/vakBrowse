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
`vakd-rest`. Both `vakd serve` (UDS) and `vakd-rest` (HTTP) now shut down
gracefully on SIGINT/SIGTERM: all sessions are closed (dropping browser
handles) and the UDS socket file is removed before exit.

Embedding from Python (no daemon needed — library owns its runtime):

```bash
pip install vakbrowse          # ships the bundled native lib + a Session class
# or bare ctypes against a cargo-built lib:
#   cargo build -p vakbrowse-ffi --release --config profile.release.strip=false
```

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

Or use the bundled SDK (the wheel bundles the cdylib; `strip=false` is applied
internally — see `python/setup.py` — because the workspace `strip=true` corrupts
a cdylib's `__LINKEDIT` alignment on macOS and dyld rejects it):

```python
from vakbrowse import Session
s = Session()
sid, url = s.open("https://example.com")
print(s.extract(sid)["text"])          # readable main-content text
print(s.batch(sid, [{"type":"navigate","url":"https://example.com"},
                     {"type":"extract"}]))   # one round-trip, fail-fast
s.close(sid)
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
  rejection): 56 passed / 2 ignored in both, clippy clean. The +6 over the
  prior 50 is: one real-stdio MCP handshake test (NDJSON, mirroring the `mcp`
  Python SDK), one identical handshake over **Content-Length** framing,
  three hermetic unit tests for the `StdioFramer` input normalizer, and a
  `wait_for_url` regression (match + timeout).
- The current cycle adds `Request::Batch` (fail-fast, one result per action)
  + `ResponsePayload::Results` across every surface, `RotateProxy` (re-launch
  the next endpoint in `SessionOptions.proxies`, **restoring the session
  URL**), `--human-timing` sub-150ms input jitter, and `--proxies a,b`. Test
  count is now **103 passed / 0 failed / 2 ignored** on macOS, Linux-root, and
  Linux-uid1000, clippy clean. New chrome-launching server tests are guarded by
  `browser_lock()` (serialized per test binary); the experimental `vakbrowse-dom`
  backend adds chrome-free coverage of the wire model (policy gate + Batch
  fail-fast surfacing a classified `ServiceError` kind) via
  `SessionManager::new(policy, Arc::new(DomLauncher))` — exactly the
  wire-model guarantee `vakd doctor` does not currently cover.
- Fixes: workspace version aligned to `0.4.0` (was `0.1.0`); CFT HTTP downloads
  now have a 30s (manifest) / 120s (binary) timeout (previously unbounded);
  `human_jitter` uses a splitmix64-mixed timestamp+counter (no longer
  `subsec_nanos % 130`, which was correlated in tight agent loops); DOM
  `wait_for_url` polls with CDP parity (was a single check → instant Timeout);
  DOM `ref_to_index("@e0")` now returns `None` (was `Some(0)`, aliasing `@e1`).
- Cookie round-trip test added via the DOM backend (hermetic `SetCookie` →
  `Cookies` → `ClearCookies` through `SessionManager`), replacing the CDP
  smoke test that asserted nothing.
  `./scripts/release.sh` bakes the pinned engine (`vakd doctor --no-probe`),
  runs all three gates, tags, and appends to `CHANGELOG.md`.
- Reproducible gates (GH Actions is intentionally disabled — see Golden Rule #6):
  `./scripts/verify.sh --mac` (host), `./scripts/verify.sh --root` and
  `./scripts/verify.sh --uid 1000` (linux/amd64 Docker, root + non-root).
  Every gate now builds with `--features vakbrowse-server/dom-backend` so the
  experimental DOM backend (`--backend dom`) is exercised alongside CDP.
  `./scripts/setup-hooks.sh` installs a `pre-push` hook that runs the fast
  host gate (`cargo test` + clippy) before every push; the linux gate is
  deliberate (it starts Docker) and is run before cutting a release tag.

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
                        #     click/fill/select/press_key/scroll/wait/find_by_css, cookies,
                        #     downloads, WebMCP list/invoke; args passed as
                        #     structured CallArgument via call_on_global)
  vakbrowse-dom         # Experimental pure-Rust backend: an html5ever-free
                        #   single-doc DOM tree + embedded QuickJS (on a
                        #   dedicated OS thread behind an mpsc bridge, since
                        #   QuickJS ctx is !Send but PageOps: Send) — so it runs
                        #   real JS with NO chromium process. eval_text,
                        #   wait_for_truthy, inline <script>, javascript: hrefs,
                        #   reactive location.href, AND a live document API
                        #   (getElementById + textContent/value get/set, backed
                        #   by the shared Arc<RwLock<Node>> so JS writes show
                        #   up in snapshot/extract/fill). Honest limits: no
                        #   layout / no network. Event dispatch lives in JS
                        #   (addEventListener/Event/requestSubmit; quick-js 0.4 is
                        #   name-only so dispatch runs in-JS, not via
                        #   call_function). `document.querySelector[s]` resolves CSS
                        #   to stable @eN refs via a dependency-free matcher
                        #   (`crates/vakbrowse-dom/src/selector.rs`).
                        #   Proves the seam is swappable. Opt-in per session via
                        #   `SessionOptions.backend = "dom"` (feature-gated
                        #   `dom-backend` on vakbrowse-server; see below).
  vakbrowse-server      # SessionManager, Request/Action/Response model, URL policy,
                        #   ServiceError structured wire errors, snapshot renderer,
                        #   UDS wire protocol (serve + client)
  vakbrowse-cli         # `vak` binary — thin clap wrapper over the wire client
  vakbrowse-mcp         # `vak-mcp` binary + VakMcp lib — MCP server (rmcp, stdio),
                        #   28 browser_* (tabs, history, screenshot/click-at,
                        #   extract, webmcp, wait_url, stealth/proxy on open);
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
- **CSS selector resolution** (`Action::FindByCss` → `ActionResult::Elements`;
  `vak find '<sel>'`; `browser_find_element {selector}`) returns `@eN` refs
  resolved against the current snapshot's element set — immediately
  `click`/`fill`-able, no intermediate snapshot. On CDP the selector is handed
  to `document.querySelectorAll` and matched back to `@eN` via the inverted
  `ref_to_ax`/`ax_to_backend` tables (so a CSS-found ref == a snapshot ref); on
  the DOM backend a dependency-free matcher walks interactive elements in
  snapshot order. Both backends return only refs resolvable in the snapshot
  (interactive elements); unsupported selectors (dom `:hover`/`+`) are an
  honest `Unsupported`, not a silent empty set.
- Actions use trusted input where it matters: clicks are real
  `Input.dispatchMouseEvent` sequences at box-model centers; fills use the
  native value setter + input/change events (React/Vue-safe). Root-frame
  clicks run `scrollIntoView({block:'center'})` on the target before reading
  its box-model center, so below-fold elements (long pages, SERP results)
  actually receive the click instead of being hit at a viewport point past the
  fold where nothing renders.
- `eval_text` stringifies every JS return type (JS-REPL semantics): strings
  pass through, numbers/booleans/undefined stringify, objects/arrays become
  JSON. This makes the canonical stealth probe `navigator.webdriver` observable
  (`false` under `--stealth`, `true` otherwise) instead of crashing with
  "invalid type: boolean …"; likewise `1+2` → `3` and `document.links.length`
  → the count. (Regression test: `eval_text_coerces_non_string_primitives`.)
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
- Snapshot self-heals: at snapshot time we reconcile against `page.url()`,
  so click-driven navigations (form submits, SPA links) update the reported
  URL and start a fresh ref turn — even though only explicit navigate()
  goes through our code.
- `click` reports its navigation outcome as `ActionResult::Clicked { navigated,
  url }` (was the silent `Done`). Detection is anchored to real `<a href>`
  clicks only (non-anchor / `javascript:`/fragment clicks return
  `navigated:false` in O(1) — no latency tax on the common click) and uses URL
  mutation as the sole signal (not the unreliable `wait_for_navigation`
  result). Bot-wall clicks (Bing/DDG accept the click but never navigate)
  surface as `navigated:false` + a daemon WARN, then **recover** via a
  ground-truth DOM `.click()` and a forced `location.href =` assignment before
  giving up honestly (see `click_recovers_from_preventdefault_wall`).
- `fill` focuses the element before setting its value; the human pattern
  fill -> press_key(Enter) therefore submits forms and SPA search boxes.
  Note for agent loops: on client-side SPAs (e.g. Wikipedia) `document.readyState`
  stays `'complete'` across route changes, so `wait` on a `readyState` predicate
  returns instantly and races the (slow) in-page navigation; wait on a URL/title
  predicate instead (`!location.href.includes('Main_Page')`). The `vak` CLI
  spells the key command `key`, with a `press_key` visible alias matching the
  `Action::PressKey` / `browser_press_key` naming.
- History actions (`back`/`forward`/`reload`) wait for navigation and
  tolerate two races: the old execution context dying mid-reload, and
  `wait_for_navigation` rejecting with "Inspected target navigated or
  closed" (which means the navigation succeeded).
- `extract` is the token-cheap reading tool: readability-style main-content
  extraction returning title/url/markdown-ish text (20KB from a 500KB
  Wikipedia page). Agents should prefer extract over eval for reading.
- Per-session proxy ships as `SessionOptions.proxy` (`--proxy` on CLI,
  `browser_open {proxy}` in MCP) — the answer to IP-reputation walls.
  Proxy *rotation* across a pool is `Action::RotateProxy`
  (`--proxies a,b` / `browser_rotate_proxy` / `vak rotate-proxy`): it
  re-launches Chrome on the next endpoint and **re-navigates to the session's
  last URL** so the agent carries on from where it left off. Honest limits:
  rotation only changes the source IP — it does NOT defeat TLS/HTTP2
  fingerprinting or behavioral biometrics (DDG/Bing/Cloudflare hard-wall even
  under `--stealth` + rotation).
- `vakbrowse-dom` (experimental pure-Rust backend, `DomLauncher`) proves the
  engine seam is swappable: it implements `EngineLauncher`/`PageOps` with no
  browser process and no JS engine. Honest limits: navigation is **file://
  only** (the tokenizer parses HTML in-process; `about:`/`http`/`https` are
  accepted by `validate_url` but no network fetch occurs); JS-dependent ops
  return `Unsupported` (`eval_text`, `wait_for_truthy`, `screenshot`,
  `click_at`, `press_key`); fills/mutations persist in the snapshot tree but
  no real rendering viewport exists. Useful for hermetic fixture-driven
  agent tests of the `Request::Batch`/`RotateProxy`/`fill→snapshot` path
  without chrome. Servo/Lightpanda remain the only route to real JS in a
  non-Chromium backend.
- Dogfood findings (real web): Wikipedia/GitHub/example.com/HN flows work
  end-to-end: a Wikipedia search -> click result lands on the article; an
  example.com -> click "Learn more" lands on www.iana.org — both proven by
  the rig parsing the real a11y snapshot to pick the link by name (no hardcoded
  refs). Bing and DuckDuckGo hard-wall ALL synthetic navigation (even under a
  `--stealth` profile): DuckDuckGo serves a CAPTCHA/empty shell (TLS/behavioral),
  and Bing result links (`bing.com/ck/a?...`) refuse to navigate even under a
  trusted `Input.dispatchMouseEvent` sequence OR a ground-truth DOM
  `element.click()` — the browser stays on the SERP. Agent recipe: prefer
  Wikipedia/example.com for search+click-through proofs; treat Bing/DDG as
  known behavioral limits, not defects. Stealth (`vak open --stealth`,
  `browser_open {stealth:true, stealth_seed:"..."}`, seed via `stealth_seed`)
  defeats webdriver/plugin/pointer tells but NOT TLS-fingerprint or
  behavioral-nav walls. MCP `browser_open` accepts an optional `stealth_seed`
  param (falls back to the `profile` id, then `"default"`).
- Sessions own a TAB REGISTRY (`tabs/new_tab/switch_tab/close_tab`); refs are
  per-tab. Snapshots merge AX trees across the frame tree (frame-prefixed ids
  `f0:` root, `f1:` …). Cross-frame clicks fire real DOM clicks on the
  resolved element because child-frame box coords are frame-relative —
  trusted mouse events apply to root-frame elements only today. Child-frame
  clicks dispatch a DOM `.click()` on the resolved element and then poll the
  top-level URL for a change (catches `_top`-targeting anchors + JS-driven
  `location.href`); same-frame iframe navigations remain invisible. OOPIF
  frames that reject frame-scoped CDP commands are skipped, not fatal. file://
  iframes are unique-origin: they cannot navigate `_top`; test signals must
  stay inside the frame.
- chromiumoxide v0.9.x is tokio-only; ureq v3 API (`into_body().into_reader()`),
  zip v8 extraction. CDP gotcha: in `Runtime.callFunctionOn` the resolved DOM
  node arrives as `this`, not as an argument.
- **MCP stdio accepts BOTH NDJSON and Content-Length framing.** rmcp 3.1.4's
  `transport::stdio()` / `AsyncRwTransport` speaks NDJSON only: `JsonRpcMessageCodec`
  encodes `{"jsonrpc":...}\n` and decodes by scanning for a `\n` delimiter
  (silently skipping any non-JSON line). Both official SDKs are NDJSON too —
  the `mcp` Python SDK (`client/stdio.py` writes `json + "\n"`, reads on `\n`)
  and `@modelcontextprotocol/sdk` TypeScript v1.26 (`shared/stdio.js`:
  `serializeMessage = JSON.stringify(msg) + '\n'`; `ReadBuffer` splits on `'\n'`;
  zero `Content-Length` references in the whole package) — so vak-mcp
  interoperated with Claude/Cursor/opencode even before this change. To also
  serve spec-literal Content-Length clients (the MCP spec *text* describes
  `Content-Length: N\r\n\r\n<bytes>`), `vak-mcp`'s **stdin** is wrapped in
  `vakbrowse_mcp::stdio_framer::StdioFramer` (`crates/vakbrowse-mcp/src/
  stdio_framer.rs`). It is a byte-level framing normalizer (no JSON parsing):
  a `tokio::io::duplex` transducer driven by `BufReader::read_until` /
  `read_exact` re-emits every incoming message — whether Content-Length-block
  or NDJSON-line, mixed freely — as an NDJSON line for rmcp's decoder. It never
  touches the write side, so **outgoing** responses stay NDJSON and every SDK
  reads them. Clients offer `protocolVersion "2025-11-25"` with empty `_meta:{}`.
  SEP-2575 `_meta` keys are only required for protocolVersion >= 2026-07-28;
  for earlier versions empty `_meta` satisfies the server. Covered by two
  real-subprocess handshake tests (NDJSON + Content-Length client) and three
  hermetic codec unit tests.
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
| P7 | Action batching (`Request::Batch`+`Results` over all surfaces), proxy rotation (`RotateProxy` w/ URL restore), `--human-timing` jitter, `scripts/release.sh` | **done** |

Post-roadmap ideas (not committed): WebDriver BiDI backend behind the engine
trait. DONE in-tree: pip packaging of the FFI (`python/`), proxy rotation
across endpoint pools (`RotateProxy`), and a non-Chromium experimental
backend (`vakbrowse-dom`) behind the `EngineLauncher`/`PageOps` seam — proven
swappable by a server integration test that drives `Request::Batch`,
`RotateProxy`, and fill→snapshot-value through `DomLauncher` with zero chrome.

| P8 | Experimental non-Chromium backend (`vakbrowse-dom`, feature-gated
  |     `dom-backend` on `vakbrowse-server`): QuickJS (`quick-js 0.4`) embedded
  |     on a dedicated OS thread behind an `mpsc` bridge (`Context: !Send` vs
  |     `PageOps: Send`). Ship `eval_text` (JS-REPL stringification),
  |     `wait_for_truthy`, inline `<script>` execution on `load`, reactive
  |     `location` (read + redirect-on-assignment, depth-bounded), live
  |     `document.getElementById` (read/write `textContent`/`value` over the
  |     shared `Arc<RwLock<Node>>`), `addEventListener`+`Event`+`requestSubmit`
  |     form-submit dispatch (JS-only loop), `javascript:` hrefs. Opt-in via
  |     `SessionOptions.backend="dom"` (`vak open --backend dom`,
  |     `browser_open {backend:"dom"}`, REST JSON); server-wide default via
  |     `VAKBROWSE_BACKEND=cdp|dom` (honored when the client omits
  |     `backend`). Routes through `SessionManager::launcher_for` under the
  |     engine seam; `Backend::Dom` without the feature fails loud (no silent
  |     Chrome fallback). Honest limits: no layout, no network. `Action::Click`
  |     returns `ActionResult::Clicked { navigated, url }` (anchor-aware URL-
  |     change ground truth) with a bot-wall recovery ladder (trusted mouse →
  |     DOM `.click()` → `location.href =`) before reporting `navigated:false`,
  |     proven over CDP + the REST/WS wire. | **done** |
| P9 | Servo/Lightpanda backend behind the engine trait (post-experimental) | pending |

## Conventions

- Edition 2024, workspace deps centralized in root `Cargo.toml`.
- Errors: single taxonomy in `vakbrowse-core::VakError`; engines map native
  errors into it at the boundary.
- No comments unless explaining non-obvious intent; keep them about "why".
- Commits: small, conventional-ish prefixes (feat:, fix:, test:, docs:).
