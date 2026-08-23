//! Minimal, stable C ABI over the shared `Request`/`Response` model.
//!
//! One blocking entry point keeps FFI simple for ctypes/Node/Go consumers:
//! `vak_request(json) -> json`. The embedded tokio runtime and session
//! manager live inside the library; sessions persist between calls.
//!
//! Strings are UTF-8 C strings; every returned string must be released with
//! `vak_string_free`.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char};
use std::sync::OnceLock;

use vakbrowse_server::{Policy, Request, SessionManager};

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime"))
}

fn manager() -> &'static SessionManager {
    static MANAGER: OnceLock<SessionManager> = OnceLock::new();
    MANAGER.get_or_init(|| SessionManager::new(Policy {
        url_allow_prefixes: std::env::var("VAKBROWSE_ALLOW_PREFIXES")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .collect(),
    }))
}

const VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

/// Returns a static version string; do not free.
#[unsafe(no_mangle)]
pub extern "C" fn vak_version() -> *const c_char {
    VERSION.as_ptr() as *const c_char
}

/// Execute one request (same JSON shape as the daemon wire protocol).
/// Always returns a JSON document — check its "Ok"/"Err" key. Free with
/// `vak_string_free`.
///
/// # Safety
/// `request_json` must be a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vak_request(request_json: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(|| {
        // SAFETY: caller guarantees a valid NUL-terminated UTF-8 C string.
        let cstr = unsafe { CStr::from_ptr(request_json) };
        let text = cstr.to_string_lossy();
        
        match serde_json::from_str::<Request>(&text) {
            Ok(request) => serde_json::to_string(&runtime().block_on(manager().handle(request)))
                .expect("response serializes"),
            Err(e) => format!("{{\"Err\":\"bad request: {e}\"}}"),
        }
    })
    .unwrap_or_else(|_| "{\"Err\":\"internal panic\"}".to_string());

    match CString::new(result) {
        Ok(s) => s.into_raw(),
        Err(_) => CString::into_raw(CString::new("{\"Err\":\"string contained NUL\"}").expect("literal")),
    }
}

/// Free a string previously returned by this library.
///
/// # Safety
/// `ptr` must be from `vak_request` and not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vak_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        // SAFETY: caller guarantees ptr came from vak_request and is live.
        drop(unsafe { CString::from_raw(ptr) });
    }
}
