//! Minimal, stable C ABI over the shared `Request`/`Response` model.
//!
//! One blocking entry point keeps FFI simple for ctypes/Node/Go consumers:
//! `vak_request(json) -> json`. The embedded tokio runtime and session
//! manager live inside the library; sessions persist between calls.
//!
//! Runtime safety: `block_on` is only legal on a thread that is *not* a
//! tokio worker. If the caller is already inside a runtime (e.g. a Python
//! `asyncio` loop or a Node native addon calling from a worker), we run the
//! future on a dedicated OS thread that `block_on`s the shared runtime from
//! the outside, avoiding the "cannot block the current thread from a runtime"
//! panic. The shared multi-thread runtime is intentionally global so the CDP
//! event-handler task (spawned per browser) stays alive across calls.
//!
//! Strings are UTF-8 C strings; every returned string must be released with
//! `vak_string_free`.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{Arc, OnceLock};

use vakbrowse_server::{Policy, Request, SessionManager};

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

fn manager() -> &'static SessionManager {
    static MANAGER: OnceLock<Arc<SessionManager>> = OnceLock::new();
    MANAGER.get_or_init(|| {
        let m = Arc::new(SessionManager::with_policy(Policy::from_env()));
        // Idle sessions hibernate (memory freed, state kept) in-process too.
        let _ctx = runtime().enter();
        m.spawn_reaper();
        m
    })
}

/// Drive a future to completion on the shared runtime, safe from a nested
/// caller runtime by way of a one-shot OS thread when one is current.
fn block_on<F, R>(f: F) -> R
where
    F: std::future::Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        // Caller is async; block_on would panic, so park the work on a
        // dedicated thread that is NOT a runtime worker.
        std::thread::spawn(move || runtime().block_on(f))
            .join()
            .expect("ffi worker thread panicked")
    } else {
        runtime().block_on(f)
    }
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
            Ok(request) => {
                let response = block_on(async { manager().handle(request).await });
                serde_json::to_string(&response).expect("response serializes")
            }
            Err(e) => format!("{{\"Err\":\"bad request: {e}\"}}"),
        }
    })
    .unwrap_or_else(|_| "{\"Err\":\"internal panic\"}".to_string());

    match CString::new(result) {
        Ok(s) => s.into_raw(),
        Err(_) => {
            CString::into_raw(CString::new("{\"Err\":\"string contained NUL\"}").expect("literal"))
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    // Regression: `vak_request` called from *inside* a tokio runtime must not
    // panic ("cannot block the current thread from within a runtime context").
    // The nested-runtime path should route through a dedicated blocking thread.
    #[tokio::test]
    async fn vak_request_works_from_within_a_runtime() {
        let req = CString::new(r#"{"type":"list_sessions"}"#).unwrap();
        let ptr = unsafe { vak_request(req.as_ptr()) };
        assert!(!ptr.is_null(), "vak_request returned a null pointer");
        let s = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe { vak_string_free(ptr) };
        assert!(s.contains("Sessions"), "got: {s}");
    }
}
