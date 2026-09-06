# Changelog

All notable changes to vakBrowse are documented here. Releases are cut with
`scripts/release.sh`, which bakes the pinned chrome-headless-shell engine,
runs the mac/linux-root/linux-uid1000 gates, tags, and appends an entry.

## Unreleased

- **Dropped the experimental DOM backend** (`vakbrowse-dom` crate, `DomLauncher`,
  `dom-backend` feature, `--backend dom` CLI/MCP/REST option, `VAKBROWSE_BACKEND`
  env var). CDP (chrome-headless-shell) is the sole engine backend. The
  `EngineLauncher`/`PageOps` trait seam remains in `vakbrowse-engine` for
  future backends, but `SessionManager` now holds a `CdpLauncher` directly
  (no `Arc<dyn EngineLauncher>` indirection). Removed the `Backend` enum,
  `SessionOptions.backend`, `SessionManager::with_policy_and_backend`, and the
  `backend` parameter from the Python SDK's `Session.open()`. This removes 35
  chrome-free tests and ~2,500 lines of non-production code. Test count:
  115 → 73 passed.
- **Version alignment**: workspace version bumped from `0.1.0` → `0.4.0` to
  match the Python SDK (`vakbrowse` PyPI) and git tags. Previously `cargo
  run -p vakd -- --version` reported `0.1.0` while `pip show vakbrowse`
  reported `0.4.0` — a source of confusion for the ctypes FFI story.
- **CFT HTTP timeouts**: `cft::http_get` (manifest fetch, 30s) and
  `blocking_download` (120s) now configure a `timeout_global` on the `ureq`
  agent, preventing indefinite hangs against `googlechromelabs.github.io` on
  slow or flaky CI/container networks. Previously had no timeout at all.
- **`human_jitter` de-correlation**: replaced wall-clock-derived
  `subsec_nanos() % 130` (highly correlated for consecutive calls in a tight
  agent loop) with a splitmix64-finalized mix of nanosecond timestamp + a
  thread-local call counter, producing well-distributed [20, 150] ms delays
  without adding an RNG dependency.
- **DOM backend `wait_for_url` CDP parity**: now polls `location.href` on a
  25ms interval up to the timeout (matching the CDP backend) instead of a
  single check-then-instant-error. The module doc is updated to clarify it
  still only observes the in-memory URL (can't see external navigations the
  DOM backend never received).
- **DOM backend `@e0` phantom ref fix**: `ref_to_index("@e0")` now returns
  `None` (via `checked_sub`) instead of silently resolving to the first
  interactive element. Refs are 1-based; `@e0` is not valid and must not
  alias `@e1`.
- **Playground showcase app** (`playground/`): a React + TypeScript + Vite +
  Tailwind UI that exercises every vakBrowse capability through `vakd-rest`'s
  `POST /playground/rpc` endpoint (same unified `Request` model as the daemon,
  CLI, MCP, and FFI). Features: interactive a11y snapshot with clickable `@eN`
  refs, action toolbar (click, fill, select, key, find, eval, extract, source,
  screenshot, file upload, cookies, downloads, batch), result pane, and a
  built-in guided tour. Feature-gated behind `vakbrowse-api/playground` (adds
  `mime_guess` dep; serves files from disk via `VAKBROWSE_PLAYGROUND_DIR` or
  `playground/static/`). Docker image includes a `playground` build target.
  Build via `./scripts/build-playground.sh`. Verified end-to-end against
  `https://example.com`, `iana.org`, and `en.wikipedia.org` (search → click
  → extract flow).
- **Cookie round-trip test**: added a hermetic DOM-backend integration test
  (`dom_backend_cookie_set_get_roundtrip`) verifying `SetCookie` → `Cookies`
  → `ClearCookies` through `SessionManager`. Replaces the CDP smoke test
  that asserted nothing (`let _ = all;` on a `file://` origin that exposes no
  cookie jar).
- **Graceful shutdown** (`vakd serve` + `vakd-rest`): both daemons now race
  their serve loop against `tokio::signal::ctrl_c()`. On SIGINT/SIGTERM, all
  sessions are closed via `SessionManager::close_all` (dropping CDP browser
  handles → tearing down chrome processes), and the UDS socket file is
  removed on clean exit. Previously Ctrl-C killed the process abruptly,
  orphaning chrome processes. `vakd-rest` uses axum's
  `with_graceful_shutdown`.
