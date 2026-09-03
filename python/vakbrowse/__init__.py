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

    def click(self, sid: str, ref: str) -> None:
        self.act(sid, {"type": "click", "ref": ref})

    def fill(self, sid: str, ref: str, text: str) -> None:
        self.act(sid, {"type": "fill", "ref": ref, "text": text})

    def press_key(self, sid: str, key: str) -> None:
        self.act(sid, {"type": "press_key", "key": key})

    def wait_url(self, sid: str, pattern: str, timeout_ms: int = 5000) -> bool:
        try:
            self.act(sid, {"type": "wait_for_url", "pattern": pattern, "timeout_ms": timeout_ms})
        except VakError:
            return False
        return True

    def rotate_proxy(self, sid: str) -> bool:
        return bool(self.act(sid, {"type": "rotate_proxy"})["ok"])

    def close(self, sid: str) -> bool:
        return self._payload({"type": "close", "session": sid})["Closed"]
