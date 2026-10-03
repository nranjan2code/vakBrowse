# vakBrowse Python SDK

Embed an agent-native Chromium browser from Python. The native cdylib
(`libvakbrowse_ffi`) ships inside the wheel — no `cargo` or daemon needed.

```bash
pip install vakbrowse
```

```python
from vakbrowse import Session

s = Session()
sid, url = s.open("file:///path/to/form.html")
tree = s.snapshot(sid)            # a11y tree with stable @eN refs
name_ref = next(e["ref"] for e in tree["elements"]
                if e["role"] == "textbox" and "name" in e["name"].lower())
s.fill(sid, name_ref, "Linus")
print(s.extract(sid))             # main-content text (20k-char window)
print(s.extract(sid, offset=20000, max_chars=5000))  # next window
s.close(sid)

# One round-trip for multiple steps (fail-fast):
s.batch(sid, [{"type": "navigate", "url": "https://example.com"},
              {"type": "extract"}])
```

### Options

`Session().open(...)` accepts the full `SessionOptions` surface:

| kwarg | maps to |
|---|---|
| `proxies=[a,b]` | `SessionOptions.proxies` (use with `rotate_proxy`) |
| `proxy="socks5://..."` | single `SessionOptions.proxy` |
| `stealth=True` | deterministic `StealthProfile` (`navigator.webdriver` hidden) |
| `human_timing=True` | sub-150ms randomized input jitter |
| `profile="agent-1"` | persistent Chromium user-data dir |

Override the native lib location with `VAKBROWSE_FFI_LIB=/path/to/libvakbrowse_ffi.so`.

### Bot-wall honesty

Rotation changes the source IP only — it does **not** defeat TLS/HTTP2
fingerprinting or behavioral biometrics. DDG/Bing/Cloudflare hard-wall even
under `stealth=True` + `rotate_proxy()`.