- **MCP stealth seed no longer hardcoded**: `browser_open` now accepts an
  optional `stealth_seed` parameter instead of always mapping `stealth:true`
  → `"mcp"`. Falls back to the `profile` id, then `"default"`, so different
  profiles/agents get different fingerprints.
- **CDP child-frame click navigation detection**: after a DOM `.click()` on a
  child-frame element, the click now polls the top-level URL for a change
  (catching `_top`-targeting anchors and JS-driven `location.href`
  assignments originating in the iframe) instead of always returning
  `stayed()`. Same-frame iframe navigations remain invisible (documented
  limitation).
- **`select_option` error classification**: the JS string returns
  (`"no-element"`, `"wrong-tag:DIV"`, `"error:..."`) are now mapped to
  `VakError::NotFound` / `VakError::Unsupported` / `VakError::Engine`
  respectively, instead of a catch-all `Engine("diagnostic: …")` leak.
- **File upload** (`Action::SetFileChooser` / `browser_set_file_chooser` /
  `vak set-file-chooser`): sets files on a `<input type=file>` element by `@eN`
  ref via CDP `DOM.setFileInputFiles` (bypasses browser security that blocks
  programmatic `.files` assignment) + dispatches `input`/`change` events. DOM
  backend returns `Unsupported`.
- **Page HTML source** (`Action::Source` / `browser_source` / `vak source`):
  returns `document.documentElement.outerHTML` — useful when the a11y snapshot
  loses details (canvas, collapsed elements, CSP-gated content). DOM backend
  serializes its in-memory tree.
- **Download listing** (`Action::Downloads` / `browser_downloads` / `vak
  downloads`): lists completed files in the session's download directory (path
  + size). DOM backend returns `Unsupported`.
- **Python SDK parity**: `vakbrowse.Session` now exposes the full action
  surface — `find`, `shot`, `click_at`, `source`, `downloads`, `select_option`,
  `set_file_chooser`, `cookies`, `set_cookie`, `clear_cookies`,
  `set_download_dir`, `scroll`, `wait_truthy`, `back`/`forward`/`reload`,
  `tabs`/`new_tab`/`switch_tab`/`close_tab`, `webmcp_tools`/`webmcp_invoke`.
  `click` now returns navigation info (`{"type":"clicked",...}`), and `open`
  accepts a `backend` parameter.
- **Multi-session concurrency** (hermetic, DOM backend): two integration tests
  (`dom_backend_multi_session_concurrency`,
  `dom_backend_multi_session_batch_isolation`) spawn parallel DOM sessions and
  prove snapshot ref isolation + batch isolation with zero chrome.

### Changed
- **`vakbrowse-dom` parser hardened in-place**: quote-aware tag scanning (`>` in
  quoted attrs no longer closes the tag), `&amp;`/`&lt;`/`&gt;`/`&quot;`/`&apos;`
  + numeric `&#NN;`/`&#xNN;` entity decoding in values and text, raw-text
  (`<script>`/`<style>` content not tokenized as markup) and RCDATA
  (`<textarea>`/`<title>`) handling, recursive-descent tree builder that
  actually nests children. Zero new dependencies — stays a fixture-test backend.
- **`vakbrowse-dom` wire-model coverage** (chrome-free): a server integration
  test now asserts the policy gate surfaces a classified `ServiceError::Policy`
  and that `Request::Batch` fails fast, surfacing the failing action's
  `ServiceError::Timeout` kind — the end-to-end classification that `vakd
  doctor` does not exercise.
- **Observable click navigation** (`Action::Click`): the CDP backend now returns
  `ActionResult::Clicked { navigated, url }` reporting whether the click
  triggered a (same-process) navigation. Detection is anchored to *real*
  `<a href>` clicks only (non-anchor / `javascript:`/fragment clicks return
  `navigated:false` in O(1) — no latency tax on the common click), and uses
  **URL mutation as ground truth** (polled, not the unreliable
  `wait_for_navigation` result which can resolve spuriously). Bot-wall clicks
  (Bing/DDG accept the click but never navigate) are surfaced as a WARN in the
  daemon and `navigated:false` to the agent instead of the silent `Done` of
  prior versions. Wire shape changes from `{"type":"done"}` to
  `{"type":"clicked","navigated":…,"url":…}` for clicks across CLI/MCP/REST/FFI.
- **`vakd doctor --probe` wire-model self-test**: `doctor` now additionally
  drives the daemon `SessionManager::handle` dispatch (open → snapshot →
  click → extract → batch → close) over a self-contained `data:` URL, asserting
  each `ResponsePayload` shape. Closes the gap where a green `doctor` did not
  guarantee the policy gate / batch fail-fast / ActionResult classification.
