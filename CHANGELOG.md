# Changelog

All notable changes to vakBrowse are documented here. Releases are cut with
`scripts/release.sh`, which bakes the pinned chrome-headless-shell engine,
runs the mac/linux-root/linux-uid1000 gates, tags, and appends an entry.

## Unreleased

- **Action batching** (`Request::Batch` + `ResponsePayload::Results`): run a
  sequence of actions in one round-trip with fail-fast, cutting agent latency
  across every surface — `vak batch`, `browser_batch`, `POST /batch`,
  UDS + WebSocket, and the FFI. One result per action, in order.
- **Proxy rotation** (`Action::RotateProxy`): re-launches Chrome on the next
  endpoint in `SessionOptions.proxies` (`--proxies a,b` / `browser_rotate_proxy`
  / `vak rotate-proxy`) and **restores the session's last URL** so an agent can
  carry on after a bot-wall challenge.
- **Human timing** (`--human-timing`): injects sub-150ms randomized input
  delays before navigate/eval/click/fill/press/scroll/click-at to break
  cadence-based behavioral tells. Honest — no TLS/HTTP2 spoofing.
- **`wait_url`**: SPA-safe URL wait polling `location.href` (already landed in
  the prior cycle).
- **verify.sh**: fixed the Linux-root Docker source-mount regression (the tree
  is now always mounted at `/src` whether or not the `vk-cargo` volume exists).
