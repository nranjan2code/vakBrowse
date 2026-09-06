# vakBrowse Playground

A showcase web app for vakBrowse's full capability surface — a browser
automation UI built on top of the `vakd-rest` HTTP API.

## Architecture

```
playground/
├── backend/             # Thin wrapper — uses vakd-rest's built-in server
├── frontend/            # React + TypeScript + Vite + Tailwind UI
│   ├── src/
│   │   ├── App.tsx             # Root: session sidebar + snapshot + result pane
│   │   ├── components/         # UI components (see below)
│   │   ├── hooks/              # React hooks (useSessions, useSession)
│   │   └── lib/
│   │       ├── actions.ts      # Typed wrappers around the /playground/rpc endpoint
│   │       └── types.ts        # Wire types mirroring vakbrowse-server's model
│   ├── vite.config.ts          # base=/playground/ in production
│   └── package.json
├── static/            # Production build output (served by vakd-rest)
├── Cargo.toml         # Not used — backend is vakd-rest with --features playground
└── build-playground.sh  # Build both frontend + Rust binary
```

### How it works

1. **Frontend** (React app) talks to a single `/playground/rpc` endpoint that
   accepts the same `Request` JSON model as the daemon's UDS protocol.
   This means every field, action, and capability available to the CLI/MCP/Daemon
   is automatically available to the playground — no API mapping needed.

2. **Backend** is `vakd-rest` compiled with `--features vakbrowse-api/playground`.
   This adds:
   - `POST /playground/rpc` — unified Request↔Response JSON RPC
   - `GET /playground[/index.html]` — serves the React SPA
   - `GET /playground/{*path}` — serves JS/CSS assets with SPA fallback

3. **Engine** — `vakd-rest` launches Chrome via the CDP backend (default) or
   the DOM backend (`VAKBROWSE_BACKEND=dom`). The playground's Open Session
   dialog lets you toggle stealth, proxy, backend, and headed mode.

### Components

| Component         | File              | Purpose                                      |
|---|---|---|
| `App`             | `App.tsx`         | Root layout: sidebar + main + result pane    |
| `SessionManager`  | `SessionManager.tsx` | Session list, open/close, advanced options |
| `SnapshotView`    | `SnapshotView.tsx`  | Clickable a11y snapshot with `@eN` refs     |
| `ActionToolbar`   | `ActionToolbar.tsx` | All actions: click, fill, eval, source, screenshot, etc. |
| `ResultPane`      | `ResultPane.tsx`   | Shows last action result (extract text, image, etc.) |
| `TourGuide`       | `TourGuide.tsx`    | Guided tour through all capabilities         |
| `useSession`      | `hooks/useSession.ts` | Session state + action dispatch             |

## Build & Run

### Option 1: All-in-one (serves UI + API from same port)

```bash
cd /path/to/vakBrowse
./scripts/build-playground.sh
# Then run:
target/debug/vakd-rest
# Open http://localhost:7788/playground/
```

### Option 2: Dev mode (hot-reload frontend)

```bash
cd /path/to/vakBrowse
# Terminal 1 — API server (without playground feature is fine for dev):
cargo run -p vakd-rest
# Terminal 2 — Vite dev server:
./scripts/build-playground.sh --dev
# Open http://localhost:3000
```

### Docker

```bash
docker build --target playground -t vakbrowse-playground .
docker run -p 7788:7788 vakbrowse-playground
# Open http://localhost:7788/playground/
```

## What You Can Do

- **Open a session** — navigate to any URL, with optional stealth mode, proxy, or DOM backend
- **Snapshot** — see all interactive elements as `@eN` refs, click any to select
- **Click & fill** — interact with elements by ref (CSS-selector-resolved refs work too)
- **Extract** — get readability-style main-content text
- **Source** — view full HTML of the current page
- **Screenshot** — capture PNG of the page
- **Eval** — run JavaScript expressions (REPL stringification)
- **File upload** — set file input elements
- **Cookies** — list, set, and clear cookies per session
- **Downloads** — list completed downloads
- **Batch** — run multiple actions in one request (fail-fast)
- **Tour** — click the `? Tour` button for a guided walkthrough of all capabilities