- **Bot-wall recovery ladder** (built on the click signal above): when an
  anchor click fails to navigate within `CLICK_NAV_TIMEOUT`, the CDP backend
  escalates through progressively less-synthetic navigation — (1) the trusted
  mouse dispatch, (2) a ground-truth DOM `element.click()`, (3) a forced
  `location.href = <href>` assignment (the one move known to defeat Bing/DDG's
  click interception, which blocks trusted-input events but not a direct
  navigation). URL mutation is the sole signal (not the unreliable
  `wait_for_navigation` result). A fixture with `preventDefault` proves the
  ladder recovers a blocked anchor (`click_recovers_from_preventdefault_wall`).
  Covered by CDP (`actions.rs`) and server (`manager.rs`) tests, plus **REST +
  WebSocket round-trip tests** proving the `Clicked` shape crosses every
  transport (`click_signal_over_ws`, `click_no_navigation_over_ws`,
  `click_navigates_over_http`). Real TLS/behavioral walls still win; only
  click-interception is defeated.
- **`extract` readability hardening**: the `innerText.length`-only scorer now
  discounts each candidate by its **link density** (`textLen * (1 - linkDensity)`),
  so a link-dense sidebar/nav-list with longer text no longer wins over a shorter
  genuine `<article>`/prose block. Selectors broadened (`section`, `.articlebody`,
  `.pagecontent`); body fallback retained and now chrome-stripped when no candidate
  has real prose. Proven by `extract_prefers_prose_over_linkdense_sidebar` against a
  fixture whose `.post` sidebar text exceeds the article's.
- **Embedding gate**: `cargo build -p vakbrowse-ffi --release
  --config profile.release.strip=false` produces a loadable cdylib (workspace
  `strip=true` corrupts macOS `__LINKEDIT`), and the Python SDK's 6/6 offline
  tests pass against it — including `act`/`batch`/`click` over ctypes+JSON.
  Reproducible gates (mac/linux-root/linux-uid1000): 101 passed / 0 failed / 2
  ignored, clippy 0.
- **Reactive `location` on the DOM backend** (`vakbrowse-dom`): `location.href`
  now reads back the current document URL, and assigning `location.href = '<url>'`
  in an inline `<script>` (the `javascript:`-href return-value path was already
  correct) drives a real redirect — depth-bounded to break loops — chrome-free.
  This lets the non-Chromium backend follow JS-driven (SPA-style) navigation;
  proven by `js_inline_script_redirects_via_location_href` +
  `js_location_href_reflects_current_url`. `parse_attr` hardened against a greedy
  quote-trim that truncated `javascript:` hrefs to the first inner quote.
- **Real-JS non-Chromium backend** (`vakbrowse-dom`): the experimental DOM
  backend now embeds **QuickJS** (`quick-js`) on a dedicated OS thread behind an
  `mpsc` bridge — QuickJS contexts are `!Send` but `PageOps: Send`, so the thread
  isolates the engine while `DomPage` stays `Send`/movable across the multi-thread
  runtime. The DOM backend is now genuinely JS-capable (no chrome): `eval_text`
  (JS-REPL stringification: primitives stringify, objects→JSON, matching CDP),
  `wait_for_truthy` (polls `Boolean(expr)`), inline `<script>` bodies captured by
  the tokenizer and executed on `load`, and `javascript:` hrefs (non-empty string
  result → navigate; `void(...)`/non-string → stay). Honest limits now **no
  layout / no network** — every other JS/DOM capability below works. Proven by
  hermetic tests with zero chrome launches. Tokenizer `parse_attr` hardened: a
  naive `trim_matches` greedily ate inner quotes, truncating `javascript:`
  hrefs — now strips only one matching outer pair.
- **Live `document` API on the DOM backend** (`vakbrowse-dom`): the QuickJS
  worker now exposes `document.getElementById` backed by the page's `Arc<RwLock<Node>>`
  tree via `quick-js` callbacks — so `document.getElementById('x').textContent`
  reads and `.textContent = v` / `.value = v` *writes* mutate the same DOM that
  `snapshot`/`extract`/`fill` consume. Proven by
  `js_document_get_element_by_id_reads_text_content` +
  `js_document_value_write_visible_to_snapshot` (a JS `.value =` write is read
back by the Rust snapshot path). Proven by
  `js_document_get_element_by_id_reads_text_content` +
  `js_document_value_write_visible_to_snapshot` (a JS `.value =` write is read
  back by the Rust snapshot path).
