"""vakBrowse Python SDK.

Embed an agent-native Chromium browser with the same request model the daemon
serves over UDS/REST/WebSocket. Each call JSON-round-trips through
`vak_request` (a stable C ABI), so behavior is identical to the wire protocol.

The native lib ships inside this package (``libvakbrowse_ffi.*``). Override
its location with ``VAKBROWSE_FFI_LIB`` if you need to point at a build tree.
"""
from __future__ import annotations

import ctypes
import json
import os
import sys

__all__ = ["Session", "VakError", "__version__"]
__version__ = "0.4.0"

_CDL = None  # lazily loaded; see get_lib()


class VakError(Exception):
    """An application-level error surfaced by the request handler."""


def _lib_name() -> str:
    if sys.platform == "darwin":
        return "libvakbrowse_ffi.dylib"
    if os.name == "nt":
        return "vakbrowse_ffi.dll"
    return "libvakbrowse_ffi.so"


def get_lib() -> ctypes.CDLL:
    """Locate and load the bundled (or overridden) cdylib."""
    global _CDL
    if _CDL is not None:
        return _CDL
    path = os.environ.get("VAKBROWSE_FFI_LIB") or os.path.join(
        os.path.dirname(os.path.abspath(__file__)), _lib_name()
    )
    if not os.path.exists(path):
        raise FileNotFoundError(
            f"vakBrowse FFI library not found at {path}. Set VAKBROWSE_FFI_LIB "
            "to a built libvakbrowse_ffi, or `pip install vakbrowse` (the wheel "
            "bundles it)."
        )
    lib = ctypes.CDLL(path)
    # vak_request(const char* json) -> char* (malloc'd; caller frees).
    lib.vak_request.argtypes = [ctypes.c_char_p]
    lib.vak_request.restype = ctypes.c_void_p
    lib.vak_string_free.argtypes = [ctypes.c_void_p]
    lib.vak_string_free.restype = None
    _CDL = lib
    return lib