- **DOM event dispatch on the backend** (`vakbrowse-dom`): `addEventListener`
  + `Event` + `requestSubmit` now work — the dispatch loop lives entirely in JS
  (a `__listeners` registry; quick-js 0.4's `call_function` is name-only and
  can't pass JS objects, so `__fireSubmit` constructs the `Event` and invokes
  the registered handler in-JS). form.html's inline
  `addEventListener('submit', h)` now fires when `requestSubmit()` is driven: `h`
  reads the field `.value`s and writes `#out.textContent`, chrome-free. Proven
  by `js_form_submit_event_dispatches_handler_to_out`. Honest limits: only
  `submit`/`requestSubmit` + `preventDefault` are wired; arbitrary
  `dispatchEvent`/synthetic events on arbitrary elements are not.
- **`--backend dom` opt-in** (feature-gated `dom-backend` on `vakbrowse-server`):
  `SessionOptions.backend = "dom"` (CLI `--backend dom`, MCP `browser_open
  {backend:"dom"}`, REST JSON `{"options":{"backend":"dom"}}`) routes the session
  to the experimental `DomLauncher` via `launcher_for` — chrome-free for a11y +
  text + JS-redirect flows. Off by default (no QuickJS in default builds);
  requests `dom` without the feature fail loud instead of silently falling back
  to Chrome. Proven by `dom_backend_opened_via_session_option_routes_to_dom`,
  gated under the feature and included in all 3 reproducible gates (99-passed
  bar).
- **Server-wide backend default** (`vakd-rest` / `vakd`): `VAKBROWSE_BACKEND=cdp|dom`
  now sets `SessionManager.default_backend`, honored when a client omits
  `SessionOptions.backend` (CLI `--backend` unset / MCP `backend` unset / REST
  omits the key → server default; explicit override still wins). `serve()`
  takes the default backend; the daemon passes it through. Proven by
  `dom_backend_server_default_backend_routes_to_dom`. The `--backend dom` entry
  above also notes `vakd-rest` now requires the feature flag to compile-link the
  DOM launcher (off by default; `--features vakbrowse-server/dom-backend`).
  All reproducible gates (`verify.sh`) now build with that feature.
- **CSS selector resolution** (`Action::FindByCss` → `ActionResult::Elements`,
  `vak find <selector>`, `browser_find_element {selector}`): agents resolve
  elements by CSS (`a[href]`, `form input[type="text"]`, `button.btn.primary`,
  descendant/child combinators, comma groups) and receive `@eN` refs that are
  immediately `click`-able — no intermediate `snapshot` needed. Both backends
  support it:
  - **CDP (real Chrome):** hands the selector to `document.querySelectorAll`
    and maps each matched `Element.backend_node_id` back to the snapshot's `@eN`
    via the inverted `ref_to_ax`/`ax_to_backend` tables, so a `find_by_css`
    ref is identical to a `snapshot` ref and drives `click`/`fill` directly.
    Pseudos (`:hover`) are deferred to Chrome's own semantics (match nothing in
    a headless, cursor-less context rather than erroring); genuinely invalid
    syntax surfaces a real error so agents can distinguish "no match" from "bad
    selector".
  - **DOM (chrome-free):** a dependency-free QuickJS-Rust matcher
    (`crates/vakbrowse-dom/src/selector.rs`) walks interactive elements in the
    same pre-order `snapshot` numbers, so returned refs slot into `ref_to_index`→`click`.
    Unsupported constructs (pseudos `:hover`, sibling `+`/`~`) return an honest
    `VakError::Unsupported`. The QuickJS worker also exposes
    `document.querySelector[s]` backed by a `_vkb_find` callback over the shared
    `Arc<RwLock<Node>>`.
  Both paths return only refs resolvable in the current snapshot (interactive
  elements); non-interactive matches (e.g. `div`) are omitted. Proven by
  `selector_descendant_and_child_combinators` (lib),
  `dom_backend_find_by_css_drives_click_to_navigate` (find `a[href="form.html"]`→click→navigate),
  `dom_backend_find_by_css_combinator_and_empty_semantics`, and CDP's
  `find_by_css_on_cdp_resolves_clickable_ref` +
  `find_by_css_on_cdp_resolves_snapshot_consistent_refs` (finds match snapshot
  refs; click navigates). Counts 101 across mac/linux-root/linux-uid1000.