class Session:
    """High-level wrapper around the vakBrowse FFI session manager.

    One process = one global session table (the lib owns a multi-thread tokio
    runtime), so you can run many sessions from a single ``Session()``.
    """

    def _request(self, obj: dict) -> dict:
        raw = json.dumps(obj, separators=(",", ":")).encode("utf-8")
        lib = get_lib()
        ptr = lib.vak_request(raw)
        if not ptr:
            raise VakError("vak_request returned NULL (internal panic?)")
        try:
            text = ctypes.string_at(ptr).decode("utf-8")
        finally:
            lib.vak_string_free(ptr)
        resp = json.loads(text)
        if "Err" in resp:
            # Transport-level failure (handler panic / encode error).
            raise VakError(resp["Err"])
        return resp["Ok"]  # ResponsePayload (externally tagged)

    def _payload(self, obj: dict) -> dict:
        """Like `_request` but also lifts app-level errors (an action failed)
        into a `VakError`. The server returns those as
        `Ok(ResponsePayload::Error{ kind: msg })`, NOT `Err`."""
        p = self._request(obj)
        if "Error" in p:
            err = p["Error"]
            kind = next(iter(err))
            raise VakError(f"{kind}: {err[kind]}")
        return p

    # ---- convenience actions ----
    def sessions(self) -> list:
        return self._payload({"type": "list_sessions"})["Sessions"]

    def open(
        self,
        url: str | None = None,
        *,
        stealth: bool = False,
        proxies: list[str] | None = None,
        proxy: str | None = None,
        human_timing: bool = False,
        profile: str | None = None,
        headed: bool = False,
    ) -> tuple[str, str]:
        """Open a session. Returns (session_id, initial_url)."""
        opts: dict = {"headless": not headed}
        if url:
            opts["url"] = url
        if profile:
            opts["profile"] = profile
        if stealth:
            opts["stealth_seed"] = profile or "default"
        if proxy:
            opts["proxy"] = proxy
        if proxies:
            opts["proxies"] = list(proxies)
        if human_timing:
            opts["human_timing"] = True
        info = self._payload({"type": "open", "options": opts})["Opened"]
        return info["id"], info["url"]

    def act(self, sid: str, action: dict) -> dict:
        """Run one action, returning its ``ActionResult`` payload."""
        return self._payload({"type": "act", "session": sid, "action": action})["Result"]

    def batch(self, sid: str, actions: list[dict]) -> list:
        """Run a sequence of actions in one round-trip (fail-fast)."""
        return self._payload({"type": "batch", "session": sid, "actions": actions})["Results"]

    def snapshot(self, sid: str) -> dict:
        """Snapshot the a11y tree; convenience on top of ``act``."""
        return self.act(sid, {"type": "snapshot"})["snapshot"]

    def extract(self, sid: str) -> str:
        return self.act(sid, {"type": "extract"})["text"]

    def navigate(self, sid: str, url: str) -> dict:
        return self.act(sid, {"type": "navigate", "url": url})

    def click(self, sid: str, ref: str) -> dict:
        """Click an element by @eN ref. Returns ``{"type":"clicked","navigated":bool,"url":str|None}``."""
        return self.act(sid, {"type": "click", "ref": ref})

    def fill(self, sid: str, ref: str, text: str) -> None:
        self.act(sid, {"type": "fill", "ref": ref, "text": text})

    def select_option(self, sid: str, ref: str, value: str) -> bool:
        """Select an `<option>` by value. Returns whether the selection took effect."""
        return bool(self.act(sid, {"type": "select_option", "ref": ref, "value": value})["ok"])

    def set_file_chooser(self, sid: str, ref: str, paths: list[str]) -> bool:
        """Set files on a ``<input type=file>`` element (by @eN ref)."""
        return bool(self.act(sid, {"type": "set_file_chooser", "ref": ref, "paths": paths})["ok"])

    def press_key(self, sid: str, key: str) -> None:
        self.act(sid, {"type": "press_key", "key": key})

    def scroll(self, sid: str, dx: float, dy: float) -> None:
        self.act(sid, {"type": "scroll", "dx": dx, "dy": dy})

    def find(self, sid: str, selector: str) -> list[str]:
        """Resolve a CSS selector to stable @eN refs (immediately clickable)."""
        return self.act(sid, {"type": "find_by_css", "selector": selector})["elements"]["refs"]

    def eval(self, sid: str, expression: str) -> str:
        """Evaluate a JS expression; return its stringified value."""
        return self.act(sid, {"type": "eval_text", "expression": expression})["text"]

    def wait_truthy(self, sid: str, expression: str, timeout_ms: int = 5000) -> None:
        """Poll a JS expression until truthy or timeout (raises VakError)."""
        self.act(sid, {"type": "wait_for_truthy", "expression": expression, "timeout_ms": timeout_ms})

    def wait_url(self, sid: str, pattern: str, timeout_ms: int = 5000) -> bool:
        """SPA-safe URL wait: poll location.href for a substring. Returns True on
        match, False on timeout (no exception raised)."""
        try:
            self.act(sid, {"type": "wait_for_url", "pattern": pattern, "timeout_ms": timeout_ms})
            return True
        except VakError:
            return False

    def shot(self, sid: str, full_page: bool = False) -> str:
        """Take a screenshot. Returns base64-encoded PNG."""
        return self.act(sid, {"type": "screenshot", "full_page": full_page})["png_base64"]

    def click_at(self, sid: str, x: float, y: float) -> None:
        """Click raw viewport coordinates (vision fallback)."""
        self.act(sid, {"type": "click_at", "x": x, "y": y})

    def source(self, sid: str) -> str:
        """Return the current page HTML source."""
        return self.act(sid, {"type": "source"})["text"]

    def downloads(self, sid: str) -> list[dict]:
        """List completed downloads in the session's download directory."""
        text = self.act(sid, {"type": "downloads"})["text"]
        return json.loads(text)

    def cookies(self, sid: str) -> list[dict]:
        """Get all cookies for the current page."""
        return self.act(sid, {"type": "cookies"})["cookies"]

    def set_cookie(self, sid: str, cookie: dict) -> None:
        """Set a cookie. ``cookie`` has keys: name, value, domain, path,
        secure (bool), http_only (bool), same_site ('Strict'|'Lax'|'None')."""
        self.act(sid, {"type": "set_cookie", "cookie": cookie})

    def clear_cookies(self, sid: str) -> None:
        self.act(sid, {"type": "clear_cookies"})

    def set_download_dir(self, sid: str, dir: str) -> None:
        """Set the download directory for this session (must be set before download starts)."""
        self.act(sid, {"type": "set_download_dir", "dir": dir})

    def back(self, sid: str) -> dict:
        return self.act(sid, {"type": "back"})

    def forward(self, sid: str) -> dict:
        return self.act(sid, {"type": "forward"})

    def reload(self, sid: str) -> dict:
        return self.act(sid, {"type": "reload"})

    def tabs(self, sid: str) -> list[dict]:
        """List tabs (active tab first)."""
        return self.act(sid, {"type": "tabs"})["tabs"]

    def new_tab(self, sid: str, url: str | None = None) -> dict:
        """Open a new tab; it becomes active."""
        action: dict = {"type": "new_tab"}
        if url:
            action["url"] = url
        return self.act(sid, action)["tab"]

    def switch_tab(self, sid: str, tab: str) -> None:
        self.act(sid, {"type": "switch_tab", "tab": tab})

    def close_tab(self, sid: str, tab: str) -> bool:
        """Close a tab. The last remaining tab cannot be closed."""
        return bool(self.act(sid, {"type": "close_tab", "tab": tab})["ok"])

    def webmcp_tools(self, sid: str) -> list[dict]:
        """List tools the page declares via WebMCP."""
        return self.act(sid, {"type": "webmcp_tools"})["tools"]

    def webmcp_invoke(self, sid: str, name: str, arguments_json: str = "{}") -> str:
        """Invoke a page-declared WebMCP tool."""
        return self.act(sid, {"type": "webmcp_invoke", "name": name, "arguments_json": arguments_json})["text"]

    def rotate_proxy(self, sid: str) -> bool:
        """Rotate to the next proxy in the session's pool (re-launches browser)."""
        return bool(self.act(sid, {"type": "rotate_proxy"})["ok"])

    def close(self, sid: str) -> bool:
        return self._payload({"type": "close", "session": sid})["Closed"]
